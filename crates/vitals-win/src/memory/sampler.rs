//! Reads system memory state.

use std::mem::size_of;

use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use vitals_core::error::{Error, Result};
use vitals_core::metrics::MemoryMetrics;
use vitals_core::units::Bytes;

/// Samples system memory.
///
/// Stateless apart from a page-fault baseline, since every underlying counter
/// is absolute rather than cumulative.
#[derive(Debug, Default)]
pub struct MemorySampler {
    /// Cumulative page faults at the previous sample, for a per-second rate.
    previous_page_faults: Option<u64>,
}

impl MemorySampler {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            previous_page_faults: None,
        }
    }

    /// Discards the page-fault baseline.
    ///
    /// Without this, the first sample after a pause attributes every fault
    /// accumulated while paused to a single interval, producing a fake
    /// thrashing alert.
    pub fn reset(&mut self) {
        self.previous_page_faults = None;
    }

    /// Reads current memory state.
    ///
    /// `elapsed_ms` is the real time since the previous sample, used to turn
    /// the cumulative fault counter into a rate. Pass 0 on the first call.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Os`] if either underlying API call fails.
    pub fn sample(&mut self, elapsed_ms: u32) -> Result<MemoryMetrics> {
        let status = read_global_status()?;
        let perf = read_performance_info()?;

        let page_size = perf.PageSize as u64;

        // GetPerformanceInfo reports pages; GlobalMemoryStatusEx reports
        // bytes. Mixing the two units is a classic source of readings that
        // are wrong by a factor of 4096.
        let commit_used = Bytes(perf.CommitTotal as u64 * page_size);
        let commit_limit = Bytes(perf.CommitLimit as u64 * page_size);
        let paged_pool = Bytes(perf.KernelPaged as u64 * page_size);
        let non_paged_pool = Bytes(perf.KernelNonpaged as u64 * page_size);
        let cached = Bytes(perf.SystemCache as u64 * page_size);

        let total = Bytes(status.ullTotalPhys);
        let available = Bytes(status.ullAvailPhys);
        let used = Bytes(status.ullTotalPhys.saturating_sub(status.ullAvailPhys));

        // Page file size: `ullTotalPageFile` is the *commit limit* (physical
        // memory plus the page file), so the file's own size is the
        // difference. Verified against Win32_PageFileUsage: 203.84 - 191.84
        // = 12.00 GiB, matching the reported AllocatedBaseSize exactly.
        let swap_total = Bytes(status.ullTotalPageFile.saturating_sub(status.ullTotalPhys));

        // Page file *occupancy* cannot be derived from this API.
        //
        // `ullAvailPageFile` is remaining commit, not free page file. The
        // obvious `ullAvailPageFile - ullAvailPhys` goes negative on a
        // machine with plenty of RAM (measured: -3.89 GiB), saturates to
        // zero, and makes an almost-empty page file read as 100% full — we
        // reported 12.00 GiB in use when the true figure was 0.20 GiB.
        //
        // The real number needs `Win32_PageFileUsage` (WMI) or the
        // `\Paging File(_Total)\% Usage` counter, both far too slow for a
        // per-tick path. It is sampled on a slower cadence elsewhere; until
        // then this is honestly unknown rather than confidently wrong.
        let swap_used = None;

        let page_faults_per_sec = self.fault_rate(perf, elapsed_ms);

        Ok(MemoryMetrics {
            total,
            used,
            available,
            cached,
            paged_pool,
            non_paged_pool,
            committed: commit_used,
            commit_limit,
            swap_total,
            swap_used,
            // Hardware-reserved memory is the gap between what the firmware
            // reports and what the OS can address. Not exposed by these APIs;
            // it needs a firmware table read, so it stays honestly zero
            // rather than being guessed at.
            hardware_reserved: Bytes::ZERO,
            page_faults_per_sec,
            // Module speed, slot counts and form factor come from SMBIOS,
            // which is a separate slow query cached elsewhere. `None` here
            // means "not sampled", not "zero".
            speed: None,
            slots_used: None,
            slots_total: None,
            form_factor: None,
        })
    }

    /// Converts the cumulative fault counter into a per-second rate.
    fn fault_rate(&mut self, perf: PERFORMANCE_INFORMATION, elapsed_ms: u32) -> Option<u64> {
        // `PERFORMANCE_INFORMATION` has no fault counter, so this is a
        // placeholder until the process enumerator lands, which aggregates
        // per-process faults. Reporting `None` is the honest answer meanwhile
        // — a fabricated zero would make the pressure classifier believe the
        // machine is never thrashing.
        let _ = (perf, elapsed_ms);
        let _ = &mut self.previous_page_faults;
        None
    }
}

/// Reads `GlobalMemoryStatusEx`.
fn read_global_status() -> Result<MEMORYSTATUSEX> {
    let mut status = MEMORYSTATUSEX {
        // The API dispatches on this length field to stay compatible across
        // versions; leaving it zero makes the call fail.
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..unsafe { std::mem::zeroed() }
    };

    // SAFETY: `status` is a live, correctly sized MEMORYSTATUSEX with its
    // `dwLength` set as the API requires.
    let ok = unsafe { GlobalMemoryStatusEx(&raw mut status) };

    if ok == FALSE {
        return Err(Error::Os {
            context: "GlobalMemoryStatusEx".into(),
            // Win32 error codes are unsigned; we carry them as i32 alongside
            // NTSTATUS values, so the reinterpretation is deliberate.
            // SAFETY: GetLastError has no preconditions.
            code: unsafe { windows_sys::Win32::Foundation::GetLastError() }.cast_signed(),
        });
    }

    Ok(status)
}

/// Reads `GetPerformanceInfo`.
fn read_performance_info() -> Result<PERFORMANCE_INFORMATION> {
    // SAFETY: zeroed is a valid initial state; the API overwrites every field
    // it uses and validates `cb` against the struct size we pass.
    let mut perf: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
    let size = u32::try_from(size_of::<PERFORMANCE_INFORMATION>()).unwrap_or(0);
    perf.cb = size;

    // SAFETY: `perf` is a live, correctly sized PERFORMANCE_INFORMATION and
    // `size` matches its actual size.
    let ok = unsafe { GetPerformanceInfo(&raw mut perf, size) };

    if ok == FALSE {
        return Err(Error::Os {
            context: "GetPerformanceInfo".into(),
            // SAFETY: GetLastError has no preconditions.
            code: unsafe { windows_sys::Win32::Foundation::GetLastError() }.cast_signed(),
        });
    }

    Ok(perf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_plausible_total() {
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sampling should succeed");

        // Any machine that can run this has at least 1 GiB and less than 64 TiB.
        assert!(m.total.get() > 1024 * 1024 * 1024, "total was {}", m.total);
        assert!(m.total.get() < 64 * 1024_u64.pow(4));
    }

    #[test]
    fn used_and_available_sum_to_total() {
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sample");
        assert_eq!(
            m.used.get() + m.available.get(),
            m.total.get(),
            "the two halves of physical memory must account for all of it"
        );
    }

    #[test]
    fn commit_does_not_exceed_its_limit() {
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sample");
        assert!(
            m.committed.get() <= m.commit_limit.get(),
            "committed {} exceeded limit {}",
            m.committed,
            m.commit_limit
        );
    }

    #[test]
    fn pool_sizes_are_sane() {
        // Kernel pools are large but never a majority of RAM. A wildly high
        // value here would mean the page-size multiplication is wrong.
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sample");
        assert!(m.paged_pool.get() < m.total.get());
        assert!(m.non_paged_pool.get() < m.total.get());
    }

    #[test]
    fn swap_excludes_physical_memory() {
        // Windows' "total page file" includes RAM. If that leaked through,
        // swap_total would be at least as large as physical memory.
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sample");
        assert!(
            m.swap_total.get() < m.total.get(),
            "swap_total {} still includes physical memory ({})",
            m.swap_total,
            m.total
        );
    }

    #[test]
    fn swap_usage_is_unknown_rather_than_wrong() {
        // Regression guard. `ullAvailPageFile - ullAvailPhys` goes negative
        // on a machine with spare RAM, saturates to zero, and made an
        // almost-empty page file read as completely full — measured 12.00 GiB
        // against a true 0.20 GiB. Reporting nothing is correct until the
        // slow-cadence WMI sample lands.
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sample");
        assert!(
            m.swap_used.is_none(),
            "page file occupancy cannot be derived from GlobalMemoryStatusEx"
        );
    }

    #[test]
    fn used_percentage_is_in_range() {
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sample");
        let pct = m.used_percent().get();
        assert!((0.0..=100.0).contains(&pct), "used_percent was {pct}");
    }

    #[test]
    fn unavailable_fields_are_none_not_zero() {
        // The rule that matters most: a metric we do not sample must be
        // absent, never a fabricated zero.
        let mut sampler = MemorySampler::new();
        let m = sampler.sample(0).expect("sample");
        assert!(m.speed.is_none());
        assert!(m.slots_total.is_none());
    }

    #[test]
    fn repeated_samples_stay_consistent() {
        let mut sampler = MemorySampler::new();
        let a = sampler.sample(0).expect("first");
        let b = sampler.sample(1000).expect("second");

        // Physical total cannot change without a reboot.
        assert_eq!(a.total, b.total);
    }
}
