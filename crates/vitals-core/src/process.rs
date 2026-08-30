//! The process domain model.

use crate::ids::{Pid, ProcessKey};
use crate::units::{Bytes, BytesPerSec, Percent};

/// What kind of thing this process is, for grouping and iconography.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessKind {
    /// Has a visible top-level window.
    App,
    /// Runs in the background with no window.
    Background,
    /// An OS service / daemon.
    Service,
    /// Part of the operating system itself.
    System,
    /// Runs inside a container or WSL distro.
    Containerized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessState {
    Running,
    /// Every thread is suspended — either by us or by the OS (UWP apps).
    Suspended,
    /// Not scheduled, waiting on IO or a synchronisation object.
    Waiting,
    /// Stopped responding to window messages.
    NotResponding,
    /// Exited but the record is still held open by a handle.
    Zombie,
}

/// Windows integrity level / POSIX privilege analogue.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrityLevel {
    Untrusted,
    Low,
    Medium,
    High,
    System,
    Protected,
}

/// Whether the OS actively protects this process from tampering.
///
/// Matters because attempting to terminate a PPL process fails with an
/// access-denied that elevation cannot fix — the UI must not offer "run as
/// admin" as a remedy here, which is exactly what Task Manager does wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum ProtectionLevel {
    None,
    /// Protected Process Light.
    Light,
    /// Fully protected — not terminable from user mode at any privilege.
    Full,
}

/// Boolean facts about a process, packed to keep [`Process`] small.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct ProcessFlags(u32);

bitflags_impls! {
    ProcessFlags: u32 {
        /// Signed by a trusted authority and the signature verified.
        SIGNED           = 1 << 0,
        /// Signature present but could not be verified.
        SIGNATURE_BROKEN = 1 << 1,
        /// Elevated (running as administrator / root).
        ELEVATED         = 1 << 2,
        /// Running under `EcoQoS` / efficiency mode.
        EFFICIENCY_MODE  = 1 << 3,
        /// A 32-bit process on a 64-bit OS.
        WOW64            = 1 << 4,
        /// Terminating this will bugcheck or log the user out.
        CRITICAL         = 1 << 5,
        /// Has at least one visible top-level window.
        HAS_WINDOW       = 1 << 6,
        /// Immersive / packaged (UWP, MSIX).
        PACKAGED         = 1 << 7,
        /// Started before we did, so its lifetime totals are incomplete.
        PREEXISTING      = 1 << 8,
        /// Lived for less than one full sample interval.
        SHORT_LIVED      = 1 << 9,
        /// Debugger currently attached.
        DEBUGGED         = 1 << 10,
        /// .NET / managed runtime loaded.
        MANAGED          = 1 << 11,
    }
}

/// A snapshot of one process at one instant.
///
/// Deliberately flat and `Clone`-cheap. The process table holds thousands of
/// these per frame; anything requiring a heap walk per process (command line,
/// full path, icon, signature) is fetched lazily on selection instead and
/// lives in a detail struct, not here.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct Process {
    /// Race-free identity. Prefer this over `key.pid` for any action.
    pub key: ProcessKey,
    pub parent: Option<Pid>,
    /// Executable name only (`chrome.exe`), not the full path.
    pub name: String,
    pub kind: ProcessKind,
    pub state: ProcessState,
    pub flags: ProcessFlags,
    pub integrity: Option<IntegrityLevel>,
    pub protection: ProtectionLevel,

    /// Share of total machine CPU, not of a single core.
    ///
    /// Task Manager's own inconsistency here (Processes tab normalises, the
    /// Details tab does not) is a perennial user complaint. We normalise
    /// everywhere and offer a per-core view as an explicit toggle.
    pub cpu: Percent,
    /// Private bytes — memory that would be freed if the process exited.
    /// The honest "how much RAM is this using" number.
    pub memory_private: Bytes,
    /// Resident working set, including shared pages.
    pub memory_working_set: Bytes,

    pub disk_read: BytesPerSec,
    pub disk_write: BytesPerSec,
    pub net_rx: BytesPerSec,
    pub net_tx: BytesPerSec,
    pub gpu: Option<Percent>,
    pub gpu_memory: Option<Bytes>,

    pub thread_count: u32,
    pub handle_count: Option<u32>,
    /// Owning user, `None` when we lack rights to query the token.
    pub user: Option<String>,
    /// Uptime in seconds.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub uptime_secs: u64,
}

impl Process {
    /// Whether terminating this process is safe to offer without a scary
    /// confirmation.
    #[must_use]
    pub fn is_safely_terminable(&self) -> bool {
        !self.flags.contains(ProcessFlags::CRITICAL)
            && self.protection == ProtectionLevel::None
            && self.kind != ProcessKind::System
    }

    /// Whether elevation could plausibly allow us to act on this process.
    ///
    /// False for protected processes: no amount of privilege helps, and
    /// offering elevation there is a dead end that wastes the user's time.
    #[must_use]
    pub fn elevation_would_help(&self) -> bool {
        self.protection == ProtectionLevel::None && !self.flags.contains(ProcessFlags::ELEVATED)
    }

    /// Total IO throughput, for a single sortable "is this thing busy" column.
    #[must_use]
    pub fn total_io(&self) -> BytesPerSec {
        BytesPerSec(
            self.disk_read
                .get()
                .saturating_add(self.disk_write.get())
                .saturating_add(self.net_rx.get())
                .saturating_add(self.net_tx.get()),
        )
    }
}

/// Expensive per-process detail, fetched only for the selected row.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
pub struct ProcessDetail {
    pub executable_path: Option<String>,
    pub command_line: Option<String>,
    pub working_directory: Option<String>,
    pub description: Option<String>,
    pub company: Option<String>,
    pub version: Option<String>,
    /// SHA-256 of the executable, for reputation lookups.
    pub sha256: Option<String>,
    pub signer: Option<String>,
    pub environment: Vec<(String, String)>,
    /// Services hosted by this process (`svchost.exe` is meaningless without).
    pub services: Vec<String>,
    /// Container or WSL distro name when containerised.
    pub container: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::Pid;

    fn sample(flags: ProcessFlags, protection: ProtectionLevel, kind: ProcessKind) -> Process {
        Process {
            key: ProcessKey::new(Pid(100), 1),
            parent: None,
            name: "test.exe".into(),
            kind,
            state: ProcessState::Running,
            flags,
            integrity: None,
            protection,
            cpu: Percent::ZERO,
            memory_private: Bytes::ZERO,
            memory_working_set: Bytes::ZERO,
            disk_read: BytesPerSec::ZERO,
            disk_write: BytesPerSec::ZERO,
            net_rx: BytesPerSec::ZERO,
            net_tx: BytesPerSec::ZERO,
            gpu: None,
            gpu_memory: None,
            thread_count: 1,
            handle_count: None,
            user: None,
            uptime_secs: 0,
        }
    }

    #[test]
    fn critical_processes_are_not_safely_terminable() {
        let p = sample(
            ProcessFlags::CRITICAL,
            ProtectionLevel::None,
            ProcessKind::Background,
        );
        assert!(!p.is_safely_terminable());
    }

    #[test]
    fn protected_processes_are_not_safely_terminable() {
        let p = sample(
            ProcessFlags::empty(),
            ProtectionLevel::Light,
            ProcessKind::Background,
        );
        assert!(!p.is_safely_terminable());
    }

    #[test]
    fn ordinary_process_is_terminable() {
        let p = sample(
            ProcessFlags::empty(),
            ProtectionLevel::None,
            ProcessKind::App,
        );
        assert!(p.is_safely_terminable());
    }

    #[test]
    fn elevation_is_pointless_against_protected_processes() {
        let p = sample(
            ProcessFlags::empty(),
            ProtectionLevel::Full,
            ProcessKind::System,
        );
        assert!(
            !p.elevation_would_help(),
            "offering elevation for a PPL process sends the user down a dead end"
        );
    }

    #[test]
    fn total_io_sums_all_four_streams() {
        let mut p = sample(
            ProcessFlags::empty(),
            ProtectionLevel::None,
            ProcessKind::App,
        );
        p.disk_read = BytesPerSec(1);
        p.disk_write = BytesPerSec(2);
        p.net_rx = BytesPerSec(4);
        p.net_tx = BytesPerSec(8);
        assert_eq!(p.total_io().get(), 15);
    }

    #[test]
    fn total_io_saturates_rather_than_panicking() {
        let mut p = sample(
            ProcessFlags::empty(),
            ProtectionLevel::None,
            ProcessKind::App,
        );
        p.disk_read = BytesPerSec(u64::MAX);
        p.disk_write = BytesPerSec(u64::MAX);
        assert_eq!(p.total_io().get(), u64::MAX);
    }
}
