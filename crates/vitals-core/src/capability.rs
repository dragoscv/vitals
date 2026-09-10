//! Declared capabilities.
//!
//! The UI asks the backend what it can do *before* rendering, so a feature
//! that this OS, this hardware, or this privilege level cannot support is
//! greyed out with a reason attached — never presented as a live button that
//! throws when clicked. Discovering limitations by failure is the defining
//! UX flaw of every tool in this category.

/// A single discrete thing a backend may or may not be able to do.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Capability {
    // ---- Observation ----
    /// Per-process disk read/write attribution (needs ETW on Windows).
    PerProcessDiskIo,
    /// Per-process network throughput attribution (needs ETW on Windows).
    PerProcessNetworkIo,
    /// Per-process GPU engine utilisation.
    PerProcessGpu,
    /// Capture of processes that live for less than one sample interval.
    ShortLivedProcesses,
    /// Detection of windows that appear and vanish quickly.
    WindowFlashCapture,
    /// Open handle and loaded-module enumeration ("what locks this file").
    HandleEnumeration,
    /// Thread-level detail including stacks.
    ThreadStacks,
    /// CPU/GPU/board temperature and fan sensors.
    Thermals,
    /// Per-component power draw in watts.
    PowerDraw,
    /// OS and application error/crash reports.
    ErrorReports,

    // ---- Mutation ----
    TerminateProcess,
    SuspendProcess,
    SetProcessPriority,
    SetProcessAffinity,
    /// Windows `EcoQoS` / equivalent efficiency mode.
    SetEfficiencyMode,
    /// Trim working sets (the honest form of "memory cleanup").
    TrimWorkingSet,
    ManagePowerPlans,
    ManageServices,
    ManageStartupItems,
    ManageScheduledTasks,
    UninstallApplications,
    /// Create/remove firewall rules for per-app network blocking.
    FirewallControl,
    /// Close an individual network connection.
    CloseConnection,
    GpuOverclock,
    FanControl,
    DiskCleanup,
}

impl Capability {
    /// Whether exercising this capability changes system state.
    ///
    /// Used to gate the entire mutating surface behind a single confirmation
    /// policy rather than remembering to guard each call site.
    #[must_use]
    pub const fn is_mutating(self) -> bool {
        !matches!(
            self,
            Self::PerProcessDiskIo
                | Self::PerProcessNetworkIo
                | Self::PerProcessGpu
                | Self::ShortLivedProcesses
                | Self::WindowFlashCapture
                | Self::HandleEnumeration
                | Self::ThreadStacks
                | Self::Thermals
                | Self::PowerDraw
                | Self::ErrorReports
        )
    }

    /// Whether this can damage hardware or lose data if misused.
    ///
    /// The UI puts these behind an explicit "Advanced / at your own risk"
    /// opt-in that is off by default.
    #[must_use]
    pub const fn is_dangerous(self) -> bool {
        matches!(
            self,
            Self::GpuOverclock | Self::FanControl | Self::DiskCleanup
        )
    }
}

/// Why a capability is unavailable, so the UI can say something useful.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "kebab-case")]
pub enum Unavailable {
    /// The platform has no equivalent facility. Permanent — hide the feature.
    NotSupportedOnPlatform,
    /// The hardware is absent (no discrete GPU, no fan controller).
    NoSuchHardware,
    /// Available, but only with elevation. Offer to elevate.
    NeedsElevation,
    /// Available, but only via the helper service, which is not installed.
    NeedsHelper,
    /// Available, but only via an optional plugin the user has not installed.
    NeedsPlugin,
    /// The user has explicitly disabled it in settings.
    DisabledByUser,
    /// Vitals has not built it yet. The platform could do it; we cannot.
    ///
    /// Distinct from the other reasons because none of them are the user's
    /// problem to solve and this one is not either — but a capability that is
    /// merely unwritten must never be reported as available. Five were, for
    /// months, because the only alternative was lying in the other direction.
    NotImplemented,
}

impl Unavailable {
    /// Whether the user can do something about this right now.
    #[must_use]
    pub const fn is_actionable(self) -> bool {
        matches!(
            self,
            Self::NeedsElevation | Self::NeedsHelper | Self::NeedsPlugin | Self::DisabledByUser
        )
    }
}

/// The full capability report for a backend at a point in time.
///
/// Re-queried when privilege changes (helper connects, app elevates) so the
/// UI can light up newly available features without a restart.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Capabilities currently usable.
    pub available: Vec<Capability>,
    /// Capabilities that are not, each with a reason.
    pub unavailable: Vec<(Capability, Unavailable)>,
}

impl Capabilities {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, cap: Capability) -> Self {
        self.available.push(cap);
        self
    }

    #[must_use]
    pub fn without(mut self, cap: Capability, why: Unavailable) -> Self {
        self.unavailable.push((cap, why));
        self
    }

    #[must_use]
    pub fn has(&self, cap: Capability) -> bool {
        self.available.contains(&cap)
    }

    /// Why `cap` is unavailable, or `None` if it is available or unknown.
    #[must_use]
    pub fn reason(&self, cap: Capability) -> Option<Unavailable> {
        self.unavailable
            .iter()
            .find(|(c, _)| *c == cap)
            .map(|(_, r)| *r)
    }

    /// Capabilities the user could unlock by acting (elevating, installing).
    ///
    /// Drives a single consolidated "unlock more" prompt instead of nagging
    /// per feature.
    #[must_use]
    pub fn actionable_gaps(&self) -> Vec<(Capability, Unavailable)> {
        self.unavailable
            .iter()
            .copied()
            .filter(|(_, why)| why.is_actionable())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_is_not_mutating() {
        assert!(!Capability::Thermals.is_mutating());
        assert!(Capability::TerminateProcess.is_mutating());
    }

    #[test]
    fn overclock_is_flagged_dangerous() {
        assert!(Capability::GpuOverclock.is_dangerous());
        assert!(!Capability::SuspendProcess.is_dangerous());
    }

    #[test]
    fn actionable_gaps_exclude_permanent_limitations() {
        let caps = Capabilities::new()
            .with(Capability::TerminateProcess)
            .without(Capability::GpuOverclock, Unavailable::NoSuchHardware)
            .without(Capability::PerProcessDiskIo, Unavailable::NeedsHelper);

        let gaps = caps.actionable_gaps();
        assert_eq!(gaps.len(), 1, "hardware absence is not actionable");
        assert_eq!(gaps[0].0, Capability::PerProcessDiskIo);
    }

    #[test]
    fn reason_is_reported_for_missing_capability() {
        let caps = Capabilities::new()
            .without(Capability::FanControl, Unavailable::NotSupportedOnPlatform);
        assert!(!caps.has(Capability::FanControl));
        assert_eq!(
            caps.reason(Capability::FanControl),
            Some(Unavailable::NotSupportedOnPlatform)
        );
    }
}
