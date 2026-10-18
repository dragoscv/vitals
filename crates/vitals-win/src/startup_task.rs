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

/// Keeps the process's multithreaded apartment open for the rest of its
/// life, once, before the first `WinRT` call.
///
/// These calls run on threads that never initialised COM, so they borrow the
/// MTA some other thread opened — the task scheduler, WMI and sensor readers
/// each open and close it around every call — and the `windows` crate caches
/// the activation factories they obtain. When the last of those threads
/// called `CoUninitialize`, the MTA was torn down under the cached factories,
/// and the next call through one jumped into released memory: the
/// `vitals-win` test binary died with `0xc0000005` twice in CI after
/// `startup_task::tests::an_unpackaged_process_reports_unknown_rather_than_off`
/// (2026-10-18), and the app runs the same readers on its own thread pool.
/// `CoIncrementMTAUsage` exists for exactly this: the MTA stays up whatever
/// other threads do.
fn keep_mta_alive() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: no preconditions. The cookie is deliberately never passed
        // to CoDecrementMTAUsage: the apartment must outlive every cached
        // factory, which live until the process exits.
        let _ = unsafe { windows::Win32::System::Com::CoIncrementMTAUsage() };
    });
}

/// Whether the task is enabled. `None` when it cannot be read (not packaged,
/// or the task id is not in the manifest).
#[must_use]
pub fn is_enabled(task_id: &str) -> Option<bool> {
    keep_mta_alive();
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
    keep_mta_alive();
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
    keep_mta_alive();
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

    /// Set by the parent below to run the teardown sequence in a child.
    const CHILD: &str = "VITALS_STARTUP_TASK_MTA_CHILD";

    #[test]
    fn a_winrt_call_survives_another_thread_closing_the_apartment_it_borrowed() {
        // The sequence runs in a child process: when it goes wrong it is an
        // access violation, which would take every other test down with it.
        let exe = std::env::current_exe().expect("test exe");
        let mut child = std::process::Command::new(exe)
            .args([
                "--exact",
                "startup_task::tests::mta_child",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait") {
                break status;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child did not exit within 60 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        assert!(status.success(), "the child died: {status:?}");
    }

    /// The body of the child above; a no-op without the variable.
    #[test]
    fn mta_child() {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        for _ in 0..50 {
            // Another thread opens the process's multithreaded apartment...
            let (opened, open) = std::sync::mpsc::channel();
            let (closing, close) = std::sync::mpsc::channel::<()>();
            let other = std::thread::spawn(move || {
                // SAFETY: balanced by the CoUninitialize below.
                let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                opened.send(hr.is_ok()).expect("send");
                close.recv().expect("recv");
                if hr.is_ok() {
                    // SAFETY: paired with the successful CoInitializeEx.
                    unsafe { CoUninitialize() };
                }
            });
            assert!(open.recv().expect("recv"), "CoInitializeEx failed");
            // ...this thread, which never initialised COM, makes WinRT calls
            // that borrow it and cache their activation factories...
            let _ = is_enabled(APP_TASK);
            let _ = launched_by_startup_task();
            // ...and then the other thread closes it, as the task scheduler
            // and WMI readers do at the end of every call.
            closing.send(()).expect("send");
            other.join().expect("join");
            let _ = is_enabled(APP_TASK);
            let _ = launched_by_startup_task();
        }
    }
}
