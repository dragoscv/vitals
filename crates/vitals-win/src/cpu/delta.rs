//! CPU time delta arithmetic.
//!
//! Deliberately pure and free of any Windows types, so the arithmetic that
//! produces every CPU percentage in the product can be tested exhaustively
//! without a machine in the loop.
//!
//! This is where monitoring tools most often go wrong. The counters involved
//! are cumulative, monotonic-in-theory, 64-bit, and occasionally *not*
//! monotonic in practice — and every one of those properties has a failure
//! mode that produces a plausible-looking but wrong percentage.

use vitals_core::units::Percent;

/// A snapshot of cumulative CPU time, in 100-nanosecond units.
///
/// Field meanings follow `NtQuerySystemInformation`'s
/// `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION`, where `idle` is a *subset* of
/// `kernel` — a detail that trips up almost everyone reading these counters
/// for the first time and silently understates kernel time if missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub idle: u64,
    /// Includes `idle`. Subtract it to get real kernel time.
    pub kernel: u64,
    pub user: u64,
    /// Time servicing deferred procedure calls.
    pub dpc: u64,
    /// Time servicing hardware interrupts.
    pub interrupt: u64,
}

impl CpuTimes {
    /// Total elapsed time across all states.
    ///
    /// `kernel` already contains `idle`, so kernel and user alone are the
    /// whole wall clock for this processor.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.kernel.saturating_add(self.user)
    }

    /// Kernel time excluding idle — the real "system" figure.
    #[must_use]
    pub const fn busy_kernel(self) -> u64 {
        self.kernel.saturating_sub(self.idle)
    }
}

/// The computed utilisation between two snapshots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuUsage {
    pub total: Percent,
    pub kernel: Percent,
    pub user: Percent,
    pub interrupt: Percent,
}

impl CpuUsage {
    pub const ZERO: Self = Self {
        total: Percent::ZERO,
        kernel: Percent::ZERO,
        user: Percent::ZERO,
        interrupt: Percent::ZERO,
    };
}

/// Computes utilisation between two cumulative snapshots.
///
/// Returns [`CpuUsage::ZERO`] rather than an error when the delta is
/// unusable. Three cases matter, and all three occur in practice:
///
/// 1. **Zero elapsed time.** Two samples in the same tick. Dividing would
///    panic or produce infinity.
/// 2. **A counter went backwards.** Happens across sleep/resume and on some
///    virtualised hosts. Treating the wrap as a huge positive delta produces
///    a spike to 100% that never happened — the single most common artefact
///    in home-grown monitors.
/// 3. **Idle exceeds total.** Timer skew between the two counters can make
///    the busy fraction slightly negative; clamping is correct here.
#[must_use]
pub fn compute_usage(previous: CpuTimes, current: CpuTimes) -> CpuUsage {
    // Case 2: any counter moving backwards invalidates the whole sample.
    // Reporting nothing is far better than reporting a fabricated spike.
    if current.idle < previous.idle
        || current.kernel < previous.kernel
        || current.user < previous.user
    {
        return CpuUsage::ZERO;
    }

    let total_delta = current.total().saturating_sub(previous.total());
    if total_delta == 0 {
        return CpuUsage::ZERO; // Case 1.
    }

    let idle_delta = current.idle.saturating_sub(previous.idle);
    // Case 3: clamp rather than underflow.
    let busy_delta = total_delta.saturating_sub(idle_delta);

    let kernel_delta = current.busy_kernel().saturating_sub(previous.busy_kernel());
    let user_delta = current.user.saturating_sub(previous.user);
    let interrupt_delta = current.interrupt.saturating_sub(previous.interrupt);

    CpuUsage {
        total: Percent::ratio(busy_delta, total_delta),
        kernel: Percent::ratio(kernel_delta, total_delta),
        user: Percent::ratio(user_delta, total_delta),
        interrupt: Percent::ratio(interrupt_delta, total_delta),
    }
}

/// Computes a process's share of the *whole machine*.
///
/// `process_delta` is the process's CPU time over the interval;
/// `wall_clock_delta` is elapsed real time; `logical_cores` is the processor
/// count.
///
/// Normalising by core count is a deliberate choice. Windows Task Manager is
/// famously inconsistent here — its Processes tab normalises and its Details
/// tab does not, so the same process shows 25% in one and 100% in the other.
/// Vitals normalises everywhere and offers a per-core view as an explicit
/// toggle, so a single number always means the same thing.
#[must_use]
pub fn process_cpu_percent(
    process_delta: u64,
    wall_clock_delta: u64,
    logical_cores: u32,
) -> Percent {
    if wall_clock_delta == 0 || logical_cores == 0 {
        return Percent::ZERO;
    }

    let available = u128::from(wall_clock_delta).saturating_mul(u128::from(logical_cores));
    if available == 0 {
        return Percent::ZERO;
    }

    let used = u128::from(process_delta).saturating_mul(100);
    Percent::new((used / available) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One second of a single processor, in 100ns units.
    const SECOND: u64 = 10_000_000;

    fn times(idle: u64, kernel: u64, user: u64) -> CpuTimes {
        CpuTimes {
            idle,
            kernel,
            user,
            dpc: 0,
            interrupt: 0,
        }
    }

    #[test]
    fn fully_idle_reports_zero() {
        let a = times(0, 0, 0);
        let b = times(SECOND, SECOND, 0);
        assert_eq!(compute_usage(a, b).total, Percent::ZERO);
    }

    #[test]
    fn fully_busy_reports_one_hundred() {
        let a = times(0, 0, 0);
        let b = times(0, 0, SECOND);
        assert_eq!(compute_usage(a, b).total, Percent::FULL);
    }

    #[test]
    fn half_idle_reports_fifty() {
        let a = times(0, 0, 0);
        // Half the second idle (inside kernel), half in user.
        let b = times(SECOND / 2, SECOND / 2, SECOND / 2);
        let usage = compute_usage(a, b);
        assert!(
            (usage.total.get() - 50.0).abs() < 0.01,
            "got {}",
            usage.total
        );
    }

    #[test]
    fn kernel_time_excludes_idle() {
        // The trap: `kernel` includes `idle`. Counting it raw would report
        // an idle machine as spending all its time in the kernel.
        let a = times(0, 0, 0);
        let b = times(SECOND, SECOND, 0);
        assert_eq!(compute_usage(a, b).kernel, Percent::ZERO);
    }

    #[test]
    fn identical_snapshots_report_zero_not_a_division_by_zero() {
        let t = times(SECOND, SECOND * 2, SECOND);
        assert_eq!(compute_usage(t, t), CpuUsage::ZERO);
    }

    #[test]
    fn a_backwards_counter_reports_zero_not_a_spike() {
        // Observed across sleep/resume and on some hypervisors. The naive
        // implementation reports 100% here, which is a lie.
        let a = times(SECOND * 10, SECOND * 20, SECOND * 5);
        let b = times(SECOND, SECOND * 2, SECOND);
        assert_eq!(
            compute_usage(a, b),
            CpuUsage::ZERO,
            "a counter reset must never be read as a busy spike"
        );
    }

    #[test]
    fn idle_exceeding_total_is_clamped_not_underflowed() {
        // Timer skew can make idle marginally exceed elapsed time.
        let a = times(0, 0, 0);
        let b = CpuTimes {
            idle: SECOND * 2,
            kernel: SECOND,
            user: 0,
            dpc: 0,
            interrupt: 0,
        };
        assert_eq!(compute_usage(a, b).total, Percent::ZERO);
    }

    #[test]
    fn usage_never_exceeds_one_hundred_percent() {
        let a = times(0, 0, 0);
        let b = times(0, SECOND * 5, SECOND * 5);
        assert!(compute_usage(a, b).total.get() <= 100.0);
    }

    #[test]
    fn saturating_arithmetic_survives_counters_near_u64_max() {
        let a = times(u64::MAX - 10, u64::MAX - 5, u64::MAX - 5);
        let b = times(u64::MAX, u64::MAX, u64::MAX);
        let usage = compute_usage(a, b);
        assert!(usage.total.get() <= 100.0);
    }

    #[test]
    fn process_using_one_full_core_of_four_reports_twenty_five_percent() {
        // The normalisation decision, pinned: one saturated core on a
        // quad-core machine is 25% of the machine, not 100%.
        assert_eq!(process_cpu_percent(SECOND, SECOND, 4), Percent(25.0));
    }

    #[test]
    fn process_saturating_every_core_reports_one_hundred() {
        assert_eq!(process_cpu_percent(SECOND * 8, SECOND, 8), Percent::FULL);
    }

    #[test]
    fn process_cpu_handles_degenerate_inputs() {
        assert_eq!(process_cpu_percent(SECOND, 0, 4), Percent::ZERO);
        assert_eq!(process_cpu_percent(SECOND, SECOND, 0), Percent::ZERO);
    }

    #[test]
    fn process_cpu_is_clamped_when_the_interval_is_understated() {
        // A late sample can make the measured process time exceed the
        // theoretical maximum. Clamp rather than report 340%.
        assert_eq!(process_cpu_percent(SECOND * 100, SECOND, 2), Percent::FULL);
    }
}
