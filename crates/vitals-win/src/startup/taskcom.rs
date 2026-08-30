//! Scheduled task definitions via the Task Scheduler COM API.
//!
//! Replaces the `schtasks.exe /query /xml ONE` spawn that [`super::tasks`]
//! documented as a stopgap. Same API — `schtasks` is a client of it — but
//! hosted in-process, so it costs neither a process creation nor the
//! serialisation of every definition through a pipe.
//!
//! ## Measured, on this machine, 221 tasks
//!
//! | route | cost |
//! |---|---|
//! | `schtasks.exe /query /xml ONE` | 148 ms |
//! | COM: enumerate folders and tasks | 51 ms |
//! | COM: enumerate + read each `Xml` | 113 ms |
//! | COM: enumerate + walk typed `Triggers` | **656 ms** |
//!
//! That last row is the trap, and it is why this module reads the XML blob
//! rather than the object model that looks more idiomatic. `IRegisteredTask::
//! Xml` is one cross-apartment call returning the whole definition;
//! `Definition.Triggers[n].Type` is a round-trip per property per trigger,
//! and at 221 tasks that is thousands of them. The typed route is 4× slower
//! than the subprocess it was meant to replace.
//!
//! Reading the XML also means the existing parser — and its tests — are
//! reused unchanged. Only the acquisition differs.
//!
//! ## Why not the two cheaper-looking routes
//!
//! Both were measured and both fail *silently*, which is why they are
//! recorded here rather than merely avoided:
//!
//! - `%SystemRoot%\System32\Tasks` is ACL'd: enumerating it unelevated
//!   returns zero entries and no error.
//! - The `TaskCache` registry index opens, then every sub-key enumeration
//!   returns nothing, for the same reason.
//!
//! An empty list that looks like a correct answer is the worst failure mode
//! available, so this takes the route that works unelevated.

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::TaskScheduler::{
    ITaskFolder, ITaskService, TASK_ENUM_HIDDEN, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::BSTR;

/// Concatenates every task definition into one document.
///
/// The shape deliberately matches what `schtasks /query /xml ONE` produced,
/// so [`super::tasks::parse_scan`] consumes it unchanged.
///
/// Returns `None` when the scheduler cannot be reached at all. That is
/// distinct from "there are no tasks": the caller reports the former as an
/// unreadable count rather than an empty list.
pub fn query_all_definitions() -> Option<String> {
    let _com = ComScope::enter()?;

    // SAFETY: the apartment is initialised for the lifetime of `_com`, and
    // CoCreateInstance is the documented way to obtain ITaskService.
    let service: ITaskService =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }.ok()?;

    // Empty variants mean "this machine, current user" — the same context
    // schtasks runs in, so the visible task set is identical.
    // SAFETY: `Connect` accepts empty variants for a local connection.
    unsafe {
        service
            .Connect(
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
            )
            .ok()?;
    }

    // SAFETY: the root folder always exists once connected.
    let root = unsafe { service.GetFolder(&BSTR::from("\\")) }.ok()?;

    let mut document = String::with_capacity(512 * 1024);
    let mut pending = vec![root];

    // Iterative rather than recursive: the task tree is user-extensible and
    // a deep or cyclic-looking hierarchy must not blow the stack of a
    // process whose job is to report on other processes.
    while let Some(folder) = pending.pop() {
        collect_folder(&folder, &mut document, &mut pending);
    }

    Some(document)
}

/// Appends one folder's definitions and queues its sub-folders.
fn collect_folder(folder: &ITaskFolder, document: &mut String, pending: &mut Vec<ITaskFolder>) {
    // SAFETY: `folder` is a live interface pointer; the flag is the
    // documented "include hidden" value.
    if let Ok(tasks) = unsafe { folder.GetTasks(TASK_ENUM_HIDDEN.0) } {
        // SAFETY: Count is a simple property read.
        let count = unsafe { tasks.Count() }.unwrap_or(0);

        // The collection is 1-based. Starting at 0 yields E_INVALIDARG for
        // the first item and silently loses it — every folder would be one
        // task short, which no test on the total would necessarily catch.
        for index in 1..=count {
            // SAFETY: `index` is within the reported count.
            let Ok(task) = (unsafe { tasks.get_Item(&VARIANT::from(index)) }) else {
                continue;
            };

            // One cross-apartment call for the entire definition. See the
            // module docs for why this beats the typed trigger walk.
            // SAFETY: `task` is live.
            if let Ok(xml) = unsafe { task.Xml() } {
                document.push_str(&xml.to_string());
                document.push('\n');
            }
        }
    }

    // SAFETY: same reasoning; 0 is the documented flags value.
    if let Ok(folders) = unsafe { folder.GetFolders(0) } {
        // SAFETY: Count is a simple property read.
        let count = unsafe { folders.Count() }.unwrap_or(0);

        for index in 1..=count {
            // SAFETY: `index` is within the reported count.
            if let Ok(sub) = unsafe { folders.get_Item(&VARIANT::from(index)) } {
                pending.push(sub);
            }
        }
    }
}

/// Holds a COM apartment open for as long as it is alive.
///
/// A guard rather than a bare call because every early return in the
/// enumeration above would otherwise leak the apartment, and this runs on the
/// IPC thread pool where the same thread is reused.
struct ComScope;

impl ComScope {
    fn enter() -> Option<Self> {
        // SAFETY: no preconditions. Multithreaded because this is called from
        // a Tauri command thread that has no message pump; asking for STA
        // there would either fail or require one.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

        // S_FALSE means the apartment was already initialised on this thread,
        // which is a success — and still requires a matching uninitialise,
        // so it is not special-cased away.
        if result.is_err() {
            return None;
        }

        Some(Self)
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        // SAFETY: paired with the CoInitializeEx in `enter`.
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scheduler_answers_and_returns_parsable_definitions() {
        let Some(document) = query_all_definitions() else {
            // A machine with no Task Scheduler service is legitimate, if
            // unusual. Failing the suite for it would be wrong.
            return;
        };

        assert!(
            document.contains("<Task"),
            "the document has no task elements at all"
        );

        // The existing parser must consume this unchanged — that is the whole
        // point of returning a concatenated document rather than a Vec.
        let scan = super::super::tasks::parse_scan(&document);

        assert!(
            scan.total_seen > 0,
            "parsed zero tasks from a non-empty document, so the shape differs \
             from what schtasks produced"
        );

        // Every Windows install has scheduled tasks that run at logon or
        // boot; finding none would mean the trigger detection broke.
        assert!(
            !scan.startup_tasks.is_empty(),
            "no startup tasks found among {} — trigger parsing is broken",
            scan.total_seen
        );
    }

    #[test]
    fn every_task_is_identifiable() {
        let Some(document) = query_all_definitions() else {
            return;
        };

        let scan = super::super::tasks::parse_scan(&document);

        // `unreadable` counts definitions with no `<URI>`. The COM path was
        // checked against 40 real tasks and all carried one; a regression
        // here would mean tasks silently vanishing from the list.
        assert_eq!(
            scan.unreadable, 0,
            "{} of {} definitions had no URI",
            scan.unreadable, scan.total_seen
        );
    }

    #[test]
    fn the_apartment_can_be_entered_repeatedly_without_leaking() {
        // This runs on an IPC thread that is reused across requests, so an
        // unbalanced CoInitializeEx would accumulate until the thread dies.
        for _ in 0..20 {
            let _ = query_all_definitions();
        }
    }
}
