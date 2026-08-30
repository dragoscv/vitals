//! What can be run, what cannot, and the repeat-and-median loop.
//!
//! # Why six of the ten benchmarks are unavailable
//!
//! The catalogue below lists every [`BenchmarkKind`], including the four disk
//! and two GPU ones this build will not perform, each with the mechanism that
//! would be required. This follows the pattern set by
//! `vitals_win::sensors::driver::DRIVER_GAPS`: a capability the UI might
//! reasonably offer resolves either to a measurement or to a stated reason,
//! never to silence.
//!
//! The reasons are not the same shape as the driver gaps, though, and the
//! difference is worth stating. A CPU temperature is missing because it is
//! *impossible* from user mode. A disk benchmark is missing because it is
//! *undesirable without asking*: writing several gigabytes to prove a
//! sequential-write figure consumes a measurable slice of an SSD's rated
//! endurance and evicts the user's working set from the filesystem cache.
//! That is a real, irreversible side effect on hardware the user owns, and it
//! needs consent and a target volume before it may happen — not a default.
//!
//! GPU is a third case again: a compute or render score needs a D3D12 or
//! Vulkan device, a command queue and a compiled shader, none of which this
//! process creates. The Tauri webview owns the only graphics context in the
//! application and it is not one a benchmark may borrow.

use std::time::{Duration, Instant};

use crate::conditions::ConditionsProbe;
use crate::workloads::{self, MemoryBuffer};
use crate::{BenchmarkKind, BenchmarkResult, median};

/// Runs per benchmark.
///
/// Three, not one: the median of three discards a single interfered run,
/// which is the common case on a machine the user is still using. Five would
/// be better statistics and roughly double the suite's duration, which is the
/// wrong trade for something a user is watching a progress bar for.
pub const RUNS_PER_BENCHMARK: usize = 3;

/// Why a benchmark cannot be run here.
///
/// Kept as a small enum rather than free-form text so the frontend can branch
/// on it and translate it, in the same way [`crate::BenchmarkKind`] is an
/// enum rather than a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// Real, irreversible writes to the user's storage. Needs an explicit
    /// opt-in and a chosen volume before it may run.
    NeedsConsent,
    /// Requires a D3D12 or Vulkan device and a compiled shader, which this
    /// process does not create.
    NeedsGraphicsContext,
}

impl Unavailable {
    /// The key the UI receives. Stable; the frontend switches on it.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::NeedsConsent => "needsConsent",
            Self::NeedsGraphicsContext => "needsGraphicsContext",
        }
    }
}

/// One entry in the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkInfo {
    pub kind: BenchmarkKind,
    /// `None` when the benchmark can be run.
    pub unavailable: Option<Unavailable>,
    /// Whole seconds the whole repeat loop is expected to take, so the UI can
    /// warn before starting. Rounded up from measured figures on a mid-range
    /// desktop; a slower machine takes longer and the estimate is labelled as
    /// rough for exactly that reason.
    pub estimated_seconds: u32,
}

impl BenchmarkInfo {
    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.unavailable.is_none()
    }
}

/// Every benchmark the UI may list, in display order.
pub const CATALOGUE: &[BenchmarkInfo] = &[
    BenchmarkInfo {
        kind: BenchmarkKind::CpuSingleThread,
        unavailable: None,
        estimated_seconds: 2,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::CpuMultiThread,
        unavailable: None,
        estimated_seconds: 3,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::MemoryBandwidth,
        // Includes the one-off half-gigabyte allocation and its page faults.
        unavailable: None,
        estimated_seconds: 3,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::MemoryLatency,
        unavailable: None,
        estimated_seconds: 3,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::DiskSequentialRead,
        unavailable: Some(Unavailable::NeedsConsent),
        estimated_seconds: 0,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::DiskSequentialWrite,
        unavailable: Some(Unavailable::NeedsConsent),
        estimated_seconds: 0,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::DiskRandomRead,
        unavailable: Some(Unavailable::NeedsConsent),
        estimated_seconds: 0,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::DiskRandomWrite,
        unavailable: Some(Unavailable::NeedsConsent),
        estimated_seconds: 0,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::GpuCompute,
        unavailable: Some(Unavailable::NeedsGraphicsContext),
        estimated_seconds: 0,
    },
    BenchmarkInfo {
        kind: BenchmarkKind::GpuRender,
        unavailable: Some(Unavailable::NeedsGraphicsContext),
        estimated_seconds: 0,
    },
];

/// The catalogue entry for a kind.
#[must_use]
pub fn info_for(kind: BenchmarkKind) -> Option<BenchmarkInfo> {
    CATALOGUE.iter().copied().find(|info| info.kind == kind)
}

/// Why running `kind` would be refused, if it would be.
#[must_use]
pub fn unavailable_reason(kind: BenchmarkKind) -> Option<Unavailable> {
    info_for(kind).and_then(|info| info.unavailable)
}

/// A whole suite of results.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkSuite {
    pub results: Vec<BenchmarkResult>,
    pub total_duration: Duration,
}

/// Runs benchmarks, reusing the expensive memory allocation across them.
///
/// A struct rather than a free function because the 512 MB working set costs
/// more to allocate and fault in than a memory benchmark costs to run, and
/// paying that once for the suite instead of once per run is the difference
/// between three seconds and fifteen.
#[derive(Debug, Default)]
pub struct Runner {
    memory: Option<MemoryBuffer>,
}

impl Runner {
    #[must_use]
    pub const fn new() -> Self {
        Self { memory: None }
    }

    /// Runs one benchmark [`RUNS_PER_BENCHMARK`] times.
    ///
    /// Returns `Err` with the reason when the kind is not runnable here,
    /// rather than skipping it: a caller that asked for a disk benchmark and
    /// silently received a suite without one has been misled about what it
    /// measured.
    ///
    /// # Errors
    ///
    /// Returns the [`Unavailable`] reason for disk and GPU kinds.
    pub fn run(&mut self, kind: BenchmarkKind) -> Result<BenchmarkResult, Unavailable> {
        if let Some(reason) = unavailable_reason(kind) {
            return Err(reason);
        }

        // Allocated before the probe starts so the page faults are not
        // counted as background load, and before the timer so the first run
        // is not charged for them.
        if matches!(
            kind,
            BenchmarkKind::MemoryBandwidth | BenchmarkKind::MemoryLatency
        ) && self.memory.is_none()
        {
            self.memory = Some(MemoryBuffer::allocate());
        }

        let probe = ConditionsProbe::start();
        let started = Instant::now();

        let mut runs = Vec::with_capacity(RUNS_PER_BENCHMARK);
        for _ in 0..RUNS_PER_BENCHMARK {
            runs.push(self.one_run(kind).score);
        }

        let elapsed = started.elapsed();
        let conditions = probe.finish(elapsed);

        Ok(BenchmarkResult {
            kind,
            // Every individual run is kept in `runs`; the score is only the
            // headline. A consumer that wants to judge the spread for itself
            // has the raw data rather than a summary statistic.
            score: median(&runs).unwrap_or(0.0),
            runs,
            unit: unit_for(kind).to_owned(),
            duration_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            conditions,
            timestamp_ms: unix_millis(),
        })
    }

    /// Runs every available benchmark in catalogue order.
    #[must_use]
    pub fn run_all_available(&mut self) -> BenchmarkSuite {
        let kinds: Vec<_> = CATALOGUE
            .iter()
            .filter(|info| info.is_available())
            .map(|info| info.kind)
            .collect();

        self.run_many(&kinds).unwrap_or(BenchmarkSuite {
            results: Vec::new(),
            total_duration: Duration::ZERO,
        })
    }

    /// Runs the given kinds, failing on the first unavailable one.
    ///
    /// # Errors
    ///
    /// Returns the reason for the first kind that cannot be run. Checked
    /// before any work starts, so a request containing one bad id does not
    /// burn ten seconds of the user's CPU before refusing.
    pub fn run_many(&mut self, kinds: &[BenchmarkKind]) -> Result<BenchmarkSuite, Unavailable> {
        for &kind in kinds {
            if let Some(reason) = unavailable_reason(kind) {
                return Err(reason);
            }
        }

        let started = Instant::now();
        let mut results = Vec::with_capacity(kinds.len());
        for &kind in kinds {
            results.push(self.run(kind)?);
        }

        Ok(BenchmarkSuite {
            results,
            total_duration: started.elapsed(),
        })
    }

    fn one_run(&self, kind: BenchmarkKind) -> workloads::Measurement {
        match kind {
            BenchmarkKind::CpuSingleThread => workloads::cpu_single_thread(),
            BenchmarkKind::CpuMultiThread => workloads::cpu_multi_thread(),
            BenchmarkKind::MemoryBandwidth => self
                .memory
                .as_ref()
                .map_or(EMPTY_MEASUREMENT, workloads::memory_bandwidth),
            BenchmarkKind::MemoryLatency => self
                .memory
                .as_ref()
                .map_or(EMPTY_MEASUREMENT, workloads::memory_latency),
            // Unreachable: `run` rejects these before dispatch. A zero here
            // would be a fabricated measurement, which is why it is paired
            // with a zero duration that the trustworthiness check rejects.
            _ => EMPTY_MEASUREMENT,
        }
    }
}

const EMPTY_MEASUREMENT: workloads::Measurement = workloads::Measurement {
    score: 0.0,
    elapsed: Duration::ZERO,
};

/// What each benchmark actually measured.
///
/// Never "points". A dimensionless score implies the numbers can be compared
/// across benchmarks and across machines with different workloads, which they
/// cannot; a unit forces the claim to stay as small as the evidence.
#[must_use]
pub const fn unit_for(kind: BenchmarkKind) -> &'static str {
    match kind {
        BenchmarkKind::CpuSingleThread | BenchmarkKind::CpuMultiThread => "ops/s",
        BenchmarkKind::MemoryBandwidth
        | BenchmarkKind::DiskSequentialRead
        | BenchmarkKind::DiskSequentialWrite
        | BenchmarkKind::DiskRandomRead
        | BenchmarkKind::DiskRandomWrite => "MB/s",
        BenchmarkKind::MemoryLatency => "ns",
        BenchmarkKind::GpuCompute | BenchmarkKind::GpuRender => "fps",
    }
}

/// Wall-clock milliseconds since the Unix epoch.
///
/// Zero if the system clock is set before 1970, which is not a state worth
/// propagating an error for.
fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_covers_every_kind_exactly_once() {
        // The UI renders straight from this list, so a kind missing here is
        // a benchmark that silently does not exist.
        assert_eq!(CATALOGUE.len(), 10);
        for info in CATALOGUE {
            assert_eq!(
                CATALOGUE.iter().filter(|c| c.kind == info.kind).count(),
                1,
                "{:?} appears more than once",
                info.kind
            );
        }
    }

    #[test]
    fn only_cpu_and_memory_are_available() {
        let available: Vec<_> = CATALOGUE
            .iter()
            .filter(|c| c.is_available())
            .map(|c| c.kind)
            .collect();

        assert_eq!(
            available,
            vec![
                BenchmarkKind::CpuSingleThread,
                BenchmarkKind::CpuMultiThread,
                BenchmarkKind::MemoryBandwidth,
                BenchmarkKind::MemoryLatency,
            ]
        );
    }

    #[test]
    fn every_unavailable_entry_states_a_reason() {
        for info in CATALOGUE.iter().filter(|c| !c.is_available()) {
            let reason = info.unavailable.expect("filtered on being unavailable");
            assert!(!reason.key().is_empty());
        }
    }

    #[test]
    fn disk_needs_consent_and_gpu_needs_a_context() {
        assert_eq!(
            unavailable_reason(BenchmarkKind::DiskSequentialWrite),
            Some(Unavailable::NeedsConsent)
        );
        assert_eq!(
            unavailable_reason(BenchmarkKind::GpuRender),
            Some(Unavailable::NeedsGraphicsContext)
        );
        assert_eq!(unavailable_reason(BenchmarkKind::CpuSingleThread), None);
    }

    #[test]
    fn running_an_unavailable_benchmark_errors() {
        let mut runner = Runner::new();
        assert_eq!(
            runner.run(BenchmarkKind::DiskRandomRead),
            Err(Unavailable::NeedsConsent)
        );
        assert_eq!(
            runner.run(BenchmarkKind::GpuCompute),
            Err(Unavailable::NeedsGraphicsContext)
        );
    }

    #[test]
    fn a_batch_containing_one_unavailable_id_is_rejected_before_any_work() {
        let mut runner = Runner::new();
        let started = Instant::now();
        let outcome = runner.run_many(&[BenchmarkKind::CpuSingleThread, BenchmarkKind::GpuCompute]);

        assert_eq!(outcome, Err(Unavailable::NeedsGraphicsContext));
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "the CPU benchmark ran before the request was refused"
        );
    }

    #[test]
    fn units_are_never_dimensionless() {
        for info in CATALOGUE {
            let unit = unit_for(info.kind);
            assert!(!unit.is_empty());
            assert_ne!(unit, "points", "{:?} has no honest unit", info.kind);
        }
    }

    #[test]
    fn a_cpu_result_records_every_run_and_real_conditions() {
        let mut runner = Runner::new();
        let result = runner
            .run(BenchmarkKind::CpuSingleThread)
            .expect("CPU benchmarks are always available");

        assert_eq!(result.runs.len(), RUNS_PER_BENCHMARK);
        assert!(result.runs.iter().all(|r| *r > 0.0 && r.is_finite()));
        assert!(result.score > 0.0);
        assert_eq!(result.unit, "ops/s");
        assert!(result.duration_ms > 0, "the suite reported no elapsed time");
        assert!(result.variability().is_some(), "three runs must give a CV");
        assert!(result.timestamp_ms > 1_600_000_000_000, "clock not read");

        // Conditions must be populated from the machine, not defaulted.
        assert!((0.0..=100.0).contains(&result.conditions.background_load));
        #[cfg(windows)]
        assert!(result.conditions.power_plan.is_some());
    }

    #[test]
    // Allocates the full 512 MB working set and runs the benchmark three
    // times. The bounds below are the point of the test and only mean
    // something against a real working set, so it cannot be shrunk — it is
    // run deliberately instead:
    //
    //   cargo test -p vitals-bench --release -- --ignored
    #[ignore = "allocates 512 MB and runs the real workload; run with --ignored"]
    fn a_memory_result_reports_bandwidth_in_mb_per_second() {
        let mut runner = Runner::new();
        let result = runner
            .run(BenchmarkKind::MemoryBandwidth)
            .expect("memory benchmarks are always available");

        assert_eq!(result.unit, "MB/s");
        // Below 1 GB/s no machine that can run this program exists; above
        // 10 TB/s means the traversal was elided or served from cache.
        assert!(
            result.score > 1_000.0 && result.score < 10_000_000.0,
            "{} MB/s is not a plausible DRAM figure",
            result.score
        );
    }
}
