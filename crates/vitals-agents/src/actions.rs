//! The AI page's writes, delegated to agentq's own CLI.
//!
//! Vitals never edits a coord file. agentq takes its named mutexes, writes
//! atomically and journals every change with the reason; re-implementing that
//! here would make a second writer that drifts from the first. So an action
//! is turned into an argument vector for `agentq.ps1` / `jobs.ps1` /
//! `reap-terminals.ps1` and run with `pwsh -NoProfile -File`.
//!
//! Arguments are passed as a vector, never a joined string, so a reason
//! containing quotes or `;` cannot become a second command.

use std::path::{Path, PathBuf};
use std::process::Command;

use vitals_core::agents::AgentqAction;

/// Shortest reason accepted. agentq's own guard rejects empty reasons; a
/// two-letter one tells the other agents nothing either.
pub const MIN_REASON: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub script: &'static str,
    pub args: Vec<String>,
}

/// Validates an action and builds its invocation.
///
/// # Errors
/// A short reason, an empty identifier, or a resource name with characters
/// agentq never produces.
pub fn plan(action: &AgentqAction) -> Result<Invocation, String> {
    Ok(match action {
        AgentqAction::Freeze { resource, reason }
        | AgentqAction::Block { resource, reason }
        | AgentqAction::Clear { resource, reason } => {
            ident_ok(resource, "resource")?;
            reason_ok(reason)?;
            let state = match action {
                AgentqAction::Freeze { .. } => "frozen",
                AgentqAction::Block { .. } => "blocked",
                _ => "clear",
            };
            agentq(&[
                "mark",
                "-Resource",
                resource,
                "-State",
                state,
                "-Reason",
                reason.trim(),
            ])
        }
        AgentqAction::Break {
            resource,
            ticket,
            reason,
        } => {
            ident_ok(resource, "resource")?;
            ident_ok(ticket, "ticket")?;
            reason_ok(reason)?;
            agentq(&[
                "break",
                "-Resource",
                resource,
                "-Id",
                ticket,
                "-Reason",
                reason.trim(),
            ])
        }
        AgentqAction::Note { message } => {
            reason_ok(message)?;
            agentq(&["note", "-Message", message.trim()])
        }
        AgentqAction::Unlease {
            path,
            lease_id,
            reason,
        } => {
            slot_path_ok(path)?;
            ident_ok(lease_id, "lease id")?;
            reason_ok(reason)?;
            agentq(&[
                "unlease",
                "-Repo",
                path,
                "-LeaseId",
                lease_id,
                "-Reason",
                reason.trim(),
            ])
        }
        AgentqAction::KillJob { job } => {
            ident_ok(job, "job id")?;
            Invocation {
                script: "jobs.ps1",
                args: owned(&["kill", job]),
            }
        }
        AgentqAction::ReapTerminals { dry_run } => {
            let mut args = owned(&["-Json"]);
            if *dry_run {
                args.push("-WhatIf".into());
            }
            Invocation {
                script: "reap-terminals.ps1",
                args,
            }
        }
    })
}

fn reason_ok(reason: &str) -> Result<(), String> {
    if reason.trim().chars().count() < MIN_REASON {
        Err(format!("reason must be at least {MIN_REASON} characters"))
    } else {
        Ok(())
    }
}

/// agentq never produces anything outside `[A-Za-z0-9-_.:]` in resource,
/// ticket, slot or job names; anything else is refused before it reaches pwsh.
fn ident_ok(value: &str, what: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c));
    if valid {
        Ok(())
    } else {
        Err(format!("invalid {what}"))
    }
}

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

/// A worktree slot lives at `<root>\.wt\<repo>\<slot>`; anything else is not a
/// slot agentq leased, so it is refused rather than handed to `unlease`.
fn slot_path_ok(path: &str) -> Result<(), String> {
    let norm = path.replace('/', "\\");
    let parts: Vec<&str> = norm.split('\\').filter(|p| !p.is_empty()).collect();
    let wt = parts.iter().position(|p| p.eq_ignore_ascii_case(".wt"));
    let ok = match wt {
        Some(i) => {
            parts.len() == i + 3 && parts[i + 1..].iter().all(|p| ident_ok(p, "slot").is_ok())
        }
        None => false,
    };
    if ok && !norm.contains("..") {
        Ok(())
    } else {
        Err("invalid slot path".into())
    }
}

fn agentq(args: &[&str]) -> Invocation {
    Invocation {
        script: "agentq.ps1",
        args: owned(args),
    }
}

/// The last `max` bytes of `text`, cut on a character boundary.
fn tail(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// `%USERPROFILE%\.copilot\bin`, where agentq installs itself.
#[must_use]
pub fn default_bin() -> Option<PathBuf> {
    if let Some(b) = std::env::var_os("AGENTQ_BIN") {
        return Some(PathBuf::from(b));
    }
    let home = std::env::var_os("USERPROFILE")?;
    Some(PathBuf::from(home).join(".copilot").join("bin"))
}

/// Output of a finished action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub ok: bool,
    pub code: Option<i32>,
    /// stdout and stderr, trimmed to the last 4 KB.
    pub output: String,
}

/// Runs an action synchronously. Call from a blocking thread.
///
/// # Errors
/// Validation failures from [`plan`], a missing script, or pwsh not starting.
pub fn run(action: &AgentqAction, bin: &Path) -> Result<Outcome, String> {
    let inv = plan(action)?;
    let script = bin.join(inv.script);
    if !script.is_file() {
        return Err(format!(
            "{} not found — is agentq installed?",
            script.display()
        ));
    }
    let mut cmd = Command::new("pwsh");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ])
    .arg(&script)
    .args(&inv.args);
    // the journal names who did it
    cmd.env("AGENTQ_SESSION", "vitals")
        .env("AGENTQ_CLIENT", "vitals")
        .env("AGENTQ_NO_REGISTRY", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("could not start pwsh: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(Outcome {
        ok: out.status.success(),
        code: out.status.code(),
        output: tail(text.trim(), 4096).to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reason_with_shell_metacharacters_stays_one_argument() {
        let inv = plan(&AgentqAction::Freeze {
            resource: "build:codai".into(),
            reason: "incident; rm -rf / \"x\"".into(),
        })
        .unwrap();
        assert_eq!(
            inv.args.last().map(String::as_str),
            Some("incident; rm -rf / \"x\"")
        );
        assert_eq!(inv.args.len(), 7);
    }

    #[test]
    fn a_short_reason_and_an_odd_resource_are_refused_before_anything_runs() {
        assert!(
            plan(&AgentqAction::Clear {
                resource: "build:codai".into(),
                reason: "ok".into()
            })
            .is_err()
        );
        assert!(
            plan(&AgentqAction::Break {
                resource: "build:codai".into(),
                ticket: "a b".into(),
                reason: "stale holder".into()
            })
            .is_err()
        );
        assert!(
            plan(&AgentqAction::Freeze {
                resource: "../x".into(),
                reason: "incident".into()
            })
            .is_err()
        );
    }

    #[test]
    fn break_always_names_the_ticket_so_a_live_holder_is_never_hit_by_accident() {
        let inv = plan(&AgentqAction::Break {
            resource: "build:machine".into(),
            ticket: "ccb73957".into(),
            reason: "pid dead".into(),
        })
        .unwrap();
        assert!(
            inv.args
                .windows(2)
                .any(|w| w[0] == "-Id" && w[1] == "ccb73957")
        );
    }

    #[test]
    fn the_reaper_dry_run_is_what_preview_sends() {
        let inv = plan(&AgentqAction::ReapTerminals { dry_run: true }).unwrap();
        assert_eq!(inv.script, "reap-terminals.ps1");
        assert!(inv.args.contains(&"-WhatIf".to_owned()));
    }

    #[test]
    fn unlease_accepts_only_a_worktree_slot_directory() {
        let mk = |p: &str| AgentqAction::Unlease {
            path: p.into(),
            lease_id: "05c26c29".into(),
            reason: "owner gone".into(),
        };
        assert!(plan(&mk(r"E:\gh\.wt\codai\task-3")).is_ok());
        assert!(
            plan(&mk(r"E:\gh\codai")).is_err(),
            "the shared clone is not a slot"
        );
        assert!(plan(&mk(r"E:\gh\.wt\codai\task-3\..\..\x")).is_err());
        assert!(plan(&mk(r"E:\gh\.wt\codai")).is_err());
    }
}
