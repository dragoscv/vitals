//! The CPU's current speed, the way Task Manager computes it.
//!
//! Task Manager's "Speed" is not a register read. It is the base frequency
//! (`Processor Frequency`, MHz) scaled by `% Processor Performance` — how
//! far above or below base the cores actually ran over the last interval,
//! which goes over 100 % under turbo. Both are ordinary performance
//! counters any user may read; no MSR, no driver.
//!
//! The Performance screen said "Not reported by your hardware" because
//! nothing ever asked (the clocks were hard-coded `None`, 2026-10-06).
//!
//! `% Processor Performance` is a rate, so the query is kept open between
//! ticks and the first collection yields nothing: `None`, not a guess.

use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

use vitals_core::units::Hertz;

/// Current and base CPU frequency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuClock {
    /// Average effective frequency across all cores over the last interval.
    pub effective: Hertz,
    /// The rated base frequency.
    pub base: Hertz,
}

/// An open PDH query over the two counters, closed on drop.
#[derive(Debug)]
pub struct ClockSampler {
    query: PDH_HQUERY,
    frequency: PDH_HCOUNTER,
    performance: PDH_HCOUNTER,
}

// SAFETY: PDH handles are process-wide and not tied to the creating thread;
// the sampler is owned by one thread at a time.
unsafe impl Send for ClockSampler {}

impl ClockSampler {
    /// Opens the query. `None` when the counter set is missing (Server Core,
    /// or counters disabled with `lodctr /d`).
    #[must_use]
    pub fn open() -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: writes a handle on success; closed in `Drop`.
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &raw mut query) } != 0 {
            return None;
        }
        // English paths so a Romanian Windows resolves them too.
        let add = |path: PCWSTR| {
            let mut counter = PDH_HCOUNTER::default();
            // SAFETY: `query` is live; `path` is a static wide string.
            (unsafe { PdhAddEnglishCounterW(query, path, 0, &raw mut counter) } == 0)
                .then_some(counter)
        };
        let frequency = add(w!("\\Processor Information(_Total)\\Processor Frequency"));
        let performance = add(w!(
            "\\Processor Information(_Total)\\% Processor Performance"
        ));
        let (Some(frequency), Some(performance)) = (frequency, performance) else {
            // SAFETY: opened above, closed once on this path.
            unsafe { PdhCloseQuery(query) };
            return None;
        };
        let sampler = Self {
            query,
            frequency,
            performance,
        };
        // Prime the rate counter; this collection has nothing to compare to.
        // SAFETY: live query.
        unsafe { PdhCollectQueryData(sampler.query) };
        Some(sampler)
    }

    /// Collects and returns the clocks since the previous call.
    pub fn sample(&mut self) -> Option<CpuClock> {
        // SAFETY: live query.
        if unsafe { PdhCollectQueryData(self.query) } != 0 {
            return None;
        }
        let base_mhz = value(self.frequency)?;
        let percent = value(self.performance)?;
        clock_from(base_mhz, percent)
    }
}

impl Drop for ClockSampler {
    fn drop(&mut self) {
        // SAFETY: live handle, closed exactly once; its counters go with it.
        unsafe { PdhCloseQuery(self.query) };
    }
}

fn value(counter: PDH_HCOUNTER) -> Option<f64> {
    let mut out = PDH_FMT_COUNTERVALUE::default();
    // SAFETY: `counter` belongs to a live query; `out` is a valid out-param.
    let status =
        unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &raw mut out) };
    if status != 0 {
        return None;
    }
    // SAFETY: PDH_FMT_DOUBLE was requested, so the union holds a double.
    let v = unsafe { out.Anonymous.doubleValue };
    v.is_finite().then_some(v)
}

/// Base MHz × performance % → the two clocks, rejecting the implausible.
///
/// A base of 0 is a counter that has not initialised; above 10 GHz or a
/// performance figure above 300 % is a counter fault, not a CPU.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // range-checked first
pub fn clock_from(base_mhz: f64, performance_percent: f64) -> Option<CpuClock> {
    if !(100.0..=10_000.0).contains(&base_mhz) || !(1.0..=300.0).contains(&performance_percent) {
        return None;
    }
    let effective_mhz = base_mhz * performance_percent / 100.0;
    Some(CpuClock {
        effective: Hertz((effective_mhz * 1e6) as u64),
        base: Hertz((base_mhz * 1e6) as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turbo_above_base_is_reported_above_base() {
        // A 3.4 GHz part running at 142 % — Task Manager would say 4.83 GHz.
        let clock = clock_from(3400.0, 142.0).expect("plausible");
        assert_eq!(clock.base, Hertz(3_400_000_000));
        assert_eq!(clock.effective, Hertz(4_828_000_000));
    }

    #[test]
    fn an_uninitialised_or_absurd_counter_is_absent_not_zero() {
        assert_eq!(clock_from(0.0, 100.0), None);
        assert_eq!(clock_from(3400.0, 0.0), None);
        assert_eq!(clock_from(3400.0, 900.0), None);
    }

    #[test]
    fn this_machine_reports_a_speed_after_one_interval() {
        let Some(mut sampler) = ClockSampler::open() else {
            return; // Counter set disabled: nothing to assert.
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        let clock = sampler.sample().expect("a second collection yields a rate");
        assert!(clock.effective.0 > 100_000_000, "{clock:?}");
    }
}
