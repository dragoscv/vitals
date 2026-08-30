//! Accumulates per-process samples into the running history.
//!
//! Process samples arrive continuously at roughly 1 Hz. Each tick carries the
//! current state of every running process — CPU time, disk I/O, memory. The
//! accumulator's job is to fold these snapshots into per-executable totals
//! without double-counting when the same PID appears in consecutive ticks, and
//! without losing data when a process exits and restarts between ticks.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use vitals_core::ids::Pid;

use super::record::{AppHistory, AppHistoryRecord, normalise_executable};

/// Per-PID state remembered between ticks for delta computation.
#[derive(Debug, Clone)]
pub struct ProcessState {
    /// Last-seen cumulative CPU time in 100ns units. Deltas are computed
    /// against this to isolate the work done between ticks.
    cpu_time_100ns: u64,
    /// Last-seen cumulative disk read bytes.
    disk_read_bytes: u64,
    /// Last-seen cumulative disk write bytes.
    disk_write_bytes: u64,
}

/// Per-process sample shape expected by the accumulator.
///
/// This mirrors the fields available from `vitals_win::process::RawProcess`
/// that matter for history. By defining a trait rather than taking a concrete
/// type, tests can inject simple stubs without needing the full Windows
/// enumeration machinery.
pub trait ProcessSample {
    fn pid(&self) -> Pid;
    fn executable_path(&self) -> Option<&Path>;
    fn cpu_time_100ns(&self) -> u64;
    fn disk_read_bytes(&self) -> u64;
    fn disk_write_bytes(&self) -> u64;
    fn private_bytes(&self) -> u64;
}

/// Accumulates one tick worth of process samples into the history.
///
/// This function is stateful: it remembers the previous tick's state per PID
/// so it can compute deltas. PIDs that appear for the first time in this tick
/// are new sessions; PIDs that were present last tick but not this tick have
/// exited. Deltas are attributed to the executable, not the PID, so a program
/// that restarts gets its time summed across launches.
pub fn accumulate<S: ProcessSample>(
    history: &mut AppHistory,
    samples: &[S],
    prev_state: &mut BTreeMap<Pid, ProcessState>,
    now: SystemTime,
) {
    let mut seen_this_tick = BTreeMap::new();

    for sample in samples {
        let pid = sample.pid();
        let Some(path) = sample.executable_path() else {
            // System processes and protected processes report no image name.
            // Accumulating them would attribute their work to a blank key,
            // which is misleading. Skip.
            continue;
        };

        let executable = normalise_executable(path);
        let cpu_100ns = sample.cpu_time_100ns();
        let disk_read = sample.disk_read_bytes();
        let disk_write = sample.disk_write_bytes();
        let private = sample.private_bytes();

        let entry = history
            .entry(executable.clone())
            .or_insert_with(|| AppHistoryRecord::new(executable.clone(), now));

        if let Some(prev) = prev_state.get(&pid) {
            // Same PID as last tick. Compute deltas, handling counter resets.
            // Counters are cumulative and monotonic unless the process
            // restarts, in which case they snap back to near-zero. A negative
            // delta signals a reset; treat it as the full new value.
            let cpu_delta = if cpu_100ns >= prev.cpu_time_100ns {
                cpu_100ns - prev.cpu_time_100ns
            } else {
                cpu_100ns
            };
            let read_delta = if disk_read >= prev.disk_read_bytes {
                disk_read - prev.disk_read_bytes
            } else {
                disk_read
            };
            let write_delta = if disk_write >= prev.disk_write_bytes {
                disk_write - prev.disk_write_bytes
            } else {
                disk_write
            };

            entry.cpu_seconds += (cpu_delta as f64) / 10_000_000.0;
            entry.disk_read_bytes = entry.disk_read_bytes.saturating_add(read_delta);
            entry.disk_write_bytes = entry.disk_write_bytes.saturating_add(write_delta);
        } else {
            // New PID. This is the start of a new session.
            entry.sessions += 1;
        }

        entry.peak_private_bytes = entry.peak_private_bytes.max(private);
        entry.last_seen = now;

        // Remember this tick's state for the next delta.
        seen_this_tick.insert(
            pid,
            ProcessState {
                cpu_time_100ns: cpu_100ns,
                disk_read_bytes: disk_read,
                disk_write_bytes: disk_write,
            },
        );
    }

    // PIDs present last tick but not this tick have exited. Drop their state.
    *prev_state = seen_this_tick;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[derive(Debug, Clone)]
    struct StubSample {
        pid: Pid,
        path: PathBuf,
        cpu_100ns: u64,
        read: u64,
        write: u64,
        private: u64,
    }

    impl ProcessSample for StubSample {
        fn pid(&self) -> Pid {
            self.pid
        }
        fn executable_path(&self) -> Option<&Path> {
            Some(&self.path)
        }
        fn cpu_time_100ns(&self) -> u64 {
            self.cpu_100ns
        }
        fn disk_read_bytes(&self) -> u64 {
            self.read
        }
        fn disk_write_bytes(&self) -> u64 {
            self.write
        }
        fn private_bytes(&self) -> u64 {
            self.private
        }
    }

    #[test]
    fn first_tick_starts_a_session() {
        let mut history = AppHistory::new();
        let mut prev = BTreeMap::new();
        let now = SystemTime::now();

        let samples = vec![StubSample {
            pid: Pid(100),
            path: PathBuf::from(r"C:\test.exe"),
            cpu_100ns: 50_000_000,
            read: 1024,
            write: 2048,
            private: 4096,
        }];

        accumulate(&mut history, &samples, &mut prev, now);

        let key = normalise_executable(&PathBuf::from(r"C:\test.exe"));
        let record = history
            .get(&key)
            .expect("record for the accumulated executable");
        assert_eq!(record.sessions, 1);
        assert_eq!(record.peak_private_bytes, 4096);
    }

    #[test]
    fn second_tick_computes_delta() {
        let mut history = AppHistory::new();
        let mut prev = BTreeMap::new();
        let now = SystemTime::now();

        let samples_1 = vec![StubSample {
            pid: Pid(100),
            path: PathBuf::from(r"C:\test.exe"),
            cpu_100ns: 50_000_000, // 5 seconds
            read: 1024,
            write: 2048,
            private: 4096,
        }];

        accumulate(&mut history, &samples_1, &mut prev, now);

        let samples_2 = vec![StubSample {
            pid: Pid(100),
            path: PathBuf::from(r"C:\test.exe"),
            cpu_100ns: 100_000_000, // 10 seconds total
            read: 2048,
            write: 4096,
            private: 8192,
        }];

        accumulate(&mut history, &samples_2, &mut prev, now);

        let key = normalise_executable(&PathBuf::from(r"C:\test.exe"));
        let record = history
            .get(&key)
            .expect("record for the accumulated executable");
        assert_eq!(record.sessions, 1);
        assert!((record.cpu_seconds - 5.0).abs() < 0.01);
        assert_eq!(record.disk_read_bytes, 1024);
        assert_eq!(record.disk_write_bytes, 2048);
        assert_eq!(record.peak_private_bytes, 8192);
    }

    #[test]
    fn restart_increments_sessions() {
        let mut history = AppHistory::new();
        let mut prev = BTreeMap::new();
        let now = SystemTime::now();

        let samples_1 = vec![StubSample {
            pid: Pid(100),
            path: PathBuf::from(r"C:\test.exe"),
            cpu_100ns: 50_000_000,
            read: 1024,
            write: 2048,
            private: 4096,
        }];

        accumulate(&mut history, &samples_1, &mut prev, now);

        // Process exits; next tick shows a new PID with the same executable.
        let samples_2 = vec![StubSample {
            pid: Pid(200),
            path: PathBuf::from(r"C:\test.exe"),
            cpu_100ns: 10_000_000,
            read: 512,
            write: 1024,
            private: 2048,
        }];

        accumulate(&mut history, &samples_2, &mut prev, now);

        let key = normalise_executable(&PathBuf::from(r"C:\test.exe"));
        let record = history
            .get(&key)
            .expect("record for the accumulated executable");
        assert_eq!(record.sessions, 2);
    }

    #[test]
    fn counter_reset_is_not_negative_delta() {
        let mut history = AppHistory::new();
        let mut prev = BTreeMap::new();
        let now = SystemTime::now();

        let samples_1 = vec![StubSample {
            pid: Pid(100),
            path: PathBuf::from(r"C:\test.exe"),
            cpu_100ns: 100_000_000,
            read: 10_000,
            write: 20_000,
            private: 4096,
        }];

        accumulate(&mut history, &samples_1, &mut prev, now);

        // Same PID but counters reset — rare, but possible if the process
        // truly is the same PID reused by the OS.
        let samples_2 = vec![StubSample {
            pid: Pid(100),
            path: PathBuf::from(r"C:\test.exe"),
            cpu_100ns: 5_000_000, // Lower than before
            read: 500,
            write: 1000,
            private: 2048,
        }];

        accumulate(&mut history, &samples_2, &mut prev, now);

        let key = normalise_executable(&PathBuf::from(r"C:\test.exe"));
        let record = history
            .get(&key)
            .expect("record for the accumulated executable");
        // The first tick credits nothing. A process may have been running for
        // hours before Vitals started, and its accumulated counters describe
        // time this history did not observe — banking them on first sight
        // would invent a past. Only the second tick contributes, and because
        // the counters went backwards it is treated as a fresh start and
        // credited in full rather than underflowing.
        assert!((record.cpu_seconds - 0.5).abs() < 0.01);
        assert_eq!(record.disk_read_bytes, 500);
        assert_eq!(record.disk_write_bytes, 1000);
    }

    #[test]
    fn multiple_processes_same_executable() {
        let mut history = AppHistory::new();
        let mut prev = BTreeMap::new();
        let now = SystemTime::now();

        let samples = vec![
            StubSample {
                pid: Pid(100),
                path: PathBuf::from(r"C:\chrome.exe"),
                cpu_100ns: 30_000_000,
                read: 1000,
                write: 2000,
                private: 5000,
            },
            StubSample {
                pid: Pid(200),
                path: PathBuf::from(r"C:\chrome.exe"),
                cpu_100ns: 20_000_000,
                read: 500,
                write: 1000,
                private: 3000,
            },
        ];

        accumulate(&mut history, &samples, &mut prev, now);

        let key = normalise_executable(&PathBuf::from(r"C:\chrome.exe"));
        let record = history
            .get(&key)
            .expect("record for the accumulated executable");
        assert_eq!(record.sessions, 2);
        assert_eq!(record.peak_private_bytes, 5000);
    }

    #[test]
    fn skips_samples_with_no_path() {
        #[derive(Debug, Clone)]
        struct NoPathSample {
            pid: Pid,
        }

        impl ProcessSample for NoPathSample {
            fn pid(&self) -> Pid {
                self.pid
            }
            fn executable_path(&self) -> Option<&Path> {
                None
            }
            fn cpu_time_100ns(&self) -> u64 {
                10_000_000
            }
            fn disk_read_bytes(&self) -> u64 {
                100
            }
            fn disk_write_bytes(&self) -> u64 {
                200
            }
            fn private_bytes(&self) -> u64 {
                1000
            }
        }

        let mut history = AppHistory::new();
        let mut prev = BTreeMap::new();
        let now = SystemTime::now();

        let samples = vec![NoPathSample { pid: Pid(0) }];

        accumulate(&mut history, &samples, &mut prev, now);

        assert!(history.is_empty());
    }
}
