//! Tauri commands exposed to the webview.
//!
//! Kept deliberately few. High-frequency data does **not** come through
//! `invoke` — it is pushed over an event channel from the sampler thread.
//! Request/response over the JS bridge at 1 Hz for thousands of rows would
//! serialise on the webview's main thread and make the UI stutter, which is
//! the exact failure this product exists to avoid.

use tauri::State;

use vitals_core::capability::Capabilities;
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
