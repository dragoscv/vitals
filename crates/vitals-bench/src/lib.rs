//! # vitals-bench
//!
//! Benchmark harness and scoring.
//!
//! ## Honesty requirements
//!
//! Benchmarks are trivially easy to make meaningless. These rules are
//! enforced by the harness rather than left to each suite:
//!
//! - Every result records the machine state it was measured under (thermal
//!   throttling, power plan, on battery). A CPU score taken while throttling
//!   is not comparable to one that was not, and silently mixing them makes
//!   the whole feature worthless.
//! - Runs are repeated and the harness reports the median plus the spread. A
//!   single run on a machine with background load is noise.
//! - A run is marked `tainted` if load appeared mid-benchmark, rather than
//!   being quietly published.
//!
//! ## What is here
//!
//! - [`workloads`] — the four measurements this build performs, and the
//!   `black_box` barriers that stop the optimiser deleting them.
//! - [`conditions`] — reads the real power plan, line status and background
//!   CPU so [`RunConditions`] is measured rather than defaulted.
//! - [`runner`] — the catalogue of every benchmark the UI may list, including
//!   the six that resolve to a stated reason instead of a number, plus the
//!   repeat-and-median loop.

use serde::{Deserialize, Serialize};

pub mod conditions;
pub mod rng;
pub mod runner;
pub mod workloads;

pub use conditions::ConditionsProbe;
pub use runner::{
    BenchmarkInfo, BenchmarkSuite, CATALOGUE, RUNS_PER_BENCHMARK, Runner, Unavailable, info_for,
    unavailable_reason, unit_for,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum BenchmarkKind {
    CpuSingleThread,
    CpuMultiThread,
    MemoryBandwidth,
    MemoryLatency,
    DiskSequentialRead,
    DiskSequentialWrite,
    DiskRandomRead,
    DiskRandomWrite,
    GpuCompute,
    GpuRender,
}

impl BenchmarkKind {
    /// The camelCase identifier the frontend uses.
    ///
    /// Deliberately separate from the `kebab-case` serde representation: that
    /// one is the persisted form in saved benchmark history and changing it
    /// would orphan stored results, whereas this is the IPC vocabulary the UI
    /// switches on. Coupling the two would mean a rename in either place
    /// silently breaking the other.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::CpuSingleThread => "cpuSingleThread",
            Self::CpuMultiThread => "cpuMultiThread",
            Self::MemoryBandwidth => "memoryBandwidth",
            Self::MemoryLatency => "memoryLatency",
            Self::DiskSequentialRead => "diskSequentialRead",
            Self::DiskSequentialWrite => "diskSequentialWrite",
            Self::DiskRandomRead => "diskRandomRead",
            Self::DiskRandomWrite => "diskRandomWrite",
            Self::GpuCompute => "gpuCompute",
            Self::GpuRender => "gpuRender",
        }
    }

    /// Parses an identifier sent by the frontend.
    ///
    /// Returns `None` for anything unrecognised so the command layer can
    /// refuse it by name, rather than falling back to a default kind and
    /// running a benchmark nobody asked for.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        CATALOGUE
            .iter()
            .map(|info| info.kind)
            .find(|kind| kind.id() == id)
    }
}

/// Conditions during a run, recorded so results are comparable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunConditions {
    pub power_plan: Option<String>,
    pub on_battery: bool,
    /// Any component reported throttling during the run.
    pub throttled: bool,
    /// Machine-wide CPU load from *other* processes, averaged over the run.
    pub background_load: f32,
    pub ambient_start_temp: Option<f32>,
    pub ambient_end_temp: Option<f32>,
}

impl RunConditions {
    /// Whether these conditions invalidate comparison with other results.
    #[must_use]
    pub fn is_tainted(&self) -> bool {
        self.throttled || self.on_battery || self.background_load > 10.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub kind: BenchmarkKind,
    /// Median across `runs`.
    pub score: f64,
    /// Individual run scores, so the spread is inspectable.
    pub runs: Vec<f64>,
    pub unit: String,
    pub duration_ms: u64,
    pub conditions: RunConditions,
    pub timestamp_ms: u64,
}

impl BenchmarkResult {
    /// Coefficient of variation across runs, as a percentage.
    ///
    /// The honest measure of whether a result is trustworthy. Above ~5% the
    /// machine was not quiet and the number should be treated with suspicion.
    #[must_use]
    pub fn variability(&self) -> Option<f64> {
        if self.runs.len() < 2 {
            return None;
        }
        let n = self.runs.len() as f64;
        let mean = self.runs.iter().sum::<f64>() / n;
        if mean == 0.0 {
            return None;
        }
        let variance = self.runs.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        Some(variance.sqrt() / mean * 100.0)
    }

    /// Whether this result should be presented as reliable.
    #[must_use]
    pub fn is_trustworthy(&self) -> bool {
        !self.conditions.is_tainted() && self.variability().is_none_or(|cv| cv < 5.0)
    }
}

/// Median of a set of scores.
///
/// Median rather than mean: benchmark distributions are right-skewed because
/// interference only ever makes a run slower, never faster, so the mean is
/// dragged by outliers that the median ignores.
#[must_use]
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    Some(if sorted.len().is_multiple_of(2) {
        f64::midpoint(sorted[mid - 1], sorted[mid])
    } else {
        sorted[mid]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conditions(throttled: bool, load: f32) -> RunConditions {
        RunConditions {
            power_plan: None,
            on_battery: false,
            throttled,
            background_load: load,
            ambient_start_temp: None,
            ambient_end_temp: None,
        }
    }

    fn result(runs: Vec<f64>, conditions: RunConditions) -> BenchmarkResult {
        BenchmarkResult {
            kind: BenchmarkKind::CpuMultiThread,
            score: median(&runs).unwrap_or(0.0),
            runs,
            unit: "points".into(),
            duration_ms: 1000,
            conditions,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn median_ignores_slow_outliers() {
        // A single interfered run must not drag the reported score.
        assert_eq!(median(&[100.0, 101.0, 99.0, 40.0, 100.0]), Some(100.0));
    }

    #[test]
    fn median_handles_even_counts_and_empty_input() {
        assert_eq!(median(&[1.0, 3.0]), Some(2.0));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn throttled_runs_are_tainted() {
        assert!(conditions(true, 0.0).is_tainted());
        assert!(!conditions(false, 0.0).is_tainted());
    }

    #[test]
    fn heavy_background_load_taints_a_run() {
        assert!(conditions(false, 40.0).is_tainted());
    }

    #[test]
    fn consistent_quiet_runs_are_trustworthy() {
        let r = result(vec![100.0, 101.0, 99.5], conditions(false, 1.0));
        assert!(r.is_trustworthy());
    }

    #[test]
    fn noisy_runs_are_not_trustworthy_even_when_untainted() {
        let r = result(vec![100.0, 50.0, 140.0], conditions(false, 1.0));
        assert!(
            !r.is_trustworthy(),
            "a 40% spread means the number is noise, whatever the conditions say"
        );
    }

    #[test]
    fn variability_needs_at_least_two_runs() {
        assert!(
            result(vec![100.0], conditions(false, 0.0))
                .variability()
                .is_none()
        );
    }

    #[test]
    fn every_id_round_trips() {
        // The frontend contract is these exact strings; a typo here is a
        // benchmark the UI can list but never start.
        for info in CATALOGUE {
            assert_eq!(BenchmarkKind::from_id(info.kind.id()), Some(info.kind));
        }
    }

    #[test]
    fn unknown_ids_are_rejected_rather_than_defaulted() {
        assert_eq!(BenchmarkKind::from_id("cpu-single-thread"), None);
        assert_eq!(BenchmarkKind::from_id(""), None);
        assert_eq!(BenchmarkKind::from_id("diskSequentialReadd"), None);
    }
}
