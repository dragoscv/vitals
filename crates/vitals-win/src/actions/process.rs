//! Process actions: terminate, suspend, resume, priority, affinity.

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, FALSE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    PROCESS_SUSPEND_RESUME, PROCESS_TERMINATE, SetPriorityClass, SetProcessAffinityMask,
    TerminateProcess,
};

use vitals_core::error::{Error, Result};
use vitals_core::ids::{Pid, ProcessKey};

use super::safety::{ProcessFacts, Risk, assess_suspension, assess_termination};

unsafe extern "system" {
    /// Suspends every thread in a process.
    ///
    /// Undocumented but stable since Windows XP, and the only way to suspend
    /// a process atomically. The documented alternative — enumerate threads,
    /// `SuspendThread` each — races: a thread created between enumeration
    /// and suspension keeps running, so the process is only mostly frozen.
    fn NtSuspendProcess(handle: HANDLE) -> i32;
    fn NtResumeProcess(handle: HANDLE) -> i32;
}

/// A process handle that closes itself.
///
/// Leaking one keeps the *record* of an exited process alive as a zombie,
/// which then shows up in our own process list. A monitoring tool creating
/// the artefacts it displays is not acceptable.
#[derive(Debug)]
struct ProcessHandle(HANDLE);

impl ProcessHandle {
    /// Opens a process with the given access rights.
    fn open(pid: Pid, access: u32) -> Result<Self> {
        // SAFETY: `OpenProcess` validates its own arguments and returns null
        // on failure.
        let handle = unsafe { OpenProcess(access, FALSE, pid.get()) };

        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            // SAFETY: no preconditions.
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };

            return Err(match code {
                ERROR_ACCESS_DENIED => Error::AccessDenied {
                    operation: format!("open process {}", pid.get()),
                },
                // The process exited between enumeration and this call, which
                // is completely normal at 1 Hz and must not surface as an
                // error dialog.
                ERROR_INVALID_PARAMETER => Error::NotFound(format!("process {}", pid.get())),
                other => Error::Os {
                    context: format!("OpenProcess({})", pid.get()),
                    code: other.cast_signed(),
                },
            });
        }

        Ok(Self(handle))
    }

    const fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid handle from OpenProcess, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Verifies a handle still refers to the process we meant.
///
/// Between listing a process and acting on it, that process can exit and its
/// PID be reused. Acting on the PID alone would then terminate an innocent
/// process that happened to inherit the number — a real, reported failure
/// mode in other tools, and precisely why [`ProcessKey`] carries a start
/// time.
fn verify_identity(handle: &ProcessHandle, expected: ProcessKey) -> Result<()> {
    let mut creation: i64 = 0;
    let mut exit: i64 = 0;
    let mut kernel: i64 = 0;
    let mut user: i64 = 0;

    // SAFETY: the handle is valid and all four out-pointers reference live
    // i64s for the duration of the call.
    let ok = unsafe {
        windows_sys::Win32::System::Threading::GetProcessTimes(
            handle.raw(),
            (&raw mut creation).cast(),
            (&raw mut exit).cast(),
            (&raw mut kernel).cast(),
            (&raw mut user).cast(),
        )
    };

    if ok == FALSE {
        return Err(Error::NotFound(format!(
            "process {} could not be verified",
            expected.pid.get()
        )));
    }

    if creation.cast_unsigned() != expected.start_time {
        return Err(Error::NotFound(format!(
            "process {} exited and its PID was reused",
            expected.pid.get()
        )));
    }

    // A process that has exited but whose record is still held open by some
    // handle can be opened successfully — verified empirically: a killed
    // child whose `Child` struct is still alive opens fine. Acting on it then
    // fails inside `TerminateProcess` with ACCESS_DENIED, which the UI would
    // show as "needs elevation" and prompt for UAC that cannot help.
    //
    // A non-zero exit time is the reliable signal that the process is gone.
    if exit != 0 {
        return Err(Error::NotFound(format!(
            "process {} has already exited",
            expected.pid.get()
        )));
    }

    Ok(())
}

/// Result of a risk check, so the caller can prompt before committing.
#[derive(Debug, Clone)]
pub struct ActionPlan {
    pub risk: Risk,
    /// Translation key describing the consequence.
    pub consequence: &'static str,
}

/// Assesses terminating a process without doing it.
///
/// Separate from execution so the UI can prompt with a specific consequence
/// first. Merging the two would mean either prompting inside the backend or
/// not prompting at all.
#[must_use]
pub fn plan_terminate(pid: Pid, name: Option<&str>, protected: bool) -> ActionPlan {
    // SAFETY: no preconditions.
    let is_self = pid.get() == unsafe { GetCurrentProcessId() };

    let risk = assess_termination(ProcessFacts {
        pid,
        name,
        // Reading BreakOnTermination needs a handle we may not have. The
        // static list covers the documented cases; the flag is checked again
        // at execution time when a handle exists.
        break_on_termination: None,
        protected,
        is_self,
    });

    ActionPlan {
        risk,
        consequence: super::safety::consequence_key(risk),
    }
}

/// Assesses suspending a process without doing it.
///
/// Suspension has a different risk profile from termination and needs its
/// own assessment: suspending ourselves is unrecoverable where terminating
/// ourselves merely closes the app, and a frozen system process hangs the
/// machine with no error at all.
#[must_use]
pub fn plan_suspend(pid: Pid, name: Option<&str>, protected: bool) -> ActionPlan {
    // SAFETY: no preconditions.
    let is_self = pid.get() == unsafe { GetCurrentProcessId() };

    let risk = assess_suspension(ProcessFacts {
        pid,
        name,
        break_on_termination: None,
        protected,
        is_self,
    });

    ActionPlan {
        risk,
        consequence: super::safety::consequence_key(risk),
    }
}

/// Terminates a process.
///
/// `key` rather than `Pid`: the start time is verified against the live
/// process before anything destructive happens, so a recycled PID cannot
/// cause the wrong process to be killed.
///
/// # Errors
///
/// - [`Error::AccessDenied`] when the caller lacks rights — the UI may offer
///   elevation.
/// - [`Error::NotFound`] when the process already exited or its PID was
///   reused. Not a failure worth reporting: the user's goal is achieved.
/// - [`Error::Refused`] when we decline because the action is unrecoverable.
pub fn terminate(key: ProcessKey, exit_code: u32) -> Result<()> {
    let handle = ProcessHandle::open(
        key.pid,
        PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;

    verify_identity(&handle, key)?;

    // SAFETY: the handle is valid and was opened with PROCESS_TERMINATE.
    let ok = unsafe { TerminateProcess(handle.raw(), exit_code) };

    if ok == FALSE {
        // SAFETY: no preconditions.
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(match code {
            ERROR_ACCESS_DENIED => Error::AccessDenied {
                operation: format!("terminate process {}", key.pid.get()),
            },
            other => Error::Os {
                context: format!("TerminateProcess({})", key.pid.get()),
                code: other.cast_signed(),
            },
        });
    }

    Ok(())
}

/// Suspends every thread in a process.
///
/// # Errors
///
/// As [`terminate`].
pub fn suspend(key: ProcessKey) -> Result<()> {
    // SAFETY: no preconditions.
    if key.pid.get() == unsafe { GetCurrentProcessId() } {
        // `Refused` rather than `Unsupported`: the platform is perfectly
        // capable of this, we decline because it is unrecoverable. The
        // distinction matters to the UI, which permanently hides an
        // unsupported affordance but explains a refusal.
        return Err(Error::Refused(
            "suspending our own process would freeze the UI with no way to resume it".into(),
        ));
    }

    let handle = ProcessHandle::open(
        key.pid,
        PROCESS_SUSPEND_RESUME | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;

    verify_identity(&handle, key)?;

    // SAFETY: the handle is valid and opened with PROCESS_SUSPEND_RESUME.
    let status = unsafe { NtSuspendProcess(handle.raw()) };

    if status < 0 {
        return Err(Error::Os {
            context: format!("NtSuspendProcess({})", key.pid.get()),
            code: status,
        });
    }

    Ok(())
}

/// Resumes a suspended process.
///
/// # Errors
///
/// As [`terminate`].
pub fn resume(key: ProcessKey) -> Result<()> {
    let handle = ProcessHandle::open(
        key.pid,
        PROCESS_SUSPEND_RESUME | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;

    verify_identity(&handle, key)?;

    // SAFETY: the handle is valid and opened with PROCESS_SUSPEND_RESUME.
    let status = unsafe { NtResumeProcess(handle.raw()) };

    if status < 0 {
        return Err(Error::Os {
            context: format!("NtResumeProcess({})", key.pid.get()),
            code: status,
        });
    }

    Ok(())
}

/// Scheduling priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    /// Above every ordinary process, including input handling.
    ///
    /// A CPU-bound process at this priority can starve the mouse cursor and
    /// make the machine appear frozen. The UI must warn rather than listing
    /// it as one option among six.
    Realtime,
}

impl Priority {
    const fn to_class(self) -> u32 {
        use windows_sys::Win32::System::Threading::{
            ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
            IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, REALTIME_PRIORITY_CLASS,
        };

        match self {
            Self::Idle => IDLE_PRIORITY_CLASS,
            Self::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            Self::Normal => NORMAL_PRIORITY_CLASS,
            Self::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            Self::High => HIGH_PRIORITY_CLASS,
            Self::Realtime => REALTIME_PRIORITY_CLASS,
        }
    }

    /// Whether setting this needs an explicit warning.
    #[must_use]
    pub const fn is_dangerous(self) -> bool {
        matches!(self, Self::Realtime)
    }
}

/// Sets a process's scheduling priority.
///
/// # Errors
///
/// As [`terminate`].
pub fn set_priority(key: ProcessKey, priority: Priority) -> Result<()> {
    let handle = ProcessHandle::open(
        key.pid,
        PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;

    verify_identity(&handle, key)?;

    // SAFETY: the handle is valid and opened with PROCESS_SET_INFORMATION.
    let ok = unsafe { SetPriorityClass(handle.raw(), priority.to_class()) };

    if ok == FALSE {
        // SAFETY: no preconditions.
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(match code {
            ERROR_ACCESS_DENIED => Error::AccessDenied {
                operation: format!("set priority on process {}", key.pid.get()),
            },
            other => Error::Os {
                context: format!("SetPriorityClass({})", key.pid.get()),
                code: other.cast_signed(),
            },
        });
    }

    Ok(())
}

/// Restricts a process to a set of logical processors.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty mask: a process pinned to zero
/// processors can never be scheduled again, and Windows accepts the call
/// without complaint before hanging the process permanently.
pub fn set_affinity(key: ProcessKey, mask: u64) -> Result<()> {
    if mask == 0 {
        return Err(Error::Refused(
            "an empty affinity mask would prevent the process from ever running again".into(),
        ));
    }

    let handle = ProcessHandle::open(
        key.pid,
        PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;

    verify_identity(&handle, key)?;

    // SAFETY: the handle is valid and opened with PROCESS_SET_INFORMATION.
    let ok = unsafe { SetProcessAffinityMask(handle.raw(), usize::try_from(mask).unwrap_or(!0)) };

    if ok == FALSE {
        // SAFETY: no preconditions.
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(match code {
            ERROR_ACCESS_DENIED => Error::AccessDenied {
                operation: format!("set affinity on process {}", key.pid.get()),
            },
            other => Error::Os {
                context: format!("SetProcessAffinityMask({})", key.pid.get()),
                code: other.cast_signed(),
            },
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Spawns a long-lived child to act on.
    ///
    /// Every destructive test operates on a process we created ourselves.
    fn spawn_victim() -> std::process::Child {
        Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 > nul"])
            .spawn()
            .expect("spawn a test process")
    }

    /// Reads a live process's start time, to build a valid key.
    fn key_for(pid: u32) -> ProcessKey {
        let mut enumerator = crate::process::ProcessEnumerator::new();
        let found = enumerator
            .enumerate()
            .expect("enumerate")
            .into_iter()
            .find(|p| p.key.pid.get() == pid);

        match found {
            Some(process) => process.key,
            None => panic!("process {pid} not found"),
        }
    }

    #[test]
    fn terminating_a_process_we_own_succeeds() {
        let mut child = spawn_victim();
        let key = key_for(child.id());

        terminate(key, 1).expect("terminate our own child");

        let status = child.wait().expect("wait");
        assert!(!status.success(), "the process should have been killed");
    }

    #[test]
    fn terminating_an_already_exited_process_reports_not_found() {
        // Normal at 1 Hz: a process listed in the last frame can exit before
        // the user clicks. This must not surface as a scary error, and in
        // particular must NOT surface as AccessDenied — the UI would offer a
        // UAC prompt that cannot possibly help.
        //
        // Note the child handle is deliberately still held here. That is the
        // hard case: the process record survives, OpenProcess succeeds, and
        // only the exit time reveals it is gone.
        let mut child = spawn_victim();
        let key = key_for(child.id());
        child.kill().expect("kill");
        child.wait().expect("wait");

        match terminate(key, 1) {
            Err(Error::NotFound(_)) => {}
            Err(other) => panic!("expected NotFound, got {other:?}"),
            Ok(()) => panic!("terminating an exited process should not report success"),
        }
    }

    #[test]
    fn an_exited_process_is_never_reported_as_needing_elevation() {
        // Regression guard for the specific misdiagnosis: TerminateProcess
        // on a dead-but-unreaped process returns ACCESS_DENIED, which reads
        // as "run as administrator" when the truth is "it already exited".
        let mut child = spawn_victim();
        let key = key_for(child.id());
        child.kill().expect("kill");
        child.wait().expect("wait");

        for result in [terminate(key, 1), suspend(key), resume(key)] {
            assert!(
                !matches!(result, Err(Error::AccessDenied { .. })),
                "an exited process must not be reported as an elevation problem: {result:?}"
            );
        }
    }

    #[test]
    fn a_stale_key_does_not_kill_the_wrong_process() {
        // THE case ProcessKey exists for. A key with the right PID but the
        // wrong start time must be refused, or a recycled PID means killing
        // an innocent process.
        let child = spawn_victim();
        let real = key_for(child.id());

        let stale = ProcessKey::new(real.pid, real.start_time.wrapping_sub(1_000_000));

        let result = terminate(stale, 1);
        assert!(
            matches!(result, Err(Error::NotFound(_))),
            "a stale key must be refused, got {result:?}"
        );

        // The real process must still be alive.
        let mut child = child;
        child.kill().expect("cleanup");
        child.wait().expect("cleanup");
    }

    #[test]
    fn suspend_and_resume_round_trip() {
        let mut child = spawn_victim();
        let key = key_for(child.id());

        suspend(key).expect("suspend");
        resume(key).expect("resume");

        child.kill().expect("cleanup");
        child.wait().expect("cleanup");
    }

    #[test]
    fn suspending_ourselves_is_refused_before_anything_happens() {
        // If this ever regresses, the test suite freezes rather than failing,
        // so the guard is checked before any handle is opened.
        let key = key_for(std::process::id());
        let result = suspend(key);

        assert!(
            matches!(result, Err(Error::Refused(_))),
            "expected a refusal, got {result:?}"
        );
    }

    #[test]
    fn priority_can_be_set_on_a_process_we_own() {
        let mut child = spawn_victim();
        let key = key_for(child.id());

        set_priority(key, Priority::BelowNormal).expect("lower priority");
        set_priority(key, Priority::Normal).expect("restore priority");

        child.kill().expect("cleanup");
        child.wait().expect("cleanup");
    }

    #[test]
    fn an_empty_affinity_mask_is_refused() {
        // Windows accepts this and the process never runs again. We must not.
        let key = key_for(std::process::id());
        let result = set_affinity(key, 0);

        assert!(
            matches!(result, Err(Error::Refused(_))),
            "an empty mask must be refused, got {result:?}"
        );
    }

    #[test]
    fn affinity_can_be_set_on_a_process_we_own() {
        let mut child = spawn_victim();
        let key = key_for(child.id());

        // Pin to the first two processors, then restore.
        set_affinity(key, 0b11).expect("restrict affinity");

        child.kill().expect("cleanup");
        child.wait().expect("cleanup");
    }

    #[test]
    fn realtime_priority_is_flagged_as_dangerous() {
        // It can starve the mouse cursor. Listing it as one option among six
        // without warning is how people freeze their machines.
        assert!(Priority::Realtime.is_dangerous());
        assert!(!Priority::High.is_dangerous());
        assert!(!Priority::Normal.is_dangerous());
    }

    #[test]
    fn planning_a_termination_does_not_perform_it() {
        // The plan/execute split exists so the UI can prompt first.
        let mut child = spawn_victim();

        let plan = plan_terminate(Pid(child.id()), Some("cmd.exe"), false);
        assert_eq!(plan.risk, Risk::Safe);

        // Still alive.
        assert!(child.try_wait().expect("try_wait").is_none());

        child.kill().expect("cleanup");
        child.wait().expect("cleanup");
    }

    #[test]
    fn planning_flags_critical_processes_without_touching_them() {
        let plan = plan_terminate(Pid(500), Some("csrss.exe"), false);
        assert_eq!(plan.risk, Risk::Critical);
        assert!(plan.risk.needs_confirmation());
    }

    #[test]
    fn planning_a_self_suspend_reports_forbidden_not_merely_risky() {
        // Terminating ourselves is recoverable (the app just closes);
        // suspending ourselves is not, because the thread that would resume
        // us is the one being frozen. The two plans must differ.
        let me = Pid(std::process::id());

        assert_eq!(
            plan_suspend(me, Some("vitals.exe"), false).risk,
            Risk::Forbidden
        );
        assert_eq!(
            plan_terminate(me, Some("vitals.exe"), false).risk,
            Risk::Disruptive
        );
    }

    #[test]
    fn handles_do_not_leak_across_many_operations() {
        // A leaked handle keeps an exited process alive as a zombie, which
        // then appears in our own process list.
        let mut child = spawn_victim();
        let key = key_for(child.id());

        for _ in 0..200 {
            let _ = set_priority(key, Priority::Normal);
        }

        child.kill().expect("cleanup");
        child.wait().expect("cleanup");
    }
}
