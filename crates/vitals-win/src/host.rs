//! Host information and capability reporting for Windows.

use vitals_core::capability::{Capabilities, Capability, Unavailable};

/// Reports static machine facts and what this backend can currently do.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsHost {
    /// Whether the current process holds an elevated token.
    pub elevated: bool,
    /// Whether the helper service is installed and reachable.
    pub helper_available: bool,
    /// Whether the optional sensor sidecar is installed.
    pub sidecar_available: bool,
}

impl WindowsHost {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            elevated: false,
            helper_available: false,
            sidecar_available: false,
        }
    }

    /// Computes the capability set from current privilege and component
    /// availability.
    ///
    /// Pure and dependency-free so the policy is unit-testable without a
    /// Windows machine in the loop — the mapping from "what is installed" to
    /// "what the UI offers" is exactly the logic that must not regress.
    #[must_use]
    pub fn capabilities(self) -> Capabilities {
        let mut caps = Capabilities::new();

        // Always available: plain enumeration needs no privilege.
        caps = caps
            .with(Capability::HandleEnumeration)
            .with(Capability::TerminateProcess)
            .with(Capability::SuspendProcess)
            .with(Capability::SetProcessPriority)
            .with(Capability::SetProcessAffinity)
            .with(Capability::SetEfficiencyMode)
            .with(Capability::TrimWorkingSet)
            .with(Capability::PerProcessGpu)
            .with(Capability::ManageStartupItems)
            .with(Capability::UninstallApplications)
            .with(Capability::ErrorReports)
            .with(Capability::WindowFlashCapture);

        // ETW-backed observation requires the helper: a kernel trace session
        // cannot be started from an unelevated process.
        for cap in [
            Capability::PerProcessDiskIo,
            Capability::PerProcessNetworkIo,
            Capability::ShortLivedProcesses,
        ] {
            caps = if self.helper_available {
                caps.with(cap)
            } else {
                caps.without(cap, Unavailable::NeedsHelper)
            };
        }

        // Privileged mutation: the helper does it, or elevation allows it
        // directly.
        let privileged = self.helper_available || self.elevated;
        for cap in [
            Capability::ManagePowerPlans,
            Capability::ManageServices,
            Capability::ManageScheduledTasks,
            Capability::FirewallControl,
            Capability::CloseConnection,
            Capability::DiskCleanup,
        ] {
            caps = if privileged {
                caps.with(cap)
            } else {
                caps.without(cap, Unavailable::NeedsElevation)
            };
        }

        // Deep sensors come from the optional sidecar; without it we fall
        // back to the far sparser WMI sources, which is still *something*,
        // so Thermals stays available either way.
        caps = caps.with(Capability::Thermals);
        caps = if self.sidecar_available {
            caps.with(Capability::PowerDraw)
                .with(Capability::FanControl)
        } else {
            caps.without(Capability::PowerDraw, Unavailable::NeedsPlugin)
                .without(Capability::FanControl, Unavailable::NeedsPlugin)
        };

        // Overclocking needs a vendor SDK, loaded at runtime from the driver.
        // Reported as plugin-gated until the vendor plugin confirms hardware.
        caps = caps.without(Capability::GpuOverclock, Unavailable::NeedsPlugin);

        // Thread stacks need symbol handling we have not built yet. Honest
        // about it rather than pretending.
        caps.without(Capability::ThreadStacks, Unavailable::NeedsHelper)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_process_control_works_without_privilege() {
        let caps = WindowsHost::new().capabilities();
        assert!(caps.has(Capability::TerminateProcess));
        assert!(caps.has(Capability::SuspendProcess));
    }

    #[test]
    fn etw_features_require_the_helper() {
        let caps = WindowsHost::new().capabilities();
        assert!(!caps.has(Capability::PerProcessDiskIo));
        assert_eq!(
            caps.reason(Capability::PerProcessDiskIo),
            Some(Unavailable::NeedsHelper)
        );
    }

    #[test]
    fn helper_unlocks_etw_and_privileged_mutation() {
        let caps = WindowsHost {
            helper_available: true,
            ..WindowsHost::new()
        }
        .capabilities();
        assert!(caps.has(Capability::PerProcessDiskIo));
        assert!(caps.has(Capability::ShortLivedProcesses));
        assert!(caps.has(Capability::FirewallControl));
    }

    #[test]
    fn elevation_alone_unlocks_mutation_but_not_etw() {
        // Elevation lets us change firewall rules directly, but the ETW
        // consumer lives in the helper, so tracing stays unavailable.
        let caps = WindowsHost {
            elevated: true,
            ..WindowsHost::new()
        }
        .capabilities();
        assert!(caps.has(Capability::FirewallControl));
        assert!(!caps.has(Capability::PerProcessDiskIo));
    }

    #[test]
    fn thermals_degrade_rather_than_disappear_without_the_sidecar() {
        let caps = WindowsHost::new().capabilities();
        assert!(
            caps.has(Capability::Thermals),
            "WMI gives us some temperatures even with no sidecar"
        );
        assert_eq!(
            caps.reason(Capability::PowerDraw),
            Some(Unavailable::NeedsPlugin)
        );
    }

    #[test]
    fn sidecar_unlocks_power_and_fan_control() {
        let caps = WindowsHost {
            sidecar_available: true,
            ..WindowsHost::new()
        }
        .capabilities();
        assert!(caps.has(Capability::PowerDraw));
        assert!(caps.has(Capability::FanControl));
    }

    #[test]
    fn gaps_are_actionable_so_the_ui_can_offer_a_fix() {
        let gaps = WindowsHost::new().capabilities().actionable_gaps();
        assert!(
            !gaps.is_empty(),
            "a bare install should be able to tell the user what to enable"
        );
        assert!(gaps.iter().all(|(_, why)| why.is_actionable()));
    }
}
