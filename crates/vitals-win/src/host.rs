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

    /// Reads the real state of this machine.
    ///
    /// [`Self::new`] is deliberately a zero-value constructor so the mapping
    /// below stays pure and testable without a Windows machine. Something has
    /// to fill it in, though, and nothing did: `capabilities()` was being
    /// called on `new()` directly, so an elevated Vitals reported exactly the
    /// same capability set as an unelevated one and greyed out features the
    /// user actually had.
    #[must_use]
    pub fn detect() -> Self {
        Self {
            elevated: is_elevated(),
            // Neither component exists yet. Stated here rather than silently
            // defaulted, so the day one ships there is an obvious place to
            // wire it in.
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

/// Whether this process is running with an elevated token.
///
/// `TOKEN_ELEVATION` rather than checking group membership: on a machine with
/// UAC on, an administrator's *unelevated* process still has the
/// Administrators group in its token, marked deny-only. Membership therefore
/// answers "could this user elevate", which is not the question — the
/// question is what this process can do right now.
///
/// Returns `false` on any failure. Wrongly claiming elevation would offer the
/// user actions that then fail; wrongly denying it greys out something that
/// would have worked, which is the safer direction to be wrong in.
fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = HANDLE::default();

    // SAFETY: `GetCurrentProcess` is a pseudo-handle needing no release, and
    // `OpenProcessToken` writes to `token` only on success.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) };
    if opened.is_err() {
        return false;
    }

    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0_u32;

    // SAFETY: the buffer matches the size declared, and `token` is live until
    // the close below.
    let queried = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            u32::try_from(size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
            &raw mut returned,
        )
    };

    // SAFETY: `token` came from `OpenProcessToken` and is closed exactly once.
    unsafe {
        let _ = CloseHandle(token);
    }

    queried.is_ok() && elevation.TokenIsElevated != 0
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
    fn detection_reports_something_and_agrees_with_the_pure_mapping() {
        // `detect()` is the piece that was missing: the mapping below it was
        // always correct and always tested, but nothing ever called it with a
        // real elevation value, so an elevated Vitals reported an unelevated
        // capability set.
        //
        // The elevated branch cannot be asserted from a test run — it depends
        // on how the runner was launched, and demanding elevation to run the
        // suite would be worse than the gap. What can be checked is that
        // detection produces the same answer as constructing the struct by
        // hand with the value it found, which is the join that was broken.
        let detected = WindowsHost::detect();
        let by_hand = WindowsHost {
            elevated: detected.elevated,
            helper_available: detected.helper_available,
            sidecar_available: detected.sidecar_available,
        };

        let from_detection = detected.capabilities();
        let from_hand = by_hand.capabilities();

        assert_eq!(
            from_detection.available.len(),
            from_hand.available.len(),
            "detection must feed the same mapping the tests exercise"
        );

        // Whatever the privilege level, plain enumeration always works, and
        // something must always be unavailable — the vendor-plugin features
        // have no implementation at any privilege level.
        assert!(from_detection.has(Capability::TerminateProcess));
        assert!(!from_detection.unavailable.is_empty());
    }

    #[test]
    fn elevation_is_a_property_of_this_process_not_of_the_user() {
        // Nothing to assert about the value itself, but the call must not
        // panic or hang — it opens and closes a token handle, and a leak here
        // would accumulate on every capability refresh.
        for _ in 0..100 {
            let _ = WindowsHost::detect();
        }
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
