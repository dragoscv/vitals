//! Sends files and folders to the Recycle Bin, and says why when it cannot.
//!
//! # Through the shell, never a delete
//!
//! `IFileOperation` with `FOFX_RECYCLEONDELETE` is what Explorer itself does
//! when you press Delete, so the item lands in the bin with its original
//! location and can be restored from there. `std::fs::remove_*` is not used
//! anywhere in this module: an item that cannot be recycled stays where it is.
//!
//! # The one case the shell would delete for good
//!
//! When an item is too large for the bin, or its drive keeps no bin, the
//! shell falls back to a permanent delete, and with confirmations suppressed
//! it does so without asking. The progress sink sees every item before it
//! goes: `PreDeleteItem` without `TSF_DELETE_RECYCLE_IF_POSSIBLE` means the
//! item would be destroyed rather than recycled, and the sink refuses it,
//! which cancels the operation. Each item runs in its own operation, so that
//! refusal (or any other failure) costs only the item it concerns.
//!
//! # Why something could not be moved
//!
//! A sharing violation means a program has the file open. Restart Manager
//! (`RmGetList`) is the documented way to ask Windows who, and it is what
//! the installer's "close these programs" list uses. For a folder the files
//! inside are registered, up to [`HOLDER_FILE_CAP`], because a folder cannot
//! be moved while any file in it is open.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_APP_TYPE, RM_PROCESS_INFO, RmConsole, RmCritical, RmEndSession,
    RmExplorer, RmGetList, RmMainWindow, RmOtherWindow, RmRegisterResources, RmService,
    RmStartSession,
};
use windows::Win32::UI::Shell::{
    FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOF_SILENT, FOFX_RECYCLEONDELETE,
    FileOperation, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl,
    IShellItem, SHCreateItemFromParsingName, TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows::core::{HRESULT, PCWSTR, PWSTR, Ref};

use super::protect::{Protection, Rules, Vetted, vet};

/// At most this many files inside a locked folder are registered with
/// Restart Manager. Registration costs a lookup per file; a folder of a
/// million files would take minutes to explain, and the first thousand
/// almost always name the program.
pub const HOLDER_FILE_CAP: usize = 1_000;

/// A program that has an item open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub pid: u32,
    /// The name Restart Manager gives, usually the window title's app name.
    pub name: String,
    /// The service's short name when the holder is a service.
    pub service: Option<String>,
    pub kind: HolderKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolderKind {
    Window,
    Service,
    Explorer,
    Console,
    /// Windows cannot close it without a restart.
    Critical,
    Other,
}

impl HolderKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Service => "service",
            Self::Explorer => "explorer",
            Self::Console => "console",
            Self::Critical => "critical",
            Self::Other => "other",
        }
    }
}

/// What happened to one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// In the Recycle Bin, and verified gone from its old place.
    Recycled,
    /// Not attempted: the path is protected.
    Refused(Protection),
    /// The Recycle Bin cannot take it, so the shell would have deleted it for
    /// good. Not done.
    WouldBePermanent,
    /// Nothing was there.
    Missing,
    /// A program has it open. `None` when Restart Manager could not say who;
    /// an empty list when it found nobody (the handle may be the kernel's, or
    /// already closed).
    Locked(Option<Vec<Holder>>),
    AccessDenied,
    Failed(i32),
}

impl Outcome {
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            Self::Recycled => "recycled",
            Self::Refused(_) => "refused",
            Self::WouldBePermanent => "wouldBePermanent",
            Self::Missing => "missing",
            Self::Locked(_) => "locked",
            Self::AccessDenied => "accessDenied",
            Self::Failed(_) => "failed",
        }
    }
}

/// Recycles each path in its own shell operation, in order.
///
/// Runs on a thread of its own: `IFileOperation` needs a single-threaded
/// apartment, and the calling thread may already be in the multi-threaded
/// one (Tauri's blocking pool is), where `CoInitializeEx` for an STA fails.
///
/// Every path is checked against `rules` here, whatever the caller already
/// checked.
#[must_use]
pub fn recycle(paths: &[String], rules: &Rules) -> Vec<Outcome> {
    let owned: Vec<String> = paths.to_vec();
    let rules = rules.clone();
    let count = owned.len();
    std::thread::Builder::new()
        .name("vitals-recycle".into())
        .spawn(move || recycle_on_sta(&owned, &rules))
        .ok()
        .and_then(|thread| thread.join().ok())
        // A thread that could not start or panicked did nothing we can see.
        .unwrap_or_else(|| vec![Outcome::Failed(E_FAIL); count])
}

/// `E_FAIL`.
const E_FAIL: i32 = 0x8000_4005_u32.cast_signed();
/// `E_ABORT`: what the sink answers to refuse a permanent delete.
const E_ABORT: HRESULT = HRESULT(0x8000_4004_u32.cast_signed());

fn recycle_on_sta(paths: &[String], rules: &Rules) -> Vec<Outcome> {
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            // SAFETY: paired with the successful CoInitializeEx below.
            unsafe { CoUninitialize() };
        }
    }
    // SAFETY: this thread is new and has no apartment yet.
    let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    if init.is_err() {
        return vec![Outcome::Failed(init.0); paths.len()];
    }
    let _com = Com;

    paths
        .iter()
        .map(|raw| match vet(raw, rules) {
            Vetted::Refused(why) => Outcome::Refused(why),
            Vetted::Missing => Outcome::Missing,
            Vetted::Allowed(path) => recycle_one(&path),
        })
        .collect()
}

/// What the sink saw during one operation.
#[derive(Debug, Default)]
struct Seen {
    /// An item was about to be destroyed rather than recycled.
    would_nuke: bool,
    /// The first failure the engine reported for an item.
    failure: Option<HRESULT>,
}

/// The sink lives in its own module so the lints below reach the code
/// `#[implement]` generates (inline(always) helpers, a reference cast to a
/// raw pointer). That code is the windows crate's, not ours to restyle.
#[allow(clippy::inline_always, clippy::ref_as_ptr)]
mod sink {
    use std::cell::RefCell;
    use std::rc::Rc;

    use windows::Win32::UI::Shell::IFileOperationProgressSink;
    use windows::core::implement;

    #[implement(IFileOperationProgressSink)]
    pub(super) struct Sink {
        pub(super) seen: Rc<RefCell<super::Seen>>,
    }
}
use sink::{Sink, Sink_Impl};

// Every method but the two delete hooks is required by the interface and
// has nothing to do for a delete.
#[allow(non_snake_case)]
impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _hr: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreDeleteItem(&self, flags: u32, _: Ref<'_, IShellItem>) -> windows::core::Result<()> {
        // Without this flag the engine is about to destroy the item. Failing
        // here cancels the operation; the item stays where it is.
        if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0.cast_unsigned() == 0 {
            self.seen.borrow_mut().would_nuke = true;
            return Err(E_ABORT.into());
        }
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        hr: HRESULT,
        _: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        if hr.is_err() {
            let mut seen = self.seen.borrow_mut();
            if seen.failure.is_none() {
                seen.failure = Some(hr);
            }
        }
        Ok(())
    }
    fn PreNewItem(&self, _: u32, _: Ref<'_, IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: Ref<'_, IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        _: HRESULT,
        _: Ref<'_, IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

fn recycle_one(path: &Path) -> Outcome {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let result = run_operation(path, &seen);
    let seen = seen.borrow();

    if seen.would_nuke {
        return Outcome::WouldBePermanent;
    }
    let failure = seen.failure.or_else(|| result.err());
    if let Some(hr) = failure {
        return classify(path, hr);
    }
    // The engine said yes; believe the filesystem.
    if std::fs::symlink_metadata(path).is_ok() {
        return Outcome::Failed(E_FAIL);
    }
    Outcome::Recycled
}

/// Builds and runs one delete-to-bin operation. `Err` carries the HRESULT.
fn run_operation(path: &Path, seen: &Rc<RefCell<Seen>>) -> Result<(), HRESULT> {
    let text = wide(path);
    // SAFETY: COM is initialised on this thread (STA); every pointer passed
    // below outlives the call it is passed to.
    unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.code())?;
        operation
            .SetOperationFlags(
                FOFX_RECYCLEONDELETE
                    | FOF_NOCONFIRMATION
                    | FOF_NOCONFIRMMKDIR
                    | FOF_NOERRORUI
                    | FOF_SILENT,
            )
            .map_err(|e| e.code())?;
        let item: IShellItem =
            SHCreateItemFromParsingName(PCWSTR(text.as_ptr()), None).map_err(|e| e.code())?;
        let sink: IFileOperationProgressSink = Sink { seen: seen.clone() }.into();
        let cookie = operation.Advise(&sink).map_err(|e| e.code())?;
        operation.DeleteItem(&item, None).map_err(|e| e.code())?;
        let performed = operation.PerformOperations();
        let _ = operation.Unadvise(cookie);
        performed.map_err(|e| e.code())?;
        if operation
            .GetAnyOperationsAborted()
            .is_ok_and(windows::core::BOOL::as_bool)
        {
            return Err(E_ABORT);
        }
    }
    Ok(())
}

/// `HRESULT_FROM_WIN32(ERROR_SHARING_VIOLATION)`, `ERROR_LOCK_VIOLATION`, and
/// the copy engine's `SHARING_VIOLATION_SRC` / `_DEST`. A folder holding an
/// open file reports `_DEST` (the move into the bin), not `_SRC`: found live
/// on D:, where it was shown as a plain failure with no holder named.
const SHARING: [u32; 4] = [0x8007_0020, 0x8007_0021, 0x8027_0027, 0x8027_0028];
/// `E_ACCESSDENIED`, `COPYENGINE_E_ACCESS_DENIED_SRC`, `..._ACCESSDENIED_READONLY`.
const DENIED: [u32; 3] = [0x8007_0005, 0x8027_0021, 0x8027_003F];
/// `COPYENGINE_E_RECYCLE_*`: too big, path too long, no bin, force nuke.
const NO_BIN: [u32; 4] = [0x8027_0036, 0x8027_0037, 0x8027_0038, 0x8027_003A];
/// File or path not found.
const GONE: [u32; 2] = [0x8007_0002, 0x8007_0003];

fn classify(path: &Path, hr: HRESULT) -> Outcome {
    let code = hr.0.cast_unsigned();
    if SHARING.contains(&code) {
        Outcome::Locked(holders_of(path).ok())
    } else if DENIED.contains(&code) {
        Outcome::AccessDenied
    } else if NO_BIN.contains(&code) {
        Outcome::WouldBePermanent
    } else if GONE.contains(&code) {
        Outcome::Missing
    } else {
        Outcome::Failed(hr.0)
    }
}

/// The programs holding `path` open, or holding any file inside it.
///
/// # Errors
///
/// The Restart Manager error code when a session could not be started,
/// registered or read.
pub fn holders_of(path: &Path) -> Result<Vec<Holder>, u32> {
    let files = if path.is_dir() {
        files_inside(path, HOLDER_FILE_CAP)
    } else {
        vec![path.to_path_buf()]
    };
    holders(&files)
}

/// Up to `cap` files under `dir`, not following links.
fn files_inside(dir: &Path, cap: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                out.push(entry.path());
                if out.len() >= cap {
                    return out;
                }
            }
        }
    }
    out
}

/// A Restart Manager session, ended on drop.
struct Session(u32);

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful RmStartSession.
        unsafe {
            let _ = RmEndSession(self.0);
        }
    }
}

/// Asks Restart Manager which processes have any of `files` open.
fn holders(files: &[PathBuf]) -> Result<Vec<Holder>, u32> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut handle = 0_u32;
    let mut key = [0_u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `key` has room for the session key and its terminator.
    let started = unsafe { RmStartSession(&raw mut handle, None, PWSTR(key.as_mut_ptr())) };
    if started != ERROR_SUCCESS {
        return Err(started.0);
    }
    let session = Session(handle);

    let wides: Vec<Vec<u16>> = files.iter().map(|f| wide(f)).collect();
    let names: Vec<PCWSTR> = wides.iter().map(|w| PCWSTR(w.as_ptr())).collect();
    // SAFETY: `names` points into `wides`, both live across the call.
    let registered = unsafe { RmRegisterResources(session.0, Some(&names), None, None) };
    if registered != ERROR_SUCCESS {
        return Err(registered.0);
    }

    let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
    loop {
        let mut needed = 0_u32;
        let mut count = u32::try_from(infos.len()).unwrap_or(u32::MAX);
        let mut reasons = 0_u32;
        let buffer = if infos.is_empty() {
            None
        } else {
            Some(infos.as_mut_ptr())
        };
        // SAFETY: `buffer` has room for `count` entries; RM writes at most
        // that many and reports how many it needs.
        let status = unsafe {
            RmGetList(
                session.0,
                &raw mut needed,
                &raw mut count,
                buffer,
                &raw mut reasons,
            )
        };
        if status == ERROR_SUCCESS {
            infos.truncate(count as usize);
            break;
        }
        if status != ERROR_MORE_DATA {
            return Err(status.0);
        }
        // Programs can open the file between the two calls; a little slack
        // saves a round trip.
        infos = vec![RM_PROCESS_INFO::default(); needed as usize + 4];
    }

    let mut out: Vec<Holder> = infos.iter().map(holder).collect();
    out.sort_by_key(|h| h.pid);
    out.dedup_by_key(|h| h.pid);
    Ok(out)
}

fn text(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(buffer.get(..end).unwrap_or(&[]))
}

fn holder(info: &RM_PROCESS_INFO) -> Holder {
    let service = text(&info.strServiceShortName);
    Holder {
        pid: info.Process.dwProcessId,
        name: text(&info.strAppName),
        service: (!service.is_empty()).then_some(service),
        kind: holder_kind(info.ApplicationType),
    }
}

fn holder_kind(kind: RM_APP_TYPE) -> HolderKind {
    match kind {
        k if k == RmMainWindow || k == RmOtherWindow => HolderKind::Window,
        k if k == RmService => HolderKind::Service,
        k if k == RmExplorer => HolderKind::Explorer,
        k if k == RmConsole => HolderKind::Console,
        k if k == RmCritical => HolderKind::Critical,
        _ => HolderKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::fs::OpenOptionsExt as _;

    use super::*;

    /// A private folder under the temp directory, removed on drop.
    ///
    /// Only this fixture's own cleanup uses `remove_dir_all`, and only on the
    /// test's temporary folder; the code under test never deletes.
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("vitals-recycle-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("fixture dir");
            Self(dir)
        }
        fn file(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, b"vitals").expect("fixture file");
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn s(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_system_path_is_refused_and_left_in_place() {
        let rules = Rules::for_this_machine();
        let hosts = r"C:\Windows\System32\drivers\etc\hosts";
        let before = std::fs::metadata(hosts).map(|m| m.len()).ok();
        let out = recycle(
            &[
                hosts.to_owned(),
                r"C:\Windows".to_owned(),
                r"C:\".to_owned(),
            ],
            &rules,
        );
        assert_eq!(
            out,
            vec![
                Outcome::Refused(Protection::SystemFolder),
                Outcome::Refused(Protection::SystemFolder),
                Outcome::Refused(Protection::DriveRoot),
            ]
        );
        assert_eq!(std::fs::metadata(hosts).map(|m| m.len()).ok(), before);
    }

    #[test]
    fn a_file_and_a_folder_go_to_the_bin_and_are_gone_from_their_place() {
        let fixture = Fixture::new("ok");
        let file = fixture.file("recycle-me.txt");
        let folder = fixture.0.join("folder");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join("inner.txt"), b"x").expect("inner");

        let out = recycle(&[s(&file), s(&folder)], &Rules::for_this_machine());
        assert_eq!(out, vec![Outcome::Recycled, Outcome::Recycled]);
        assert!(!file.exists());
        assert!(!folder.exists());
    }

    #[test]
    fn a_missing_item_is_reported_missing_not_recycled() {
        let fixture = Fixture::new("missing");
        let out = recycle(
            &[s(&fixture.0.join("never-was.txt"))],
            &Rules::for_this_machine(),
        );
        assert_eq!(out, vec![Outcome::Missing]);
    }

    #[test]
    fn a_file_held_open_is_left_and_its_holder_is_named() {
        let fixture = Fixture::new("locked");
        let file = fixture.file("held.txt");
        // Opened without FILE_SHARE_DELETE, as most programs open a document.
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&file)
            .expect("open");

        let out = recycle(&[s(&file)], &Rules::for_this_machine());
        assert!(file.exists(), "a held file must stay where it is");
        let Some(Outcome::Locked(Some(holders))) = out.first() else {
            panic!("expected a locked outcome with holders, got {out:?}");
        };
        assert!(
            holders.iter().any(|h| h.pid == std::process::id()),
            "this test process holds the file: {holders:?}"
        );
        drop(held);
    }

    #[test]
    fn restart_manager_names_the_holder_of_a_file_in_a_folder() {
        let fixture = Fixture::new("folder-rm");
        let inner = fixture.0.join("sub");
        std::fs::create_dir_all(&inner).expect("sub");
        let file = inner.join("open.txt");
        std::fs::write(&file, b"x").expect("write");
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1)
            .open(&file)
            .expect("open");
        let holders = holders_of(&fixture.0).expect("restart manager");
        assert!(holders.iter().any(|h| h.pid == std::process::id()));
        drop(held);
        assert_eq!(holders_of(&fixture.0).expect("rm"), Vec::new());
    }

    #[test]
    fn a_folder_with_a_file_held_open_is_left_and_the_holder_is_named() {
        // Found live on D:: the shell reports this as a sharing violation on
        // the destination, which was shown as a bare failure with an HRESULT.
        let fixture = Fixture::new("folder-held");
        let folder = fixture.0.join("docs");
        std::fs::create_dir_all(&folder).expect("docs");
        let file = folder.join("open.txt");
        std::fs::write(&file, b"x").expect("write");
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1)
            .open(&file)
            .expect("open");

        let out = recycle(&[s(&folder)], &Rules::for_this_machine());
        assert!(file.exists(), "a folder with an open file stays");
        let Some(Outcome::Locked(Some(holders))) = out.first() else {
            panic!("expected locked with holders, got {out:?}");
        };
        assert!(holders.iter().any(|h| h.pid == std::process::id()));
        drop(held);
    }
}
