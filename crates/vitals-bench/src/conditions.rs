//! Measuring the machine state a benchmark ran under.
//!
//! A score without its conditions is not a measurement, it is a rumour. The
//! same chip reports wildly different figures on battery, under a power-saver
//! scheme, or with a browser reindexing in the background, and a suite that
//! records only the number invites the user to compare two runs that were
//! never comparable. Everything here exists so that [`RunConditions`] is
//! populated from the real machine rather than defaulted.
//!
//! # Windows only, and honest about it
//!
//! The probe below is `cfg(windows)` because the power scheme, the AC line
//! status and the ACPI thermal zones all come from `vitals-win`. On other
//! platforms the fallback returns a conditions struct whose optional fields
//! are `None` — not zeroes, and not a fabricated "Balanced" plan.

use std::time::Duration;

use crate::RunConditions;

/// Samples machine state across a benchmark run.
///
/// Constructed before the workload, consumed after it, so the background load
/// figure covers exactly the interval that was measured rather than some
/// window around it.
#[derive(Debug)]
pub struct ConditionsProbe {
    inner: platform::Probe,
}

impl ConditionsProbe {
    /// Captures the starting state and primes the CPU baseline.
    ///
    /// Priming matters: every CPU utilisation figure on Windows is a delta
    /// between two cumulative counters, so a probe that only sampled at the
    /// end would report the average since boot and call it background load.
    #[must_use]
    pub fn start() -> Self {
        Self {
            inner: platform::Probe::start(),
        }
    }

    /// Closes the interval and returns what the machine was doing.
    ///
    /// `elapsed` is the wall time the workload occupied, used to convert this
    /// process's CPU time into a share of the machine so it can be excluded
    /// from the background figure.
    #[must_use]
    pub fn finish(self, elapsed: Duration) -> RunConditions {
        self.inner.finish(elapsed)
    }
}

#[cfg(windows)]
mod platform {
    use std::time::Duration;

    use vitals_win::cpu::CpuSampler;
    use vitals_win::sensors::{LineStatus, PowerMode, aggregate_battery, read_power_state};

    use crate::RunConditions;

    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            process: isize,
            creation: *mut i64,
            exit: *mut i64,
            kernel: *mut i64,
            user: *mut i64,
        ) -> i32;
    }

    /// This process's consumed CPU time, in 100 ns units.
    ///
    /// Declared locally rather than pulled from `windows-sys` for the same
    /// reason `vitals-win` pins its own `NtQuerySystemInformation` signature:
    /// the layout has been stable since NT and adding a second Win32 binding
    /// crate to this package for two symbols is a poor trade.
    fn self_cpu_100ns() -> Option<u64> {
        let (mut creation, mut exit, mut kernel, mut user) = (0i64, 0i64, 0i64, 0i64);

        // SAFETY: the pseudo-handle from `GetCurrentProcess` is always valid
        // and needs no closing, and all four out-pointers reference live
        // locals for the duration of the call.
        let ok = unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
            )
        };

        (ok != 0).then(|| kernel.cast_unsigned().saturating_add(user.cast_unsigned()))
    }

    /// The warmest ACPI thermal zone, when the machine exposes any.
    ///
    /// Explicitly *not* a CPU temperature — see `vitals_win::sensors::thermal`
    /// for why a zone is usually a chipset or skin sensor. It is recorded as
    /// ambient because that is the closest honest description, and it is
    /// `None` rather than zero whenever the query is denied or the firmware
    /// declares no zones, which is the common case unelevated.
    fn ambient_temp() -> Option<f32> {
        let scan = vitals_win::sensors::read_thermal_zones();
        scan.zones
            .iter()
            .map(|zone| zone.temperature.0)
            .fold(None, |acc: Option<f32>, t| {
                Some(acc.map_or(t, |a| a.max(t)))
            })
    }

    #[derive(Debug)]
    pub struct Probe {
        cpu: CpuSampler,
        self_cpu_start: Option<u64>,
        start_temp: Option<f32>,
    }

    impl Probe {
        pub fn start() -> Self {
            let cores = vitals_win::cpu::logical_core_count();
            let mut cpu = CpuSampler::new(cores);
            // Prime the baseline. The returned values are all zero by
            // construction on a fresh sampler, so they are discarded.
            let _ = cpu.sample();

            Self {
                cpu,
                self_cpu_start: self_cpu_100ns(),
                start_temp: ambient_temp(),
            }
        }

        pub fn finish(mut self, elapsed: Duration) -> RunConditions {
            // Machine-wide busy percentage, averaged over the logical
            // processors, for exactly the interval since `start`.
            let machine_load = self.cpu.sample().map_or(0.0, |per_core| {
                if per_core.is_empty() {
                    0.0
                } else {
                    per_core.iter().map(|u| u.total.get()).sum::<f32>() / per_core.len() as f32
                }
            });

            // Subtract our own consumption. Without this the benchmark
            // reports its own load as background interference and taints
            // every single run — a multi-threaded CPU benchmark pins the
            // machine at 100% by design.
            let cores = u32::try_from(vitals_win::cpu::logical_core_count()).unwrap_or(1);
            let self_load = match (self.self_cpu_start, self_cpu_100ns()) {
                (Some(before), Some(after)) if elapsed > Duration::ZERO && cores > 0 => {
                    let used_100ns = after.saturating_sub(before) as f64;
                    let available_100ns = elapsed.as_secs_f64() * 10_000_000.0 * f64::from(cores);
                    ((used_100ns / available_100ns) * 100.0) as f32
                }
                _ => 0.0,
            };

            let power = read_power_state();
            let battery = aggregate_battery();

            RunConditions {
                power_plan: Some(describe_plan(power.mode)),
                // `AggregateBattery` is preferred over the line status when
                // present: `GetSystemPowerStatus` reports `Unknown` inside
                // virtual machines, and defaulting an unknown to "on mains"
                // would quietly untaint a run that may well have been on
                // battery.
                on_battery: battery.map_or(power.line == LineStatus::Battery, |b| !b.on_ac),
                // Throttle state lives in `IA32_THERM_STATUS`, a
                // model-specific register that requires ring 0. This build
                // ships no kernel driver, so it genuinely cannot be read —
                // see `vitals_win::sensors::driver::DRIVER_GAPS`. Reported as
                // `false` because the DTO field is a bool, and stated here so
                // nobody mistakes it for a measurement: a throttled run will
                // still show up, as a depressed score with the power plan and
                // ambient temperature beside it.
                throttled: false,
                background_load: (machine_load - self_load).max(0.0),
                ambient_start_temp: self.start_temp,
                ambient_end_temp: ambient_temp(),
            }
        }
    }

    /// The active scheme as a short label.
    ///
    /// The scheme's friendly name is localised and OEM-supplied, so it is not
    /// used: a "Dell Optimized" plan would be unrecognisable to anyone
    /// comparing results. The classification is stable and translatable.
    fn describe_plan(mode: PowerMode) -> String {
        match mode {
            PowerMode::BestPowerEfficiency => "powerSaver",
            PowerMode::Balanced => "balanced",
            PowerMode::BestPerformance => "highPerformance",
            PowerMode::Custom => "custom",
        }
        .to_owned()
    }
}

#[cfg(not(windows))]
mod platform {
    use std::time::Duration;

    use crate::RunConditions;

    /// The non-Windows placeholder.
    ///
    /// Every optional field is `None` and the booleans are `false`, which is
    /// the honest answer to "we have no backend here yet" — not a guess at
    /// what the machine was probably doing.
    #[derive(Debug)]
    pub struct Probe;

    impl Probe {
        pub const fn start() -> Self {
            Self
        }

        pub fn finish(self, _elapsed: Duration) -> RunConditions {
            RunConditions {
                power_plan: None,
                on_battery: false,
                throttled: false,
                background_load: 0.0,
                ambient_start_temp: None,
                ambient_end_temp: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_over_real_work_produces_conditions() {
        let probe = ConditionsProbe::start();
        let started = std::time::Instant::now();
        std::hint::black_box(crate::workloads::cpu_single_thread());
        let conditions = probe.finish(started.elapsed());

        // Load is a percentage, so anything outside 0..=100 means the
        // subtraction of our own usage went wrong.
        assert!(
            (0.0..=100.0).contains(&conditions.background_load),
            "background load {} is not a percentage",
            conditions.background_load
        );

        #[cfg(windows)]
        assert!(
            conditions.power_plan.is_some(),
            "Windows always has an active power scheme"
        );
    }

    #[test]
    fn a_busy_benchmark_does_not_taint_itself() {
        // The regression this guards: counting the benchmark's own CPU as
        // background load pins the figure at 100% and marks every result
        // untrustworthy, which would make the whole feature useless.
        let probe = ConditionsProbe::start();
        let started = std::time::Instant::now();
        std::hint::black_box(crate::workloads::cpu_multi_thread());
        let conditions = probe.finish(started.elapsed());

        assert!(
            conditions.background_load < 90.0,
            "the benchmark counted itself as background load: {}",
            conditions.background_load
        );
    }
}
