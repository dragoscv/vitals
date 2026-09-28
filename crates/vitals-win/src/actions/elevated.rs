//! Retrying one process action as administrator.
//!
//! About a third of the processes on a normal desktop belong to SYSTEM or
//! another account, and an unelevated Vitals is refused every action on
//! them. The risk dialog has offered "Retry as administrator" since it was
//! written, but nothing behind the button existed.
//!
//! The design is **one UAC prompt per action**, not an elevated app. The
//! running instance re-launches this executable under `runas` with
//! [`PROCESS_ACTION_ARG`], the child performs exactly that one action and
//! exits with a code the parent maps back to an error. The UI never runs
//! elevated, so a compromised webview gains nothing from it.
//!
//! The child trusts nothing it was handed beyond the key: it re-verifies the
//! process identity (a PID can be recycled while the prompt is on screen)
//! and re-assesses the risk. A `forbidden` target is refused there too, so
//! the elevated pass cannot be used to reach what the dialog would not
//! offer.

use vitals_core::error::{Error, Result};
use vitals_core::ids::{Pid, ProcessKey};

use super::process::{Consent, plan_suspend, plan_terminate, resume, suspend, terminate};
use super::safety::Risk;
use super::taskmgr::run_elevated;

/// The argument the elevated instance is started with.
///
/// Public for the same reason as `SET_REPLACEMENT_ARG`: the app must
/// recognise it before Tauri builds anything, and one spelling is what keeps
/// the two ends from drifting.
pub const PROCESS_ACTION_ARG: &str = "--elevated-process-action";

/// An action the elevated instance may perform.
///
/// Deliberately short. Priority and affinity are reversible and rarely
/// denied on processes a user cares about; ending and pausing are what
/// people reach for on a runaway service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevatedAction {
    Terminate,
    Suspend,
    Resume,
}

impl ElevatedAction {
    const fn as_arg(self) -> &'static str {
        match self {
            Self::Terminate => "terminate",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
        }
    }

    fn from_arg(arg: &str) -> Option<Self> {
        match arg {
            "terminate" => Some(Self::Terminate),
            "suspend" => Some(Self::Suspend),
            "resume" => Some(Self::Resume),
            _ => None,
        }
    }
}

/// Exit codes of the elevated instance, one per error it can report.
///
/// A process can only hand back a number, so each error the parent must
/// tell apart gets its own. Anything else is an `Os` failure with the code.
mod exit {
    pub const OK: u32 = 0;
    pub const NOT_FOUND: u32 = 10;
    pub const ACCESS_DENIED: u32 = 11;
    pub const REFUSED: u32 = 12;
    pub const BAD_ARGS: u32 = 13;
    pub const FAILED: u32 = 14;
}

/// The arguments for one elevated action, in the order [`parse_args`] reads.
#[must_use]
pub fn format_args(action: ElevatedAction, key: ProcessKey, consent: Consent) -> String {
    format!(
        "{PROCESS_ACTION_ARG} {} {} {} {}",
        action.as_arg(),
        key.pid.get(),
        key.start_time,
        match consent {
            Consent::Confirmed => "confirmed",
            Consent::Unconfirmed => "unconfirmed",
        }
    )
}

/// Reads the arguments after [`PROCESS_ACTION_ARG`].
///
/// `None` for anything malformed. The caller is our own parent, so a bad
/// argument is a bug, and guessing would mean acting on the wrong process.
#[must_use]
pub fn parse_args<I, S>(rest: I) -> Option<(ElevatedAction, ProcessKey, Consent)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut rest = rest.into_iter();
    let action = ElevatedAction::from_arg(rest.next()?.as_ref())?;
    let pid: u32 = rest.next()?.as_ref().parse().ok()?;
    let start_time: u64 = rest.next()?.as_ref().parse().ok()?;
    let consent = match rest.next()?.as_ref() {
        "confirmed" => Consent::Confirmed,
        "unconfirmed" => Consent::Unconfirmed,
        _ => return None,
    };
    if rest.next().is_some() {
        return None;
    }
    Some((action, ProcessKey::new(Pid(pid), start_time), consent))
}

/// Asks for administrator approval and performs `action` on `key`.
///
/// Blocks until the elevated instance exits — the length of the UAC prompt
/// plus one syscall — so callers run it off the UI thread.
///
/// # Errors
///
/// - [`Error::Refused`] when the prompt was dismissed or the target is one
///   the elevated pass will not touch.
/// - [`Error::NotFound`] when the process exited or its PID was reused
///   while the prompt was up.
/// - [`Error::AccessDenied`] when even an administrator is refused — a
///   protected process the plan could not see.
/// - [`Error::Os`] for anything else.
pub fn run_as_admin(action: ElevatedAction, key: ProcessKey, consent: Consent) -> Result<()> {
    let code = run_elevated(
        &format_args(action, key, consent),
        "the process was left as it was",
    )?;
    from_exit_code(code, key)
}

fn from_exit_code(code: u32, key: ProcessKey) -> Result<()> {
    let pid = key.pid.get();
    match code {
        exit::OK => Ok(()),
        exit::NOT_FOUND => Err(Error::NotFound(format!(
            "process {pid} exited before the elevated action ran"
        ))),
        exit::ACCESS_DENIED => Err(Error::AccessDenied {
            operation: format!("act on process {pid}, even as administrator"),
        }),
        exit::REFUSED => Err(Error::Refused(format!(
            "Windows does not allow process {pid} to be changed, even as administrator"
        ))),
        other => Err(Error::Os {
            context: format!("the elevated action on process {pid}"),
            code: other.cast_signed(),
        }),
    }
}

/// The elevated instance's whole job: one action, then an exit code.
///
/// Returns the code rather than exiting so it is testable.
#[must_use]
pub fn perform<I, S>(rest: I) -> u32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some((action, key, consent)) = parse_args(rest) else {
        return exit::BAD_ARGS;
    };

    // A cheap PID-only pre-check (System, Idle, ourselves) before asking for
    // the debug privilege. The real assessment happens inside `terminate` /
    // `suspend`, on their own handle, from the live image name, protection
    // level and BreakOnTermination flag — so a critical process is refused
    // here too unless the user confirmed the desktop dialog.
    let plan = match action {
        ElevatedAction::Suspend => plan_suspend(key.pid, None, false),
        ElevatedAction::Terminate | ElevatedAction::Resume => plan_terminate(key.pid, None, false),
    };
    if plan.risk == Risk::Forbidden {
        return exit::REFUSED;
    }

    // An elevated token *holds* SeDebugPrivilege but has it disabled, and
    // without it OpenProcess on a SYSTEM-owned process is still denied —
    // measured live: the first version of this path reported "access
    // denied, even as administrator" for a SYSTEM `ping.exe`. Enabled only
    // in this short-lived child, never in the app. Failing to enable it is
    // not fatal: processes of another *user* need only elevation, so the
    // action is still attempted and a denial is reported as one.
    let _debug = enable_debug_privilege();

    let result = match action {
        ElevatedAction::Terminate => terminate(key, 1, consent),
        ElevatedAction::Suspend => suspend(key, consent),
        ElevatedAction::Resume => resume(key),
    };

    match result {
        Ok(()) => exit::OK,
        Err(Error::NotFound(_)) => exit::NOT_FOUND,
        Err(Error::AccessDenied { .. }) => exit::ACCESS_DENIED,
        Err(Error::Refused(_)) => exit::REFUSED,
        Err(_) => exit::FAILED,
    }
}

/// Enables `SeDebugPrivilege` on this process's token. `false` on any
/// failure, including a token that does not hold it (not elevated).
fn enable_debug_privilege() -> bool {
    use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID, WIN32_ERROR};
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_DEBUG_NAME,
        SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// `ERROR_NOT_ALL_ASSIGNED`: the call "succeeds" without granting.
    const NOT_ALL_ASSIGNED: WIN32_ERROR = WIN32_ERROR(1300);

    let mut token = HANDLE::default();
    // SAFETY: pseudo-handle, and `token` is written only on success.
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &raw mut token,
        )
    };
    if opened.is_err() {
        return false;
    }

    let mut luid = LUID::default();
    // SAFETY: `SE_DEBUG_NAME` is a static wide string; `luid` is a live out-pointer.
    let looked_up = unsafe { LookupPrivilegeValueW(None, SE_DEBUG_NAME, &raw mut luid) };

    let granted = looked_up.is_ok() && {
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        // SAFETY: `token` is live and opened for adjustment; `privileges` is
        // a correctly sized single-entry array.
        let adjusted = unsafe {
            AdjustTokenPrivileges(token, false, Some(&raw const privileges), 0, None, None)
        };
        // SAFETY: no preconditions.
        adjusted.is_ok() && unsafe { GetLastError() } != NOT_ALL_ASSIGNED
    };

    // SAFETY: `token` came from `OpenProcessToken` and is closed exactly once.
    unsafe {
        let _ = CloseHandle(token);
    }
    granted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arguments_round_trip_so_the_child_acts_on_exactly_the_process_the_parent_named() {
        let key = ProcessKey::new(Pid(4242), 134_350_137_695_808_977);
        for action in [
            ElevatedAction::Terminate,
            ElevatedAction::Suspend,
            ElevatedAction::Resume,
        ] {
            for consent in [Consent::Confirmed, Consent::Unconfirmed] {
                let args = format_args(action, key, consent);
                let mut parts = args.split(' ');
                assert_eq!(parts.next(), Some(PROCESS_ACTION_ARG));
                assert_eq!(parse_args(parts), Some((action, key, consent)));
            }
        }
    }

    #[test]
    fn malformed_arguments_are_refused_rather_than_guessed() {
        for bad in [
            vec![],
            vec!["terminate"],
            vec!["terminate", "12"],
            vec!["kill", "12", "34"],
            vec!["terminate", "-1", "34"],
            vec!["terminate", "12", "x"],
            // Consent is required, never defaulted: a missing word must not
            // silently mean either answer.
            vec!["terminate", "12", "34"],
            vec!["terminate", "12", "34", "yes"],
            vec!["terminate", "12", "34", "confirmed", "extra"],
        ] {
            assert_eq!(parse_args(bad.clone()), None, "{bad:?}");
            assert_eq!(perform(bad.clone()), exit::BAD_ARGS, "{bad:?}");
        }
    }

    #[test]
    fn the_elevated_pass_refuses_a_forbidden_target_even_when_asked() {
        // PID 4 is System: forbidden by the static assessment. The elevated
        // instance must not become a way round the dialog's refusal.
        let args = format_args(
            ElevatedAction::Terminate,
            ProcessKey::new(Pid(4), 1),
            Consent::Confirmed,
        );
        assert_eq!(perform(args.split(' ').skip(1)), exit::REFUSED);
    }

    #[test]
    fn a_stale_key_is_reported_as_gone_not_acted_on() {
        // Our own PID with a wrong start time: identity verification must
        // reject it, so the child reports NotFound and touches nothing.
        let pid = std::process::id();
        let args = format_args(
            ElevatedAction::Resume,
            ProcessKey::new(Pid(pid), 1),
            Consent::Unconfirmed,
        );
        assert_eq!(perform(args.split(' ').skip(1)), exit::NOT_FOUND);
    }

    #[test]
    fn every_exit_code_maps_back_to_the_error_the_ui_branches_on() {
        let key = ProcessKey::new(Pid(9), 9);
        assert!(from_exit_code(exit::OK, key).is_ok());
        assert!(matches!(
            from_exit_code(exit::NOT_FOUND, key),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            from_exit_code(exit::ACCESS_DENIED, key),
            Err(Error::AccessDenied { .. })
        ));
        assert!(matches!(
            from_exit_code(exit::REFUSED, key),
            Err(Error::Refused(_))
        ));
        assert!(matches!(
            from_exit_code(exit::FAILED, key),
            Err(Error::Os { .. })
        ));
    }
}
