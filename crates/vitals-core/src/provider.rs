//! Provider traits: the seam between the OS-agnostic core and each platform.
//!
//! Every trait here is deliberately **synchronous**. Sampling is CPU-bound
//! syscall work, not IO waiting; wrapping it in `async` would add executor
//! overhead to the hottest path in the product and buy nothing. The sampler
//! runs these on a dedicated thread and publishes results over a channel.
//!
//! Traits are split by concern rather than gathered into one god-trait so a
//! platform can implement them incrementally — the Linux backend can ship
//! [`ProcessProvider`] long before [`PowerProvider`] exists.

use crate::capability::Capabilities;
use crate::error::Result;
use crate::ids::{Pid, ProcessKey};
use crate::metrics::SystemMetrics;
use crate::process::{Process, ProcessDetail};
use crate::sensor::Sensor;
use crate::units::Bytes;

/// Static facts about the machine, queried once at startup.
pub trait HostProvider: Send + Sync {
    fn host_info(&self) -> Result<HostInfo>;

    /// What this backend can do right now, given current privileges.
    fn capabilities(&self) -> Capabilities;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct HostInfo {
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub architecture: String,
    pub cpu_model: String,
    pub cpu_vendor: String,
    pub physical_cores: u32,
    pub logical_cores: u32,
    /// Populated on hybrid designs (Intel P/E, ARM big.LITTLE).
    ///
    /// Without this the per-core view is misleading: an E-core at 100% and a
    /// P-core at 100% are not the same event, and treating them alike is why
    /// most monitors misreport load on 12th-gen-and-later Intel.
    pub core_topology: Option<Vec<CoreClass>>,
    pub total_memory: Bytes,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub boot_time_ms: u64,
    pub is_virtual_machine: bool,
    pub motherboard: Option<String>,
    pub bios_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum CoreClass {
    Performance,
    Efficiency,
    /// Low-power island (Meteor Lake and later).
    LowPower,
    Standard,
}

/// Machine-wide metric sampling.
pub trait SystemProvider: Send + Sync {
    /// Samples all machine-wide metrics.
    ///
    /// Called on every tick, so it must be allocation-light and must never
    /// block on a slow WMI query — expensive sources belong on their own
    /// slower cadence behind a cache.
    fn sample(&mut self) -> Result<SystemMetrics>;

    /// Resets rate baselines. Called after a pause so the first frame after
    /// resuming does not report a huge fake spike from the accumulated delta.
    fn reset_baseline(&mut self);
}

/// Process enumeration and control.
pub trait ProcessProvider: Send + Sync {
    /// Enumerates all visible processes.
    fn sample(&mut self) -> Result<Vec<Process>>;

    /// Expensive detail for one process, fetched on selection only.
    fn detail(&self, key: ProcessKey) -> Result<ProcessDetail>;

    /// Terminates a process.
    ///
    /// Takes a [`ProcessKey`], not a [`Pid`], so a stale reference fails
    /// cleanly instead of killing whatever inherited the number.
    fn terminate(&self, key: ProcessKey) -> Result<()>;

    /// Terminates a process and all its descendants.
    fn terminate_tree(&self, key: ProcessKey) -> Result<()>;

    fn suspend(&self, key: ProcessKey) -> Result<()>;
    fn resume(&self, key: ProcessKey) -> Result<()>;

    fn set_priority(&self, key: ProcessKey, priority: Priority) -> Result<()>;

    /// Pins a process to a set of logical CPUs, given as a bitmask.
    fn set_affinity(&self, key: ProcessKey, mask: u64) -> Result<()>;

    /// Toggles `EcoQoS` / efficiency mode.
    fn set_efficiency_mode(&self, key: ProcessKey, enabled: bool) -> Result<()>;

    /// Trims the working set, returning bytes actually released.
    ///
    /// Returns the measured delta rather than `()` precisely because this
    /// operation is usually counterproductive: the pages are paged back in on
    /// next access, costing time. Reporting the real number lets the UI tell
    /// the truth instead of implying a win.
    fn trim_working_set(&self, key: ProcessKey) -> Result<Bytes>;

    /// Children of `pid`, for building the tree without a full re-walk.
    fn children(&self, pid: Pid) -> Result<Vec<Pid>>;
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum Priority {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    /// Can starve the input thread and hang the desktop. The UI requires an
    /// explicit confirmation for this one.
    Realtime,
}

impl Priority {
    /// Whether selecting this can plausibly hang the machine.
    #[must_use]
    pub const fn needs_confirmation(self) -> bool {
        matches!(self, Self::Realtime)
    }
}

/// Hardware sensor access.
pub trait SensorProvider: Send + Sync {
    fn sample(&mut self) -> Result<Vec<Sensor>>;

    /// Writes to a controllable sensor (fan duty, power limit).
    fn write(&self, id: &crate::ids::SensorId, value: f32) -> Result<()>;
}

/// Storage enumeration and management.
pub trait StorageProvider: Send + Sync {
    /// Recursively sizes a directory tree, for the storage treemap.
    ///
    /// `progress` is invoked periodically so a scan of a multi-terabyte
    /// volume can report progress and be cancelled, rather than appearing
    /// frozen for minutes — the failure mode of every built-in disk tool.
    fn scan_tree(
        &self,
        root: &std::path::Path,
        progress: &mut dyn FnMut(ScanProgress) -> ScanControl,
    ) -> Result<DirectoryNode>;

    /// Categories of reclaimable space, without deleting anything.
    fn cleanup_candidates(&self) -> Result<Vec<CleanupCategory>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanControl {
    Continue,
    Cancel,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct ScanProgress {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub files_seen: u64,
    pub bytes_seen: Bytes,
    pub current_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct DirectoryNode {
    pub name: String,
    pub size: Bytes,
    /// Size occupied on disk, which differs from `size` for sparse and
    /// compressed files. Showing logical size alone makes a treemap lie.
    pub size_on_disk: Bytes,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub file_count: u64,
    pub children: Vec<DirectoryNode>,
    pub is_directory: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct CleanupCategory {
    pub id: String,
    pub name: String,
    pub description: String,
    pub reclaimable: Bytes,
    /// Whether removing this is risk-free.
    ///
    /// Drives default selection: safe categories are pre-ticked, unsafe ones
    /// (downloads, hibernation file) never are.
    pub safe: bool,
    pub paths: Vec<String>,
}

/// Power plan / policy management.
pub trait PowerProvider: Send + Sync {
    fn power_plans(&self) -> Result<Vec<PowerPlan>>;
    fn active_plan(&self) -> Result<PowerPlan>;
    fn set_active_plan(&self, id: &str) -> Result<()>;
    fn create_plan(&self, from: &str, name: &str) -> Result<PowerPlan>;
    fn delete_plan(&self, id: &str) -> Result<()>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct PowerPlan {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub active: bool,
    /// Whether this is an OS-provided plan that cannot be deleted.
    pub built_in: bool,
}

/// Network connection enumeration and control.
pub trait NetworkProvider: Send + Sync {
    fn connections(&mut self) -> Result<Vec<Connection>>;

    /// Forcibly closes a TCP connection.
    fn close_connection(&self, conn: &Connection) -> Result<()>;

    /// Blocks a program from the network via the platform firewall.
    fn block_program(&self, executable_path: &str, direction: Direction) -> Result<String>;

    fn unblock_program(&self, rule_id: &str) -> Result<()>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct Connection {
    pub protocol: Protocol,
    pub local_address: String,
    pub local_port: u16,
    pub remote_address: Option<String>,
    pub remote_port: Option<u16>,
    pub state: ConnectionState,
    pub owner_pid: Option<Pid>,
    pub owner_name: Option<String>,
    /// Reverse-DNS or geo-IP annotation. Populated lazily and only when the
    /// user opts in, since it necessarily involves outbound lookups.
    pub remote_host: Option<String>,
    pub country: Option<String>,
    pub bytes_sent: Option<Bytes>,
    pub bytes_received: Option<Bytes>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    Tcp,
    Udp,
    Tcp6,
    Udp6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionState {
    Established,
    Listen,
    SynSent,
    SynReceived,
    FinWait1,
    FinWait2,
    TimeWait,
    Closed,
    CloseWait,
    LastAck,
    Closing,
    DeleteTcb,
    /// UDP has no connection state.
    Stateless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Inbound,
    Outbound,
    Both,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_realtime_priority_demands_confirmation() {
        assert!(Priority::Realtime.needs_confirmation());
        assert!(!Priority::High.needs_confirmation());
        assert!(!Priority::Normal.needs_confirmation());
    }

    #[test]
    fn priority_ordering_is_meaningful() {
        assert!(Priority::Idle < Priority::Normal);
        assert!(Priority::Normal < Priority::Realtime);
    }
}
