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
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

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

    // SAFETY: `on_unhandled` has the signature the filter type requires and
    // is a plain function that lives for the life of the process.
    unsafe { SetUnhandledExceptionFilter(Some(on_unhandled)) };
    Ok(())
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
        assert!(write_dump(&wide(path.as_os_str()), null(), 0));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"MDMP", "not a minidump");
        // Stacks and modules only: a heap dump of a test process is tens of
        // megabytes; this one must stay small enough to attach to an issue.
        assert!(bytes.len() < 16 * 1024 * 1024, "{} bytes", bytes.len());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_native_crash_leaves_a_minidump_behind() {
        let dir = temp_dir("child");
        let exe = std::env::current_exe().unwrap();
        let mut child = std::process::Command::new(exe)
            .args([
                "--exact",
                "crash::tests::crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD_DIR, &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        // Bounded: if Windows Error Reporting ever holds the dead process
        // open, this test fails instead of hanging the whole suite.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                panic!("the crashing child did not exit within 60 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };
        assert!(
            !status.success(),
            "the child was meant to crash: {status:?}"
        );
        let bytes = std::fs::read(dir.join(DUMP_FILE)).expect("no crash.dmp after a crash");
        assert_eq!(&bytes[..4], b"MDMP");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Not a test on its own: the body of the child process above.
    #[test]
    #[ignore = "run only as the crashing child of a_native_crash_leaves_a_minidump_behind"]
    fn crash_child() {
        let Some(dir) = std::env::var_os(CHILD_DIR) else {
            return;
        };
        // No "has stopped working" dialog: on a machine with a desktop
        // session WER would otherwise wait for someone to close it.
        // SAFETY: no preconditions.
        unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(
                windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX,
            );
        }
        install(Path::new(&dir)).unwrap();
        // SAFETY: deliberately not. Writing through null is the access
        // violation this module exists to record.
        unsafe { std::ptr::write_volatile(null_mut::<u32>(), 1) };
    }
}
