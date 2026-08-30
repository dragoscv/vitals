//! Reads per-processor CPU times from the NT native API.

use std::mem::{MaybeUninit, size_of};

use vitals_core::error::{Error, Result};

use super::delta::{CpuTimes, CpuUsage, compute_usage};

/// Mirrors `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION`.
///
/// Declared here rather than taken from `windows-sys` because the definition
/// lives behind the `Wdk` feature set, whose availability has moved between
/// releases. The layout is stable and has been since Windows NT 4; pinning it
/// locally removes a moving dependency from the hottest path in the product.
///
/// All times are in 100-nanosecond units.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct ProcessorPerformanceInformation {
    idle_time: i64,
    /// Includes idle time.
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
}

/// `SystemProcessorPerformanceInformation`
const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION: i32 = 8;

unsafe extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: i32,
        system_information: *mut core::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

/// Samples per-processor CPU times.
///
/// Holds the previous snapshot so utilisation can be computed as a delta;
/// keep one instance for the lifetime of the process. Every value here is a
/// difference between two points in time, so a fresh instance necessarily
/// reports zero on its first call.
#[derive(Debug)]
pub struct CpuSampler {
    /// Reused across calls so a 1 Hz sample loop performs no allocation.
    buffer: Vec<ProcessorPerformanceInformation>,
    previous: Vec<CpuTimes>,
    logical_cores: usize,
    /// Whether a baseline has been captured. Distinguishes "genuinely 0%"
    /// from "we have nothing to compare against yet".
    primed: bool,
}

impl CpuSampler {
    /// Creates a sampler for a machine with `logical_cores` processors.
    #[must_use]
    pub fn new(logical_cores: usize) -> Self {
        Self {
            buffer: vec![ProcessorPerformanceInformation::default(); logical_cores],
            previous: vec![CpuTimes::default(); logical_cores],
            logical_cores,
            primed: false,
        }
    }

    /// Whether a baseline exists, so the next sample will be meaningful.
    #[must_use]
    pub const fn is_primed(&self) -> bool {
        self.primed
    }

    /// Discards the baseline.
    ///
    /// Called after a pause: without this, the first sample after resuming
    /// covers the entire paused interval and reports a spike that never
    /// happened.
    pub fn reset(&mut self) {
        self.primed = false;
    }

    /// Reads current per-processor times.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Os`] if the native call fails.
    pub fn read(&mut self) -> Result<&[CpuTimes]> {
        let byte_len = size_of::<ProcessorPerformanceInformation>() * self.logical_cores;
        let mut returned: u32 = 0;

        // SAFETY: `buffer` is a `Vec` of exactly `logical_cores` repr(C)
        // structs, so the pointer is valid for `byte_len` bytes and correctly
        // aligned. The kernel writes at most that many bytes, bounded by the
        // length we pass. `returned` is a valid pointer to a live `u32`.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
                self.buffer.as_mut_ptr().cast(),
                u32::try_from(byte_len).unwrap_or(u32::MAX),
                &raw mut returned,
            )
        };

        if status < 0 {
            return Err(Error::Os {
                context: "NtQuerySystemInformation(ProcessorPerformance)".into(),
                code: status,
            });
        }

        // A CPU can be hot-added, or the kernel can return fewer entries than
        // requested inside a container. Trusting our own count would read
        // uninitialised memory as real measurements.
        let entries = (returned as usize) / size_of::<ProcessorPerformanceInformation>();
        let count = entries.min(self.logical_cores);

        self.previous.truncate(count);
        while self.previous.len() < count {
            self.previous.push(CpuTimes::default());
        }

        Ok(&self.previous[..count])
    }

    /// Samples and returns per-processor utilisation since the previous call.
    ///
    /// The first call after construction or [`reset`](Self::reset) primes the
    /// baseline and reports zero for every processor.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Os`] if the native call fails.
    pub fn sample(&mut self) -> Result<Vec<CpuUsage>> {
        let byte_len = size_of::<ProcessorPerformanceInformation>() * self.logical_cores;
        let mut returned: u32 = 0;

        // SAFETY: as in `read` above.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
                self.buffer.as_mut_ptr().cast(),
                u32::try_from(byte_len).unwrap_or(u32::MAX),
                &raw mut returned,
            )
        };

        if status < 0 {
            return Err(Error::Os {
                context: "NtQuerySystemInformation(ProcessorPerformance)".into(),
                code: status,
            });
        }

        let entries = (returned as usize) / size_of::<ProcessorPerformanceInformation>();
        let count = entries.min(self.logical_cores);

        let current: Vec<CpuTimes> = self.buffer[..count]
            .iter()
            .map(|raw| CpuTimes {
                // The kernel declares these signed, but they are cumulative
                // durations and never negative. Cast rather than clamp so a
                // genuinely absurd value shows up as one, instead of being
                // silently rewritten to zero.
                idle: raw.idle_time as u64,
                kernel: raw.kernel_time as u64,
                user: raw.user_time as u64,
                dpc: raw.dpc_time as u64,
                interrupt: raw.interrupt_time as u64,
            })
            .collect();

        let usage = if self.primed && self.previous.len() == current.len() {
            self.previous
                .iter()
                .zip(&current)
                .map(|(prev, cur)| compute_usage(*prev, *cur))
                .collect()
        } else {
            vec![CpuUsage::ZERO; current.len()]
        };

        self.previous = current;
        self.primed = true;

        Ok(usage)
    }
}

/// Number of logical processors, from the OS.
#[must_use]
pub fn logical_core_count() -> usize {
    // SAFETY: `GetSystemInfo` writes a fully initialised SYSTEM_INFO into the
    // pointer it is given and cannot fail.
    unsafe {
        let mut info =
            MaybeUninit::<windows_sys::Win32::System::SystemInformation::SYSTEM_INFO>::uninit();
        windows_sys::Win32::System::SystemInformation::GetSystemInfo(info.as_mut_ptr());
        info.assume_init().dwNumberOfProcessors as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_at_least_one_logical_core() {
        assert!(logical_core_count() >= 1);
    }

    #[test]
    fn first_sample_is_zero_because_there_is_no_baseline() {
        // Critical: a fresh sampler must not report a huge value derived from
        // the machine's entire uptime.
        let mut sampler = CpuSampler::new(logical_core_count());
        assert!(!sampler.is_primed());

        let usage = sampler.sample().expect("sampling should succeed");
        assert!(
            usage
                .iter()
                .all(|u| u.total == vitals_core::units::Percent::ZERO)
        );
        assert!(sampler.is_primed());
    }

    #[test]
    fn second_sample_produces_values_in_range() {
        let mut sampler = CpuSampler::new(logical_core_count());
        sampler.sample().expect("prime");

        // Busy-wait briefly so there is a measurable delta.
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(80) {
            std::hint::spin_loop();
        }

        let usage = sampler.sample().expect("sample");
        assert!(
            !usage.is_empty(),
            "a machine must report at least one processor"
        );
        for u in &usage {
            assert!(
                (0.0..=100.0).contains(&u.total.get()),
                "utilisation out of range: {}",
                u.total
            );
            assert!((0.0..=100.0).contains(&u.kernel.get()));
            assert!((0.0..=100.0).contains(&u.user.get()));
        }
    }

    #[test]
    fn reset_discards_the_baseline() {
        // After a pause the next sample must not attribute the whole paused
        // interval to the process.
        let mut sampler = CpuSampler::new(logical_core_count());
        sampler.sample().expect("prime");
        assert!(sampler.is_primed());

        sampler.reset();
        assert!(!sampler.is_primed());

        let usage = sampler.sample().expect("sample");
        assert!(
            usage
                .iter()
                .all(|u| u.total == vitals_core::units::Percent::ZERO)
        );
    }

    #[test]
    fn reports_one_entry_per_logical_processor() {
        let cores = logical_core_count();
        let mut sampler = CpuSampler::new(cores);
        let usage = sampler.sample().expect("sample");
        assert_eq!(usage.len(), cores);
    }
}
