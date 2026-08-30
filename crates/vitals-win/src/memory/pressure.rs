//! Memory pressure classification.
//!
//! Pure and platform-free so the judgement can be tested exhaustively.
//!
//! ## Why utilisation alone is the wrong signal
//!
//! "Memory used: 92%" is the number every tool shows and it is close to
//! meaningless. A modern OS deliberately fills RAM with cache, because unused
//! RAM is wasted RAM — a machine sitting at 92% with a healthy commit
//! headroom and no faulting is working exactly as designed.
//!
//! What actually predicts a machine falling over is **commit charge
//! approaching the commit limit** (allocations start failing) and **sustained
//! hard page faulting** (the machine is thrashing to disk). Those are the
//! signals here.

use vitals_core::units::{Bytes, Percent};

/// How much trouble the machine's memory is actually in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressure {
    /// Plenty of headroom.
    Normal,
    /// Commit is climbing. Nothing is failing yet, but a large allocation
    /// might.
    Elevated,
    /// Allocations are close to failing, or the machine is paging heavily.
    High,
    /// Allocations are failing or about to. Applications will start crashing.
    Critical,
}

impl MemoryPressure {
    /// Whether the user should be told without being asked.
    #[must_use]
    pub const fn warrants_alert(self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

/// Inputs to the classification.
#[derive(Debug, Clone, Copy)]
pub struct PressureInputs {
    pub physical_total: Bytes,
    pub physical_available: Bytes,
    pub commit_used: Bytes,
    pub commit_limit: Bytes,
    /// Hard faults per second — faults that actually hit the disk.
    ///
    /// `None` when unavailable. Soft faults are irrelevant here: they are
    /// satisfied from the standby list in microseconds and happen constantly
    /// on a perfectly healthy machine.
    pub hard_faults_per_sec: Option<u64>,
}

/// Sustained hard faulting above this rate means the machine is thrashing.
///
/// Chosen from the physics rather than from taste: each hard fault is a disk
/// round trip. A few hundred per second is normal during application launch;
/// a sustained thousand means the working set no longer fits in RAM and every
/// process is waiting on storage.
const THRASHING_FAULTS_PER_SEC: u64 = 1_000;

/// Classifies memory pressure.
#[must_use]
pub fn classify_pressure(inputs: PressureInputs) -> MemoryPressure {
    let commit = Percent::ratio(inputs.commit_used.get(), inputs.commit_limit.get()).get();

    let available_ratio = if inputs.physical_total.get() == 0 {
        100.0
    } else {
        Percent::ratio(inputs.physical_available.get(), inputs.physical_total.get()).get()
    };

    let thrashing = inputs
        .hard_faults_per_sec
        .is_some_and(|f| f >= THRASHING_FAULTS_PER_SEC);

    // Commit exhaustion is the hard failure mode: past this point `malloc`
    // returns null and applications crash, regardless of physical memory.
    if commit >= 98.0 {
        return MemoryPressure::Critical;
    }

    // Thrashing with almost no physical memory left is functionally critical
    // even if commit has headroom — the machine is unusable.
    if thrashing && available_ratio < 5.0 {
        return MemoryPressure::Critical;
    }

    if commit >= 90.0 || thrashing {
        return MemoryPressure::High;
    }

    // Physical exhaustion alone is only "elevated": the OS still has the
    // page file, and this is the state a healthy cache-filled machine sits in.
    if commit >= 75.0 || available_ratio < 10.0 {
        return MemoryPressure::Elevated;
    }

    MemoryPressure::Normal
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn inputs(
        total_gib: u64,
        available_gib: u64,
        commit_gib: u64,
        limit_gib: u64,
        faults: Option<u64>,
    ) -> PressureInputs {
        PressureInputs {
            physical_total: Bytes(total_gib * GIB),
            physical_available: Bytes(available_gib * GIB),
            commit_used: Bytes(commit_gib * GIB),
            commit_limit: Bytes(limit_gib * GIB),
            hard_faults_per_sec: faults,
        }
    }

    #[test]
    fn an_idle_machine_is_normal() {
        assert_eq!(
            classify_pressure(inputs(32, 24, 8, 64, Some(0))),
            MemoryPressure::Normal
        );
    }

    #[test]
    fn high_utilisation_with_commit_headroom_is_not_high_pressure() {
        // THE key case. 30 of 32 GiB in use looks alarming and is not:
        // the OS has filled RAM with cache, commit is comfortable, and
        // nothing is faulting. Every tool that alerts here is crying wolf.
        let p = classify_pressure(inputs(32, 2, 20, 64, Some(10)));
        assert!(
            p < MemoryPressure::High,
            "a cache-filled machine must not be reported as under high pressure, got {p:?}"
        );
    }

    #[test]
    fn commit_near_the_limit_is_critical() {
        // 63 of 64 GiB committed: allocations are about to start failing.
        assert_eq!(
            classify_pressure(inputs(32, 8, 63, 64, Some(0))),
            MemoryPressure::Critical
        );
    }

    #[test]
    fn commit_climbing_is_high_before_it_is_critical() {
        assert_eq!(
            classify_pressure(inputs(32, 8, 59, 64, Some(0))),
            MemoryPressure::High
        );
    }

    #[test]
    fn sustained_hard_faulting_is_high_pressure_regardless_of_commit() {
        // Thrashing is the state users describe as "my PC froze", and it can
        // happen with commit far from its limit.
        assert_eq!(
            classify_pressure(inputs(8, 1, 4, 32, Some(5_000))),
            MemoryPressure::High
        );
    }

    #[test]
    fn thrashing_with_no_physical_memory_left_is_critical() {
        assert_eq!(
            classify_pressure(inputs(8, 0, 4, 32, Some(5_000))),
            MemoryPressure::Critical
        );
    }

    #[test]
    fn occasional_faulting_is_not_pressure() {
        // Application launch produces bursts of hard faults on any machine.
        assert_eq!(
            classify_pressure(inputs(32, 20, 8, 64, Some(300))),
            MemoryPressure::Normal
        );
    }

    #[test]
    fn missing_fault_data_does_not_fabricate_pressure() {
        // Unavailable is not the same as zero, and must not become an alarm.
        assert_eq!(
            classify_pressure(inputs(32, 20, 8, 64, None)),
            MemoryPressure::Normal
        );
    }

    #[test]
    fn zero_totals_do_not_panic_or_divide_by_zero() {
        let p = classify_pressure(PressureInputs {
            physical_total: Bytes::ZERO,
            physical_available: Bytes::ZERO,
            commit_used: Bytes::ZERO,
            commit_limit: Bytes::ZERO,
            hard_faults_per_sec: None,
        });
        assert_eq!(p, MemoryPressure::Normal);
    }

    #[test]
    fn only_serious_states_raise_an_alert() {
        assert!(!MemoryPressure::Normal.warrants_alert());
        assert!(!MemoryPressure::Elevated.warrants_alert());
        assert!(MemoryPressure::High.warrants_alert());
        assert!(MemoryPressure::Critical.warrants_alert());
    }

    #[test]
    fn severity_ordering_is_meaningful() {
        assert!(MemoryPressure::Normal < MemoryPressure::Elevated);
        assert!(MemoryPressure::Elevated < MemoryPressure::High);
        assert!(MemoryPressure::High < MemoryPressure::Critical);
    }
}
