//! Tauri commands exposed to the webview.
//!
//! Kept deliberately few. High-frequency data does **not** come through
//! `invoke` — it is pushed over an event channel from the sampler thread.
//! Request/response over the JS bridge at 1 Hz for thousands of rows would
//! serialise on the webview's main thread and make the UI stutter, which is
//! the exact failure this product exists to avoid.

#[cfg(windows)]
use serde::Deserialize;
use tauri::State;

use vitals_core::capability::Capabilities;
use vitals_core::ids::ProcessKey;
use vitals_core::provider::HostInfo;
use vitals_core::sample::SampleRate;

use crate::state::AppState;

/// Error type returned to the frontend.
///
/// Carries a machine-readable `kind` alongside the message so the UI can
/// branch — offering elevation for a denial, staying silent for a process
/// that simply exited — rather than showing every failure as the same toast.
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CommandError {
    #[error("{message}")]
    AccessDenied { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Unsupported { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl From<vitals_core::Error> for CommandError {
    fn from(err: vitals_core::Error) -> Self {
        let message = err.to_string();
        match err {
            vitals_core::Error::AccessDenied { .. }
            | vitals_core::Error::HelperUnavailable { .. } => Self::AccessDenied { message },
            vitals_core::Error::NotFound(_) => Self::NotFound { message },
            vitals_core::Error::Unsupported(_) => Self::Unsupported { message },
            _ => Self::Internal { message },
        }
    }
}

type CommandResult<T> = std::result::Result<T, CommandError>;

/// Static machine facts, fetched once at startup.
///
/// Read on demand rather than cached in state: it is a few milliseconds, it
/// is asked for once when the About panel opens, and caching it would mean
/// deciding when to invalidate a value that can genuinely change under a
/// running process — a CPU cannot be hot-swapped, but a VM can be migrated.
#[tauri::command]
pub fn get_host_info() -> CommandResult<HostInfo> {
    #[cfg(windows)]
    {
        Ok(vitals_win::hostinfo::read())
    }

    #[cfg(not(windows))]
    {
        Err(CommandError::Unsupported {
            message: "host info needs a platform backend, and only Windows has one".into(),
        })
    }
}

/// What the backend can currently do, given privileges and installed parts.
#[tauri::command]
// Tauri's command macro requires `State` by value; it cannot be borrowed.
#[allow(clippy::needless_pass_by_value)]
pub fn get_capabilities(state: State<'_, AppState>) -> Capabilities {
    state.capabilities()
}

/// The alerts currently raised, most serious first.
///
/// Request/response for the initial list; changes arrive as
/// `vitals://alerts` events. Splitting the two means a window that opens
/// mid-episode sees the alert immediately instead of waiting for the next
/// transition.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_alerts(alerts: State<'_, crate::alerts::Alerts>) -> Vec<vitals_core::alerts::Alert> {
    alerts.active()
}

/// The Notifications panel's switches, pushed whenever they change.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_alert_prefs(alerts: State<'_, crate::alerts::Alerts>, prefs: crate::alerts::AlertPrefs) {
    alerts.set_prefs(prefs);
}

/// "Why is my PC slow?" — the current alerts joined against the process list.
///
/// Request/response, and the webview supplies the processes it already
/// holds, because the alternative is the backend cloning six hundred
/// processes every tick for a button that is pressed once a week. The
/// system metrics come from the same frame so the two agree on the instant.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn diagnose(
    alerts: State<'_, crate::alerts::Alerts>,
    system: vitals_core::metrics::SystemMetrics,
    processes: Vec<vitals_core::process::Process>,
) -> vitals_core::diagnosis::Diagnosis {
    vitals_core::diagnosis::diagnose(&alerts.active(), &system, &processes)
}

/// Localised toast titles, pushed at start and on a language change.
///
/// Toasts render in Rust because they must fire while the window is hidden;
/// the strings live in the webview because that is where i18n is. So the
/// webview hands them over once.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_alert_strings(
    alerts: State<'_, crate::alerts::Alerts>,
    strings: crate::alerts::NotificationStrings,
) {
    alerts.set_strings(strings);
}

/// Whether the title-bar × hides Vitals instead of quitting it.
///
/// Pushed from the webview whenever the setting changes, and once after
/// hydration, because the decision is made in Rust — the window event fires
/// before the frontend hears about it, so asking the webview then would be
/// too late.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_close_to_tray(tray: State<'_, crate::tray::Tray>, enabled: bool) {
    tray.set_close_to_tray(enabled);
}

/// Localised tray menu labels and tooltip words.
///
/// Same argument as [`set_alert_strings`]: the tray renders in Rust so it
/// works with the window hidden, but i18n lives in the webview.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_tray_strings(tray: State<'_, crate::tray::Tray>, strings: crate::tray::TrayStrings) {
    tray.set_strings(&strings);
}

/// Exits the process.
///
/// With `closeToTray` on, the × no longer quits, so there has to be something
/// that does. Invoked by the tray's Quit item and available to the UI.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// Adjusts the sampling cadence.
///
/// Called by the frontend on window visibility and focus changes so a hidden
/// window costs almost nothing.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_sample_rate(state: State<'_, AppState>, rate: SampleRate) {
    state.set_sample_rate(rate);
}

/// Reveals the main window once the UI has painted.
///
/// The window is created hidden (`"visible": false`) so the user never sees
/// an empty white rectangle while the webview boots — a flash that makes a
/// native app feel like a web page. The cost of that choice is that
/// something must actually show it, and if nothing does the app runs with no
/// visible window at all.
///
/// Called from the frontend after first paint rather than from `setup`,
/// because at `setup` time the webview has not rendered anything yet.
#[tauri::command]
// Tauri injects the window by value; it cannot hand us a borrow.
#[allow(clippy::needless_pass_by_value)]
pub fn show_main_window(window: tauri::Window) -> CommandResult<()> {
    // Tauri injects the window the call came from, so no lookup is needed —
    // and using the caller's window is more correct than looking up "main"
    // by label, which would break the moment a second window exists.
    window.show().map_err(|e| vitals_core::Error::Os {
        context: format!("show window: {e}"),
        code: 0,
    })?;
    window.set_focus().map_err(|e| vitals_core::Error::Os {
        context: format!("focus window: {e}"),
        code: 0,
    })?;

    Ok(())
}

/// Whether the process was started by the autostart entry.
///
/// The Run-key command line always carries `--minimized`; whether the window
/// actually stays hidden is the frontend's decision, made from the
/// `startMinimised` setting once it has hydrated. Doing it here would need
/// the setting before the store is readable.
#[must_use]
pub fn launched_minimised() -> bool {
    std::env::args().skip(1).any(|arg| arg == "--minimized")
}

/// Runtime facts about how this instance was started.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    /// Started by the autostart entry rather than by the user.
    pub autostarted: bool,
}

/// Lets the frontend decide whether to reveal the window at all.
#[tauri::command]
pub fn get_launch_options() -> LaunchOptions {
    LaunchOptions {
        autostarted: launched_minimised(),
    }
}

/// How dangerous an action is, as the frontend sees it.
///
/// Mirrors [`vitals_win::Risk`] rather than re-exporting it so the wire
/// contract does not change shape when a platform backend does.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionRisk {
    Safe,
    Disruptive,
    Critical,
    Forbidden,
}

/// What will happen if an action proceeds.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPlan {
    pub risk: ActionRisk,
    /// Translation key for the consequence, so the UI stays localised.
    pub consequence: String,
    pub needs_confirmation: bool,
    /// Whether "retry as administrator" is worth offering.
    ///
    /// False for protected processes, where elevation cannot help and the
    /// prompt is a dead end.
    pub elevation_might_help: bool,
}

/// Assesses terminating a process, without doing it.
///
/// Separate from [`terminate_process`] so the UI can state a specific
/// consequence before committing. A single generic "are you sure?" is what
/// trains people to click through warnings.
#[tauri::command]
#[cfg(windows)]
// Tauri deserialises command arguments into owned values; it cannot hand us a
// borrow.
#[allow(clippy::needless_pass_by_value)]
pub fn plan_terminate_process(pid: u32, name: Option<String>, protected: bool) -> ActionPlan {
    use vitals_core::ids::Pid;

    let plan = vitals_win::actions::plan_terminate(Pid(pid), name.as_deref(), protected);

    ActionPlan {
        risk: map_risk(plan.risk),
        consequence: plan.consequence.to_owned(),
        needs_confirmation: plan.risk.needs_confirmation(),
        elevation_might_help: plan.risk.elevation_might_help(),
    }
}

#[cfg(windows)]
const fn map_risk(risk: vitals_win::Risk) -> ActionRisk {
    match risk {
        vitals_win::Risk::Safe => ActionRisk::Safe,
        vitals_win::Risk::Disruptive => ActionRisk::Disruptive,
        vitals_win::Risk::Critical => ActionRisk::Critical,
        vitals_win::Risk::Forbidden => ActionRisk::Forbidden,
    }
}

/// Assesses suspending a process, without doing it.
#[tauri::command]
#[cfg(windows)]
#[allow(clippy::needless_pass_by_value)]
pub fn plan_suspend_process(pid: u32, name: Option<String>, protected: bool) -> ActionPlan {
    use vitals_core::ids::Pid;

    let plan = vitals_win::actions::plan_suspend(Pid(pid), name.as_deref(), protected);

    ActionPlan {
        risk: map_risk(plan.risk),
        consequence: plan.consequence.to_owned(),
        needs_confirmation: plan.risk.needs_confirmation(),
        elevation_might_help: plan.risk.elevation_might_help(),
    }
}

/// Terminates a process.
///
/// Takes the start time as well as the PID: between the frame that listed
/// the process and this call, it can exit and its PID be reused. Acting on
/// the PID alone would kill whichever process inherited the number.
#[tauri::command]
#[cfg(windows)]
pub fn terminate_process(pid: u32, start_time: u64) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::terminate(ProcessKey::new(Pid(pid), start_time), 1)?;
    Ok(())
}

/// Suspends every thread in a process.
#[tauri::command]
#[cfg(windows)]
pub fn suspend_process(pid: u32, start_time: u64) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::suspend(ProcessKey::new(Pid(pid), start_time))?;
    Ok(())
}

/// Resumes a suspended process.
#[tauri::command]
#[cfg(windows)]
pub fn resume_process(pid: u32, start_time: u64) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::resume(ProcessKey::new(Pid(pid), start_time))?;
    Ok(())
}

/// Scheduling priority, as the webview names it.
///
/// A DTO rather than deriving `Serialize` onto `vitals_win::Priority`: the
/// platform enum maps onto Windows priority classes and is free to grow a
/// variant that has no meaning to the UI. This is the wire contract, and it
/// is meant to be boring.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg(windows)]
#[serde(rename_all = "kebab-case")]
pub enum PriorityDto {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

#[cfg(windows)]
impl From<PriorityDto> for vitals_win::Priority {
    fn from(value: PriorityDto) -> Self {
        match value {
            PriorityDto::Idle => Self::Idle,
            PriorityDto::BelowNormal => Self::BelowNormal,
            PriorityDto::Normal => Self::Normal,
            PriorityDto::AboveNormal => Self::AboveNormal,
            PriorityDto::High => Self::High,
            PriorityDto::Realtime => Self::Realtime,
        }
    }
}

/// Changes a process's scheduling priority.
///
/// `vitals-win` has implemented this since the actions module landed, and the
/// capability report advertised it — but no command exposed it, so the UI
/// listed it as unimplemented. The backend was claiming a capability the
/// frontend had no way to reach.
///
/// Realtime is deliberately reachable. A CPU-bound process there can starve
/// input and make the machine look frozen, which is why the UI warns; hiding
/// the option outright would just send people to Task Manager to do the same
/// thing with less warning.
#[tauri::command]
#[cfg(windows)]
#[allow(clippy::needless_pass_by_value)]
pub fn set_process_priority(pid: u32, start_time: u64, priority: PriorityDto) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::set_priority(ProcessKey::new(Pid(pid), start_time), priority.into())?;
    Ok(())
}

/// Pins a process to a set of logical processors.
///
/// The mask is a bitfield, one bit per logical processor. An empty mask is
/// refused by the backend rather than being treated as "all": a process
/// affinitised to no processor cannot be scheduled at all, and Windows
/// returns a bare `INVALID_PARAMETER` that says nothing about why.
#[tauri::command]
#[cfg(windows)]
pub fn set_process_affinity(pid: u32, start_time: u64, mask: u64) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::set_affinity(ProcessKey::new(Pid(pid), start_time), mask)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_denied_maps_to_an_elevation_prompt() {
        let err: CommandError = vitals_core::Error::AccessDenied {
            operation: "terminate".into(),
        }
        .into();
        assert!(matches!(err, CommandError::AccessDenied { .. }));
    }

    #[test]
    fn a_vanished_process_maps_to_not_found_not_a_crash() {
        let err: CommandError = vitals_core::Error::NotFound("pid 42".into()).into();
        assert!(matches!(err, CommandError::NotFound { .. }));
    }

    #[test]
    fn missing_helper_is_treated_as_fixable_by_elevating() {
        let err: CommandError = vitals_core::Error::HelperUnavailable {
            reason: "not installed".into(),
        }
        .into();
        assert!(
            matches!(err, CommandError::AccessDenied { .. }),
            "the UI should offer to install/start the helper, not show a generic failure"
        );
    }

    #[test]
    fn errors_serialise_with_a_discriminating_tag() {
        let err = CommandError::NotFound {
            message: "gone".into(),
        };
        let json = serde_json::to_string(&err).expect("serialise");
        // kebab-case matches the generated TypeScript unions, so the frontend
        // discriminates on the same literals everywhere.
        assert!(json.contains("\"kind\":\"not-found\""), "got {json}");
    }
}
