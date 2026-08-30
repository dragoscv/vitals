//! Tauri commands exposed to the webview.
//!
//! Kept deliberately few. High-frequency data does **not** come through
//! `invoke` — it is pushed over an event channel from the sampler thread.
//! Request/response over the JS bridge at 1 Hz for thousands of rows would
//! serialise on the webview's main thread and make the UI stutter, which is
//! the exact failure this product exists to avoid.

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
#[tauri::command]
pub fn get_host_info() -> CommandResult<HostInfo> {
    Err(CommandError::Unsupported {
        message: "host info requires the platform backend, which is in progress".into(),
    })
}

/// What the backend can currently do, given privileges and installed parts.
#[tauri::command]
// Tauri's command macro requires `State` by value; it cannot be borrowed.
#[allow(clippy::needless_pass_by_value)]
pub fn get_capabilities(state: State<'_, AppState>) -> Capabilities {
    state.capabilities()
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
