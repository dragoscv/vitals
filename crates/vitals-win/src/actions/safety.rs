//! Which processes are safe to act on.
//!
//! Pure and platform-free, so the judgement is exhaustively testable without
//! risking a machine.
//!
//! ## Why this exists
//!
//! Terminating the wrong process does not show an error — it bugchecks the
//! machine or logs the user out. `csrss.exe`, `wininit.exe`, `smss.exe` and
//! `services.exe` are marked critical by the kernel: killing one triggers
//! `CRITICAL_PROCESS_DIED` immediately.
//!
//! Task Manager's own guard is a generic "this is a critical system process"
//! dialog that appears *after* you click End Task, wording every case
//! identically. That teaches people to click through it. This module
//! distinguishes what will bugcheck, what will merely break the session, and
//! what cannot be terminated at all — so the UI can say something specific
//! and true.

use vitals_core::ids::Pid;

/// How dangerous acting on a process is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    /// Ordinary user process.
    Safe,
    /// Losing it degrades the session but the machine survives — the shell,
    /// the audio service, the input stack.
    Disruptive,
    /// Terminating it bugchecks the machine or forces a logout.
    Critical,
    /// The OS will refuse regardless of privilege, so the UI must not offer
    /// to elevate. Task Manager offers it anyway, which is a dead end.
    Forbidden,
}

impl Risk {
    /// Whether the user must confirm before the action proceeds.
    #[must_use]
    pub const fn needs_confirmation(self) -> bool {
        matches!(self, Self::Disruptive | Self::Critical)
    }

    /// Whether the action can succeed at all.
    #[must_use]
    pub const fn is_possible(self) -> bool {
        !matches!(self, Self::Forbidden)
    }

    /// Whether offering "retry as administrator" makes sense.
    ///
    /// False for [`Forbidden`](Self::Forbidden): elevation cannot help, and
    /// offering it sends the user through a UAC prompt to reach the same
    /// failure.
    #[must_use]
    pub const fn elevation_might_help(self) -> bool {
        !matches!(self, Self::Forbidden)
    }
}

/// Processes the kernel marks critical.
///
/// Terminating any of these bugchecks with `CRITICAL_PROCESS_DIED`. The list
/// is a fast pre-filter for the UI; the authoritative check is the
/// `BreakOnTermination` flag queried per process, since a driver can mark
/// others at runtime.
const KERNEL_CRITICAL: &[&str] = &[
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "smss.exe",
    "lsass.exe",
    "system",
    "registry",
    "memory compression",
    "secure system",
];

/// Processes whose loss breaks the session without killing the machine.
///
/// Worth a specific warning rather than a generic one: "this will close your
/// desktop and all open windows" is actionable, "this is a critical system
/// process" is not.
const SESSION_CRITICAL: &[&str] = &[
    "explorer.exe",
    "dwm.exe",
    "sihost.exe",
    "ctfmon.exe",
    "fontdrvhost.exe",
    "audiodg.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
];

/// The idle process. Not a real process; nothing can be done to it.
const IDLE_PID: u32 = 0;

/// The System process. Kernel-mode, not terminable from user mode.
const SYSTEM_PID: u32 = 4;

/// Inputs to a risk assessment.
#[derive(Debug, Clone, Copy)]
pub struct ProcessFacts<'a> {
    pub pid: Pid,
    pub name: Option<&'a str>,
    /// The kernel's `BreakOnTermination` flag, when it could be read.
    ///
    /// Authoritative when present: a driver can mark a process critical at
    /// runtime, and no static list will know about it.
    pub break_on_termination: Option<bool>,
    /// Protected Process Light or Full.
    pub protected: bool,
    /// Whether this is our own process.
    pub is_self: bool,
}

/// Classifies how risky terminating a process is.
#[must_use]
pub fn assess_termination(facts: ProcessFacts<'_>) -> Risk {
    // Pseudo-processes first: PID 0 and 4 are not user-mode processes and no
    // amount of privilege changes that.
    if facts.pid.get() == IDLE_PID || facts.pid.get() == SYSTEM_PID {
        return Risk::Forbidden;
    }

    // Protected processes reject termination from user mode at any privilege
    // level. Offering elevation here is the dead end Task Manager walks users
    // into.
    if facts.protected {
        return Risk::Forbidden;
    }

    // The kernel's own flag beats any list we maintain.
    if facts.break_on_termination == Some(true) {
        return Risk::Critical;
    }

    let Some(name) = facts.name else {
        // An unnamed process is usually protected or exiting. Treat unknowns
        // as disruptive rather than safe: the cost of a needless prompt is
        // far lower than the cost of an unwarned bugcheck.
        return Risk::Disruptive;
    };

    let lower = name.to_ascii_lowercase();

    if KERNEL_CRITICAL.contains(&lower.as_str()) {
        return Risk::Critical;
    }

    if SESSION_CRITICAL.contains(&lower.as_str()) {
        return Risk::Disruptive;
    }

    // Terminating ourselves works but takes the UI with it, so it deserves a
    // confirmation rather than silently closing the app.
    if facts.is_self {
        return Risk::Disruptive;
    }

    Risk::Safe
}

/// Classifies suspending a process.
///
/// Suspension is reversible and so less dangerous than termination — but
/// suspending the wrong system process deadlocks the machine just as
/// thoroughly, and with no error message. Freezing `csrss.exe` hangs every
/// window in the session with no way back except a hard reset.
#[must_use]
pub fn assess_suspension(facts: ProcessFacts<'_>) -> Risk {
    if facts.pid.get() == IDLE_PID || facts.pid.get() == SYSTEM_PID || facts.protected {
        return Risk::Forbidden;
    }

    // Suspending ourselves freezes the UI with no way to unfreeze it, since
    // the thread that would resume us is the one being suspended.
    if facts.is_self {
        return Risk::Forbidden;
    }

    if facts.break_on_termination == Some(true) {
        return Risk::Critical;
    }

    let Some(name) = facts.name else {
        return Risk::Disruptive;
    };

    let lower = name.to_ascii_lowercase();

    if KERNEL_CRITICAL.contains(&lower.as_str()) {
        return Risk::Critical;
    }

    if SESSION_CRITICAL.contains(&lower.as_str()) {
        return Risk::Disruptive;
    }

    Risk::Safe
}

/// A specific, honest explanation of the consequence.
///
/// Returned as a translation key rather than English so the UI stays
/// localised. A generic "are you sure?" trains people to click through.
#[must_use]
pub const fn consequence_key(risk: Risk) -> &'static str {
    match risk {
        Risk::Safe => "process.confirm.safe",
        Risk::Disruptive => "process.confirm.disruptive",
        Risk::Critical => "process.confirm.critical",
        Risk::Forbidden => "process.confirm.forbidden",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(pid: u32, name: &str) -> ProcessFacts<'_> {
        ProcessFacts {
            pid: Pid(pid),
            name: Some(name),
            break_on_termination: Some(false),
            protected: false,
            is_self: false,
        }
    }

    #[test]
    fn an_ordinary_process_is_safe() {
        assert_eq!(assess_termination(facts(1234, "notepad.exe")), Risk::Safe);
    }

    #[test]
    fn the_idle_process_cannot_be_touched() {
        assert_eq!(
            assess_termination(facts(0, "System Idle Process")),
            Risk::Forbidden
        );
    }

    #[test]
    fn the_system_process_cannot_be_terminated_from_user_mode() {
        assert_eq!(assess_termination(facts(4, "System")), Risk::Forbidden);
    }

    #[test]
    fn kernel_critical_processes_are_flagged_critical() {
        for name in ["csrss.exe", "wininit.exe", "services.exe", "lsass.exe"] {
            assert_eq!(
                assess_termination(facts(500, name)),
                Risk::Critical,
                "{name} must be flagged as bugcheck-inducing"
            );
        }
    }

    #[test]
    fn the_kernel_flag_overrides_the_static_list() {
        // A driver can mark any process critical at runtime. Our list cannot
        // know about that, so the flag has to win.
        let mut f = facts(9999, "some-driver-service.exe");
        f.break_on_termination = Some(true);
        assert_eq!(assess_termination(f), Risk::Critical);
    }

    #[test]
    fn matching_is_case_insensitive() {
        // Windows paths are case-insensitive and different APIs disagree on
        // casing, so a case-sensitive check would silently miss.
        assert_eq!(assess_termination(facts(500, "CSRSS.EXE")), Risk::Critical);
        assert_eq!(assess_termination(facts(500, "CsrSs.Exe")), Risk::Critical);
    }

    #[test]
    fn the_shell_is_disruptive_but_not_fatal() {
        // Killing explorer.exe is a legitimate troubleshooting step and it
        // restarts itself. It should warn, not refuse.
        let risk = assess_termination(facts(2000, "explorer.exe"));
        assert_eq!(risk, Risk::Disruptive);
        assert!(risk.is_possible());
        assert!(risk.needs_confirmation());
    }

    #[test]
    fn a_protected_process_is_forbidden_not_merely_denied() {
        // The distinction that matters: elevation cannot help, so the UI must
        // not offer it. Task Manager offers "run as administrator" here and
        // it always fails.
        let mut f = facts(3000, "MsMpEng.exe");
        f.protected = true;

        let risk = assess_termination(f);
        assert_eq!(risk, Risk::Forbidden);
        assert!(!risk.elevation_might_help());
        assert!(!risk.is_possible());
    }

    #[test]
    fn an_unnamed_process_is_treated_cautiously() {
        // Better a needless prompt than an unwarned bugcheck.
        let mut f = facts(5000, "x");
        f.name = None;
        assert_eq!(assess_termination(f), Risk::Disruptive);
    }

    #[test]
    fn terminating_ourselves_asks_first() {
        let mut f = facts(1234, "vitals.exe");
        f.is_self = true;
        assert_eq!(assess_termination(f), Risk::Disruptive);
    }

    #[test]
    fn suspending_ourselves_is_forbidden_because_it_is_unrecoverable() {
        // The thread that would resume us is the one being suspended. There
        // is no way back short of killing the app from outside.
        let mut f = facts(1234, "vitals.exe");
        f.is_self = true;
        assert_eq!(assess_suspension(f), Risk::Forbidden);
    }

    #[test]
    fn suspending_a_kernel_process_is_as_dangerous_as_killing_it() {
        // A frozen csrss.exe hangs every window with no error and no way back
        // except a hard reset.
        assert_eq!(assess_suspension(facts(500, "csrss.exe")), Risk::Critical);
    }

    #[test]
    fn suspending_an_ordinary_process_is_safe() {
        assert_eq!(assess_suspension(facts(1234, "chrome.exe")), Risk::Safe);
    }

    #[test]
    fn only_dangerous_actions_require_confirmation() {
        assert!(!Risk::Safe.needs_confirmation());
        assert!(Risk::Disruptive.needs_confirmation());
        assert!(Risk::Critical.needs_confirmation());
        // Forbidden needs no confirmation because it is never offered.
        assert!(!Risk::Forbidden.needs_confirmation());
    }

    #[test]
    fn risk_ordering_reflects_severity() {
        assert!(Risk::Safe < Risk::Disruptive);
        assert!(Risk::Disruptive < Risk::Critical);
        assert!(Risk::Critical < Risk::Forbidden);
    }

    #[test]
    fn every_risk_has_a_distinct_message() {
        // A generic dialog for every case is what trains people to click
        // through warnings.
        let keys: Vec<_> = [
            Risk::Safe,
            Risk::Disruptive,
            Risk::Critical,
            Risk::Forbidden,
        ]
        .iter()
        .map(|r| consequence_key(*r))
        .collect();

        let mut unique = keys.clone();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(unique.len(), keys.len(), "two risk levels share wording");
    }
}
