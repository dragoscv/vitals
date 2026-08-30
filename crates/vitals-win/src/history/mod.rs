//! App history: per-executable resource usage accumulated over time.
//!
//! This is Vitals' own history, starting from the moment it was first run. It
//! is **not** Windows' SRUM database (System Resource Usage Monitor), which
//! requires elevation and an ESE reader. Task Manager's "App history" tab
//! reads SRUM; we accumulate from our own sampler. The consequence: on a fresh
//! install, history is empty and builds from that point forward.

pub mod accumulate;
pub mod persist;
pub mod record;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use vitals_core::ids::Pid;

pub use accumulate::{ProcessSample, accumulate};
pub use persist::{load, save};
pub use record::{AppHistory, AppHistoryRecord, normalise_executable};

use crate::process::RawProcess;

/// Implements `ProcessSample` for `RawProcess` so the accumulator can work
/// with real process enumerations.
impl ProcessSample for RawProcess {
    fn pid(&self) -> Pid {
        self.key.pid
    }

    fn executable_path(&self) -> Option<&Path> {
        self.name.as_ref().map(|s| Path::new(s.as_str()))
    }

    fn cpu_time_100ns(&self) -> u64 {
        self.cpu_time()
    }

    fn disk_read_bytes(&self) -> u64 {
        self.read_bytes
    }

    fn disk_write_bytes(&self) -> u64 {
        self.write_bytes
    }

    fn private_bytes(&self) -> u64 {
        self.private_bytes
    }
}

/// Stateful app history store.
///
/// Holds the accumulated history plus the per-PID state needed to compute
/// deltas between ticks. Callers load it once, call `record` on every sampler
/// tick, and periodically call `save` to persist the history.
#[derive(Debug, Default)]
pub struct AppHistoryStore {
    history: AppHistory,
    prev_state: BTreeMap<Pid, accumulate::ProcessState>,
}

impl AppHistoryStore {
    /// Creates a store from a previously saved history file.
    ///
    /// If the file does not exist, returns an empty history. If the file is
    /// corrupt, logs the error and returns empty rather than failing — a
    /// corrupt local file must not prevent the app from launching.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let history = persist::load(path);
        Self {
            history,
            prev_state: BTreeMap::new(),
        }
    }

    /// Accumulates a batch of process samples into the history.
    ///
    /// This is the hot path, called on every sampler tick. Deltas are computed
    /// against the previous tick's state, which is maintained internally.
    pub fn record(&mut self, samples: &[RawProcess]) {
        let now = SystemTime::now();
        accumulate::accumulate(&mut self.history, samples, &mut self.prev_state, now);
    }

    /// Returns a snapshot of the current history.
    ///
    /// The returned vector is sorted by CPU seconds descending, which is the
    /// usual presentation order — "what has used the most resources" is the
    /// primary question.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AppHistoryRecord> {
        let mut records: Vec<_> = self.history.values().cloned().collect();
        records.sort_by(|a, b| {
            b.cpu_seconds
                .partial_cmp(&a.cpu_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        records
    }

    /// Persists the history to a JSON file.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the file cannot be written. Unlike `load`, a
    /// save failure is propagated rather than silently ignored — a save that
    /// does nothing is a worse UX than a visible error.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        persist::save(path, &self.history)
    }

    /// Clears all history and resets internal state.
    pub fn clear(&mut self) {
        self.history.clear();
        self.prev_state.clear();
    }

    /// Returns the number of distinct executables in the history.
    #[must_use]
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Whether the history is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

/// A history store shared between the sampler thread and the UI.
///
/// The sampler writes on every tick; the App history screen reads on demand.
/// A plain `Mutex` is enough — the write is once a second and the read is once
/// per user action, so the two effectively never contend.
#[derive(Debug, Clone, Default)]
pub struct SharedHistory(std::sync::Arc<std::sync::Mutex<AppHistoryStore>>);

impl SharedHistory {
    /// Replaces the contents with a history loaded from disk.
    ///
    /// Separate from construction because the sampler creates the store
    /// before the app knows where its data directory is.
    pub fn load_from(&self, path: &Path) {
        self.with_mut(|store| *store = AppHistoryStore::load(path));
    }

    /// Folds one tick of samples in.
    pub fn record(&self, samples: &[RawProcess]) {
        self.with_mut(|store| store.record(samples));
    }

    /// Copies the accumulated records out.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AppHistoryRecord> {
        self.with_mut(|store| store.snapshot())
    }

    /// Discards everything, and the file backing it.
    pub fn clear_and_save(&self, path: &Path) {
        self.with_mut(|store| {
            store.clear();
            let _ = store.save(path);
        });
    }

    /// Writes the history to disk.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        self.with_mut(|store| store.save(path))
    }

    /// Runs `f` against the store, recovering from a poisoned lock.
    ///
    /// A panic in one of these short critical sections would otherwise make
    /// the history permanently inaccessible for the rest of the session. The
    /// data is an append-only usage tally, not something whose invariants a
    /// panic could subtly corrupt, so continuing with it is safer than
    /// bringing down a system monitor over a usage statistic.
    fn with_mut<T>(&self, f: impl FnOnce(&mut AppHistoryStore) -> T) -> T {
        let mut guard = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::process::RawProcess;
    use vitals_core::ids::{Pid, ProcessKey};

    fn stub_process(pid: u32, name: &str, cpu_100ns: u64, read: u64, write: u64) -> RawProcess {
        RawProcess {
            key: ProcessKey {
                pid: Pid(pid),
                start_time: 0,
            },
            parent: None,
            name: Some(name.into()),
            session_id: 1,
            base_priority: 8,
            thread_count: 1,
            handle_count: 100,
            kernel_time: cpu_100ns / 2,
            user_time: cpu_100ns / 2,
            create_time: 0,
            private_bytes: 10_000,
            working_set: 20_000,
            peak_working_set: 30_000,
            virtual_size: 100_000,
            page_faults: 10,
            hard_faults: 1,
            read_bytes: read,
            write_bytes: write,
            other_bytes: 0,
            read_ops: 1,
            write_ops: 1,
        }
    }

    #[test]
    fn store_records_and_snapshots() {
        let mut store = AppHistoryStore {
            history: AppHistory::new(),
            prev_state: BTreeMap::new(),
        };

        let samples = vec![
            stub_process(100, r"C:\app1.exe", 50_000_000, 1000, 2000),
            stub_process(200, r"C:\app2.exe", 30_000_000, 500, 1000),
        ];

        store.record(&samples);

        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 2);
        // Sorted by CPU descending.
        assert_eq!(snapshot[0].name, "app1.exe");
        assert_eq!(snapshot[1].name, "app2.exe");
    }

    #[test]
    fn store_clear_resets_state() {
        let mut store = AppHistoryStore {
            history: AppHistory::new(),
            prev_state: BTreeMap::new(),
        };

        let samples = vec![stub_process(100, r"C:\app.exe", 50_000_000, 1000, 2000)];

        store.record(&samples);
        assert_eq!(store.len(), 1);

        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn store_save_and_load_round_trip() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("vitals-test-store.json");

        let mut store = AppHistoryStore {
            history: AppHistory::new(),
            prev_state: BTreeMap::new(),
        };

        let samples = vec![stub_process(100, r"C:\test.exe", 50_000_000, 1024, 2048)];
        store.record(&samples);

        store.save(&path).expect("test fixture is well-formed");

        let loaded = AppHistoryStore::load(&path);
        assert_eq!(loaded.len(), 1);

        let snapshot = loaded.snapshot();
        assert_eq!(snapshot[0].name, "test.exe");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let path = PathBuf::from("does-not-exist-store.json");
        let store = AppHistoryStore::load(&path);
        assert!(store.is_empty());
    }
}
