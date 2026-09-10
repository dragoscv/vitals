//! `vitals eco <pid> [--off]` — Windows 11 efficiency mode for one process.
//!
//! The one process *action* the CLI issues. It takes a bare PID because that
//! is what a person has in front of them, but never acts on one: the PID is
//! resolved to a full `ProcessKey` (PID + start time) from a fresh sample
//! first, and a PID that is not in the sample is refused. That is the same
//! rule the phone and the desktop follow — a recycled PID must not be
//! throttled because its predecessor was slow.

use anyhow::{Context, Result};
use serde::Serialize;
use vitals_core::ids::{Pid, ProcessKey};
use vitals_core::process::Process;
use vitals_server::ControlRequest;

use crate::source::Source;

/// Finds the key for `pid` in a process list, or explains why not.
///
/// # Errors
/// The PID is not in the list — it has exited, never existed, or is the idle
/// process the sampler omits.
pub fn resolve(list: &[Process], pid: u32) -> Result<ProcessKey> {
    list.iter()
        .find(|p| p.key.pid.get() == pid)
        .map(|p| p.key)
        .with_context(|| format!("no process with PID {pid} in the current sample"))
}

/// The request the command sends, built separately so its shape is testable
/// without a socket.
#[must_use]
pub fn request(key: ProcessKey, enabled: bool) -> ControlRequest {
    ControlRequest::SetEfficiencyMode { key, enabled }
}

/// What `--json` prints. `efficiencyMode` is `null` when the state could not
/// be read back — a remote host, or a process that denies a handle — because
/// "we could not look" and "it is off" are different facts.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub pid: Pid,
    pub start_time: u64,
    pub efficiency_mode: Option<bool>,
}

/// # Errors
/// The PID is not running, the host refused, or this platform cannot act.
pub fn run(source: &mut Source, pid: u32, enabled: bool, json: bool) -> Result<()> {
    let view = source.snapshot()?;
    let key = resolve(&view.processes(), pid)?;
    source.control(request(key, enabled))?;

    let state = source.efficiency_mode(key)?;
    let outcome = Outcome {
        pid: key.pid,
        start_time: key.start_time,
        efficiency_mode: state,
    };

    if json {
        println!("{}", serde_json::to_string(&outcome)?);
    } else {
        let shown = match state {
            Some(true) => "on",
            Some(false) => "off",
            None => "applied (state could not be read back)",
        };
        println!("efficiency mode for PID {pid}: {shown}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::fixtures;

    #[test]
    fn a_pid_missing_from_the_sample_is_refused_rather_than_guessed() {
        let list = vec![fixtures::process("code.exe", 10, 1.0)];
        let error = resolve(&list, 11).unwrap_err();
        assert!(error.to_string().contains("PID 11"), "{error}");
    }

    #[test]
    fn the_resolved_key_carries_the_sampled_start_time_not_just_the_pid() {
        let mut p = fixtures::process("code.exe", 10, 1.0);
        p.key.start_time = 133_724_800_000_000_000;
        let key = resolve(&[p], 10).unwrap();
        assert_eq!(key.pid.get(), 10);
        assert_eq!(key.start_time, 133_724_800_000_000_000);
    }

    #[test]
    fn the_control_body_is_the_kebab_case_tag_the_server_parses() {
        let key = ProcessKey {
            pid: Pid(42),
            start_time: 7,
        };
        let json = serde_json::to_value(request(key, false)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "action": "set-efficiency-mode",
                "key": { "pid": 42, "startTime": 7 },
                "enabled": false
            })
        );
    }

    #[test]
    fn json_output_keeps_an_unreadable_state_as_null_not_false() {
        let out = Outcome {
            pid: Pid(42),
            start_time: 7,
            efficiency_mode: None,
        };
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"{"pid":42,"startTime":7,"efficiencyMode":null}"#
        );
    }
}
