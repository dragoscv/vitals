//! MSIX startup tasks: how a Store-installed Vitals starts at logon.
//!
//! A packaged app's writes to `HKCU\…\Run` land in its private registry
//! hive, which Windows never reads at logon — so the Run-key route that the
//! direct installer uses silently does nothing inside the Store package. The
//! package manifest declares `windows.startupTask` entries instead, and these
//! functions turn them on and off. Every function is meaningful only inside a
//! package; outside one the `WinRT` calls fail and the answers are `None`/`Err`.

use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::{AppInstance, StartupTask, StartupTaskState};
use windows::core::HSTRING;

/// The task that starts Vitals minimised to the tray.
pub const APP_TASK: &str = "VitalsStartup";
/// The task that starts the lag watchdog.
pub const WATCHDOG_TASK: &str = "VitalsWatchdogStartup";

/// Whether the task is enabled. `None` when it cannot be read (not packaged,
/// or the task id is not in the manifest).
#[must_use]
pub fn is_enabled(task_id: &str) -> Option<bool> {
    let task = StartupTask::GetAsync(&HSTRING::from(task_id))
        .ok()?
        .join()
        .ok()?;
    Some(enabled(task.State().ok()?))
}

/// Turns the task on or off and returns the state Windows actually settled on.
///
/// Enabling can be refused — the user turned it off in Task Manager's Startup
/// tab (`DisabledByUser`, which only the user can undo) or policy forbids it —
/// so the caller must show the returned state, not the requested one.
///
/// # Errors
///
/// When the task cannot be found or the `WinRT` call fails, typically because
/// the process is not packaged.
pub fn set(task_id: &str, on: bool) -> windows::core::Result<bool> {
    let task = StartupTask::GetAsync(&HSTRING::from(task_id))?.join()?;
    if on {
        let state = task.RequestEnableAsync()?.join()?;
        Ok(enabled(state))
    } else {
        // `Disable` on a policy-enabled task is a no-op; report what is true.
        task.Disable()?;
        Ok(enabled(task.State()?))
    }
}

/// Whether this process was started by a startup task rather than a person,
/// so it should stay in the tray. Packaged apps get no command-line argument
/// from a startup task; the activation kind is the only signal.
#[must_use]
pub fn launched_by_startup_task() -> bool {
    AppInstance::GetActivatedEventArgs()
        .and_then(|args| args.Kind())
        .is_ok_and(|kind| kind == ActivationKind::StartupTask)
}

fn enabled(state: StartupTaskState) -> bool {
    state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unpackaged_process_reports_unknown_rather_than_off() {
        // "Off" would make the Settings switch lie to a direct-install user;
        // unknown lets the caller fall back to the Run-key route.
        assert_eq!(is_enabled(APP_TASK), None);
    }

    #[test]
    fn an_unpackaged_process_was_not_started_by_a_startup_task() {
        assert!(!launched_by_startup_task());
    }
}
