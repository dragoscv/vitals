//! User session enumeration and per-session resource rollup.
//!
//! Mirrors Task Manager's Users tab: lists every logon session on this machine
//! with its user, state, and resource consumption.

mod enumerate;
mod rollup;
mod session;

pub use enumerate::enumerate_sessions;
pub use rollup::{ProcessSample, SessionRollup, rollup_by_session};
pub use session::{LogonSession, SessionState};

use vitals_core::error::Result;

/// Full user inventory: sessions and their resource rollups.
#[derive(Debug, Clone)]
pub struct UserInventory {
    pub sessions: Vec<LogonSession>,
    pub rollups: Vec<SessionRollup>,
}

/// Captures the current user sessions.
///
/// This is the low-cost variant: sessions only, no resource rollup. Use this
/// when you only need the user list and state, not their CPU or memory totals.
pub fn collect() -> Result<Vec<LogonSession>> {
    enumerate_sessions()
}

/// Captures sessions plus per-session resource rollup.
///
/// Accepts a process sample from the main sampler and aggregates it by session.
/// The sample should come from the same instant the sessions are enumerated to
/// avoid reporting stale figures.
#[must_use]
pub fn snapshot(processes: &[ProcessSample]) -> UserInventory {
    // Session enumeration can fail (access denied, shutdown in progress), but
    // the rollup is pure. If enumeration fails, return an empty inventory
    // rather than propagating the error — the error handling for "Users tab
    // is unavailable" belongs in the UI layer, not here.
    let sessions = enumerate_sessions().unwrap_or_default();
    let rollups = rollup_by_session(processes);

    UserInventory { sessions, rollups }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_returns_sessions() {
        let sessions = collect().expect("enumeration should succeed");
        assert!(!sessions.is_empty(), "expected at least one session");
    }

    #[test]
    fn snapshot_with_empty_process_list() {
        let inventory = snapshot(&[]);
        // Should still return the session list even with no processes.
        assert!(
            !inventory.sessions.is_empty(),
            "expected sessions even with no processes"
        );
        // Rollups may be empty if no processes exist, which is fine.
    }

    #[test]
    fn snapshot_with_processes() {
        use vitals_core::ids::Pid;

        let processes = vec![
            ProcessSample {
                pid: Pid(1000),
                session_id: 1,
                cpu_percent: 10.0,
                private_bytes: 1024 * 1024,
            },
            ProcessSample {
                pid: Pid(2000),
                session_id: 1,
                cpu_percent: 5.0,
                private_bytes: 512 * 1024,
            },
        ];

        let inventory = snapshot(&processes);

        assert!(!inventory.sessions.is_empty());
        assert!(!inventory.rollups.is_empty());

        // Session 1 should have a rollup with both processes.
        let rollup = inventory
            .rollups
            .iter()
            .find(|r| r.session_id == 1)
            .expect("expected rollup for session 1");

        assert_eq!(rollup.process_count, 2);
        assert!((rollup.cpu_percent - 15.0).abs() < 1e-9);
        assert_eq!(rollup.memory_bytes, 1024 * 1024 + 512 * 1024);
    }
}
