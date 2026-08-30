//! Per-session resource rollup from process samples.
//!
//! Pure functions that fold process metrics into per-session totals. This is
//! deliberately separate from the WTS enumeration so it can be unit-tested
//! without touching Windows APIs.

use std::collections::HashMap;

use vitals_core::ids::Pid;

/// A minimal process sample carrying only what the rollup needs.
///
/// The real sampler produces a richer structure; this is the subset used here.
/// Accepting a concrete type rather than a generic or trait means tests can
/// construct samples without importing the full core types.
#[derive(Debug, Clone)]
pub struct ProcessSample {
    pub pid: Pid,
    pub session_id: u32,
    /// Instantaneous CPU percentage, 0.0 to 100.0 * core count.
    pub cpu_percent: f64,
    /// Private working set in bytes.
    pub private_bytes: u64,
}

/// Per-session resource totals.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRollup {
    pub session_id: u32,
    pub process_count: usize,
    /// Sum of CPU percentages of all processes in this session. Can exceed
    /// 100.0 on a multi-core machine.
    pub cpu_percent: f64,
    /// Sum of private bytes of all processes in this session.
    pub memory_bytes: u64,
}

impl SessionRollup {
    /// An empty rollup for a session with no processes.
    #[must_use]
    pub const fn empty(session_id: u32) -> Self {
        Self {
            session_id,
            process_count: 0,
            cpu_percent: 0.0,
            memory_bytes: 0,
        }
    }
}

/// Aggregates process samples into per-session totals.
///
/// This is a pure function and does not touch Windows APIs. The test coverage
/// is in this module rather than behind `#[cfg(windows)]`.
#[must_use]
pub fn rollup_by_session(processes: &[ProcessSample]) -> Vec<SessionRollup> {
    let mut map: HashMap<u32, SessionRollup> = HashMap::new();

    for proc in processes {
        let entry = map.entry(proc.session_id).or_insert_with(|| SessionRollup {
            session_id: proc.session_id,
            process_count: 0,
            cpu_percent: 0.0,
            memory_bytes: 0,
        });

        entry.process_count += 1;
        entry.cpu_percent += proc.cpu_percent;
        entry.memory_bytes = entry.memory_bytes.saturating_add(proc.private_bytes);
    }

    let mut result: Vec<_> = map.into_values().collect();
    result.sort_by_key(|r| r.session_id);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(session_id: u32, cpu: f64, mem: u64) -> ProcessSample {
        ProcessSample {
            // Wrapping, because one test deliberately passes `u32::MAX` as a
            // session id. The pid only has to be distinct per sample here, so
            // wrapping is fine — panicking would be the test helper failing,
            // not the code under test.
            pid: Pid(1000_u32.wrapping_add(session_id)),
            session_id,
            cpu_percent: cpu,
            private_bytes: mem,
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        let result = rollup_by_session(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn single_process_single_session() {
        let processes = vec![sample(1, 12.5, 1024 * 1024)];
        let result = rollup_by_session(&processes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].session_id, 1);
        assert_eq!(result[0].process_count, 1);
        assert!((result[0].cpu_percent - 12.5).abs() < 1e-9);
        assert_eq!(result[0].memory_bytes, 1024 * 1024);
    }

    #[test]
    fn multiple_processes_same_session() {
        let processes = vec![
            sample(1, 10.0, 500_000),
            sample(1, 5.0, 300_000),
            sample(1, 2.5, 200_000),
        ];
        let result = rollup_by_session(&processes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].session_id, 1);
        assert_eq!(result[0].process_count, 3);
        assert!((result[0].cpu_percent - 17.5).abs() < 1e-9);
        assert_eq!(result[0].memory_bytes, 1_000_000);
    }

    #[test]
    fn multiple_sessions() {
        let processes = vec![
            sample(0, 1.0, 100_000),
            sample(0, 0.5, 50_000),
            sample(1, 10.0, 1_000_000),
            sample(1, 5.0, 500_000),
            sample(2, 20.0, 2_000_000),
        ];
        let result = rollup_by_session(&processes);

        // Results are sorted by session_id.
        assert_eq!(result.len(), 3);

        assert_eq!(result[0].session_id, 0);
        assert_eq!(result[0].process_count, 2);
        assert!((result[0].cpu_percent - 1.5).abs() < 1e-9);
        assert_eq!(result[0].memory_bytes, 150_000);

        assert_eq!(result[1].session_id, 1);
        assert_eq!(result[1].process_count, 2);
        assert!((result[1].cpu_percent - 15.0).abs() < 1e-9);
        assert_eq!(result[1].memory_bytes, 1_500_000);

        assert_eq!(result[2].session_id, 2);
        assert_eq!(result[2].process_count, 1);
        assert!((result[2].cpu_percent - 20.0).abs() < 1e-9);
        assert_eq!(result[2].memory_bytes, 2_000_000);
    }

    #[test]
    fn process_with_unknown_session() {
        // A process whose session_id could not be determined (should never
        // happen, but if it does, it gets its own rollup entry).
        let processes = vec![sample(0xFFFF_FFFF, 5.0, 100_000)];
        let result = rollup_by_session(&processes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].session_id, 0xFFFF_FFFF);
        assert_eq!(result[0].process_count, 1);
    }

    #[test]
    fn cpu_can_exceed_one_hundred() {
        // Multiple processes in one session can consume more than 100% CPU
        // on a multi-core machine.
        let processes = vec![
            sample(1, 80.0, 100_000),
            sample(1, 75.0, 100_000),
            sample(1, 60.0, 100_000),
        ];
        let result = rollup_by_session(&processes);

        assert!((result[0].cpu_percent - 215.0).abs() < 1e-9);
    }

    #[test]
    fn memory_saturates_not_wraps() {
        // Memory totals use saturating addition to prevent wraparound on an
        // absurdly large memory footprint.
        let processes = vec![sample(1, 0.0, u64::MAX - 1000), sample(1, 0.0, 2000)];
        let result = rollup_by_session(&processes);

        assert_eq!(result[0].memory_bytes, u64::MAX);
    }
}
