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

use vitals_core::agents::{AgentqAction, AgentqStatus};

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
    /// Working directory for the script. agentq resolves the repository from
    /// where it runs, so `land` and `commit` must be started inside the clone
    /// they act on; `run` needs it only for a cloud patch upload.
    pub cwd: Option<String>,
}

/// Validates an action and builds its invocation.
///
/// # Errors
/// A short reason, an empty identifier, or a resource name with characters
/// agentq never produces.
///
/// One arm per verb, each delegating to the helper that owns the validation
/// that verb needs. The split is not cosmetic: this was one function until the
/// fifteen verbs pushed it past the line budget in `clippy.toml`, and a group
/// that grows again should move out the same way.
pub fn plan(action: &AgentqAction) -> Result<Invocation, String> {
    match action {
        AgentqAction::Freeze { resource, reason } => plan_mark(resource, reason, "frozen"),
        AgentqAction::Block { resource, reason } => plan_mark(resource, reason, "blocked"),
        AgentqAction::Clear { resource, reason } => plan_mark(resource, reason, "clear"),
        AgentqAction::Break {
            resource,
            ticket,
            reason,
        } => plan_break(resource, ticket, reason),
        AgentqAction::Note { message } => plan_note(message),
        AgentqAction::Unlease {
            path,
            lease_id,
            reason,
        } => plan_unlease(path, lease_id, reason),
        AgentqAction::KillJob { job } => plan_kill_job(job),
        AgentqAction::ReapTerminals { dry_run } => Ok(plan_reap_terminals(*dry_run)),
        AgentqAction::Propose { reason } => plan_propose(reason.as_deref()),
        AgentqAction::Sweep { dry_run } => Ok(plan_sweep(*dry_run)),
        AgentqAction::Bump { ticket, by, reason } => plan_bump(ticket.as_deref(), *by, reason),
        AgentqAction::Burst {
            resource,
            by,
            minutes,
            reason,
        } => plan_burst(resource.as_deref(), *by, *minutes, reason),
        AgentqAction::Land {
            repo,
            branch,
            onto,
            purpose,
            gate,
        } => plan_land(repo, branch, onto, purpose, gate.as_deref()),
        AgentqAction::Run {
            resource,
            purpose,
            command,
            lane,
            place,
            repo,
        } => plan_run(
            resource,
            purpose,
            command.as_deref(),
            lane.as_deref(),
            place.as_deref(),
            repo.as_deref(),
        ),
        AgentqAction::Commit {
            repo,
            purpose,
            message,
            paths,
        } => plan_commit(repo, purpose, message, paths),
    }
}

/// `mark` — freeze, block and clear are one agentq call with a different state.
fn plan_mark(resource: &str, reason: &str, state: &str) -> Result<Invocation, String> {
    ident_ok(resource, "resource")?;
    reason_ok(reason)?;
    Ok(agentq(&[
        "mark",
        "-Resource",
        resource,
        "-State",
        state,
        "-Reason",
        reason.trim(),
    ]))
}

fn plan_break(resource: &str, ticket: &str, reason: &str) -> Result<Invocation, String> {
    ident_ok(resource, "resource")?;
    ident_ok(ticket, "ticket")?;
    reason_ok(reason)?;
    Ok(agentq(&[
        "break",
        "-Resource",
        resource,
        "-Id",
        ticket,
        "-Reason",
        reason.trim(),
    ]))
}

fn plan_note(message: &str) -> Result<Invocation, String> {
    reason_ok(message)?;
    Ok(agentq(&["note", "-Message", message.trim()]))
}

fn plan_unlease(path: &str, lease_id: &str, reason: &str) -> Result<Invocation, String> {
    slot_path_ok(path)?;
    ident_ok(lease_id, "lease id")?;
    reason_ok(reason)?;
    Ok(agentq(&[
        "unlease",
        "-Repo",
        path,
        "-LeaseId",
        lease_id,
        "-Reason",
        reason.trim(),
    ]))
}

fn plan_kill_job(job: &str) -> Result<Invocation, String> {
    ident_ok(job, "job id")?;
    Ok(Invocation {
        script: "jobs.ps1",
        args: owned(&["kill", job]),
        cwd: None,
    })
}

/// Built from literals, so nothing here can be refused: no validation, and the
/// return type says so.
fn plan_reap_terminals(dry_run: bool) -> Invocation {
    let mut args = owned(&["-Json"]);
    if dry_run {
        args.push("-WhatIf".into());
    }
    Invocation {
        script: "reap-terminals.ps1",
        args,
        cwd: None,
    }
}

fn plan_propose(reason: Option<&str>) -> Result<Invocation, String> {
    let mut args = owned(&["propose"]);
    // The reason is optional here: agentq's own diagnosis is the point, and an
    // empty `-Reason` tells the other agents nothing.
    if let Some(reason) = reason.map(str::trim).filter(|r| !r.is_empty()) {
        reason_ok(reason)?;
        args.push("-Reason".into());
        args.push(reason.to_owned());
    }
    Ok(Invocation {
        script: "agentq.ps1",
        args,
        cwd: None,
    })
}

/// No validation either: `sweep` takes no argument but its own dry-run flag.
fn plan_sweep(dry_run: bool) -> Invocation {
    let mut args = owned(&["sweep", "-Json"]);
    if dry_run {
        args.push("-WhatIf".into());
    }
    Invocation {
        script: "agentq.ps1",
        args,
        cwd: None,
    }
}

fn plan_bump(ticket: Option<&str>, by: i32, reason: &str) -> Result<Invocation, String> {
    reason_ok(reason)?;
    let mut args = owned(&["bump"]);
    if let Some(ticket) = ticket.filter(|t| !t.is_empty()) {
        ident_ok(ticket, "ticket")?;
        args.push("-Id".into());
        args.push(ticket.to_owned());
    }
    // Zero is refused rather than passed through: it changes no order, so it is
    // always a form that was filled in wrongly.
    if by == 0 || !(-50..=50).contains(&by) {
        return Err("bump size must be between -50 and 50, and not zero".into());
    }
    args.push("-By".into());
    args.push(by.to_string());
    args.push("-Reason".into());
    args.push(reason.trim().to_owned());
    Ok(Invocation {
        script: "agentq.ps1",
        args,
        cwd: None,
    })
}

fn plan_burst(
    resource: Option<&str>,
    by: i32,
    minutes: i32,
    reason: &str,
) -> Result<Invocation, String> {
    reason_ok(reason)?;
    let mut args = owned(&["burst"]);
    if let Some(resource) = resource.filter(|r| !r.is_empty()) {
        ident_ok(resource, "resource")?;
        args.push("-Resource".into());
        args.push(resource.to_owned());
    }
    if !(1..=8).contains(&by) {
        return Err("burst size must be between 1 and 8".into());
    }
    if !(1..=1440).contains(&minutes) {
        return Err("burst must last between 1 and 1440 minutes".into());
    }
    args.push("-By".into());
    args.push(by.to_string());
    args.push("-Minutes".into());
    args.push(minutes.to_string());
    args.push("-Reason".into());
    args.push(reason.trim().to_owned());
    Ok(Invocation {
        script: "agentq.ps1",
        args,
        cwd: None,
    })
}

fn plan_land(
    repo: &str,
    branch: &str,
    onto: &str,
    purpose: &str,
    gate: Option<&str>,
) -> Result<Invocation, String> {
    repo_ok(repo, "repository")?;
    branch_ok(branch, "branch")?;
    if !matches!(onto, "main" | "dev") {
        return Err("land target must be main or dev".into());
    }
    reason_ok(purpose)?;
    let mut args = owned(&["land", "-Branch"]);
    args.push(branch.to_owned());
    args.push("-Onto".into());
    args.push(onto.to_owned());
    args.push("-Purpose".into());
    args.push(purpose.trim().to_owned());
    if let Some(gate) = gate.map(str::trim).filter(|g| !g.is_empty()) {
        args.push("-Gate".into());
        args.push(gate.to_owned());
    }
    Ok(Invocation {
        script: "agentq.ps1",
        args,
        cwd: Some(repo.to_owned()),
    })
}

fn plan_run(
    resource: &str,
    purpose: &str,
    command: Option<&str>,
    lane: Option<&str>,
    place: Option<&str>,
    repo: Option<&str>,
) -> Result<Invocation, String> {
    ident_ok(resource, "resource")?;
    reason_ok(purpose)?;
    let lane = lane.filter(|l| !l.is_empty());
    if let Some(lane) = lane {
        if !matches!(lane, "interactive" | "quick" | "gate" | "heavy" | "batch") {
            return Err("unknown lane".into());
        }
        ident_ok(lane, "lane")?;
    }
    let place = place.filter(|p| !p.is_empty());
    if let Some(place) = place
        && !matches!(place, "local" | "cloud" | "auto")
    {
        return Err("run destination must be local, cloud or auto".into());
    }
    let repo = repo.filter(|r| !r.is_empty());
    if let Some(repo) = repo {
        repo_ok(repo, "repository")?;
    }
    let mut args = owned(&["run", "-Resource"]);
    args.push(resource.to_owned());
    args.push("-Purpose".into());
    args.push(purpose.trim().to_owned());
    if let Some(lane) = lane {
        args.push("-Lane".into());
        args.push(lane.to_owned());
    }
    if let Some(place) = place {
        args.push("-Where".into());
        args.push(place.to_owned());
    }
    // `--` separates agentq's own flags from the command it runs, and the
    // command is one argv element, so a `;` in it stays literal.
    if let Some(command) = command.map(str::trim).filter(|c| !c.is_empty()) {
        args.push("--".into());
        args.push(command.to_owned());
    }
    Ok(Invocation {
        script: "agentq.ps1",
        args,
        cwd: repo.map(str::to_owned),
    })
}

fn plan_commit(
    repo: &str,
    purpose: &str,
    message: &str,
    paths: &[String],
) -> Result<Invocation, String> {
    repo_ok(repo, "repository")?;
    reason_ok(purpose)?;
    if message.trim().is_empty() {
        return Err("commit message is required".into());
    }
    let paths: Vec<String> = paths
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| {
            if p.contains(',') || p.contains(['\n', '\r']) || p.len() > 512 {
                Err("invalid path".to_owned())
            } else {
                Ok(p.to_owned())
            }
        })
        .collect::<Result<_, _>>()?;
    if paths.is_empty() {
        return Err("at least one path is required".into());
    }
    Ok(Invocation {
        script: "agentq.ps1",
        args: vec![
            "commit".into(),
            "-Purpose".into(),
            purpose.trim().to_owned(),
            "-Message".into(),
            message.trim().to_owned(),
            "-Paths".into(),
            paths.join(","),
            "-ExpectRepo".into(),
            repo.to_owned(),
        ],
        cwd: Some(repo.to_owned()),
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
        cwd: None,
    }
}

/// A repository directory an agentq verb may be started in.
///
/// Existence is checked when the command runs ([`Runner::run`]); this only
/// refuses a value that could not be a path, so a stray quote, a newline or a
/// `..` never reaches the shell.
fn repo_ok(value: &str, what: &str) -> Result<(), String> {
    let t = value.trim();
    let ok = !t.is_empty()
        && t.len() <= 512
        && !t.contains(['\n', '\r'])
        && !t.contains("..")
        && (t.contains('\\') || t.contains('/'));
    if ok {
        Ok(())
    } else {
        Err(format!("invalid {what}"))
    }
}

/// A git ref name.
///
/// [`ident_ok`] is too strict here — every topic branch this machine lands
/// starts with `feat/` or `publish/` — and too loose in the other direction:
/// a leading `-` is an option to `git`, `..` is a revision range, and the
/// rest are characters git itself refuses in a ref.
fn branch_ok(value: &str, what: &str) -> Result<(), String> {
    let t = value.trim();
    let clean = !t.is_empty()
        && t.len() <= 128
        && !t.starts_with('-')
        && !t.contains("..")
        && !t.contains([
            ' ', '\t', '\n', '\r', '~', '^', ':', '?', '*', '\\', '@', '{', '}',
        ]);
    let segments = t.split('/');
    let ok = clean
        && segments
            .clone()
            .all(|p| !p.is_empty() && !p.starts_with('.') && p != ".lock")
        && !t.ends_with('/');
    if ok {
        Ok(())
    } else {
        Err(format!("invalid {what}"))
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

/// Probes the machine for agentq, so the AI page knows whether it is worth
/// showing and which actions could work.
///
/// `profile` comes from the OS (`vitals_win::ownership::user_profile`), never
/// from the environment (see [`bin_in`]). `script` alone is not enough: a
/// script with no state directory is an install that has never run.
#[must_use]
pub fn status(profile: Option<&Path>) -> AgentqStatus {
    AgentqStatus {
        script: profile.is_some_and(|p| bin_in(p).join("agentq.ps1").is_file()),
        coord: crate::agentq::default_root().is_some_and(|r| r.is_dir()),
        clients: crate::sessions::Roots::detect().any_present(),
        windows: cfg!(windows),
    }
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
        if let Some(dir) = &inv.cwd {
            if !Path::new(dir).is_dir() {
                return Err(format!("{dir} is not a directory"));
            }
            cmd.current_dir(dir);
        }
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

    #[test]
    fn propose_without_a_reason_sends_no_empty_flag() {
        let inv = plan(&AgentqAction::Propose { reason: None }).unwrap();
        assert_eq!(inv.args, vec!["propose"]);
        assert_eq!(
            plan(&AgentqAction::Propose {
                reason: Some("   ".into())
            })
            .unwrap()
            .args,
            vec!["propose"],
            "whitespace is not a reason"
        );
        let with = plan(&AgentqAction::Propose {
            reason: Some("waited 40 min; check the holder".into()),
        })
        .unwrap();
        assert_eq!(
            with.args,
            vec!["propose", "-Reason", "waited 40 min; check the holder"]
        );
    }

    #[test]
    fn only_the_preview_sweep_is_a_dry_run() {
        assert!(
            plan(&AgentqAction::Sweep { dry_run: true })
                .unwrap()
                .args
                .contains(&"-WhatIf".to_owned())
        );
        assert!(
            !plan(&AgentqAction::Sweep { dry_run: false })
                .unwrap()
                .args
                .contains(&"-WhatIf".to_owned()),
            "a real sweep must not be a preview"
        );
    }

    #[test]
    fn land_runs_inside_the_clone_it_names_and_only_onto_main_or_dev() {
        let action = |repo: &str, branch: &str, onto: &str| AgentqAction::Land {
            repo: repo.into(),
            branch: branch.into(),
            onto: onto.into(),
            purpose: "ship the fix".into(),
            gate: None,
        };
        let inv = plan(&action(r"E:\gh\vitals", "feat/agents", "main")).unwrap();
        assert_eq!(inv.cwd.as_deref(), Some(r"E:\gh\vitals"));
        assert!(
            inv.args
                .windows(2)
                .any(|w| w[0] == "-Branch" && w[1] == "feat/agents")
        );
        assert!(
            inv.args
                .windows(2)
                .any(|w| w[0] == "-Onto" && w[1] == "main")
        );
        assert!(!inv.args.contains(&"-Gate".to_owned()));

        assert!(plan(&action(r"E:\gh\vitals", "feat/x", "staging")).is_err());
        assert!(plan(&action(r"E:\gh\vitals", "feat x", "main")).is_err());
        assert!(
            plan(&action("vitals", "feat/x", "main")).is_err(),
            "a bare name is not a clone"
        );
    }

    #[test]
    fn land_passes_a_gate_as_one_argument_so_its_shell_metacharacters_stay_literal() {
        let inv = plan(&AgentqAction::Land {
            repo: r"E:\gh\vitals".into(),
            branch: "feat/x".into(),
            onto: "dev".into(),
            purpose: "gate it".into(),
            gate: Some("pnpm verify; echo done".into()),
        })
        .unwrap();
        assert_eq!(
            inv.args.last().map(String::as_str),
            Some("pnpm verify; echo done")
        );
    }

    #[test]
    fn run_puts_the_command_behind_the_separator_as_one_argument() {
        let inv = plan(&AgentqAction::Run {
            resource: "build:codai".into(),
            purpose: "typecheck".into(),
            command: Some("pnpm typecheck; exit 0".into()),
            lane: Some("gate".into()),
            place: Some("cloud".into()),
            repo: Some(r"E:\gh\.wt\codai\task-2".into()),
        })
        .unwrap();
        let sep = inv.args.iter().position(|a| a == "--").expect("separator");
        assert_eq!(inv.args[sep + 1..], ["pnpm typecheck; exit 0"]);
        assert_eq!(inv.args.len(), sep + 2, "nothing may follow the command");
        assert!(
            inv.args
                .windows(2)
                .any(|w| w[0] == "-Lane" && w[1] == "gate")
        );
        assert!(
            inv.args
                .windows(2)
                .any(|w| w[0] == "-Where" && w[1] == "cloud")
        );
        assert_eq!(inv.cwd.as_deref(), Some(r"E:\gh\.wt\codai\task-2"));
    }

    #[test]
    fn run_refuses_a_lane_or_a_destination_agentq_does_not_have() {
        let action = |lane: Option<&str>, place: Option<&str>| AgentqAction::Run {
            resource: "build:codai".into(),
            purpose: "typecheck".into(),
            command: None,
            lane: lane.map(str::to_owned),
            place: place.map(str::to_owned),
            repo: None,
        };
        assert!(plan(&action(Some("gate"), None)).is_ok());
        assert!(plan(&action(None, Some("auto"))).is_ok());
        assert!(plan(&action(Some("turbo"), None)).is_err());
        assert!(plan(&action(None, Some("both"))).is_err());
    }

    #[test]
    fn commit_stages_exactly_the_paths_it_names_in_the_repo_it_names() {
        let inv = plan(&AgentqAction::Commit {
            repo: r"E:\gh\vitals".into(),
            purpose: "fix the parser".into(),
            message: "fix(agents): accept dryRun".into(),
            paths: vec!["crates/vitals-core/src/agents.rs".into(), "a.ts".into()],
        })
        .unwrap();
        assert_eq!(inv.cwd.as_deref(), Some(r"E:\gh\vitals"));
        assert!(
            inv.args
                .windows(2)
                .any(|w| w[0] == "-Paths" && w[1] == "crates/vitals-core/src/agents.rs,a.ts")
        );
        assert!(
            inv.args
                .windows(2)
                .any(|w| w[0] == "-ExpectRepo" && w[1] == r"E:\gh\vitals"),
            "agentq must refuse a commit from the wrong clone"
        );
        assert!(
            !inv.args.contains(&"-A".to_owned()) && !inv.args.contains(&".".to_owned()),
            "a shared clone is never staged wholesale"
        );
    }

    #[test]
    fn commit_refuses_a_path_that_would_become_two_or_an_empty_list() {
        let action = |paths: Vec<String>| AgentqAction::Commit {
            repo: r"E:\gh\vitals".into(),
            purpose: "fix the parser".into(),
            message: "fix: x".into(),
            paths,
        };
        assert!(plan(&action(vec!["a,b".into()])).is_err());
        assert!(plan(&action(vec!["a\nb".into()])).is_err());
        assert!(plan(&action(vec!["  ".into()])).is_err());
        assert!(plan(&action(vec![])).is_err());
    }

    #[test]
    fn bump_and_burst_refuse_a_size_the_guardrail_would_only_clamp() {
        let bump = |by: i32| AgentqAction::Bump {
            ticket: None,
            by,
            reason: "long wait".into(),
        };
        assert!(plan(&bump(20)).is_ok());
        assert!(plan(&bump(-5)).is_ok());
        assert!(plan(&bump(0)).is_err(), "a no-op bump is a mistake");
        assert!(plan(&bump(100)).is_err());

        let burst = |by: i32, minutes: i32| AgentqAction::Burst {
            resource: None,
            by,
            minutes,
            reason: "burst".into(),
        };
        assert!(plan(&burst(1, 30)).is_ok());
        assert!(plan(&burst(0, 30)).is_err());
        assert!(plan(&burst(9, 30)).is_err());
        assert!(plan(&burst(1, 0)).is_err());
        assert!(plan(&burst(1, 1441)).is_err());
    }

    #[test]
    fn a_bump_that_names_a_ticket_keeps_the_identity_guard() {
        assert!(
            plan(&AgentqAction::Bump {
                ticket: Some("ccb73957".into()),
                by: 10,
                reason: "long wait".into(),
            })
            .is_ok()
        );
        assert!(
            plan(&AgentqAction::Bump {
                ticket: Some("two words".into()),
                by: 10,
                reason: "long wait".into(),
            })
            .is_err()
        );
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
