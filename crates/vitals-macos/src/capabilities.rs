//! What this macOS build can actually do.
//!
//! A capability is listed as available only when there is code behind it
//! that the UI can reach. Windows-only facilities (services, scheduled
//! tasks, `EcoQoS`, working-set trimming, uninstalling…) are permanently
//! unavailable here; the sandboxed store build loses process control and the
//! sensors too, and says so.

use vitals_core::capability::Unavailable;
use vitals_core::{Capabilities, Capability};

#[must_use]
pub fn capabilities() -> Capabilities {
    let mut caps = Capabilities::new();

    let process_control = [
        Capability::TerminateProcess,
        Capability::SuspendProcess,
        Capability::SetProcessPriority,
    ];
    let sensors = [Capability::Thermals];

    if crate::APP_STORE {
        for cap in process_control.into_iter().chain(sensors) {
            caps = caps.without(cap, Unavailable::AppSandbox);
        }
        caps = caps.without(Capability::PerProcessDiskIo, Unavailable::AppSandbox);
    } else {
        for cap in process_control.into_iter().chain(sensors) {
            caps = caps.with(cap);
        }
        // proc_pid_rusage gives every own-user process its disk bytes, with
        // no helper, unlike Windows where it needs an ETW session.
        caps = caps.with(Capability::PerProcessDiskIo);
    }

    for cap in [
        Capability::PerProcessNetworkIo,
        Capability::PerProcessGpu,
        Capability::HandleEnumeration,
        Capability::ThreadStacks,
        Capability::PowerDraw,
        Capability::ErrorReports,
        Capability::ShortLivedProcesses,
        Capability::FanControl,
    ] {
        caps = caps.without(cap, Unavailable::NotImplemented);
    }

    for cap in [
        Capability::SetProcessAffinity,
        Capability::SetEfficiencyMode,
        Capability::TrimWorkingSet,
        Capability::WindowFlashCapture,
        Capability::ManagePowerPlans,
        Capability::ManageServices,
        Capability::ManageStartupItems,
        Capability::ManageScheduledTasks,
        Capability::UninstallApplications,
        Capability::FirewallControl,
        Capability::CloseConnection,
        Capability::GpuOverclock,
        Capability::DiskCleanup,
    ] {
        caps = caps.without(cap, Unavailable::NotSupportedOnPlatform);
    }

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_capability_is_reported_exactly_once() {
        let caps = capabilities();
        let mut all: Vec<Capability> = caps
            .available
            .iter()
            .copied()
            .chain(caps.unavailable.iter().map(|(c, _)| *c))
            .collect();
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n, "a capability is listed twice");
        assert_eq!(n, 26, "a capability is missing from the report");
    }

    #[test]
    fn windows_only_facilities_are_never_offered() {
        let caps = capabilities();
        for cap in [Capability::ManageServices, Capability::SetEfficiencyMode] {
            assert_eq!(caps.reason(cap), Some(Unavailable::NotSupportedOnPlatform));
        }
    }

    #[test]
    fn the_direct_build_can_end_processes_and_the_store_build_says_why_not() {
        let caps = capabilities();
        if crate::APP_STORE {
            assert_eq!(
                caps.reason(Capability::TerminateProcess),
                Some(Unavailable::AppSandbox)
            );
        } else {
            assert!(caps.has(Capability::TerminateProcess));
        }
    }
}
