//! A minidump on a native crash, and a relaunch afterwards (PERF-8).
//!
//! The desktop's panic hook (`crashlog.rs`) covers Rust panics. It cannot
//! cover a fault inside Windows or a driver call — an access violation in
//! `NtQuerySystemInformation`'s buffer handling, a `WebView2` crash taking the
//! host down, a stack overflow — because none of those is a panic. Without
//! this, such a crash left nothing at all: Windows Error Reporting keeps no
//! local dump unless a registry key nobody sets says so.
//!
//! Two pieces:
//!
//! - **`crash.dmp`.** An unhandled-exception filter wakes a thread that was
//!   started in advance and has it call `MiniDumpWriteDump`. The work is not
//!   done on the crashing thread because that thread may have no stack left
//!   (a stack overflow runs the filter on the few kilobytes Windows keeps in
//!   reserve) and may hold the heap lock. The dump holds thread stacks and the
//!   module list, not the heap: enough to find the faulting call, small, and
//!   far less likely to contain anything personal. It is overwritten each
//!   time and never leaves the machine.
//! - **A relaunch.** `RegisterApplicationRestart` asks Windows Error Reporting
//!   to start the app again after a crash or a hang it terminated. Windows
//!   only does so for a process that had been running for at least a minute,
//!   which is the crash-loop guard. The filter returns
//!   `EXCEPTION_CONTINUE_SEARCH` precisely so that WER still sees the crash.
//!
//! A Rust panic in a release build (`panic = "abort"`) ends in `__fastfail`,
//! which skips exception filters by design: panics leave `crash.txt`, not a
//! dump, and are not restarted. That is deliberate — a panic is a bug that
//! would most likely recur straight away.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CREATE_ALWAYS, CreateFileW, FILE_ATTRIBUTE_NORMAL};
use windows_sys::Win32::System::Diagnostics::Debug::{
    EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION, MINIDUMP_TYPE,
    MiniDumpNormal, MiniDumpWithThreadInfo, MiniDumpWithUnloadedModules, MiniDumpWriteDump,
    SetUnhandledExceptionFilter,
};
use windows_sys::Win32::System::Recovery::{
    RESTART_NO_PATCH, RESTART_NO_REBOOT, RegisterApplicationRestart,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, INFINITE, SetEvent,
    WaitForSingleObject,
};

/// The file name inside the directory given to [`install`].
pub const DUMP_FILE: &str = "crash.dmp";

/// Stacks, thread state and modules; no heap. See the module comment.
const DUMP_TYPE: MINIDUMP_TYPE =
    MiniDumpNormal | MiniDumpWithThreadInfo | MiniDumpWithUnloadedModules;

/// How long the crashing thread waits for the dump. A hung dbghelp must not
/// keep a dead process on screen; WER's own report follows either way.
const DUMP_TIMEOUT_MS: u32 = 15_000;

/// Set once by [`install`]. Handles are kept as integers so the struct is
/// `Sync`; they live for the life of the process.
struct Dumper {
    path: Vec<u16>,
    request: usize,
    done: usize,
}

static DUMPER: OnceLock<Dumper> = OnceLock::new();
static PENDING: AtomicPtr<EXCEPTION_POINTERS> = AtomicPtr::new(null_mut());
static PENDING_THREAD: AtomicU32 = AtomicU32::new(0);
/// A second fault while the first is being dumped must not wait on a dump
/// thread that will never answer it.
static ENTERED: AtomicBool = AtomicBool::new(false);
/// The filter that was installed before ours, called after the dump so a
/// runtime that set one first (`WebView2`, a driver shim) still sees the
/// fault. Zero when there was none. Stored as an address because
/// `LPTOP_LEVEL_EXCEPTION_FILTER` is not `Sync`.
static PREVIOUS: AtomicUsize = AtomicUsize::new(0);

/// The signature `SetUnhandledExceptionFilter` takes and returns.
type Filter = unsafe extern "system" fn(*const EXCEPTION_POINTERS) -> i32;

/// What a filter returned by `SetUnhandledExceptionFilter` was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Previous {
    /// No filter was installed.
    Nothing,
    /// Ours: nobody replaced it.
    Ours,
    /// Someone else's, at this address: chain to it.
    Other(usize),
}

/// Pure, so the one rule that matters — never chain to ourselves, which
/// would recurse until the stack the filter runs on is gone — is tested.
fn classify_previous(previous: usize, ours: usize) -> Previous {
    if previous == 0 {
        Previous::Nothing
    } else if previous == ours {
        Previous::Ours
    } else {
        Previous::Other(previous)
    }
}

fn our_address() -> usize {
    on_unhandled as Filter as usize
}

/// Installs [`on_unhandled`] and remembers whatever it displaced. Returns
/// whether ours was already the filter.
fn set_filter() -> bool {
    // SAFETY: `on_unhandled` has the signature the filter type requires and
    // is a plain function that lives for the life of the process.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(on_unhandled)) };
    match classify_previous(previous.map_or(0, |f| f as usize), our_address()) {
        Previous::Ours => true,
        Previous::Nothing => false,
        Previous::Other(address) => {
            PREVIOUS.store(address, Ordering::SeqCst);
            false
        }
    }
}

/// Starts the dump thread and installs the unhandled-exception filter.
/// Idempotent: a second call keeps the first directory.
///
/// # Errors
/// The directory cannot be created, or Windows refused an event or a thread.
/// The caller should log and carry on — a monitor without crash dumps is
/// still a monitor.
pub fn install(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if DUMPER.get().is_some() {
        return Ok(());
    }
    let path = wide(dir.join(DUMP_FILE).as_os_str());

    // SAFETY: no preconditions; null attributes and name are documented.
    let request = unsafe { CreateEventW(null(), 0, 0, null()) };
    // SAFETY: as above.
    let done = unsafe { CreateEventW(null(), 0, 0, null()) };
    if request.is_null() || done.is_null() {
        let error = std::io::Error::last_os_error();
        close(request);
        close(done);
        return Err(error);
    }
    let dumper = Dumper {
        path,
        request: request as usize,
        done: done as usize,
    };
    if DUMPER.set(dumper).is_err() {
        // Lost a race with another caller; theirs is installed.
        close(request);
        close(done);
        return Ok(());
    }

    std::thread::Builder::new()
        .name("vitals-crash-dump".into())
        .stack_size(256 * 1024)
        .spawn(dump_thread)?;

    let _ = set_filter();
    Ok(())
}

/// Puts the filter back if something installed after [`install`] replaced
/// it, chaining to the newcomer (ENG2-15). Returns whether ours was still
/// in place; `false` before [`install`] has run, without installing.
///
/// The app calls this once its window and webview exist: `WebView2` and the
/// GPU stack load during setup, and any of them may call
/// `SetUnhandledExceptionFilter`, silently turning the dumps off.
#[must_use]
pub fn reassert() -> bool {
    if DUMPER.get().is_none() {
        return false;
    }
    set_filter()
}

/// Asks Windows Error Reporting to relaunch the app with `args` after a
/// crash or a hang it ended. Not after an update (the installer restarts it
/// itself) and not after a reboot (autostart already does).
///
/// Returns whether Windows accepted the registration.
#[must_use]
pub fn register_restart(args: &str) -> bool {
    let args = wide(std::ffi::OsStr::new(args));
    // SAFETY: `args` is NUL-terminated and Windows copies it.
    let hr =
        unsafe { RegisterApplicationRestart(args.as_ptr(), RESTART_NO_PATCH | RESTART_NO_REBOOT) };
    hr >= 0
}

unsafe extern "system" fn on_unhandled(info: *const EXCEPTION_POINTERS) -> i32 {
    if ENTERED.swap(true, Ordering::SeqCst) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    if let Some(dumper) = DUMPER.get() {
        PENDING.store(info.cast_mut(), Ordering::SeqCst);
        // SAFETY: no preconditions.
        PENDING_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        // SAFETY: both handles were created by `install` and are never closed.
        unsafe {
            SetEvent(dumper.request as HANDLE);
            WaitForSingleObject(dumper.done as HANDLE, DUMP_TIMEOUT_MS);
        }
    }
    let previous = PREVIOUS.load(Ordering::SeqCst);
    if let Previous::Other(address) = classify_previous(previous, our_address()) {
        // SAFETY: the address was returned by SetUnhandledExceptionFilter as
        // the installed filter, which is a function of exactly this type,
        // and module code is never unloaded while it is the filter.
        let chained: Filter = unsafe { std::mem::transmute::<usize, Filter>(address) };
        // SAFETY: called with the pointers Windows gave us, as it would have.
        return unsafe { chained(info) };
    }
    EXCEPTION_CONTINUE_SEARCH
}

fn dump_thread() {
    let Some(dumper) = DUMPER.get() else {
        return;
    };
    // SAFETY: the handle was created by `install` and is never closed.
    unsafe { WaitForSingleObject(dumper.request as HANDLE, INFINITE) };
    let exception = PENDING.load(Ordering::SeqCst);
    let thread = PENDING_THREAD.load(Ordering::SeqCst);
    let _ = write_dump(&dumper.path, exception, thread);
    // SAFETY: as above.
    unsafe { SetEvent(dumper.done as HANDLE) };
}

/// Writes a dump of this process to `path` (NUL-terminated UTF-16). A null
/// `exception` writes one without an exception record, which is how the
/// tests exercise the writer without crashing.
fn write_dump(path: &[u16], exception: *const EXCEPTION_POINTERS, thread_id: u32) -> bool {
    // SAFETY: `path` is NUL-terminated; every other argument is a constant
    // or null where the API documents null as allowed.
    let file = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE,
            0,
            null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE {
        return false;
    }
    let info = MINIDUMP_EXCEPTION_INFORMATION {
        ThreadId: thread_id,
        ExceptionPointers: exception.cast_mut(),
        ClientPointers: 0,
    };
    let param = if exception.is_null() {
        null()
    } else {
        &raw const info
    };
    // SAFETY: `file` is a writable handle we own; `param` is null or points
    // at `info`, which outlives the call.
    let ok = unsafe {
        MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file,
            DUMP_TYPE,
            param,
            null(),
            null(),
        )
    } != 0;
    close(file);
    ok
}

fn close(handle: HANDLE) {
    if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
        // SAFETY: the caller owns `handle`.
        unsafe { CloseHandle(handle) };
    }
}

fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Set by the parent test to make the re-executed test binary crash.
    const CHILD_DIR: &str = "VITALS_CRASH_TEST_DIR";
    /// Set by the parent test to make the re-executed test binary write a
    /// dump, without crashing, to the path it names.
    const CHILD_WRITE: &str = "VITALS_CRASH_TEST_WRITE";

    /// Runs one test of this binary in a process of its own and returns its
    /// exit status.
    ///
    /// Every dump in these tests is written by a child, never by the test
    /// binary itself. `MiniDumpWriteDump` on its own process suspends every
    /// other thread; with a dozen tests running in parallel one of them is
    /// sooner or later suspended holding the heap or loader lock that dbghelp
    /// then needs, and the whole suite hangs with nothing on screen (CI ran
    /// 75 minutes that way twice, after `crash_child ... ok`). The child runs
    /// only the one test on one thread, so nothing else can hold a lock.
    ///
    /// Bounded: a child that hangs anyway — dbghelp, Windows Error Reporting
    /// keeping a dead process open — is killed and fails this test instead
    /// of stalling the suite.
    fn run_child(test: &str, var: &str, value: &std::ffi::OsStr) -> std::process::ExitStatus {
        let exe = std::env::current_exe().unwrap();
        let mut child = std::process::Command::new(exe)
            .args(["--exact", test, "--test-threads=1", "--nocapture"])
            .env(var, value)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child running {test} did not exit within 60 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// Has a child process write a dump to `path` and says whether it did.
    fn write_dump_in_child(path: &Path) -> bool {
        run_child("crash::tests::write_child", CHILD_WRITE, path.as_os_str()).success()
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vitals-crash-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_writer_produces_a_file_with_the_minidump_signature() {
        let dir = temp_dir("writer");
        let path = dir.join(DUMP_FILE);
        assert!(write_dump_in_child(&path), "the writer reported failure");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"MDMP", "not a minidump");
        // Stacks and modules only: a heap dump of a test process is tens of
        // megabytes; this one must stay small enough to attach to an issue.
        assert!(bytes.len() < 16 * 1024 * 1024, "{} bytes", bytes.len());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_displaced_filter_is_chained_but_ours_never_is_so_a_fault_cannot_recurse() {
        let ours = our_address();
        assert_eq!(classify_previous(0, ours), Previous::Nothing);
        assert_eq!(classify_previous(ours, ours), Previous::Ours);
        assert_eq!(classify_previous(0x1234, ours), Previous::Other(0x1234));
    }

    #[test]
    fn reasserting_before_install_neither_installs_nor_claims_to_be_in_place() {
        // No test in this binary installs in-process (only the crash child
        // does, in its own process), so the dumper is absent here.
        assert!(DUMPER.get().is_none());
        assert!(!reassert());
    }

    #[test]
    fn the_writer_output_is_overwritten_rather_than_appended() {
        // Two dump sizes cannot be compared: a dump's size follows the threads
        // alive at that instant, and parallel tests start and end them. So the
        // file is pre-filled with junk larger than any dump this writer makes
        // (the test above bounds it at 16 MiB): an append would keep the junk
        // at the front and the length above it.
        let dir = temp_dir("writer");
        let path = dir.join(DUMP_FILE);
        let junk = vec![b'X'; 17 * 1024 * 1024];
        std::fs::write(&path, &junk).unwrap();
        assert!(write_dump_in_child(&path), "the writer reported failure");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            &bytes[..4],
            b"MDMP",
            "the old contents are still at the front"
        );
        assert!(bytes.len() < junk.len(), "{} bytes: appended", bytes.len());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_native_crash_leaves_a_minidump_behind() {
        let dir = temp_dir("child");
        let status = run_child("crash::tests::crash_child", CHILD_DIR, dir.as_os_str());
        assert!(
            !status.success(),
            "the child was meant to crash: {status:?}"
        );
        let bytes = std::fs::read(dir.join(DUMP_FILE)).expect("no crash.dmp after a crash");
        assert_eq!(&bytes[..4], b"MDMP");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The body of the writer children above; a no-op without the variable,
    /// like `crash_child`. Exits non-zero (a panic) when the writer fails.
    #[test]
    fn write_child() {
        let Some(path) = std::env::var_os(CHILD_WRITE) else {
            return;
        };
        assert!(
            write_dump(&wide(&path), null(), 0),
            "MiniDumpWriteDump failed"
        );
    }

    /// The body of the child process above. Without the variable the parent
    /// sets it does nothing, so a plain `cargo test` passes it straight
    /// through (CI forbids `#[ignore]` outside vitals-bench).
    #[test]
    fn crash_child() {
        let Some(dir) = std::env::var_os(CHILD_DIR) else {
            return;
        };
        // No "has stopped working" dialog: on a machine with a desktop
        // session WER would otherwise wait for someone to close it.
        // SAFETY: no preconditions.
        unsafe {
            use windows_sys::Win32::System::Diagnostics::Debug::{
                SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SetErrorMode,
            };
            SetErrorMode(SEM_NOGPFAULTERRORBOX | SEM_FAILCRITICALERRORS);
        }
        install(Path::new(&dir)).unwrap();
        // SAFETY: deliberately not. Writing through null is the access
        // violation this module exists to record.
        unsafe { std::ptr::write_volatile(null_mut::<u32>(), 1) };
    }
}
