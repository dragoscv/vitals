//! Process control on macOS: signals and nice values.
//!
//! Every action takes a [`ProcessKey`] and re-reads the process's start time
//! immediately before acting, so a PID recycled since the user tapped "End"
//! is refused instead of signalled. `kill(2)` has the same race window as
//! Windows' `OpenProcess` path; checking on the line before is the best any
//! unprivileged caller can do.
//!
//! The sandboxed store build refuses everything with `Unsupported`, matching
//! the capability report, rather than letting the kernel answer `EPERM`.

use vitals_core::error::{Error, Result};
use vitals_core::ids::ProcessKey;
use vitals_core::provider::Priority;
use vitals_core::{Capability, Pid};

use crate::mach;

/// PIDs nobody may end from Vitals: the kernel and launchd (ending launchd
/// logs every user out and panics the machine).
fn is_critical(pid: Pid) -> bool {
    pid.get() <= 1
}

fn check(key: ProcessKey, cap: Capability) -> Result<libc::pid_t> {
    if crate::APP_STORE {
        return Err(Error::Unsupported(cap));
    }
    let pid = key.pid;
    let start = mach::start_time_us(pid.get())
        .ok_or_else(|| Error::NotFound(format!("process {}", pid.get())))?;
    if ProcessKey::new(pid, start) != key {
        return Err(Error::NotFound(format!(
            "process {} has exited and the number was reused",
            pid.get()
        )));
    }
    if pid.get() == std::process::id() {
        return Err(Error::Refused("Vitals will not end itself".into()));
    }
    libc::pid_t::try_from(pid.get()).map_err(|_| Error::NotFound(format!("process {}", pid.get())))
}

fn signal(pid: libc::pid_t, sig: libc::c_int, operation: &str) -> Result<()> {
    // SAFETY: kill has no memory preconditions.
    if unsafe { libc::kill(pid, sig) } == 0 {
        return Ok(());
    }
    Err(errno_error(operation))
}

fn errno_error(operation: &str) -> Error {
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EPERM | libc::EACCES) => Error::AccessDenied {
            operation: operation.to_owned(),
        },
        Some(libc::ESRCH) => Error::NotFound(operation.to_owned()),
        code => Error::Os {
            context: operation.to_owned(),
            code: code.unwrap_or(0),
        },
    }
}

/// Ends a process with `SIGTERM`, which lets it save and clean up. The
/// desktop's "End process" is the same request Activity Monitor's "Quit"
/// makes; a process that ignores it can be ended again.
///
/// # Errors
///
/// `NotFound` when the key no longer matches, `AccessDenied` for another
/// user's process, `Refused` for launchd, the kernel or Vitals itself.
pub fn terminate(key: ProcessKey) -> Result<()> {
    if is_critical(key.pid) {
        return Err(Error::Refused(
            "ending launchd shuts down the session".into(),
        ));
    }
    let pid = check(key, Capability::TerminateProcess)?;
    signal(pid, libc::SIGTERM, "terminate")
}

/// Stops every thread of a process (`SIGSTOP`).
///
/// # Errors
///
/// As [`terminate`].
pub fn suspend(key: ProcessKey) -> Result<()> {
    if is_critical(key.pid) {
        return Err(Error::Refused(
            "suspending launchd freezes the session".into(),
        ));
    }
    let pid = check(key, Capability::SuspendProcess)?;
    signal(pid, libc::SIGSTOP, "suspend")
}

/// Continues a stopped process (`SIGCONT`).
///
/// # Errors
///
/// As [`terminate`].
pub fn resume(key: ProcessKey) -> Result<()> {
    let pid = check(key, Capability::SuspendProcess)?;
    signal(pid, libc::SIGCONT, "resume")
}

/// Maps a Windows-style priority class onto a nice value.
///
/// Raising priority above normal needs root on macOS; an unprivileged app
/// gets `AccessDenied` from the kernel and says so, rather than silently
/// doing nothing. `Realtime` has no nice equivalent and is refused.
///
/// # Errors
///
/// As [`terminate`], plus `Unsupported` for `Realtime`.
pub fn set_priority(key: ProcessKey, priority: Priority) -> Result<()> {
    let nice = match priority {
        Priority::Idle => 20,
        Priority::BelowNormal => 10,
        Priority::Normal => 0,
        Priority::AboveNormal => -5,
        Priority::High => -10,
        Priority::Realtime => return Err(Error::Unsupported(Capability::SetProcessPriority)),
    };
    let pid = check(key, Capability::SetProcessPriority)?;
    // SAFETY: setpriority has no memory preconditions.
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
    if rc == 0 {
        Ok(())
    } else {
        Err(errno_error("set priority"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_sleeper() -> (std::process::Child, ProcessKey) {
        let child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        let mut start = None;
        for _ in 0..50 {
            start = mach::start_time_us(pid);
            if start.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let key = ProcessKey::new(Pid(pid), start.expect("start time"));
        (child, key)
    }

    #[test]
    fn a_stale_key_is_refused_rather_than_signalling_whatever_has_the_number() {
        let (mut child, key) = spawn_sleeper();
        let stale = ProcessKey::new(key.pid, 1);
        if crate::APP_STORE {
            assert!(matches!(terminate(stale), Err(Error::Unsupported(_))));
        } else {
            assert!(matches!(terminate(stale), Err(Error::NotFound(_))));
        }
        // The real process is untouched.
        assert!(child.try_wait().expect("wait").is_none());
        let _ = child.kill();
    }

    #[test]
    fn suspend_resume_and_terminate_reach_the_process() {
        if crate::APP_STORE {
            return;
        }
        let (mut child, key) = spawn_sleeper();
        suspend(key).expect("suspend");
        std::thread::sleep(std::time::Duration::from_millis(50));
        let status = mach::bsd_info(key.pid.get()).expect("bsd info").pbi_status;
        assert_eq!(status, libc::SSTOP, "SIGSTOP did not stop it");
        resume(key).expect("resume");
        set_priority(key, Priority::BelowNormal).expect("lower priority");
        terminate(key).expect("terminate");
        let exit = child.wait().expect("wait");
        assert!(!exit.success());
    }

    #[test]
    fn launchd_and_the_kernel_are_never_ended() {
        let launchd = ProcessKey::new(Pid(1), mach::start_time_us(1).unwrap_or(0));
        assert!(matches!(terminate(launchd), Err(Error::Refused(_))));
        assert!(matches!(
            terminate(ProcessKey::new(Pid(0), 0)),
            Err(Error::Refused(_))
        ));
    }

    #[test]
    fn realtime_is_refused() {
        let (mut child, key) = spawn_sleeper();
        assert!(matches!(
            set_priority(key, Priority::Realtime),
            Err(Error::Unsupported(_))
        ));
        let _ = child.kill();
    }
}
