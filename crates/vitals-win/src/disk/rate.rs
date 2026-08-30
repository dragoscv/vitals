//! Disk throughput and latency arithmetic.
//!
//! Pure, so the rate maths can be tested without a disk.

use vitals_core::units::{BytesPerSec, Percent};

/// Cumulative disk counters, as reported by `IOCTL_DISK_PERFORMANCE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskCounters {
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub read_count: u64,
    pub write_count: u64,
    /// Cumulative time spent reading, in 100ns units.
    pub read_time: u64,
    pub write_time: u64,
    /// Cumulative time with at least one request outstanding, 100ns units.
    pub idle_time: u64,
    pub query_time: u64,
}

/// Rates derived from two counter snapshots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiskRates {
    pub read: BytesPerSec,
    pub write: BytesPerSec,
    /// Share of the interval the disk had work outstanding.
    pub active_time: Percent,
    /// Mean time to service a request, in milliseconds.
    ///
    /// The number that actually correlates with a machine feeling slow.
    /// Throughput does not: an `NVMe` drive at 40 MB/s of `4K` random reads is
    /// far more painful than the same drive at 2 GB/s sequential, and only
    /// latency distinguishes them.
    pub response_ms: Option<f32>,
    pub iops: u64,
}

impl DiskRates {
    pub const ZERO: Self = Self {
        read: BytesPerSec::ZERO,
        write: BytesPerSec::ZERO,
        active_time: Percent::ZERO,
        response_ms: None,
        iops: 0,
    };
}

/// 100ns intervals per second.
const TICKS_PER_SEC: u64 = 10_000_000;

/// Computes rates between two counter snapshots.
///
/// `elapsed_ticks` is real elapsed time in 100ns units. Returns
/// [`DiskRates::ZERO`] when the interval is unusable — zero-length, or with a
/// counter that ran backwards after a device reset or hot-plug.
#[must_use]
pub fn compute_rates(
    previous: DiskCounters,
    current: DiskCounters,
    elapsed_ticks: u64,
) -> DiskRates {
    if elapsed_ticks == 0 {
        return DiskRates::ZERO;
    }

    // A counter going backwards means the device was reset or replaced.
    // Treating the wrap as a delta would report a petabyte-per-second spike.
    if current.bytes_read < previous.bytes_read
        || current.bytes_written < previous.bytes_written
        || current.idle_time < previous.idle_time
    {
        return DiskRates::ZERO;
    }

    let read_delta = current.bytes_read - previous.bytes_read;
    let write_delta = current.bytes_written - previous.bytes_written;
    let reads = current.read_count.saturating_sub(previous.read_count);
    let writes = current.write_count.saturating_sub(previous.write_count);
    let ops = reads.saturating_add(writes);

    let read = BytesPerSec(scale_per_second(read_delta, elapsed_ticks));
    let write = BytesPerSec(scale_per_second(write_delta, elapsed_ticks));

    // Active time is the inverse of idle. The device reports idle directly,
    // which is more reliable than summing read and write time — those overlap
    // on any drive with a queue depth above one, and summing them yields
    // "120% busy" on every NVMe device.
    let idle_delta = current.idle_time - previous.idle_time;
    let busy = elapsed_ticks.saturating_sub(idle_delta);
    let active_time = Percent::ratio(busy, elapsed_ticks);

    let service_delta = current
        .read_time
        .saturating_sub(previous.read_time)
        .saturating_add(current.write_time.saturating_sub(previous.write_time));

    // Latency is only meaningful when something was actually serviced.
    // Dividing by zero operations would produce NaN, which renders as a
    // blank cell and looks like a bug.
    let response_ms = if ops == 0 {
        None
    } else {
        Some((service_delta as f64 / ops as f64 / 10_000.0) as f32)
    };

    DiskRates {
        read,
        write,
        active_time,
        response_ms,
        iops: scale_per_second(ops, elapsed_ticks),
    }
}

/// Converts a per-interval count into a per-second rate.
///
/// Uses 128-bit intermediates: a fast `NVMe` drive over a long interval
/// overflows `u64` when multiplied by the tick rate.
#[inline]
fn scale_per_second(delta: u64, elapsed_ticks: u64) -> u64 {
    if elapsed_ticks == 0 {
        return 0;
    }
    let scaled = u128::from(delta).saturating_mul(u128::from(TICKS_PER_SEC));
    u64::try_from(scaled / u128::from(elapsed_ticks)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = TICKS_PER_SEC;

    fn counters(read: u64, written: u64, reads: u64, writes: u64, idle: u64) -> DiskCounters {
        DiskCounters {
            bytes_read: read,
            bytes_written: written,
            read_count: reads,
            write_count: writes,
            read_time: 0,
            write_time: 0,
            idle_time: idle,
            query_time: 0,
        }
    }

    #[test]
    fn throughput_is_per_second_not_per_interval() {
        // 100 MB over two seconds is 50 MB/s.
        let a = counters(0, 0, 0, 0, 0);
        let b = counters(100_000_000, 0, 10, 0, 0);
        let rates = compute_rates(a, b, SECOND * 2);
        assert_eq!(rates.read.get(), 50_000_000);
    }

    #[test]
    fn an_idle_disk_reports_zero_active_time() {
        let a = counters(0, 0, 0, 0, 0);
        let b = counters(0, 0, 0, 0, SECOND);
        assert_eq!(compute_rates(a, b, SECOND).active_time, Percent::ZERO);
    }

    #[test]
    fn a_saturated_disk_reports_full_active_time() {
        let a = counters(0, 0, 0, 0, 0);
        let b = counters(1_000, 0, 1, 0, 0);
        assert_eq!(compute_rates(a, b, SECOND).active_time, Percent::FULL);
    }

    #[test]
    fn active_time_is_derived_from_idle_not_from_summed_service_time() {
        // Read and write time overlap at queue depth > 1. Summing them would
        // exceed the interval and report over 100% busy.
        let a = DiskCounters::default();
        let b = DiskCounters {
            read_time: SECOND * 3,
            write_time: SECOND * 2,
            idle_time: SECOND / 2,
            read_count: 100,
            write_count: 100,
            ..Default::default()
        };
        let rates = compute_rates(a, b, SECOND);
        assert!(
            rates.active_time.get() <= 100.0,
            "got {}",
            rates.active_time
        );
        assert!((rates.active_time.get() - 50.0).abs() < 0.01);
    }

    #[test]
    fn a_zero_interval_reports_nothing_rather_than_dividing_by_zero() {
        let c = counters(1_000, 1_000, 1, 1, 0);
        assert_eq!(compute_rates(c, c, 0), DiskRates::ZERO);
    }

    #[test]
    fn a_counter_reset_reports_nothing_rather_than_a_spike() {
        // Happens on device hot-plug and after some driver updates.
        let a = counters(1_000_000_000, 1_000_000_000, 100, 100, SECOND * 10);
        let b = counters(1_000, 1_000, 1, 1, SECOND);
        assert_eq!(compute_rates(a, b, SECOND), DiskRates::ZERO);
    }

    #[test]
    fn latency_is_none_when_nothing_was_serviced() {
        // NaN here would render as a blank cell and read as a bug.
        let a = DiskCounters::default();
        let b = DiskCounters {
            idle_time: SECOND,
            ..Default::default()
        };
        assert!(compute_rates(a, b, SECOND).response_ms.is_none());
    }

    #[test]
    fn latency_is_the_mean_service_time_per_operation() {
        // 10 operations taking 50ms of service time in total = 5ms each.
        let a = DiskCounters::default();
        let b = DiskCounters {
            read_count: 10,
            read_time: 500_000, // 50ms in 100ns units
            ..Default::default()
        };
        let rates = compute_rates(a, b, SECOND);
        let ms = rates.response_ms.expect("operations occurred");
        assert!((ms - 5.0).abs() < 0.01, "expected 5ms, got {ms}");
    }

    #[test]
    fn iops_counts_reads_and_writes() {
        let a = DiskCounters::default();
        let b = counters(0, 0, 300, 200, 0);
        assert_eq!(compute_rates(a, b, SECOND).iops, 500);
    }

    #[test]
    fn extreme_throughput_does_not_overflow() {
        // A 128-bit intermediate is required: a fast NVMe delta multiplied by
        // the tick rate exceeds u64.
        let a = DiskCounters::default();
        let b = counters(u64::MAX / 2, 0, 1, 0, 0);
        let rates = compute_rates(a, b, SECOND / 100);
        assert!(
            rates.read.get() > 0,
            "saturated to zero instead of clamping high"
        );
    }
}
