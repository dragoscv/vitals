//! # vitals-agents
//!
//! Reads agentq's coordination state and the AI client sessions on this
//! machine into one [`AgentsSnapshot`] for the AI page (ADR-0038).
//!
//! Read-only. The only writes the page can cause go through agentq's own CLI
//! ([`actions`]), so agentq's mutexes, journal and reasons stay authoritative.
//! OS-agnostic: the one platform question — "when did this pid start?" — is
//! passed in as a closure.

pub mod actions;
pub mod agentq;
pub mod cloud;
pub mod journal;
pub mod sessions;
pub mod time;

use std::path::PathBuf;

use vitals_core::agents::AgentsSnapshot;

pub use agentq::ProcStart;
pub use sessions::Roots;

/// Statistics window, in hours.
pub const STATS_WINDOW_HOURS: u32 = 24;

/// Keeps incremental state (journal offset, session offsets) between reads.
#[derive(Debug)]
pub struct Reader {
    root: Option<PathBuf>,
    journal: journal::JournalTail,
    sessions: sessions::SessionTracker,
}

impl Reader {
    /// A reader over the real locations for the current user.
    #[must_use]
    pub fn detect() -> Self {
        Self::new(agentq::default_root(), Roots::detect())
    }

    #[must_use]
    pub fn new(root: Option<PathBuf>, roots: Roots) -> Self {
        Self {
            root,
            journal: journal::JournalTail::new(),
            sessions: sessions::SessionTracker::new(roots),
        }
    }

    /// The agentq state directory, when it exists.
    #[must_use]
    pub fn root(&self) -> Option<&PathBuf> {
        self.root.as_ref()
    }

    /// Reads everything that changed since the previous call.
    pub fn read(&mut self, probe: ProcStart<'_>) -> AgentsSnapshot {
        let now = time::now_ms();
        self.read_at(probe, now)
    }

    /// [`Self::read`] with an explicit clock, for tests.
    pub fn read_at(&mut self, probe: ProcStart<'_>, now: u64) -> AgentsSnapshot {
        let sessions = self.sessions.poll(probe, now);
        let Some(root) = self.root.clone().filter(|r| r.is_dir()) else {
            return AgentsSnapshot {
                taken_at_ms: now,
                sessions,
                ..AgentsSnapshot::default()
            };
        };
        self.journal.poll(&root.join("journal.jsonl"), now);
        AgentsSnapshot {
            taken_at_ms: now,
            agentq_installed: true,
            resources: agentq::read_resources(&root, probe, now),
            leases: agentq::read_leases(&root, probe, now),
            releases: agentq::read_releases(&root, now),
            jobs: agentq::read_jobs(&root, probe, now),
            marks: agentq::read_marks(&root),
            registry: agentq::read_registry(&root, now),
            cloud_down: agentq::read_cloud_down(&root, now),
            machine: agentq::read_machine_cap(&root),
            journal: self.journal.timeline(),
            stats: self.journal.stats(now, STATS_WINDOW_HOURS),
            sessions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_without_agentq_reports_not_installed_and_empty_lists() {
        let mut r = Reader::new(
            Some(PathBuf::from("Z:/definitely/not/here")),
            Roots::default(),
        );
        let s = r.read_at(&|_| None, 1_000);
        assert!(!s.agentq_installed);
        assert!(s.resources.is_empty() && s.journal.is_empty());
        assert_eq!(s.stats.wait_p50_sec, None);
    }
}
