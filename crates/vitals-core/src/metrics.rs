//! Machine-wide metrics: the data behind the Performance surface.

use crate::ids::{DiskId, GpuId, NicId};
use crate::units::{Bytes, BytesPerSec, Celsius, Hertz, Percent, Watts};

/// Everything sampled about the machine in one tick.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct SystemMetrics {
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub disks: Vec<DiskMetrics>,
    pub networks: Vec<NetworkMetrics>,
    pub gpus: Vec<GpuMetrics>,
    /// Total system power draw where measurable (laptops, some desktops).
    pub power_draw: Option<Watts>,
    pub battery: Option<BatteryMetrics>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct CpuMetrics {
    pub total: Percent,
    /// Per logical processor.
    ///
    /// A plain `Vec` despite being allocated every tick: this is one
    /// allocation per *frame*, not per process, so it is far below the noise
    /// floor of the syscalls that produced the data. A `SmallVec` here would
    /// be optimising the wrong thing and would not survive the boundary to
    /// TypeScript anyway.
    pub per_core: Vec<Percent>,
    /// Kernel-mode share of `total`.
    pub kernel: Percent,
    /// Current effective clock, averaged across active cores.
    ///
    /// "Effective" rather than requested: modern CPUs park and boost
    /// constantly, and reporting the nominal base clock (as Task Manager
    /// largely does) tells the user nothing about what is happening now.
    pub effective_clock: Option<Hertz>,
    pub max_clock: Option<Hertz>,
    pub temperature: Option<Celsius>,
    pub power: Option<Watts>,
    /// True when the CPU is being held below its capability by heat or power
    /// limits. The headline signal for "why is my PC slow".
    pub throttled: Option<ThrottleReason>,
    pub process_count: u32,
    pub thread_count: u32,
    pub handle_count: Option<u32>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub uptime_secs: u64,
    /// Context switches per second.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub context_switches: Option<u64>,
    /// Interrupts per second.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub interrupts: Option<u64>,
}

/// Why the hardware is running below its rated capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum ThrottleReason {
    Thermal,
    PowerLimit,
    CurrentLimit,
    VoltageDrop,
    /// Deliberately limited by the active power plan.
    PowerPolicy,
    Unknown,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMetrics {
    pub total: Bytes,
    /// In use by processes and the kernel — the number users care about.
    pub used: Bytes,
    pub available: Bytes,
    /// Cached file data. Counted as available, because it is.
    pub cached: Bytes,
    /// Kernel memory that can be paged out.
    pub paged_pool: Bytes,
    /// Kernel memory that cannot.
    pub non_paged_pool: Bytes,
    pub committed: Bytes,
    pub commit_limit: Bytes,
    pub swap_total: Bytes,
    /// Page file / swap actually occupied.
    ///
    /// `None` where the platform cannot report it cheaply. On Windows the
    /// figure is only available through WMI or a performance counter, both
    /// far too slow for the per-tick path, so it arrives on a slower cadence.
    /// Zero would be a lie — an idle page file and an unmeasured one are
    /// different facts.
    pub swap_used: Option<Bytes>,
    /// Hardware-reserved, invisible to the OS.
    pub hardware_reserved: Bytes,
    /// Page faults per second — the real indicator of memory pressure.
    /// A machine at 90% RAM with no faulting is fine; one at 60% that is
    /// thrashing is not, and only this number distinguishes them.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub page_faults_per_sec: Option<u64>,
    pub speed: Option<Hertz>,
    pub slots_used: Option<u32>,
    pub slots_total: Option<u32>,
    pub form_factor: Option<String>,
}

impl MemoryMetrics {
    #[must_use]
    pub fn used_percent(&self) -> Percent {
        Percent::ratio(self.used.get(), self.total.get())
    }

    /// Whether the system is under genuine memory pressure.
    ///
    /// High utilisation alone is not pressure — unused RAM is wasted RAM.
    /// Pressure is utilisation *plus* commit approaching its limit, which is
    /// when allocations start failing and the machine falls over.
    #[must_use]
    pub fn is_under_pressure(&self) -> bool {
        let commit_ratio = Percent::ratio(self.committed.get(), self.commit_limit.get());
        self.used_percent().get() > 85.0 && commit_ratio.get() > 90.0
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct DiskMetrics {
    pub id: DiskId,
    pub name: String,
    pub model: Option<String>,
    /// `C:`, `/dev/nvme0n1`, etc.
    pub mount: Option<String>,
    pub kind: DiskKind,
    pub total: Bytes,
    pub free: Bytes,
    pub read: BytesPerSec,
    pub write: BytesPerSec,
    /// Share of time the disk had at least one IO outstanding.
    pub active_time: Percent,
    /// Average response time in milliseconds. The number that actually
    /// correlates with a machine feeling slow — far more than throughput.
    pub response_ms: Option<f32>,
    pub queue_depth: Option<f32>,
    pub temperature: Option<Celsius>,
    /// SMART overall health, when readable.
    pub health: Option<DiskHealth>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum DiskKind {
    Hdd,
    Ssd,
    Nvme,
    Removable,
    Network,
    Optical,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealth {
    /// Remaining endurance, 100 = new.
    pub life_remaining: Option<Percent>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub power_on_hours: Option<u64>,
    pub total_written: Option<Bytes>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub reallocated_sectors: Option<u64>,
    /// SMART says the drive is failing. Surfaced prominently — this is the
    /// single most valuable thing a monitoring tool can tell someone.
    pub failing: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMetrics {
    pub id: NicId,
    pub name: String,
    pub adapter: Option<String>,
    pub kind: NetworkKind,
    pub rx: BytesPerSec,
    pub tx: BytesPerSec,
    pub rx_total: Bytes,
    pub tx_total: Bytes,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub link_speed: Option<u64>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub mac: Option<String>,
    pub connected: bool,
    /// Wi-Fi signal strength.
    pub signal: Option<Percent>,
    pub ssid: Option<String>,
    /// Dropped packets per second — invisible in Task Manager, and the first
    /// sign of a failing cable or saturated link.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub errors_per_sec: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkKind {
    Ethernet,
    WiFi,
    Cellular,
    Bluetooth,
    Loopback,
    Virtual,
    Vpn,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct GpuMetrics {
    pub id: GpuId,
    pub name: String,
    pub vendor: GpuVendor,
    /// Per-engine utilisation (3D, Copy, Video Decode, Compute...).
    ///
    /// A single "GPU %" is close to meaningless: a machine can sit at 5%
    /// overall while the video-decode engine is pinned. Task Manager gets
    /// this right and it is one of its few genuinely good ideas.
    pub engines: Vec<GpuEngine>,
    /// Highest utilisation across engines, for a single summary column.
    pub utilization: Percent,
    pub memory_used: Option<Bytes>,
    pub memory_total: Option<Bytes>,
    pub shared_memory_used: Option<Bytes>,
    pub core_clock: Option<Hertz>,
    pub memory_clock: Option<Hertz>,
    pub temperature: Option<Celsius>,
    pub hotspot_temperature: Option<Celsius>,
    pub power: Option<Watts>,
    pub power_limit: Option<Watts>,
    pub fan_percent: Option<Percent>,
    pub fan_rpm: Option<u32>,
    pub throttled: Option<ThrottleReason>,
    pub driver_version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct GpuEngine {
    pub name: String,
    pub utilization: Percent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Qualcomm,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct BatteryMetrics {
    pub charge: Percent,
    pub charging: bool,
    /// Estimated seconds remaining, when the OS will tell us.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub time_remaining_secs: Option<u64>,
    /// Current draw; negative while charging.
    pub power: Option<Watts>,
    /// Full-charge capacity as a share of design capacity — battery wear.
    pub health: Option<Percent>,
    pub cycle_count: Option<u32>,
    pub temperature: Option<Celsius>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(used: u64, total: u64, committed: u64, limit: u64) -> MemoryMetrics {
        MemoryMetrics {
            total: Bytes(total),
            used: Bytes(used),
            committed: Bytes(committed),
            commit_limit: Bytes(limit),
            ..Default::default()
        }
    }

    #[test]
    fn high_usage_alone_is_not_pressure() {
        // 90% used but commit is comfortable: this machine is healthy.
        let m = memory(90, 100, 50, 200);
        assert!(
            !m.is_under_pressure(),
            "unused RAM is wasted RAM; high utilisation is not a fault"
        );
    }

    #[test]
    fn high_usage_with_commit_near_limit_is_pressure() {
        let m = memory(90, 100, 95, 100);
        assert!(m.is_under_pressure());
    }

    #[test]
    fn used_percent_handles_zero_total() {
        let m = memory(0, 0, 0, 0);
        assert_eq!(m.used_percent(), Percent::ZERO);
        assert!(!m.is_under_pressure());
    }
}
