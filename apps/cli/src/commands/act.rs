//! `vitals kill | suspend | resume | priority` — the process actions.
//!
//! Same rule as `eco`: a person types a bare PID, but nothing acts on one.
//! The PID is resolved to a full `ProcessKey` from a fresh sample and the
//! action carries that key, so the platform layer re-checks the start time
//! against the live process and a recycled PID is refused rather than hit.
//!
//! None of these is ever "confirmed" in the risk model's sense. The desktop
//! shows a dialog before ending a critical process; a script cannot see one,
//! so a critical or protected target is refused here exactly as it is for
//! the phone.

use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;
use vitals_core::ids::{Pid, ProcessKey};
use vitals_server::ControlRequest;

use super::eco::resolve;
use crate::source::Source;

/// Scheduling priority, spelt as the control endpoint parses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Level {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    /// Starves input handling; a remote host refuses it.
    Realtime,
}

impl Level {
    /// The wire string, identical to clap's spelling.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::BelowNormal => "below-normal",
            Self::Normal => "normal",
            Self::AboveNormal => "above-normal",
            Self::High => "high",
            Self::Realtime => "realtime",
        }
    }
}

/// What the user asked for, before it has a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Kill,
    Suspend,
    Resume,
    Priority(Level),
}

impl Action {
    /// The tag `--json` prints and the human line names.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Kill => "terminate",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
            Self::Priority(_) => "set-priority",
        }
    }

    /// The request for `key`, built separately so its shape is testable.
    #[must_use]
    pub fn request(self, key: ProcessKey) -> ControlRequest {
        match self {
            Self::Kill => ControlRequest::Terminate { key },
            Self::Suspend => ControlRequest::Suspend { key },
            Self::Resume => ControlRequest::Resume { key },
            Self::Priority(level) => ControlRequest::SetPriority {
                key,
                priority: level.wire().to_owned(),
            },
        }
    }
}

/// What `--json` prints on success. Failures exit non-zero with the reason
/// on stderr, so a script needs only the exit code.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub pid: Pid,
    pub start_time: u64,
    pub action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<&'static str>,
}

/// # Errors
/// The PID is not running, the host or Windows refused, or this platform
/// cannot act.
pub fn run(source: &mut Source, pid: u32, action: Action, json: bool) -> Result<()> {
    let view = source.snapshot()?;
    let key = resolve(&view.processes(), pid)?;
    source.control(action.request(key))?;

    let outcome = Outcome {
        pid: key.pid,
        start_time: key.start_time,
        action: action.name(),
        priority: match action {
            Action::Priority(level) => Some(level.wire()),
            _ => None,
        },
    };
    if json {
        println!("{}", serde_json::to_string(&outcome)?);
    } else {
        let done = match action {
            Action::Kill => "ended".to_owned(),
            Action::Suspend => "suspended".to_owned(),
            Action::Resume => "resumed".to_owned(),
            Action::Priority(level) => format!("priority set to {}", level.wire()),
        };
        println!("PID {pid}: {done}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: ProcessKey = ProcessKey {
        pid: Pid(42),
        start_time: 7,
    };

    #[test]
    fn every_action_serialises_to_the_tag_the_server_parses_with_the_full_key() {
        let cases = [
            (
                Action::Kill,
                serde_json::json!({"action":"terminate","key":{"pid":42,"startTime":7}}),
            ),
            (
                Action::Suspend,
                serde_json::json!({"action":"suspend","key":{"pid":42,"startTime":7}}),
            ),
            (
                Action::Resume,
                serde_json::json!({"action":"resume","key":{"pid":42,"startTime":7}}),
            ),
            (
                Action::Priority(Level::BelowNormal),
                serde_json::json!({"action":"set-priority","key":{"pid":42,"startTime":7},"priority":"below-normal"}),
            ),
        ];
        for (action, expected) in cases {
            let json = serde_json::to_value(action.request(KEY)).unwrap();
            assert_eq!(json, expected, "{action:?}");
        }
    }

    #[test]
    fn the_priority_spelling_clap_accepts_is_the_spelling_the_server_parses() {
        // A mismatch here is a CLI that accepts `--help`-listed values the
        // host then refuses as "unknown priority".
        for level in Level::value_variants() {
            let shown = level.to_possible_value().unwrap();
            assert_eq!(shown.get_name(), level.wire());
        }
    }

    #[test]
    fn json_output_names_the_priority_only_for_a_priority_change() {
        let kill = Outcome {
            pid: Pid(42),
            start_time: 7,
            action: Action::Kill.name(),
            priority: None,
        };
        assert_eq!(
            serde_json::to_string(&kill).unwrap(),
            r#"{"pid":42,"startTime":7,"action":"terminate"}"#
        );
    }
}
