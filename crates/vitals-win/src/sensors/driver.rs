//! What a kernel driver unlocks, and which of it Vitals has.
//!
//! This file deliberately contains no measurement code. It exists so that
//! every sensor the UI might reasonably display has an entry that either
//! resolves to a reading or resolves to a *reason*, and so that the reason
//! is written down once rather than rediscovered by each person who asks
//! "why is there no CPU temperature".
//!
//! # The physical situation
//!
//! Core temperature lives in a model-specific register: `IA32_THERM_STATUS`
//! (`0x19C`) on Intel, `SMN`/`SMU` mailbox reads on AMD. Reading an MSR
//! requires the `RDMSR` instruction, which is ring 0. Fan tachometers and
//! VRM voltages live on a Super-I/O chip (Nuvoton, ITE, Fintek) behind ISA
//! port I/O at `0x2E`/`0x4E`, or on an `SMBus` behind the chipset — also ring
//! 0. There is no user-mode path to any of them. This is not a Windows
//! limitation to be worked around; it is the CPU's privilege model.
//!
//! Consequently every tool that shows a core temperature — `HWiNFO`, Core
//! Temp, `LibreHardwareMonitor`, MSI Afterburner — ships a signed kernel
//! driver whose entire job is to expose `RDMSR` and port I/O to user mode.
//! That is also why several of them have been the subject of CVEs: a driver
//! that offers unrestricted MSR and port access to any caller is a local
//! privilege-escalation primitive, and `WinRing0` in particular is
//! blocklisted by Microsoft's vulnerable-driver list for exactly this
//! reason.
//!
//! # Why Vitals does not ship its own
//!
//! A kernel driver must be signed with an EV certificate and submitted to
//! the Microsoft Hardware Developer Centre for attestation signing.
//! [`docs/distribution.md`] records the decision to remain free of paid
//! signing infrastructure, so Vitals signs no driver of its own.
//!
//! # The one it uses: `PawnIO`, through an optional service
//!
//! CPU temperature and package power come from the third-party signed
//! `PawnIO` driver, whose sandboxed modules expose only declared registers
//! (unlike `WinRing0`). A SYSTEM service the user installs on request reads
//! it and publishes the numbers on a pipe ([`super::cpu_service`], ADR-0031).
//! Those readings carry
//! [`SensorSource::KernelDriver`](super::reading::SensorSource::KernelDriver),
//! and the matching gaps close only while they are measured
//! (`inventory::closed_by` in the desktop app). Super-I/O sensors — board,
//! VRM, fans, rails — are outside `PawnIO`'s module set and stay listed.

use vitals_core::capability::{Capability, Unavailable};

/// A sensor class this build cannot read, and what it would take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverGap {
    /// The capability the UI should grey out.
    pub capability: Capability,
    /// What the user can display next to it.
    pub reason: Unavailable,
    /// Short label, e.g. `CPU core temperature`.
    pub label: &'static str,
    /// The concrete mechanism required, named precisely enough to act on.
    pub requirement: &'static str,
}

/// Everything this build cannot measure, with the reason for each.
///
/// Rendered as-is in the "Devices & Sensors" section. Listing the gaps
/// explicitly is the alternative to the usual behaviour of showing nothing
/// and letting the user conclude the app is broken.
pub const DRIVER_GAPS: &[DriverGap] = &[
    DriverGap {
        capability: Capability::Thermals,
        reason: Unavailable::NeedsPlugin,
        label: "CPU core temperature",
        requirement: "RDMSR of IA32_THERM_STATUS (0x19C) on Intel, or SMN THM reads on AMD. \
                  Ring 0 only: install the optional Vitals sensors service (signed PawnIO \
                  driver, one administrator prompt) and this gap closes.",
    },
    DriverGap {
        capability: Capability::Thermals,
        reason: Unavailable::NeedsPlugin,
        label: "GPU temperature",
        requirement: "Read on NVIDIA through the driver's own nvml.dll (sensors::nvml), so \
                  this gap is only listed when no NVIDIA GPU reported it. AMD needs ADLX \
                  and Intel IGCL, neither of which ships in System32.",
    },
    DriverGap {
        capability: Capability::Thermals,
        reason: Unavailable::NeedsPlugin,
        label: "Motherboard and VRM temperatures",
        requirement: "Super-I/O register reads over ISA port I/O at 0x2E/0x4E (Nuvoton, ITE, \
                      Fintek). Ring 0 only.",
    },
    DriverGap {
        capability: Capability::Thermals,
        reason: Unavailable::NeedsPlugin,
        label: "Drive temperature",
        requirement: "SMART attribute 194 via IOCTL_STORAGE_QUERY_PROPERTY, or the NVMe SMART \
                      log page. Unprivileged for the protocol command but the pass-through \
                      IOCTL needs administrator rights.",
    },
    DriverGap {
        capability: Capability::FanControl,
        reason: Unavailable::NeedsPlugin,
        label: "Fan speed (RPM)",
        requirement: "Super-I/O tachometer registers, same ring-0 port I/O as board \
                      temperatures. ACPI exposes fans as on/off devices only and reports no \
                      tachometer, so there is no unprivileged partial answer.",
    },
    DriverGap {
        capability: Capability::PowerDraw,
        reason: Unavailable::NeedsPlugin,
        label: "CPU package power",
        requirement: "Intel RAPL (MSR_PKG_ENERGY_STATUS, 0x611) or the AMD equivalent, read \
                  by the optional Vitals sensors service. The Windows Energy Estimation \
                  Engine is not a substitute: it models power from utilisation.",
    },
    DriverGap {
        capability: Capability::PowerDraw,
        reason: Unavailable::NeedsPlugin,
        label: "GPU board power",
        requirement: "Read on NVIDIA through nvml.dll (nvmlDeviceGetPowerUsage), so listed \
                  only when no NVIDIA GPU reported it. AMD needs ADLX and Intel IGCL, \
                  neither of which ships with Windows.",
    },
    DriverGap {
        capability: Capability::Thermals,
        reason: Unavailable::NeedsPlugin,
        label: "Rail voltages (Vcore, +12V, +5V)",
        requirement: "Super-I/O ADC channels over ISA port I/O. Ring 0, and the scaling \
                      factors are board-specific — a wrong divider yields a plausible-looking \
                      voltage, which is why guessing them is not an option.",
    },
];

/// The gaps affecting one capability.
#[must_use]
pub fn gaps_for(capability: Capability) -> Vec<&'static DriverGap> {
    DRIVER_GAPS
        .iter()
        .filter(|gap| gap.capability == capability)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gap_names_a_concrete_mechanism() {
        for gap in DRIVER_GAPS {
            assert!(!gap.label.is_empty());
            // A requirement that does not name an API, register or SDK is
            // not actionable, and the whole point of this list is that a
            // future implementer knows where to start.
            assert!(
                gap.requirement.len() > 40,
                "{} has a vague requirement",
                gap.label
            );
            assert!(
                gap.reason.is_actionable(),
                "{} claims to be permanently impossible, which is false: a \
                 driver or vendor SDK would supply it",
                gap.label
            );
        }
    }

    #[test]
    fn the_headline_gaps_are_all_listed() {
        let labels: Vec<_> = DRIVER_GAPS.iter().map(|g| g.label).collect();
        assert!(labels.iter().any(|l| l.contains("CPU core")));
        assert!(labels.iter().any(|l| l.contains("Fan speed")));
        assert!(labels.iter().any(|l| l.contains("GPU temperature")));
        assert!(labels.iter().any(|l| l.contains("voltages")));
    }

    #[test]
    fn gaps_group_by_capability() {
        assert!(!gaps_for(Capability::PowerDraw).is_empty());
        assert_eq!(gaps_for(Capability::FanControl).len(), 1);
        // A capability with no gaps must return empty, not everything.
        assert!(gaps_for(Capability::TerminateProcess).is_empty());
    }
}
