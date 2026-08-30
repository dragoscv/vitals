//! Network throughput arithmetic.

use vitals_core::units::BytesPerSec;

/// Cumulative interface counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NetworkCounters {
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub packets_received: u64,
    pub packets_sent: u64,
    /// Inbound packets discarded or in error.
    pub errors_in: u64,
    pub errors_out: u64,
    /// Packets dropped for lack of buffer space.
    pub discards_in: u64,
    pub discards_out: u64,
}

/// Rates derived from two snapshots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkRates {
    pub rx: BytesPerSec,
    pub tx: BytesPerSec,
    pub packets_rx: u64,
    pub packets_tx: u64,
    /// Errors and discards per second, combined.
    ///
    /// Invisible in Task Manager, and the first sign of a failing cable, a
    /// saturated link or a driver problem. A connection can show healthy
    /// throughput while quietly discarding a tenth of its packets.
    pub errors_per_sec: u64,
}

impl NetworkRates {
    pub const ZERO: Self = Self {
        rx: BytesPerSec::ZERO,
        tx: BytesPerSec::ZERO,
        packets_rx: 0,
        packets_tx: 0,
        errors_per_sec: 0,
    };
}

/// Computes rates between two snapshots.
///
/// `elapsed_ms` is real elapsed time. Returns [`NetworkRates::ZERO`] for an
/// unusable interval — zero-length, or a counter that went backwards.
///
/// Counters reset when an interface is disabled and re-enabled, when a cable
/// is unplugged, or when a VPN adapter reconnects. Treating that as a delta
/// reports a multi-terabit spike, which then rescales every chart on the page
/// and hides the real traffic.
#[must_use]
pub fn compute_rates(
    previous: NetworkCounters,
    current: NetworkCounters,
    elapsed_ms: u64,
) -> NetworkRates {
    if elapsed_ms == 0 {
        return NetworkRates::ZERO;
    }

    if current.bytes_received < previous.bytes_received || current.bytes_sent < previous.bytes_sent
    {
        return NetworkRates::ZERO;
    }

    let rx_delta = current.bytes_received - previous.bytes_received;
    let tx_delta = current.bytes_sent - previous.bytes_sent;

    let errors = current
        .errors_in
        .saturating_sub(previous.errors_in)
        .saturating_add(current.errors_out.saturating_sub(previous.errors_out))
        .saturating_add(current.discards_in.saturating_sub(previous.discards_in))
        .saturating_add(current.discards_out.saturating_sub(previous.discards_out));

    NetworkRates {
        rx: BytesPerSec(per_second(rx_delta, elapsed_ms)),
        tx: BytesPerSec(per_second(tx_delta, elapsed_ms)),
        packets_rx: per_second(
            current
                .packets_received
                .saturating_sub(previous.packets_received),
            elapsed_ms,
        ),
        packets_tx: per_second(
            current.packets_sent.saturating_sub(previous.packets_sent),
            elapsed_ms,
        ),
        errors_per_sec: per_second(errors, elapsed_ms),
    }
}

/// Converts a per-interval count into a per-second rate.
///
/// 128-bit intermediate: a 100 Gb/s interface over a long interval overflows
/// `u64` once multiplied by 1000.
#[inline]
fn per_second(delta: u64, elapsed_ms: u64) -> u64 {
    if elapsed_ms == 0 {
        return 0;
    }
    let scaled = u128::from(delta).saturating_mul(1000);
    u64::try_from(scaled / u128::from(elapsed_ms)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counters(rx: u64, tx: u64) -> NetworkCounters {
        NetworkCounters {
            bytes_received: rx,
            bytes_sent: tx,
            ..Default::default()
        }
    }

    #[test]
    fn throughput_is_per_second_not_per_interval() {
        // 2 MB over two seconds is 1 MB/s.
        let rates = compute_rates(counters(0, 0), counters(2_000_000, 0), 2000);
        assert_eq!(rates.rx.get(), 1_000_000);
    }

    #[test]
    fn an_idle_interface_reports_zero() {
        let c = counters(5_000, 5_000);
        assert_eq!(compute_rates(c, c, 1000), NetworkRates::ZERO);
    }

    #[test]
    fn a_zero_interval_reports_nothing_rather_than_dividing_by_zero() {
        assert_eq!(
            compute_rates(counters(0, 0), counters(1_000, 1_000), 0),
            NetworkRates::ZERO
        );
    }

    #[test]
    fn a_counter_reset_reports_nothing_rather_than_a_spike() {
        // Unplugging a cable or reconnecting a VPN resets these. A naive
        // delta reports terabits per second and rescales every chart.
        let a = counters(10_000_000_000, 10_000_000_000);
        let b = counters(1_000, 1_000);
        assert_eq!(compute_rates(a, b, 1000), NetworkRates::ZERO);
    }

    #[test]
    fn errors_and_discards_are_summed_across_both_directions() {
        let a = NetworkCounters::default();
        let b = NetworkCounters {
            errors_in: 2,
            errors_out: 3,
            discards_in: 4,
            discards_out: 1,
            ..Default::default()
        };
        assert_eq!(compute_rates(a, b, 1000).errors_per_sec, 10);
    }

    #[test]
    fn a_link_can_be_fast_and_lossy_at_the_same_time() {
        // The case this metric exists for: healthy throughput while quietly
        // discarding packets. Task Manager shows only the throughput.
        let a = NetworkCounters::default();
        let b = NetworkCounters {
            bytes_received: 100_000_000,
            packets_received: 70_000,
            discards_in: 5_000,
            ..Default::default()
        };
        let rates = compute_rates(a, b, 1000);
        assert!(rates.rx.get() > 0);
        assert_eq!(rates.errors_per_sec, 5_000);
    }

    #[test]
    fn sub_second_intervals_scale_up_correctly() {
        // 100 KB in 100ms is 1 MB/s.
        let rates = compute_rates(counters(0, 0), counters(100_000, 0), 100);
        assert_eq!(rates.rx.get(), 1_000_000);
    }

    #[test]
    fn extreme_throughput_does_not_overflow() {
        let rates = compute_rates(counters(0, 0), counters(u64::MAX / 2, 0), 1);
        assert!(
            rates.rx.get() > 0,
            "saturated to zero instead of clamping high"
        );
    }

    #[test]
    fn packet_rates_are_reported_alongside_bytes() {
        let a = NetworkCounters::default();
        let b = NetworkCounters {
            packets_received: 1_500,
            packets_sent: 800,
            ..Default::default()
        };
        let rates = compute_rates(a, b, 1000);
        assert_eq!(rates.packets_rx, 1_500);
        assert_eq!(rates.packets_tx, 800);
    }
}
