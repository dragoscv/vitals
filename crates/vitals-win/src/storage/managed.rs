//! Space that only Windows may free, freed by Windows' own tools.
//!
//! Some reclaimable locations are not files Vitals should touch at all: the
//! component store is hard-linked into `System32`, `hiberfil.sys` is owned by
//! the kernel, `Windows.old` is the rollback image and the update caches are
//! in use by services. Each has a supported way out, and this module runs
//! exactly that and nothing else:
//!
//! - Disk Cleanup (`cleanmgr /sagerun`) with **one** handler ticked;
//! - DISM `/StartComponentCleanup` for the component store;
//! - `powercfg /hibernate off` for the hibernation file.
//!
//! **No file is removed by Vitals here.** There is no `std::fs::remove_*`
//! in this module, by design.
//!
//! # One UAC prompt per action
//!
//! Same shape as `actions::elevated`: the running app re-launches itself
//! under `runas` with [`STORAGE_CLEANUP_ARG`] and one tool key; the elevated
//! child re-checks the key against a closed list, refuses a risky tool that
//! was not confirmed, runs the tool and exits with its code. The UI never
//! runs elevated.
//!
//! # Freed is measured, never estimated
//!
//! Before the tool runs, the location and the drive's free space are read;
//! after it exits, both are read again. The report carries the four raw
//! figures. A tool that frees nothing reports nothing freed, and a location
//! that could not be read says so rather than claiming zero.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vitals_core::error::{Error, Result};

use super::cleanup::CleanupKind;

/// The argument the elevated instance is started with.
pub const STORAGE_CLEANUP_ARG: &str = "--elevated-storage-cleanup";

/// The Disk Cleanup profile number Vitals writes and removes again. Any
/// value 0–9999 works; a fixed one means a crashed run's leftovers are
/// overwritten by the next, never accumulated.
const SAGESET: u32 = 7331;

/// A Disk Cleanup handler, by its `VolumeCaches` subkey.
///
/// A closed list: the elevated child accepts nothing else, so a crafted
/// argument cannot tick "Downloads" (whose handler deletes the user's
/// Downloads folder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    TemporaryFiles,
    UpdateCleanup,
    DeliveryOptimization,
    Minidumps,
    MemoryDump,
    ThumbnailCache,
    RecycleBin,
    PreviousInstallations,
}

impl Handler {
    const ALL: [Self; 8] = [
        Self::TemporaryFiles,
        Self::UpdateCleanup,
        Self::DeliveryOptimization,
        Self::Minidumps,
        Self::MemoryDump,
        Self::ThumbnailCache,
        Self::RecycleBin,
        Self::PreviousInstallations,
    ];

    /// The subkey under `Explorer\VolumeCaches`.
    #[must_use]
    pub const fn subkey(self) -> &'static str {
        match self {
            Self::TemporaryFiles => "Temporary Files",
            Self::UpdateCleanup => "Update Cleanup",
            Self::DeliveryOptimization => "Delivery Optimization Files",
            Self::Minidumps => "System error minidump files",
            Self::MemoryDump => "System error memory dump files",
            Self::ThumbnailCache => "Thumbnail Cache",
            Self::RecycleBin => "Recycle Bin",
            Self::PreviousInstallations => "Previous Installations",
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::TemporaryFiles => "temporaryFiles",
            Self::UpdateCleanup => "updateCleanup",
            Self::DeliveryOptimization => "deliveryOptimization",
            Self::Minidumps => "minidumps",
            Self::MemoryDump => "memoryDump",
            Self::ThumbnailCache => "thumbnailCache",
            Self::RecycleBin => "recycleBin",
            Self::PreviousInstallations => "previousInstallations",
        }
    }
}

/// The Windows tool that frees one kind of space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedTool {
    DiskCleanup(Handler),
    ComponentCleanup,
    HibernateOff,
}

impl ManagedTool {
    /// `diskCleanup` | `componentCleanup` | `hibernateOff`, for the UI.
    #[must_use]
    pub const fn family(self) -> &'static str {
        match self {
            Self::DiskCleanup(_) => "diskCleanup",
            Self::ComponentCleanup => "componentCleanup",
            Self::HibernateOff => "hibernateOff",
        }
    }

    /// Whether running it cannot be taken back: emptying the bin makes every
    /// deletion in it permanent, removing `Windows.old` removes the way back
    /// to the previous build, and turning hibernation off also turns off
    /// fast startup. These need their own confirmation, here and in the
    /// elevated child.
    #[must_use]
    pub const fn is_risky(self) -> bool {
        matches!(
            self,
            Self::DiskCleanup(Handler::RecycleBin | Handler::PreviousInstallations)
                | Self::HibernateOff
        )
    }

    /// The argument word the elevated child reads.
    #[must_use]
    pub fn as_arg(self) -> String {
        match self {
            Self::DiskCleanup(handler) => format!("cleanmgr.{}", handler.key()),
            Self::ComponentCleanup => "dism.components".to_owned(),
            Self::HibernateOff => "powercfg.hibernateOff".to_owned(),
        }
    }

    fn from_arg(arg: &str) -> Option<Self> {
        match arg {
            "dism.components" => Some(Self::ComponentCleanup),
            "powercfg.hibernateOff" => Some(Self::HibernateOff),
            _ => {
                let key = arg.strip_prefix("cleanmgr.")?;
                Handler::ALL
                    .into_iter()
                    .find(|h| h.key() == key)
                    .map(Self::DiskCleanup)
            }
        }
    }
}

/// Which tool frees a candidate, or `None` when the space is not
/// Windows-managed (a browser cache, a package cache, your own temp folder
/// — those go through the review basket).
#[must_use]
pub fn tool_for(kind: CleanupKind, path: &Path) -> Option<ManagedTool> {
    use CleanupKind as K;
    let lower = path.to_string_lossy().to_ascii_lowercase();
    Some(match kind {
        K::SystemTemp => ManagedTool::DiskCleanup(Handler::TemporaryFiles),
        K::WindowsUpdateCache => ManagedTool::DiskCleanup(Handler::UpdateCleanup),
        K::DeliveryOptimisation => ManagedTool::DiskCleanup(Handler::DeliveryOptimization),
        K::ThumbnailCache => ManagedTool::DiskCleanup(Handler::ThumbnailCache),
        K::RecycleBin => ManagedTool::DiskCleanup(Handler::RecycleBin),
        K::PreviousWindows => ManagedTool::DiskCleanup(Handler::PreviousInstallations),
        K::Hibernation => ManagedTool::HibernateOff,
        K::ComponentStore => ManagedTool::ComponentCleanup,
        // Only the two dump locations Disk Cleanup has a handler for. Live
        // kernel reports and per-app crash dumps have none, and guessing a
        // handler would run one that does not touch them.
        K::CrashDump if lower.ends_with("\\minidump") => {
            ManagedTool::DiskCleanup(Handler::Minidumps)
        }
        K::CrashDump if lower.ends_with("\\memory.dmp") => {
            ManagedTool::DiskCleanup(Handler::MemoryDump)
        }
        K::CrashDump | K::UserTemp | K::BrowserCache | K::PackageManagerCache => return None,
    })
}

/// Whether the elevated child may run a risky tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    Confirmed,
    Unconfirmed,
}

/// Exit codes of the elevated child that are Vitals' own, not the tool's.
///
/// In a range no Windows tool returns (they return 0, 3010 or an HRESULT
/// such as `0x800F081F`), so the tool's own code can pass straight through.
mod exit {
    pub const OK: u32 = 0;
    /// `ERROR_SUCCESS_REBOOT_REQUIRED`, what DISM returns when a restart
    /// finishes the job.
    pub const OK_RESTART: u32 = 3010;
    pub const BAD_ARGS: u32 = 0xE5C0_0001;
    pub const REFUSED: u32 = 0xE5C0_0002;
    pub const NOT_STARTED: u32 = 0xE5C0_0003;
    pub const REGISTRY: u32 = 0xE5C0_0004;
}

/// The arguments for one elevated cleanup, in the order [`parse_args`] reads.
#[must_use]
pub fn format_args(tool: ManagedTool, consent: Consent) -> String {
    format!(
        "{STORAGE_CLEANUP_ARG} {} {}",
        tool.as_arg(),
        match consent {
            Consent::Confirmed => "confirmed",
            Consent::Unconfirmed => "unconfirmed",
        }
    )
}

/// Reads the arguments after [`STORAGE_CLEANUP_ARG`]. `None` for anything
/// malformed: the caller is our own parent, so a bad argument is a bug.
#[must_use]
pub fn parse_args<I, S>(rest: I) -> Option<(ManagedTool, Consent)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut rest = rest.into_iter();
    let tool = ManagedTool::from_arg(rest.next()?.as_ref())?;
    let consent = match rest.next()?.as_ref() {
        "confirmed" => Consent::Confirmed,
        "unconfirmed" => Consent::Unconfirmed,
        _ => return None,
    };
    if rest.next().is_some() {
        return None;
    }
    Some((tool, consent))
}

/// The elevated child's whole job: one tool, then an exit code.
#[must_use]
pub fn perform<I, S>(rest: I) -> u32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some((tool, consent)) = parse_args(rest) else {
        return exit::BAD_ARGS;
    };
    // Checked here as well as in the app: the child must not be a way to
    // run what the confirmation dialog was never shown for.
    if tool.is_risky() && consent != Consent::Confirmed {
        return exit::REFUSED;
    }
    match tool {
        ManagedTool::DiskCleanup(handler) => disk_cleanup(handler),
        ManagedTool::ComponentCleanup => run_tool(
            "dism.exe",
            &[
                "/Online",
                "/Cleanup-Image",
                "/StartComponentCleanup",
                "/NoRestart",
            ],
        ),
        ManagedTool::HibernateOff => run_tool("powercfg.exe", &["/hibernate", "off"]),
    }
}

/// Ticks exactly one handler in profile [`SAGESET`], runs it, and removes
/// the profile again whatever happened.
fn disk_cleanup(handler: Handler) -> u32 {
    // A previous run that died between write and clear would leave its
    // handler ticked in this profile; clear every one we could have written.
    for other in Handler::ALL {
        let _ = registry::clear_flag(other.subkey(), SAGESET);
    }
    if registry::set_flag(handler.subkey(), SAGESET).is_err() {
        return exit::REGISTRY;
    }
    let code = run_tool("cleanmgr.exe", &[&format!("/sagerun:{SAGESET}")]);
    let _ = registry::clear_flag(handler.subkey(), SAGESET);
    code
}

/// Runs a tool from System32 by absolute path and returns its exit code.
fn run_tool(name: &str, args: &[&str]) -> u32 {
    use std::os::windows::process::CommandExt as _;
    /// No console flashing up beside the app; Disk Cleanup still shows its
    /// own progress window, which is Windows telling the user what it does.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Some(program) = system32(name) else {
        return exit::NOT_STARTED;
    };
    match std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
    {
        Ok(status) => status.code().map_or(exit::NOT_STARTED, i32::cast_unsigned),
        Err(_) => exit::NOT_STARTED,
    }
}

/// `%SystemRoot%\System32\<name>`, never a bare name: an elevated process
/// resolving `dism.exe` through `PATH` would run whatever came first.
fn system32(name: &str) -> Option<PathBuf> {
    let root = std::env::var_os("SystemRoot")?;
    let path = PathBuf::from(root).join("System32").join(name);
    path.is_file().then_some(path)
}

mod registry {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_DWORD, RegCloseKey,
        RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
    };

    const ROOT: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn with_key(subkey: &str, f: impl FnOnce(HKEY) -> u32) -> Result<(), u32> {
        let path = wide(&format!("{ROOT}\\{subkey}"));
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` is NUL-terminated; `key` is closed below. Opened,
        // not created: a handler Windows does not have is not one to tick.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                path.as_ptr(),
                0,
                KEY_SET_VALUE | KEY_WOW64_64KEY,
                &raw mut key,
            )
        };
        if status != 0 {
            return Err(status);
        }
        let status = f(key);
        // SAFETY: `key` came from RegOpenKeyExW and is closed exactly once.
        unsafe { RegCloseKey(key) };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    fn value_name(profile: u32) -> Vec<u16> {
        wide(&format!("StateFlags{profile:04}"))
    }

    /// `2` is "selected" in a Disk Cleanup profile.
    pub fn set_flag(subkey: &str, profile: u32) -> Result<(), u32> {
        let name = value_name(profile);
        let data = 2_u32.to_le_bytes();
        with_key(subkey, |key| {
            // SAFETY: `key` is open for writing; `name` is NUL-terminated and
            // `data` is four live bytes.
            unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_DWORD, data.as_ptr(), 4) }
        })
    }

    pub fn clear_flag(subkey: &str, profile: u32) -> Result<(), u32> {
        let name = value_name(profile);
        // SAFETY: `key` is open for writing; `name` is NUL-terminated.
        with_key(subkey, |key| unsafe { RegDeleteValueW(key, name.as_ptr()) })
    }
}

/// How the tool finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finished {
    Done,
    /// Done, and Windows finishes the job on the next restart.
    NeedsRestart,
    /// The tool itself reported a failure, with its exit code.
    ToolFailed(u32),
}

fn finished_from(code: u32) -> Result<Finished> {
    match code {
        exit::OK => Ok(Finished::Done),
        exit::OK_RESTART => Ok(Finished::NeedsRestart),
        exit::REFUSED => Err(Error::Refused(
            "a risky cleanup needs its own confirmation".to_owned(),
        )),
        exit::BAD_ARGS => Err(Error::Os {
            context: "the elevated cleanup did not understand its arguments".to_owned(),
            code: code.cast_signed(),
        }),
        exit::NOT_STARTED => Err(Error::Os {
            context: "the Windows tool could not be started".to_owned(),
            code: code.cast_signed(),
        }),
        exit::REGISTRY => Err(Error::Os {
            context: "Disk Cleanup could not be told which item to clean".to_owned(),
            code: code.cast_signed(),
        }),
        other => Ok(Finished::ToolFailed(other)),
    }
}

/// Where a run has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Reading the location and the drive before anything runs.
    Measuring,
    /// The UAC prompt is on screen.
    Approval,
    /// The tool is running.
    Running,
    /// The tool exited; reading the same figures again.
    Remeasuring,
}

impl Stage {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Measuring => "measuring",
            Self::Approval => "approval",
            Self::Running => "running",
            Self::Remeasuring => "remeasuring",
        }
    }
}

/// One progress report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub elapsed: Duration,
    /// Free space gained on the drive so far; `None` when it cannot be read.
    /// Negative when something else wrote to the drive meanwhile.
    pub drive_freed: Option<i64>,
}

/// What one run measured. The raw figures, not a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub finished: Finished,
    /// On-disk bytes at the location; `Some(0)` when it is gone, `None` when
    /// it exists but could not be read.
    pub location_before: Option<u64>,
    pub location_after: Option<u64>,
    /// Free bytes on the location's drive.
    pub drive_before: Option<u64>,
    pub drive_after: Option<u64>,
    pub elapsed: Duration,
}

impl Report {
    /// How much smaller the location got. Negative if it grew.
    #[must_use]
    pub fn location_freed(&self) -> Option<i64> {
        delta(self.location_before, self.location_after)
    }

    /// How much free space the drive gained.
    #[must_use]
    pub fn drive_freed(&self) -> Option<i64> {
        delta(self.drive_after, self.drive_before)
    }
}

fn delta(larger: Option<u64>, smaller: Option<u64>) -> Option<i64> {
    let larger = i64::try_from(larger?).ok()?;
    let smaller = i64::try_from(smaller?).ok()?;
    Some(larger - smaller)
}

/// A running elevated instance, as [`run_with`] needs it.
pub trait Running {
    /// `Ok(None)` while still running after `timeout`, `Ok(Some(code))` once
    /// it has exited.
    ///
    /// # Errors
    ///
    /// When the wait itself fails.
    fn wait(&mut self, timeout: Duration) -> Result<Option<u32>>;
}

impl Running for crate::actions::taskmgr::ElevatedProcess {
    fn wait(&mut self, timeout: Duration) -> Result<Option<u32>> {
        Self::wait(self, Some(timeout))
    }
}

/// How often progress is reported while the tool runs.
const POLL: Duration = Duration::from_millis(500);

/// Measures, asks for approval, runs the tool, measures again.
///
/// Blocks for the length of the UAC prompt plus the tool's run — minutes for
/// DISM — so callers run it off the UI thread.
///
/// # Errors
///
/// - [`Error::Refused`] when a risky tool was not confirmed, or the UAC
///   prompt was dismissed. Nothing ran.
/// - [`Error::Os`] when the elevated instance or the tool could not start.
pub fn run(
    tool: ManagedTool,
    location: &Path,
    consent: Consent,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<Report> {
    let exe = std::env::current_exe()?;
    run_with(tool, location, consent, on_progress, |args| {
        crate::actions::taskmgr::spawn_program_elevated(&exe, args, "nothing was cleaned")
    })
}

/// [`run`] with the elevation step injected, so the refusal and measurement
/// paths are testable without a UAC prompt.
///
/// # Errors
///
/// As [`run`].
pub fn run_with<R, L>(
    tool: ManagedTool,
    location: &Path,
    consent: Consent,
    on_progress: &mut dyn FnMut(Progress),
    launch: L,
) -> Result<Report>
where
    R: Running,
    L: FnOnce(&str) -> Result<R>,
{
    if tool.is_risky() && consent != Consent::Confirmed {
        return Err(Error::Refused(
            "this cleanup cannot be undone and was not confirmed, so nothing ran".to_owned(),
        ));
    }
    let started = Instant::now();
    let drive = drive_root(location);
    // WinSxS is mostly hard links into System32: a walk would report the
    // links as the store's own and take a minute doing it. The drive's free
    // space is the honest figure for that one.
    let measure = |path: &Path| {
        if tool == ManagedTool::ComponentCleanup {
            None
        } else {
            measure_location(path)
        }
    };
    let report = |stage, drive_freed| Progress {
        stage,
        elapsed: started.elapsed(),
        drive_freed,
    };

    on_progress(report(Stage::Measuring, None));
    let location_before = measure(location);
    let drive_before = drive.as_deref().and_then(free_bytes);

    on_progress(report(Stage::Approval, None));
    let mut child = launch(&format_args(tool, consent))?;

    let code = loop {
        if let Some(code) = child.wait(POLL)? {
            break code;
        }
        let now = drive.as_deref().and_then(free_bytes);
        on_progress(report(Stage::Running, delta(now, drive_before)));
    };
    let finished = finished_from(code)?;

    on_progress(report(Stage::Remeasuring, None));
    let location_after = measure(location);
    let drive_after = drive.as_deref().and_then(free_bytes);

    Ok(Report {
        finished,
        location_before,
        location_after,
        drive_before,
        drive_after,
        elapsed: started.elapsed(),
    })
}

/// On-disk size of a file or folder: `Some(0)` when it does not exist (the
/// tool removed it, which is the point), `None` when it exists but could not
/// be read.
#[must_use]
pub fn measure_location(path: &Path) -> Option<u64> {
    match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(0),
        Err(_) => None,
        Ok(meta) if meta.is_file() => Some(meta.len()),
        Ok(_) => super::size_of(path, None).map(|b| b.0),
    }
}

/// `C:\` for any path on C:.
fn drive_root(path: &Path) -> Option<PathBuf> {
    match path.components().next()? {
        std::path::Component::Prefix(prefix) => {
            let mut root = PathBuf::from(prefix.as_os_str());
            root.push("\\");
            Some(root)
        }
        _ => None,
    }
}

/// Total free bytes on the volume holding `root` — every free byte, not the
/// caller's quota, because that is what a cleanup changes.
#[must_use]
pub fn free_bytes(root: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = root
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let (mut caller, mut total, mut free) = (0_u64, 0_u64, 0_u64);
    // SAFETY: `wide` is NUL-terminated; the out-pointers are live u64s.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &raw mut caller,
            &raw mut total,
            &raw mut free,
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        /// Exit code, returned after `polls` timeouts.
        code: u32,
        polls: u32,
        on_exit: Option<Box<dyn FnOnce()>>,
    }

    impl Running for Fake {
        fn wait(&mut self, _timeout: Duration) -> Result<Option<u32>> {
            if self.polls > 0 {
                self.polls -= 1;
                return Ok(None);
            }
            if let Some(f) = self.on_exit.take() {
                f();
            }
            Ok(Some(self.code))
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vitals-managed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    const EVERY_TOOL: [ManagedTool; 10] = [
        ManagedTool::DiskCleanup(Handler::TemporaryFiles),
        ManagedTool::DiskCleanup(Handler::UpdateCleanup),
        ManagedTool::DiskCleanup(Handler::DeliveryOptimization),
        ManagedTool::DiskCleanup(Handler::Minidumps),
        ManagedTool::DiskCleanup(Handler::MemoryDump),
        ManagedTool::DiskCleanup(Handler::ThumbnailCache),
        ManagedTool::DiskCleanup(Handler::RecycleBin),
        ManagedTool::DiskCleanup(Handler::PreviousInstallations),
        ManagedTool::ComponentCleanup,
        ManagedTool::HibernateOff,
    ];

    #[test]
    fn the_arguments_round_trip_so_the_child_runs_exactly_the_tool_the_parent_named() {
        for tool in EVERY_TOOL {
            for consent in [Consent::Confirmed, Consent::Unconfirmed] {
                let args = format_args(tool, consent);
                let mut words = args.split(' ');
                assert_eq!(words.next(), Some(STORAGE_CLEANUP_ARG));
                assert_eq!(parse_args(words), Some((tool, consent)), "{args}");
            }
        }
    }

    #[test]
    fn the_child_refuses_a_handler_outside_the_closed_list() {
        // "DownloadsFolder" is a real Disk Cleanup handler that deletes the
        // user's Downloads. It must not be reachable by a crafted argument.
        for bad in [
            vec![],
            vec!["cleanmgr.downloadsFolder", "confirmed"],
            vec!["cleanmgr.DownloadsFolder", "confirmed"],
            vec!["cleanmgr.", "confirmed"],
            vec!["dism.components"],
            vec!["dism.components", "yes"],
            vec!["dism.components", "confirmed", "extra"],
            vec!["del", "confirmed"],
        ] {
            assert_eq!(parse_args(bad.clone()), None, "{bad:?}");
            assert_eq!(perform(bad.clone()), exit::BAD_ARGS, "{bad:?}");
        }
    }

    #[test]
    fn the_child_refuses_a_risky_tool_that_was_not_confirmed() {
        for tool in EVERY_TOOL.into_iter().filter(|t| t.is_risky()) {
            let args = format_args(tool, Consent::Unconfirmed);
            assert_eq!(perform(args.split(' ').skip(1)), exit::REFUSED, "{args}");
        }
    }

    #[test]
    fn hibernation_windows_old_and_the_bin_are_the_risky_ones() {
        let risky: Vec<_> = EVERY_TOOL.into_iter().filter(|t| t.is_risky()).collect();
        assert_eq!(
            risky,
            [
                ManagedTool::DiskCleanup(Handler::RecycleBin),
                ManagedTool::DiskCleanup(Handler::PreviousInstallations),
                ManagedTool::HibernateOff,
            ]
        );
    }

    #[test]
    fn every_windows_managed_kind_maps_to_a_tool_and_app_caches_do_not() {
        use CleanupKind as K;
        let p = Path::new(r"C:\x");
        for kind in [
            K::SystemTemp,
            K::WindowsUpdateCache,
            K::DeliveryOptimisation,
            K::ThumbnailCache,
            K::RecycleBin,
            K::PreviousWindows,
            K::Hibernation,
            K::ComponentStore,
        ] {
            assert!(tool_for(kind, p).is_some(), "{kind:?}");
        }
        for kind in [K::UserTemp, K::BrowserCache, K::PackageManagerCache] {
            assert_eq!(tool_for(kind, p), None, "{kind:?}");
        }
        assert_eq!(
            tool_for(K::CrashDump, Path::new(r"C:\Windows\Minidump")),
            Some(ManagedTool::DiskCleanup(Handler::Minidumps))
        );
        assert_eq!(
            tool_for(K::CrashDump, Path::new(r"C:\MEMORY.DMP")),
            Some(ManagedTool::DiskCleanup(Handler::MemoryDump))
        );
        assert_eq!(
            tool_for(K::CrashDump, Path::new(r"C:\Windows\LiveKernelReports")),
            None,
            "no Disk Cleanup handler touches live kernel reports"
        );
    }

    #[test]
    fn a_risky_tool_without_confirmation_never_reaches_the_uac_prompt() {
        let dir = scratch("unconfirmed");
        let mut stages = Vec::new();
        let result = run_with(
            ManagedTool::HibernateOff,
            &dir,
            Consent::Unconfirmed,
            &mut |p| stages.push(p.stage),
            |_| -> Result<Fake> { panic!("an unconfirmed risky tool must not be launched") },
        );
        assert!(matches!(result, Err(Error::Refused(_))));
        assert!(stages.is_empty(), "nothing was measured either: {stages:?}");
    }

    #[test]
    fn a_declined_uac_prompt_is_a_refusal_and_nothing_is_run_or_remeasured() {
        let dir = scratch("declined");
        let mut stages = Vec::new();
        let result = run_with(
            ManagedTool::DiskCleanup(Handler::ThumbnailCache),
            &dir,
            Consent::Unconfirmed,
            &mut |p| stages.push(p.stage),
            // What `spawn_program_elevated` returns for ERROR_CANCELLED.
            |_| -> Result<Fake> {
                Err(Error::Refused(
                    "administrator approval was declined, so nothing was cleaned".into(),
                ))
            },
        );
        assert!(matches!(result, Err(Error::Refused(m)) if m.contains("declined")));
        assert_eq!(stages, [Stage::Measuring, Stage::Approval]);
    }

    #[test]
    fn freed_space_is_what_was_measured_before_and_after_not_an_estimate() {
        let dir = scratch("measured");
        let doomed = dir.join("doomed.bin");
        let kept = dir.join("kept.bin");
        std::fs::write(&doomed, vec![7_u8; 256 * 1024]).expect("write");
        std::fs::write(&kept, vec![7_u8; 64 * 1024]).expect("write");
        let before = measure_location(&dir).expect("readable");

        let mut stages = Vec::new();
        let mut launched = String::new();
        let report = run_with(
            ManagedTool::DiskCleanup(Handler::TemporaryFiles),
            &dir,
            Consent::Unconfirmed,
            &mut |p| stages.push(p.stage),
            |args| {
                launched = args.to_owned();
                // Stands in for the Windows tool: it removes one file of two.
                let doomed = doomed.clone();
                Ok(Fake {
                    code: 0,
                    polls: 2,
                    on_exit: Some(Box::new(move || {
                        std::fs::remove_file(doomed).expect("remove");
                    })),
                })
            },
        )
        .expect("ran");

        assert_eq!(
            launched,
            format!("{STORAGE_CLEANUP_ARG} cleanmgr.temporaryFiles unconfirmed")
        );
        assert_eq!(report.finished, Finished::Done);
        assert_eq!(report.location_before, Some(before));
        let after = measure_location(&dir).expect("readable");
        assert_eq!(report.location_after, Some(after));
        assert!(after < before);
        assert_eq!(
            report.location_freed(),
            Some(i64::try_from(before - after).expect("fits"))
        );
        assert!(report.drive_before.is_some() && report.drive_after.is_some());
        assert_eq!(
            stages,
            [
                Stage::Measuring,
                Stage::Approval,
                Stage::Running,
                Stage::Running,
                Stage::Remeasuring
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_tool_that_frees_nothing_reports_nothing_freed() {
        let dir = scratch("nothing");
        std::fs::write(dir.join("stays.bin"), vec![1_u8; 32 * 1024]).expect("write");
        let report = run_with(
            ManagedTool::DiskCleanup(Handler::DeliveryOptimization),
            &dir,
            Consent::Unconfirmed,
            &mut |_| {},
            |_| {
                Ok(Fake {
                    code: 0,
                    polls: 0,
                    on_exit: None,
                })
            },
        )
        .expect("ran");
        assert_eq!(report.location_freed(), Some(0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_location_the_tool_removed_measures_zero_and_an_unreadable_one_measures_nothing() {
        assert_eq!(
            measure_location(Path::new(r"C:\vitals-does-not-exist-9d1c")),
            Some(0)
        );
    }

    #[test]
    fn exit_codes_map_to_what_the_ui_branches_on() {
        assert_eq!(finished_from(0).ok(), Some(Finished::Done));
        assert_eq!(finished_from(3010).ok(), Some(Finished::NeedsRestart));
        assert_eq!(
            finished_from(0x800F_081F).ok(),
            Some(Finished::ToolFailed(0x800F_081F)),
            "a DISM HRESULT is the tool's failure, reported with its code"
        );
        assert!(matches!(
            finished_from(exit::REFUSED),
            Err(Error::Refused(_))
        ));
        for own in [exit::BAD_ARGS, exit::NOT_STARTED, exit::REGISTRY] {
            assert!(
                matches!(finished_from(own), Err(Error::Os { .. })),
                "{own:#x}"
            );
        }
    }

    #[test]
    fn the_drive_is_taken_from_the_location() {
        assert_eq!(
            drive_root(Path::new(r"C:\Windows\Temp")),
            Some(PathBuf::from(r"C:\"))
        );
        assert!(free_bytes(Path::new(r"C:\")).is_some_and(|b| b > 0));
    }

    #[test]
    fn tools_are_resolved_from_system32_never_from_path() {
        let dism = system32("dism.exe").expect("dism ships with Windows");
        assert!(
            dism.to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(r"\system32\dism.exe")
        );
        assert_eq!(system32("vitals-not-a-tool.exe"), None);
    }
}
