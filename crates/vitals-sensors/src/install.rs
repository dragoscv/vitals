//! Elevated `install` / `uninstall`.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vitals_sensors::{
    HELPER_EXE, PAWNIO_SETUP_SHA256, PAWNIO_UNINSTALL_KEY, SERVICE_NAME, exit, pawnio_version_ok,
    read_pipe, sha256_hex,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::HSTRING;
use windows_service::service::{
    ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

const SETUP_COPY_NAME: &str = "PawnIO_setup.exe";

#[derive(Debug)]
pub struct Failure {
    pub code: i32,
    pub message: String,
}

impl Failure {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            code: exit::FAILED,
            message,
        }
    }
}

pub fn is_elevated() -> bool {
    // SAFETY: the token handle is closed below; the output struct is sized
    // correctly for TokenElevation.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token).is_err() {
            return false;
        }
        let mut elev = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elev).cast()),
            u32::try_from(size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
            &raw mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elev.TokenIsElevated != 0
    }
}

fn require_elevated() -> Result<(), Failure> {
    if is_elevated() {
        Ok(())
    } else {
        Err(Failure::new(
            exit::NOT_ELEVATED,
            "access denied: run from an elevated (Administrator) prompt",
        ))
    }
}

fn env_dir(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map_or_else(|| PathBuf::from(fallback), PathBuf::from)
}

/// `%ProgramFiles%\Vitals Sensors` — admin-only, so the file the SCM starts
/// as SYSTEM cannot be replaced by the user who asked for it.
pub fn install_dir() -> PathBuf {
    env_dir("ProgramFiles", r"C:\Program Files").join("Vitals Sensors")
}

fn log_path() -> PathBuf {
    env_dir("ProgramData", r"C:\ProgramData")
        .join("Vitals Sensors")
        .join("install.log")
}

struct Log(Option<std::fs::File>);

impl Log {
    fn open() -> Self {
        let p = log_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        Self(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok(),
        )
    }

    fn line(&mut self, msg: &str) {
        println!("{msg}");
        if let Some(f) = &mut self.0 {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = writeln!(f, "[{secs}] {msg}");
        }
    }
}

/// `DisplayVersion` of the `PawnIO` uninstall entry (e.g. "2.2.0.0").
pub fn pawnio_version() -> Option<String> {
    let mut buf = [0u16; 128];
    let mut len = u32::try_from(buf.len() * 2).unwrap_or(0);
    // SAFETY: buffer and length are consistent.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(PAWNIO_UNINSTALL_KEY),
            &HSTRING::from("DisplayVersion"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut len),
        )
    };
    if rc.is_err() {
        return None;
    }
    let n = (len as usize / 2).min(buf.len());
    let s = String::from_utf16_lossy(&buf[..n]);
    Some(s.trim_end_matches('\0').trim().to_owned())
}

fn ensure_pawnio(setup: Option<&Path>, dir: &Path, log: &mut Log) -> Result<(), Failure> {
    let v = pawnio_version();
    if pawnio_version_ok(v.as_deref()) {
        log.line(&format!(
            "PawnIO {} already installed",
            v.unwrap_or_default()
        ));
        return Ok(());
    }
    let Some(src) = setup else {
        return Err(Failure::new(
            exit::NO_PAWNIO,
            "PawnIO not installed (pass --pawnio-setup <PawnIO_setup.exe 2.2.0>)",
        ));
    };
    // Hash the COPY in the admin-only directory, so the file that runs is the
    // file that was hashed. The source sits in the user's temp directory,
    // where the unelevated user could swap it between check and use.
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let copy = dir.join(SETUP_COPY_NAME);
    std::fs::copy(src, &copy).map_err(|e| format!("copy {}: {e}", src.display()))?;
    let result = run_pinned_setup(&copy, log);
    let _ = std::fs::remove_file(&copy);
    result
}

fn run_pinned_setup(copy: &Path, log: &mut Log) -> Result<(), Failure> {
    let bytes = std::fs::read(copy).map_err(|e| format!("read {}: {e}", copy.display()))?;
    let got = sha256_hex(&bytes);
    if got != PAWNIO_SETUP_SHA256 {
        return Err(Failure::from(format!(
            "PawnIO setup sha256 mismatch: got {got}, want {PAWNIO_SETUP_SHA256}"
        )));
    }
    drop(bytes);
    log.line("running PawnIO setup (-install -silent)");
    let status = std::process::Command::new(copy)
        .args(["-install", "-silent"])
        .status()
        .map_err(|e| format!("run PawnIO setup: {e}"))?;
    log.line(&format!("PawnIO setup exited {status}"));
    let v = pawnio_version();
    if !pawnio_version_ok(v.as_deref()) {
        return Err(Failure::new(
            exit::NO_PAWNIO,
            format!("PawnIO still not installed after setup ({status})"),
        ));
    }
    log.line(&format!("PawnIO {} installed", v.unwrap_or_default()));
    Ok(())
}

fn remove_service(log: &mut Log) -> Result<(), Failure> {
    let mgr = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| format!("open SCM: {e}"))?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let Ok(svc) = mgr.open_service(SERVICE_NAME, access) else {
        return Ok(());
    };
    if let Ok(st) = svc.query_status()
        && st.current_state != ServiceState::Stopped
    {
        log.line("stopping existing service");
        let _ = svc.stop();
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) {
            match svc.query_status() {
                Ok(s) if s.current_state == ServiceState::Stopped => break,
                Err(_) => break,
                _ => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }
    svc.delete().map_err(|e| format!("delete service: {e}"))?;
    drop(svc);
    log.line("deleted existing service");
    // Deletion completes once the last handle closes; give the SCM a moment,
    // or the create below fails with "marked for deletion".
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(5)
        && mgr
            .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
            .is_ok()
    {
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

/// Copies this executable into the install directory. The modules are
/// embedded, so it is the whole payload.
fn copy_payload(dir: &Path, log: &mut Log) -> Result<PathBuf, Failure> {
    let exe = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let target = dir.join(HELPER_EXE);
    let same = std::fs::canonicalize(&exe).ok() == std::fs::canonicalize(&target).ok();
    if !same {
        std::fs::copy(&exe, &target).map_err(|e| format!("copy exe: {e}"))?;
    }
    log.line(&format!("copied {HELPER_EXE} to {}", dir.display()));
    Ok(target)
}

fn create_and_start(exe: &Path, log: &mut Log) -> Result<(), Failure> {
    let mgr = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(|e| format!("open SCM: {e}"))?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from("Vitals sensors"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.to_path_buf(),
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let svc = mgr
        .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
        .map_err(|e| format!("create service: {e}"))?;
    let _ = svc.set_description(
        "Read-only CPU temperature and package power for Vitals, through the PawnIO driver.",
    );
    svc.start::<&str>(&[])
        .map_err(|e| format!("start service: {e}"))?;
    log.line("service created and started");
    Ok(())
}

pub fn install(pawnio_setup: Option<&Path>) -> Result<(), Failure> {
    require_elevated()?;
    let mut log = Log::open();
    log.line(&format!(
        "vitals-sensors {} install",
        env!("CARGO_PKG_VERSION")
    ));
    let dir = install_dir();
    let r = (|| -> Result<(), Failure> {
        ensure_pawnio(pawnio_setup, &dir, &mut log)?;
        remove_service(&mut log)?;
        let exe = copy_payload(&dir, &mut log)?;
        create_and_start(&exe, &mut log)?;
        let reading = first_reading(&mut log)?;
        log.line(&reading.to_json());
        Ok(())
    })();
    if let Err(e) = &r {
        log.line(&format!("install failed: {}", e.message));
    }
    r
}

/// Waits for the refresher's first reading.
///
/// The pipe comes up before the driver has been read, serving `"starting"`;
/// logging that as the result made a working install look broken. A service
/// that answers with a real error (unsupported CPU) is still installed
/// correctly — the error is the machine's, and the UI shows it.
fn first_reading(log: &mut Log) -> Result<vitals_sensors::Reading, Failure> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let r = read_pipe(Duration::from_secs(5))?;
        let placeholder = !r.ok && r.error.as_deref() == Some("starting");
        if !placeholder || Instant::now() >= deadline {
            return Ok(r);
        }
        log.line("waiting for the first reading");
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub fn uninstall() -> Result<(), Failure> {
    require_elevated()?;
    let mut log = Log::open();
    log.line(&format!(
        "vitals-sensors {} uninstall",
        env!("CARGO_PKG_VERSION")
    ));
    remove_service(&mut log)?;
    let dir = install_dir();
    if dir.exists() {
        // The running exe may be the installed copy, which Windows cannot
        // delete while it runs; remove what can be removed and say so.
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => log.line(&format!("removed {}", dir.display())),
            Err(e) => log.line(&format!("could not fully remove {}: {e}", dir.display())),
        }
    }
    // PawnIO is a shared driver other tools (FanControl, LibreHardwareMonitor)
    // also use; removing it is not ours to decide.
    log.line("PawnIO left installed (shared with other tools)");
    Ok(())
}
