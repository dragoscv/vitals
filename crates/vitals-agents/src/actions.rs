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
//!
//! # Whose script, with whose rights (ENG2, S21-04)
//!
//! The scripts live in the user's profile and the user can edit them, which
//! is fine while they run with the user's own rights. Three guards keep it
//! that way ([`Runner::run`]):
//!
//! - **Never elevated.** An elevated Vitals would run a file any program of
//!   this user can rewrite with administrator rights: a UAC bypass. Actions
//!   are refused while the process is elevated.
//! - **Only from the profile, only if owned.** The folder comes from the
//!   shell's profile path, never from an inherited variable, and the script
//!   must be owned by this account (or Administrators/SYSTEM).
//! - **A deadline.** A script that hangs is killed after [`TIMEOUT`] instead
//!   of holding a blocking thread, and the LAN request with it, for ever.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vitals_core::agents::AgentqAction;

/// Shortest reason accepted. agentq's own guard rejects empty reasons; a
/// two-letter one tells the other agents nothing either.
pub const MIN_REASON: usize = 4;

/// How long an action may run. agentq's own commands finish in a second or
/// two; `unlease` backs a slot up first, which on a slow push takes longer.
pub const TIMEOUT: Duration = Duration::from_secs(120);

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

/// `<profile>\.copilot\bin`, where agentq installs itself. The caller passes
/// the profile from the OS (`vitals_win::ownership::user_profile`), never
/// from `%USERPROFILE%` or `AGENTQ_BIN`, which whoever started Vitals sets.
#[must_use]
pub fn bin_in(profile: &Path) -> PathBuf {
    profile.join(".copilot").join("bin")
}

/// Output of a finished action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub ok: bool,
    pub code: Option<i32>,
    /// stdout and stderr, trimmed to the last 4 KB.
    pub output: String,
}

/// Refusal while Vitals runs as administrator (see the module docs).
pub const ELEVATED_REFUSAL: &str = "Vitals is running as administrator. agentq actions run only \
     from a normal Vitals, so a script your account can edit is never started with administrator \
     rights.";

/// Everything an action needs from the host, so the guards are testable
/// without an elevated process or a foreign account.
pub struct Runner<'a> {
    /// agentq's script folder ([`bin_in`]).
    pub bin: &'a Path,
    /// Whether this process is elevated.
    pub elevated: bool,
    /// Whether a script file may be run: owned by this account, Administrators or SYSTEM.
    pub trusted: &'a dyn Fn(&Path) -> bool,
    /// The program to start the script with; `pwsh` outside tests.
    pub shell: &'a str,
    pub timeout: Duration,
}

impl std::fmt::Debug for Runner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("bin", &self.bin)
            .field("elevated", &self.elevated)
            .field("shell", &self.shell)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Runner<'_> {
    /// Runs an action synchronously. Call from a blocking thread.
    ///
    /// # Errors
    /// Validation failures from [`plan`], an elevated process, a missing or
    /// untrusted script, the shell not starting, or the deadline passing.
    pub fn run(&self, action: &AgentqAction) -> Result<Outcome, String> {
        let inv = plan(action)?;
        if self.elevated {
            return Err(ELEVATED_REFUSAL.to_owned());
        }
        let script = self.bin.join(inv.script);
        if !script.is_file() {
            return Err(format!(
                "{} not found — is agentq installed?",
                script.display()
            ));
        }
        if !(self.trusted)(&script) {
            return Err(format!(
                "{} is not owned by your account or by administrators; not running it",
                script.display()
            ));
        }
        let mut cmd = Command::new(self.shell);
        configure(&mut cmd, &script, &inv.args);
        run_with_deadline(cmd, self.timeout)
    }
}

fn configure(cmd: &mut Command, script: &Path, args: &[String]) {
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ])
    .arg(script)
    .args(args);
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
}

/// Starts `cmd`, collects its output and kills it if `timeout` passes.
fn run_with_deadline(mut cmd: Command, timeout: Duration) -> Result<Outcome, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start pwsh: {e}"))?;
    // drained on threads so a chatty script cannot fill a pipe and stall
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "agentq did not finish within {} s and was stopped",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("waiting for pwsh: {e}")),
        }
    };
    let mut text = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    text.push_str(&String::from_utf8_lossy(&err.join().unwrap_or_default()));
    Ok(Outcome {
        ok: status.success(),
        code: status.code(),
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

    fn note() -> AgentqAction {
        AgentqAction::Note {
            message: "from the tests".into(),
        }
    }

    /// A bin folder holding an `agentq.ps1`, removed on drop.
    struct Bin(PathBuf);
    impl Bin {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("vitals-aq-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("agentq.ps1"), "exit 0").unwrap();
            Self(dir)
        }
    }
    impl Drop for Bin {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn runner<'a>(bin: &'a Path, elevated: bool, trusted: &'a dyn Fn(&Path) -> bool) -> Runner<'a> {
        Runner {
            bin,
            elevated,
            trusted,
            // never a real program: reaching the spawn shows every guard passed
            shell: "vitals-no-such-shell-for-tests",
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn an_elevated_vitals_refuses_every_agentq_action_so_a_user_writable_script_never_runs_as_admin()
     {
        let bin = Bin::new("elev");
        let asked = std::cell::Cell::new(false);
        let trusted = |_: &Path| {
            asked.set(true);
            true
        };
        let err = runner(&bin.0, true, &trusted).run(&note()).unwrap_err();
        assert_eq!(err, ELEVATED_REFUSAL);
        assert!(!asked.get(), "refused before the script is even looked at");
    }

    #[test]
    fn a_script_not_owned_by_this_account_or_administrators_is_refused() {
        let bin = Bin::new("own");
        let err = runner(&bin.0, false, &|_| false).run(&note()).unwrap_err();
        assert!(err.contains("not owned"), "{err}");
    }

    #[test]
    fn an_owned_script_in_an_unelevated_process_gets_as_far_as_starting_the_shell() {
        let bin = Bin::new("ok");
        let seen = std::cell::RefCell::new(PathBuf::new());
        let trusted = |p: &Path| {
            *seen.borrow_mut() = p.to_path_buf();
            true
        };
        let err = runner(&bin.0, false, &trusted).run(&note()).unwrap_err();
        assert!(err.starts_with("could not start pwsh"), "{err}");
        assert_eq!(*seen.borrow(), bin.0.join("agentq.ps1"));
    }

    #[test]
    fn the_script_folder_is_built_from_the_profile_passed_in_not_from_the_environment() {
        assert_eq!(
            bin_in(Path::new(r"C:\Users\me")),
            Path::new(r"C:\Users\me").join(".copilot").join("bin")
        );
    }

    #[test]
    fn a_hung_script_is_stopped_at_the_deadline_instead_of_holding_the_thread() {
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
            c
        };
        #[cfg(not(windows))]
        let cmd = {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        let started = Instant::now();
        let err = run_with_deadline(cmd, Duration::from_millis(1500)).unwrap_err();
        assert!(err.contains("did not finish"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_quick_command_returns_its_exit_code_and_output_before_the_deadline() {
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", "echo hello & exit 3"]);
            c
        };
        #[cfg(not(windows))]
        let cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", "echo hello; exit 3"]);
            c
        };
        let out = run_with_deadline(cmd, Duration::from_secs(20)).unwrap();
        assert!(!out.ok);
        assert_eq!(out.code, Some(3));
        assert!(out.output.contains("hello"), "{}", out.output);
    }
}
