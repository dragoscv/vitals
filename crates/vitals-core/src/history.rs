//! The machine-wide row long-range history keeps.
//!
//! Lives in core rather than in the store so the type crosses the IPC
//! boundary (ts-rs exports it), and so the CLI and the LAN API can speak it
//! without linking SQLite. The store persists it; everyone else just reads it.
//!
//! Deliberately small. Per-process history is a separate concern (totals per
//! executable); this is the time series behind the Performance charts once
//! they reach further back than the in-memory ring buffers do. One row per
//! tick at the finest tier is roughly 80 bytes, so a day at 1 Hz is under
//! 7 MB before pruning — and the finest tier only keeps an hour.

use serde::{Deserialize, Serialize};

/// One point on every machine-wide chart.
///
/// Rates are per second, sizes in bytes, percentages 0–100. Fields the
/// hardware could not report are `None`, and stay `None` through rollup — an
/// hourly average of "unknown" is still unknown, not zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct MachineSample {
    /// Unix time in seconds.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ts: i64,
    pub cpu_percent: f32,
    pub cpu_kernel_percent: f32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub memory_used: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub memory_total: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub disk_read_bps: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub disk_write_bps: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub net_rx_bps: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub net_tx_bps: u64,
    /// Busiest engine on the busiest GPU, when any GPU reports one.
    pub gpu_percent: Option<f32>,
    pub cpu_temp_c: Option<f32>,
    pub power_draw_w: Option<f32>,
}

impl MachineSample {
    /// Reduces a full metrics tree to the row the store keeps.
    #[must_use]
    pub fn from_metrics(ts: i64, m: &crate::metrics::SystemMetrics) -> Self {
        Self {
            ts,
            cpu_percent: m.cpu.total.0,
            cpu_kernel_percent: m.cpu.kernel.0,
            memory_used: m.memory.used.0,
            memory_total: m.memory.total.0,
            disk_read_bps: m.disks.iter().map(|d| d.read.0).sum(),
            disk_write_bps: m.disks.iter().map(|d| d.write.0).sum(),
            net_rx_bps: m.networks.iter().map(|n| n.rx.0).sum(),
            net_tx_bps: m.networks.iter().map(|n| n.tx.0).sum(),
            gpu_percent: m
                .gpus
                .iter()
                .map(|g| g.utilization.0)
                .fold(None, |acc, v| Some(acc.map_or(v, |a: f32| a.max(v)))),
            cpu_temp_c: m.cpu.temperature.map(|c| c.0),
            power_draw_w: m.power_draw.map(|w| w.0),
        }
    }

    /// Folds a run of finer samples into one coarser one at `ts`.
    ///
    /// Averages the gauges (CPU, memory, temperature) and the rates — a
    /// rate averaged over a minute is the minute's mean throughput, which is
    /// what a minute-resolution chart should show. `memory_total` takes the
    /// maximum so a RAM upgrade mid-window does not average into a
    /// non-existent size.
    ///
    /// Returns `None` for an empty run: there is no honest value for it.
    #[must_use]
    pub fn rollup(ts: i64, samples: &[Self]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let n = samples.len() as f64;
        let mean_gauge =
            |f: fn(&Self) -> f32| (samples.iter().map(|s| f64::from(f(s))).sum::<f64>() / n) as f32;
        let mean_rate = |f: fn(&Self) -> u64| {
            (samples.iter().map(|s| f(s) as f64).sum::<f64>() / n).round() as u64
        };
        let mean_optional = |f: fn(&Self) -> Option<f32>| {
            let present: Vec<f64> = samples.iter().filter_map(f).map(f64::from).collect();
            if present.is_empty() {
                None
            } else {
                Some((present.iter().sum::<f64>() / present.len() as f64) as f32)
            }
        };

        Some(Self {
            ts,
            cpu_percent: mean_gauge(|s| s.cpu_percent),
            cpu_kernel_percent: mean_gauge(|s| s.cpu_kernel_percent),
            memory_used: mean_rate(|s| s.memory_used),
            memory_total: samples.iter().map(|s| s.memory_total).max().unwrap_or(0),
            disk_read_bps: mean_rate(|s| s.disk_read_bps),
            disk_write_bps: mean_rate(|s| s.disk_write_bps),
            net_rx_bps: mean_rate(|s| s.net_rx_bps),
            net_tx_bps: mean_rate(|s| s.net_tx_bps),
            gpu_percent: mean_optional(|s| s.gpu_percent),
            cpu_temp_c: mean_optional(|s| s.cpu_temp_c),
            power_draw_w: mean_optional(|s| s.power_draw_w),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(ts: i64, cpu: f32, gpu: Option<f32>) -> MachineSample {
        MachineSample {
            ts,
            cpu_percent: cpu,
            cpu_kernel_percent: cpu / 2.0,
            memory_used: 1_000,
            memory_total: 4_000,
            disk_read_bps: 100,
            disk_write_bps: 50,
            net_rx_bps: 10,
            net_tx_bps: 5,
            gpu_percent: gpu,
            cpu_temp_c: None,
            power_draw_w: None,
        }
    }

    #[test]
    fn rollup_of_nothing_is_nothing() {
        assert!(MachineSample::rollup(0, &[]).is_none());
    }

    #[test]
    fn rollup_averages_gauges_and_rates() {
        let r = MachineSample::rollup(60, &[s(1, 10.0, None), s(2, 30.0, None)]).unwrap();
        assert_eq!(r.ts, 60);
        assert!((r.cpu_percent - 20.0).abs() < f32::EPSILON);
        assert_eq!(r.disk_read_bps, 100);
    }

    #[test]
    fn unknown_stays_unknown_and_does_not_drag_the_mean_to_zero() {
        // Two samples, one GPU reading. The rollup is that reading, not half
        // of it — a missing measurement is not a measurement of zero.
        let r = MachineSample::rollup(60, &[s(1, 0.0, Some(80.0)), s(2, 0.0, None)]).unwrap();
        assert_eq!(r.gpu_percent, Some(80.0));

        let none = MachineSample::rollup(60, &[s(1, 0.0, None), s(2, 0.0, None)]).unwrap();
        assert_eq!(none.gpu_percent, None);
    }

    #[test]
    fn memory_total_takes_the_maximum() {
        let mut a = s(1, 0.0, None);
        a.memory_total = 8_000;
        let b = s(2, 0.0, None);
        assert_eq!(
            MachineSample::rollup(60, &[a, b]).unwrap().memory_total,
            8_000
        );
    }
}
