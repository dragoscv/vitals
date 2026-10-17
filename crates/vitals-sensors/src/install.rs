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
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
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

/// Program Files from the shell's known-folder API.
///
/// Never `%ProgramFiles%`: this runs elevated, with an environment inherited
/// from the unelevated user who launched it. Trusting the variable let that
/// user choose where a SYSTEM service's executable is copied — a folder they
/// can write, and therefore a file they can replace and have the SCM run as
/// SYSTEM. No fallback either: guessing `C:\Program Files` on a machine
/// where it is elsewhere is the same mistake with a constant.
fn program_files() -> Result<PathBuf, Failure> {
    // SAFETY: the GUID is a static; the returned buffer is freed below.
    let raw = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None) }
        .map_err(|e| format!("locate Program Files: {e}"))?;
    // SAFETY: on success `raw` is a NUL-terminated string from the shell.
    let text = unsafe { raw.to_string() };
    // SAFETY: allocated by the shell with CoTaskMemAlloc, freed exactly once.
    unsafe { CoTaskMemFree(Some(raw.0.cast_const().cast())) };
    text.map(PathBuf::from)
        .map_err(|e| Failure::from(format!("locate Program Files: {e}")))
}

/// `Program Files\Vitals Sensors` — admin-only, so the file the SCM starts
/// as SYSTEM cannot be replaced by the user who asked for it.
///
/// # Errors
///
/// When the shell cannot say where Program Files is.
pub fn install_dir() -> Result<PathBuf, Failure> {
    Ok(program_files()?.join("Vitals Sensors"))
}

/// The install log, inside the install directory.
///
/// It used to live in `%ProgramData%\Vitals Sensors`, created and appended
/// to by this elevated process. `ProgramData` lets ordinary users create
/// folders, so one could pre-create `Vitals Sensors` with a junction or an
/// `install.log` symlink pointing anywhere, and the elevated append would
/// write — or create — that target as administrator. Program Files offers
/// no such opening: only administrators can create the folder at all.
fn log_path(dir: &Path) -> PathBuf {
    dir.join("install.log")
}

/// The log is truncated when it grows past this. Each install appends a
/// dozen lines, so a megabyte is years of history; without a cap, a
/// misbehaving retry loop would fill the system drive as administrator.
const LOG_CAP_BYTES: u64 = 1024 * 1024;

struct Log(Option<std::fs::File>);

impl Log {
    /// Opens the log in `dir`, or a console-only log when there is no
    /// install directory to put it in.
    fn open(dir: Option<&Path>) -> Self {
        let Some(dir) = dir else {
            return Self(None);
        };
        let _ = std::fs::create_dir_all(dir);
        let path = log_path(dir);
        let oversized = std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_CAP_BYTES);
        let mut options = std::fs::OpenOptions::new();
        options.create(true);
        if oversized {
            options.write(true).truncate(true);
        } else {
            options.append(true);
        }
        Self(options.open(path).ok())
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
    let dir = install_dir()?;
    let mut log = Log::open(Some(&dir));
    log.line(&format!(
        "vitals-sensors {} install",
        env!("CARGO_PKG_VERSION")
    ));
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
    let dir = install_dir()?;
    // Console only: the log lives in the directory this removes, and
    // recreating it just to write "removed" would leave the folder behind.
    let mut log = Log::open(None);
    log.line(&format!(
        "vitals-sensors {} uninstall",
        env!("CARGO_PKG_VERSION")
    ));
    remove_service(&mut log)?;
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_install_directory_is_the_known_program_files_folder_and_not_a_user_folder() {
        let dir = install_dir().map_err(|f| f.message).unwrap();
        assert!(dir.ends_with("Vitals Sensors"), "{}", dir.display());
        let parent = dir.parent().unwrap().to_string_lossy().to_ascii_lowercase();
        assert!(parent.ends_with("program files"), "{parent}");
        let profile = std::env::var("USERPROFILE").unwrap_or_default();
        assert!(
            profile.is_empty() || !dir.starts_with(&profile),
            "a user-writable install directory is a SYSTEM service anyone can replace"
        );
    }

    #[test]
    fn the_log_is_written_inside_the_admin_only_install_directory() {
        // Not ProgramData, where a standard user can pre-plant a junction or
        // a symlink for the elevated append to follow.
        let dir = install_dir().map_err(|f| f.message).unwrap();
        assert_eq!(log_path(&dir).parent(), Some(dir.as_path()));
        assert!(
            !log_path(&dir)
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("programdata")
        );
    }

    #[test]
    fn an_oversized_log_is_truncated_rather_than_appended_to() {
        let dir = std::env::temp_dir().join(format!("vitals-sensors-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = log_path(&dir);
        std::fs::write(
            &path,
            vec![b'x'; usize::try_from(LOG_CAP_BYTES).unwrap() + 1],
        )
        .unwrap();
        Log::open(Some(&dir)).line("fresh");
        let len = std::fs::metadata(&path).unwrap().len();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(len < 128, "the log kept growing past its cap: {len} bytes");
    }

    #[test]
    fn a_log_under_the_cap_is_appended_to() {
        let dir = std::env::temp_dir().join(format!("vitals-sensors-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = log_path(&dir);
        std::fs::write(&path, b"earlier\n").unwrap();
        Log::open(Some(&dir)).line("later");
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            text.starts_with("earlier\n") && text.contains("later"),
            "{text}"
        );
    }
}
