//! Actions a client with `control` scope may request.
//!
//! The server does not perform any of these itself — it has no idea what a
//! process is. It validates scope, deserialises the request and hands it to
//! the [`Controller`] the host application supplied, which on the desktop is
//! the same code path the context menu uses. One implementation of "end
//! task", reached from two places.

use serde::{Deserialize, Serialize};
use vitals_core::ids::ProcessKey;

/// A control command from a client.
///
/// Every variant carries a [`ProcessKey`] (PID + start time), never a bare
/// PID: a phone that took a second to tap "End" must not kill whatever
/// recycled the PID in the meantime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum ControlRequest {
    Terminate {
        key: ProcessKey,
    },
    Suspend {
        key: ProcessKey,
    },
    Resume {
        key: ProcessKey,
    },
    /// `priority` is the same string set the desktop uses
    /// (`idle`, `below-normal`, `normal`, `above-normal`, `high`, `realtime`).
    SetPriority {
        key: ProcessKey,
        priority: String,
    },
    /// Windows 11 efficiency mode (`EcoQoS` + low priority). `enabled: false`
    /// restores the process's normal scheduling.
    ///
    /// This is the only per-process *setting* exposed here. The desktop's
    /// handles and modules panels are deliberately **not** mirrored as
    /// routes: each allocates megabytes per call and runs only while a user
    /// has that section expanded. A polling client would turn an on-demand
    /// cost into a continuous one on someone else's machine.
    SetEfficiencyMode {
        key: ProcessKey,
        enabled: bool,
    },
}

/// Why a control request was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ControlError {
    /// The token can read but not act.
    #[error("this token is read-only")]
    Forbidden,
    /// The process is gone, or the key no longer matches (PID recycled).
    #[error("process not found")]
    NotFound,
    /// Windows refused. Elevation might help, or it might be protected.
    #[error("access denied")]
    AccessDenied,
    /// The host has no way to do this (no backend, unsupported priority…).
    #[error("unsupported: {message}")]
    Unsupported { message: String },
    #[error("{message}")]
    Internal { message: String },
}

/// What the host application must provide for control to work.
///
/// Object-safe and `Send + Sync` so it can sit in the router's shared state.
/// A host that cannot act (the CLI without a sampler, a non-Windows build)
/// returns [`ControlError::Unsupported`] from everything rather than
/// pretending.
pub trait Controller: Send + Sync {
    fn apply(&self, request: ControlRequest) -> Result<(), ControlError>;
}

/// A controller that refuses everything, for hosts with nothing to control.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoControl;

impl Controller for NoControl {
    fn apply(&self, _request: ControlRequest) -> Result<(), ControlError> {
        Err(ControlError::Unsupported {
            message: "this host cannot act on processes".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::ids::Pid;

    #[test]
    fn requests_round_trip_as_tagged_json() {
        let req = ControlRequest::Terminate {
            key: ProcessKey {
                pid: Pid(42),
                start_time: 7,
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""action":"terminate""#), "{json}");
        let back: ControlRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn set_efficiency_mode_is_tagged_in_kebab_case_and_carries_a_boolean() {
        let json =
            r#"{"action":"set-efficiency-mode","key":{"pid":42,"startTime":7},"enabled":true}"#;
        let req: ControlRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req,
            ControlRequest::SetEfficiencyMode {
                key: ProcessKey {
                    pid: Pid(42),
                    start_time: 7,
                },
                enabled: true,
            }
        );
        let back = serde_json::to_string(&req).unwrap();
        assert!(back.contains(r#""action":"set-efficiency-mode""#), "{back}");
        assert!(back.contains(r#""enabled":true"#), "{back}");
    }

    #[test]
    fn set_efficiency_mode_without_the_flag_is_rejected_rather_than_defaulted() {
        // A missing `enabled` must not quietly mean "on" or "off".
        let json = r#"{"action":"set-efficiency-mode","key":{"pid":42,"startTime":7}}"#;
        assert!(serde_json::from_str::<ControlRequest>(json).is_err());
    }

    #[test]
    fn no_control_refuses_honestly() {
        let err = NoControl
            .apply(ControlRequest::Resume {
                key: ProcessKey {
                    pid: Pid(1),
                    start_time: 0,
                },
            })
            .unwrap_err();
        assert!(matches!(err, ControlError::Unsupported { .. }));
    }
}
