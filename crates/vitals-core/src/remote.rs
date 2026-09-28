//! Shapes served to remote clients that cannot afford a full frame.
//!
//! A keyframe with 600 processes is about 250 KB of JSON. That is fine for
//! the desktop webview and for a phone holding one stream open, but it is
//! not fine for a watch that wakes once a minute to redraw a tile, or for a
//! home-screen widget refreshed by `WorkManager`. Both need the machine-wide
//! numbers and the few processes that explain them, and nothing else.
//! (ADR-0033.)

use crate::metrics::SystemMetrics;
use crate::process::Process;

/// The machine now, plus its busiest processes.
///
/// Built from the server's materialised view, so it is as current as the
/// last frame and never a half-applied delta.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub timestamp_ms: u64,
    pub system: SystemMetrics,
    /// The busiest processes by CPU, most first. At most the `top` the
    /// client asked for.
    pub top: Vec<Process>,
    /// Every process on the machine, not just those in `top`, so a client
    /// can say "5 of 612".
    pub process_count: u32,
}

impl Summary {
    /// Reduces a full process list to a summary with the `n` busiest.
    ///
    /// Ties on CPU are broken by private memory, so two idle refreshes of
    /// the same machine list the same processes instead of shuffling a
    /// dozen at 0 %.
    #[must_use]
    pub fn from_parts(
        seq: u64,
        timestamp_ms: u64,
        system: SystemMetrics,
        processes: &[Process],
        n: usize,
    ) -> Self {
        let mut ranked: Vec<&Process> = processes.iter().collect();
        ranked.sort_by(|a, b| {
            b.cpu
                .get()
                .total_cmp(&a.cpu.get())
                .then_with(|| b.memory_private.get().cmp(&a.memory_private.get()))
                .then_with(|| a.key.pid.0.cmp(&b.key.pid.0))
        });
        Self {
            seq,
            timestamp_ms,
            system,
            top: ranked.into_iter().take(n).cloned().collect(),
            process_count: u32::try_from(processes.len()).unwrap_or(u32::MAX),
        }
    }
}

/// One sensor reading, flattened for a remote client.
///
/// `unit`, `source` and `quality` are translation keys, the same ones the
/// desktop's Devices screen uses: `temperature`, `power`, `voltage`,
/// `fanSpeed`, `charge`, `percent`; `measured`, `derived`, `nameplate`.
/// `label` is the backend's English label, because the set of sensors has
/// no fixed shape to hang translations on.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct SensorLine {
    /// Stable across requests, for charting one reading over time.
    pub key: String,
    pub label: String,
    pub value: f32,
    pub unit: String,
    pub source: String,
    pub quality: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{Pid, ProcessKey};
    use crate::process::{ProcessFlags, ProcessKind, ProcessState, ProtectionLevel};
    use crate::units::{Bytes, BytesPerSec, Percent};

    fn process(pid: u32, cpu: f32, private: u64) -> Process {
        Process {
            key: ProcessKey::new(Pid(pid), 1),
            parent: None,
            name: format!("p{pid}.exe"),
            kind: ProcessKind::App,
            state: ProcessState::Running,
            flags: ProcessFlags::empty(),
            integrity: None,
            protection: ProtectionLevel::None,
            cpu: Percent(cpu),
            memory_private: Bytes(private),
            memory_working_set: Bytes(private),
            disk_read: BytesPerSec::ZERO,
            disk_write: BytesPerSec::ZERO,
            net_rx: None,
            net_tx: None,
            gpu: None,
            gpu_memory: None,
            thread_count: 1,
            handle_count: None,
            user: None,
            uptime_secs: 1,
        }
    }

    #[test]
    fn the_busiest_come_first_and_the_count_covers_the_whole_machine() {
        let all = [
            process(1, 2.0, 10),
            process(2, 40.0, 10),
            process(3, 9.0, 10),
        ];
        let s = Summary::from_parts(7, 1, SystemMetrics::default(), &all, 2);
        let pids: Vec<u32> = s.top.iter().map(|p| p.key.pid.0).collect();
        assert_eq!(pids, vec![2, 3]);
        assert_eq!(s.process_count, 3);
    }

    #[test]
    fn idle_processes_are_ranked_by_memory_so_the_list_does_not_shuffle() {
        let all = [
            process(1, 0.0, 10),
            process(2, 0.0, 500),
            process(3, 0.0, 50),
        ];
        let s = Summary::from_parts(1, 1, SystemMetrics::default(), &all, 3);
        let pids: Vec<u32> = s.top.iter().map(|p| p.key.pid.0).collect();
        assert_eq!(pids, vec![2, 3, 1]);
    }
}
