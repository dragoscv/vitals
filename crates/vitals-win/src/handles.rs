//! Open kernel handles, enumerated on demand.
//!
//! ## Why this is never sampled on a timer
//!
//! `NtQuerySystemInformation(SystemExtendedHandleInformation)` returns the
//! **whole machine's** handle table in one allocation — every handle held by
//! every process. On an ordinary desktop that is upwards of 400 000 entries
//! and tens of megabytes, and there is no per-process variant to ask for
//! instead. Doing that once a second would dwarf the entire rest of the
//! sampler, so this is only ever called when a user opens the handles view
//! for one selected process.
//!
//! ## The hang
//!
//! Naming a handle means `NtQueryObject(ObjectNameInformation)`, and on a
//! handle to a **synchronous named pipe whose other end is not reading**,
//! that call never returns. It is not slow: it blocks in the file system
//! driver forever, and there is no timeout parameter and no way to cancel
//! it. Every tool in this space has shipped a hang from it.
//!
//! The mitigation used here is a dedicated worker thread per enumeration: it
//! performs the naming calls and posts results back over a channel, and the
//! caller waits with a deadline. If the deadline expires the thread is
//! **abandoned, not killed** — `TerminateThread` on a thread stopped inside
//! a driver leaks its stack and can corrupt the loader lock, which is a
//! worse outcome than one leaked thread that unblocks when the pipe does.
//! The handles named up to that point are still returned, and the rest carry
//! `name: None`, which is the honest answer.
//!
//! The obvious cheaper alternatives were rejected: skipping every `File`
//! handle loses the names users actually want (open documents, log files),
//! and probing the pipe's state first is itself a `NtQueryObject` call.

use std::sync::mpsc;
use std::time::Duration;

use vitals_core::error::{Error, Result};
use vitals_core::ids::{Pid, ProcessKey};
use vitals_core::process::HandleInfo;
use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, FALSE, HANDLE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, PROCESS_DUP_HANDLE};

use crate::actions::process::{ProcessHandle, verify_identity};
use crate::process::raw::{
    NtQuerySystemInformation, STATUS_BUFFER_OVERFLOW, STATUS_BUFFER_TOO_SMALL,
    STATUS_INFO_LENGTH_MISMATCH, UnicodeString,
};

/// `SystemExtendedHandleInformation`
const SYSTEM_EXTENDED_HANDLE_INFORMATION: i32 = 64;

/// `ObjectNameInformation`
const OBJECT_NAME_INFORMATION: i32 = 1;

/// `ObjectTypeInformation`
const OBJECT_TYPE_INFORMATION: i32 = 2;

/// How long the naming worker gets before its remaining handles are reported
/// unnamed.
///
/// Long enough that a busy machine still names everything (a healthy
/// `NtQueryObject` is microseconds, and a large process holds a few thousand
/// handles), short enough that a wedged pipe does not make the UI look
/// broken.
const NAMING_BUDGET: Duration = Duration::from_millis(750);

/// One row of the system handle table.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SystemHandleTableEntryInfoEx {
    object: *mut std::ffi::c_void,
    unique_process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

/// The header the table entries follow.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SystemHandleInformationEx {
    number_of_handles: usize,
    reserved: usize,
}

unsafe extern "system" {
    fn NtQueryObject(
        Handle: HANDLE,
        ObjectInformationClass: i32,
        ObjectInformation: *mut std::ffi::c_void,
        ObjectInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32;

    fn NtDuplicateObject(
        SourceProcessHandle: HANDLE,
        SourceHandle: HANDLE,
        TargetProcessHandle: HANDLE,
        TargetHandle: *mut HANDLE,
        DesiredAccess: u32,
        HandleAttributes: u32,
        Options: u32,
    ) -> i32;
}

/// Lists the handles open in one process.
///
/// The list is a point-in-time snapshot: a process opens and closes handles
/// constantly, so two calls a moment apart legitimately differ.
///
/// # Errors
///
/// - [`Error::NotFound`] when the process has exited or its PID was reused.
///   Returned in preference to a partial list — half a dead process's
///   handles is worse than an honest failure.
/// - [`Error::AccessDenied`] when the process denies `PROCESS_DUP_HANDLE`,
///   which is every protected and most system processes when unelevated.
/// - [`Error::Os`] when the system handle table cannot be read at all.
pub fn for_process(key: ProcessKey) -> Result<Vec<HandleInfo>> {
    // Identity first. Enumerating the table for a recycled PID would return
    // a complete, plausible, entirely wrong list.
    let target = ProcessHandle::open(
        key.pid,
        PROCESS_DUP_HANDLE
            | windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
    )?;
    verify_identity(&target, key)?;

    let table = read_table()?;
    let mut rows: Vec<HandleInfo> = table
        .into_iter()
        .filter(|entry| entry.unique_process_id == key.pid.get() as usize)
        .map(|entry| HandleInfo {
            pid: key.pid,
            value: entry.handle_value as u64,
            kind: None,
            name: None,
            granted_access: entry.granted_access,
        })
        .collect();

    // Re-verify: the table read is not atomic with the identity check, and a
    // process that exited midway would otherwise contribute a stale slice of
    // somebody else's handles.
    verify_identity(&target, key)?;

    describe(&target, &mut rows);
    Ok(rows)
}

/// Reads the whole system handle table.
fn read_table() -> Result<Vec<SystemHandleTableEntryInfoEx>> {
    // Starts well above a typical machine's requirement: the table grows
    // between the size query and the read on every busy system, so a tight
    // first guess simply guarantees an extra megabyte-scale allocation.
    let mut buffer = vec![0_u8; 4 * 1024 * 1024];

    loop {
        let mut returned: u32 = 0;
        let capacity = u32::try_from(buffer.len()).unwrap_or(u32::MAX);

        // SAFETY: the pointer is valid for `capacity` bytes from a Vec of
        // exactly that length, and the kernel is told the true size.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                buffer.as_mut_ptr().cast(),
                capacity,
                &raw mut returned,
            )
        };

        if status >= 0 {
            break;
        }

        if status != STATUS_INFO_LENGTH_MISMATCH {
            return Err(Error::Os {
                context: "NtQuerySystemInformation(SystemExtendedHandleInformation)".into(),
                code: status,
            });
        }

        // Handles are created constantly, so `returned` is a floor. Growing
        // past it avoids spinning here on a machine under load.
        let needed = (returned as usize).max(buffer.len());
        let next = needed.saturating_add(needed / 2);
        if next > 512 * 1024 * 1024 {
            return Err(Error::Os {
                context: "system handle table exceeded 512 MiB".into(),
                code: status,
            });
        }
        buffer.resize(next, 0);
    }

    // SAFETY: the call succeeded, so the kernel wrote a header followed by
    // `number_of_handles` entries; both reads below are bounds-checked
    // against the buffer we own.
    Ok(unsafe { parse_table(&buffer) })
}

/// Copies the table entries out of the raw buffer.
///
/// # Safety
///
/// `buffer` must have been filled by `SystemExtendedHandleInformation`.
unsafe fn parse_table(buffer: &[u8]) -> Vec<SystemHandleTableEntryInfoEx> {
    let header_size = size_of::<SystemHandleInformationEx>();
    if buffer.len() < header_size {
        return Vec::new();
    }

    // SAFETY: the length check above covers this read.
    let header = unsafe {
        buffer
            .as_ptr()
            .cast::<SystemHandleInformationEx>()
            .read_unaligned()
    };

    let entry_size = size_of::<SystemHandleTableEntryInfoEx>();
    // Bound the count by what the buffer can actually hold. The kernel is
    // trusted, but a count that outruns the buffer must not become an
    // out-of-bounds read.
    let available = (buffer.len() - header_size) / entry_size;
    let count = header.number_of_handles.min(available);

    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let offset = header_size + index * entry_size;
        // SAFETY: `offset + entry_size <= buffer.len()` by construction of
        // `count`. Unaligned because the kernel packs the table.
        out.push(unsafe {
            buffer
                .as_ptr()
                .add(offset)
                .cast::<SystemHandleTableEntryInfoEx>()
                .read_unaligned()
        });
    }
    out
}

/// Fills in type and name for as many handles as the budget allows.
///
/// Consumes the process handle: it is moved onto the worker thread, which may
/// outlive this call if a name blocks, and the handle must stay open for as
/// long as the thread might use it.
fn describe(target: &ProcessHandle, rows: &mut [HandleInfo]) {
    let values: Vec<u64> = rows.iter().map(|row| row.value).collect();
    let (sender, receiver) = mpsc::channel::<(usize, Option<String>, Option<String>)>();

    // The worker may outlive this call — a wedged name leaves it blocked in a
    // driver — so it cannot borrow the caller's handle. It gets its own
    // duplicate, which it closes when it finishes, however long that takes.
    let Some(owned) = duplicate_for_worker(target) else {
        // Without a handle nothing can be named. Every row stays `None`,
        // which is the honest outcome.
        return;
    };

    // Abandoned rather than joined when the budget expires. See the module
    // comment: killing a thread blocked in a driver is worse than leaking one.
    std::thread::Builder::new()
        .name("vitals-handle-names".into())
        .spawn(move || {
            for (index, value) in values.into_iter().enumerate() {
                // SAFETY: `owned` is our own duplicate of the target process
                // handle, open for the whole loop; `value` is a handle value
                // the kernel reported for that process.
                let described = unsafe { describe_one(owned.0 as HANDLE, value) };
                if sender.send((index, described.0, described.1)).is_err() {
                    // The caller gave up. Nothing left to report to.
                    return;
                }
            }
        })
        .map_or_else(
            |_| {
                // A machine that cannot spawn a thread has larger problems;
                // every handle simply stays unnamed, which is honest.
            },
            |_joined| {
                let deadline = std::time::Instant::now() + NAMING_BUDGET;
                while let Some(remaining) =
                    deadline.checked_duration_since(std::time::Instant::now())
                {
                    match receiver.recv_timeout(remaining) {
                        Ok((index, kind, name)) => {
                            if let Some(row) = rows.get_mut(index) {
                                row.kind = kind;
                                row.name = name;
                            }
                        }
                        // Worker finished, or wedged. Either way we are done.
                        Err(_) => break,
                    }
                }
            },
        );
}

/// An owned process handle addressed as an integer so it can cross a thread.
///
/// A Windows `HANDLE` is a `*mut c_void` in the bindings and therefore not
/// `Send`, but it is not a pointer into this process's address space — it is
/// an index into the kernel's per-process handle table, and it is valid on
/// every thread of the process that owns it. Carrying it as a `usize` states
/// that, and closing it in `Drop` means an abandoned worker still releases
/// it whenever it finally unblocks.
struct OwnedHandle(usize);

// SAFETY: see the type's documentation — a handle is a process-wide table
// index, not thread-affine, and this wrapper has unique ownership of it.
unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: we own this duplicate and close it exactly once.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0 as HANDLE) };
    }
}

/// Duplicates the target process handle for the naming worker to own.
fn duplicate_for_worker(target: &ProcessHandle) -> Option<OwnedHandle> {
    let mut duplicate: HANDLE = std::ptr::null_mut();

    // SAFETY: both process arguments are pseudo-handles to ourselves, the
    // source handle is valid, and `duplicate` is a live HANDLE.
    let ok = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            target.raw(),
            GetCurrentProcess(),
            &raw mut duplicate,
            0,
            FALSE,
            DUPLICATE_SAME_ACCESS,
        )
    };

    if ok == FALSE || duplicate.is_null() {
        return None;
    }
    Some(OwnedHandle(duplicate as usize))
}

/// Duplicates one handle into this process and asks the object what it is.
///
/// # Safety
///
/// `process` must be an open handle carrying `PROCESS_DUP_HANDLE`, and
/// `value` a handle value valid inside that process.
unsafe fn describe_one(process: HANDLE, value: u64) -> (Option<String>, Option<String>) {
    let mut local: HANDLE = std::ptr::null_mut();

    // SAFETY: guaranteed by the caller's contract; `local` is a live HANDLE.
    let status = unsafe {
        NtDuplicateObject(
            process,
            value as HANDLE,
            GetCurrentProcess(),
            &raw mut local,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };

    if status < 0 || local.is_null() {
        // Not every handle can be duplicated — kernel-only objects, and
        // handles the target opened without duplicate rights.
        return (None, None);
    }

    // SAFETY: `local` is a valid duplicate we own.
    let kind = unsafe { query_string(local, OBJECT_TYPE_INFORMATION) };
    // SAFETY: as above. This is the call that can block forever on a
    // synchronous pipe, which is why the whole function runs off-thread.
    let name = unsafe { query_string(local, OBJECT_NAME_INFORMATION) };

    // SAFETY: closing our own duplicate exactly once.
    unsafe { windows_sys::Win32::Foundation::CloseHandle(local) };

    (kind, name)
}

/// Reads a `UNICODE_STRING`-prefixed object information class.
///
/// # Safety
///
/// `handle` must be a valid object handle owned by this process.
unsafe fn query_string(handle: HANDLE, class: i32) -> Option<String> {
    // Both classes lead with a UNICODE_STRING whose buffer normally follows
    // it. 1 KiB covers every real object name; the loop handles the rest.
    let mut buffer = vec![0_u8; 1024];

    loop {
        let mut returned: u32 = 0;
        let capacity = u32::try_from(buffer.len()).ok()?;

        // SAFETY: the buffer is valid for `capacity` bytes and the kernel is
        // told its true size.
        let status = unsafe {
            NtQueryObject(
                handle,
                class,
                buffer.as_mut_ptr().cast(),
                capacity,
                &raw mut returned,
            )
        };

        if status >= 0 {
            break;
        }

        if status != STATUS_INFO_LENGTH_MISMATCH
            && status != STATUS_BUFFER_OVERFLOW
            && status != STATUS_BUFFER_TOO_SMALL
        {
            return None;
        }

        let needed = (returned as usize).max(buffer.len() * 2);
        if needed > 64 * 1024 {
            return None;
        }
        buffer.resize(needed, 0);
    }

    if buffer.len() < size_of::<UnicodeString>() {
        return None;
    }

    // SAFETY: the call succeeded, so a UNICODE_STRING sits at the head of
    // the buffer with its own buffer pointer into the tail.
    let string = unsafe { buffer.as_ptr().cast::<UnicodeString>().read_unaligned() };

    // SAFETY: the kernel wrote the pointer and its byte length together;
    // both remain valid while `buffer` is alive, which it is here.
    unsafe { string.to_string_lossy() }.filter(|s| !s.is_empty())
}

/// Whether this build can enumerate handles at all.
///
/// A capability question rather than a measurement, so the UI can hide the
/// view instead of showing an error after a click.
#[must_use]
pub fn is_supported() -> bool {
    read_table().is_ok()
}

/// The PID a handle table row belongs to, for callers grouping across
/// processes.
#[must_use]
pub const fn owning_pid(handle: &HandleInfo) -> Pid {
    handle.pid
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn spawn_victim() -> std::process::Child {
        Command::new("cmd")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn a test process")
    }

    fn key_for(pid: u32) -> ProcessKey {
        let mut enumerator = crate::process::ProcessEnumerator::new();
        let found = enumerator
            .enumerate()
            .expect("enumerate")
            .into_iter()
            .find(|p| p.key.pid.get() == pid);

        match found {
            Some(process) => process.key,
            None => panic!("process {pid} not found"),
        }
    }

    #[test]
    fn our_own_process_holds_handles_and_at_least_one_is_named() {
        // SAFETY: no preconditions.
        let key = key_for(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() });
        let handles = for_process(key).expect("enumerate our own handles");

        assert!(
            !handles.is_empty(),
            "a running process always holds handles"
        );
        assert!(
            handles.iter().any(|h| h.kind.is_some()),
            "no handle could be typed — NtQueryObject or the duplication is broken"
        );
        assert!(
            handles.iter().all(|h| h.pid == key.pid),
            "a handle was attributed to the wrong process"
        );
    }

    #[test]
    fn an_exited_process_yields_an_error_rather_than_a_partial_list() {
        let mut child = spawn_victim();
        let key = key_for(child.id());
        child.kill().expect("kill");
        child.wait().expect("wait");

        let result = for_process(key);
        assert!(
            result.is_err(),
            "a dead process must not produce a list, got {:?} rows",
            result.map(|r| r.len())
        );
    }

    #[test]
    fn a_recycled_pid_is_rejected_by_the_start_time_rather_than_listed() {
        // A key that names a live PID with a start time that cannot be its
        // own. Listing this would return the current occupant's handles under
        // the previous occupant's identity.
        // SAFETY: no preconditions.
        let live = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        let mut key = key_for(live);
        key.start_time = key.start_time.wrapping_sub(1_000_000);

        assert!(matches!(for_process(key), Err(Error::NotFound(_))));
    }

    #[test]
    fn the_system_process_is_denied_rather_than_reported_as_holding_nothing() {
        // PID 4 refuses PROCESS_DUP_HANDLE unelevated. An empty Vec here
        // would claim the kernel holds no handles, which is absurd; the only
        // acceptable answers are an error or, elevated, a non-empty list.
        let key = key_for(4);
        match for_process(key) {
            Err(_) => {}
            Ok(handles) => assert!(
                !handles.is_empty(),
                "System reported zero handles — that is a fabricated answer"
            ),
        }
    }

    #[test]
    fn naming_completes_within_its_budget_even_though_a_pipe_could_block() {
        // The guarantee is bounded latency, not completeness. Our own process
        // holds pipe handles (the test harness captures output), so this
        // exercises the path that historically hung.
        // SAFETY: no preconditions.
        let key = key_for(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() });

        let start = std::time::Instant::now();
        let _ = for_process(key).expect("enumerate");
        let elapsed = start.elapsed();

        assert!(
            elapsed < NAMING_BUDGET * 4,
            "enumeration took {elapsed:?}, which means the budget is not being enforced"
        );
    }

    #[test]
    fn the_table_parser_refuses_a_count_that_outruns_its_buffer() {
        // A truncated buffer claiming a million handles must yield the
        // entries that are actually there, not read past the end.
        let mut buffer = vec![0_u8; size_of::<SystemHandleInformationEx>()];
        buffer[0] = 0xFF;
        buffer[1] = 0xFF;

        // SAFETY: the parser's contract is exactly that it bounds itself.
        let parsed = unsafe { parse_table(&buffer) };
        assert!(parsed.is_empty());
    }
}
