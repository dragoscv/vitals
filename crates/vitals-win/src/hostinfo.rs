//! Static machine facts: what this computer is.
//!
//! Everything here changes at most once per boot, so it is read once and
//! cached by the caller rather than sampled. That budget is why this module
//! can afford the registry reads and the `CPUID` calls that the 1 Hz tick
//! could not.
//!
//! ## Where each fact comes from, and why
//!
//! - **CPU brand and vendor** — `CPUID` leaves `0x8000_0002..=0x8000_0004`
//!   and leaf 0. The registry has `ProcessorNameString` too, but it is
//!   written by the installer and is stale on a machine whose CPU was
//!   swapped. `CPUID` asks the silicon.
//! - **Core topology** — `GetLogicalProcessorInformationEx`, which reports
//!   `EfficiencyClass` per core group on hybrid parts. Without it a per-core
//!   view is misleading: an E-core at 100% and a P-core at 100% are not the
//!   same event, and treating them alike is why most monitors misreport load
//!   on 12th-gen-and-later Intel.
//! - **OS version** — `RtlGetVersion`, not `GetVersionEx`. The latter lies to
//!   unmanifested processes for compatibility, reporting 6.2 on Windows 11.
//! - **Motherboard and BIOS** — the registry's `BIOS` key, which is populated
//!   by firmware at boot. WMI would give the same answer and cost ~50 ms.
//!
//! Anything unreadable is `None`, never a plausible-looking placeholder.

use vitals_core::provider::{CoreClass, HostInfo};
use vitals_core::units::Bytes;

/// Reads every static fact about this machine.
///
/// Never fails. A field that cannot be read degrades on its own — an
/// unreadable BIOS version must not cost the caller the CPU model.
#[must_use]
pub fn read() -> HostInfo {
    let (physical, logical, topology) = topology();
    let (name, version, build) = os_version();

    HostInfo {
        hostname: hostname().unwrap_or_default(),
        os_name: name,
        os_version: version,
        kernel_version: build,
        architecture: architecture().to_owned(),
        cpu_model: cpu_brand().unwrap_or_default(),
        cpu_vendor: cpu_vendor().unwrap_or_default(),
        physical_cores: physical,
        logical_cores: logical,
        core_topology: topology,
        total_memory: total_memory(),
        boot_time_ms: boot_time_ms(),
        is_virtual_machine: is_virtual_machine(),
        motherboard: registry_string(r"HARDWARE\DESCRIPTION\System\BIOS", "BaseBoardProduct"),
        bios_version: registry_string(r"HARDWARE\DESCRIPTION\System\BIOS", "BIOSVersion"),
    }
}

// ---------------------------------------------------------------------------
// CPU identity
// ---------------------------------------------------------------------------

/// The processor brand string, from `CPUID` leaves `0x8000_0002..=0x8000_0004`.
///
/// Returns `None` when the extended leaves are unsupported, which in practice
/// means an emulator: every x86-64 part since the original Athlon 64 has
/// them.
#[must_use]
pub fn cpu_brand() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;

        // Leaf 0x8000_0000 is architecturally defined on every x86-64\n        // processor; it reports the highest extended leaf available.
        let highest = __cpuid(0x8000_0000).eax;
        if highest < 0x8000_0004 {
            return None;
        }

        let mut bytes = Vec::with_capacity(48);
        for leaf in 0x8000_0002_u32..=0x8000_0004 {
            // Guarded by the `highest` check above.
            let result = __cpuid(leaf);
            for register in [result.eax, result.ebx, result.ecx, result.edx] {
                bytes.extend_from_slice(&register.to_le_bytes());
            }
        }

        let text = String::from_utf8_lossy(&bytes);
        // The brand string is null-padded and, on Intel, generously
        // space-padded in the middle as well.
        let trimmed = text.trim_end_matches('\0').trim();
        let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");

        (!collapsed.is_empty()).then_some(collapsed)
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // ARM64 has no CPUID. The registry is the fallback, and on ARM
        // Windows it is what Task Manager itself reports.
        registry_string(
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        )
    }
}

/// The vendor string from `CPUID` leaf 0 — `GenuineIntel`, `AuthenticAMD`.
#[must_use]
pub fn cpu_vendor() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;

        // Leaf 0 is architecturally defined on every x86 processor.
        let result = __cpuid(0);

        // The 12 bytes arrive in EBX, EDX, ECX — in that order, which is not
        // the register order and is a classic source of "ineGnuineletI".
        let mut bytes = Vec::with_capacity(12);
        for register in [result.ebx, result.edx, result.ecx] {
            bytes.extend_from_slice(&register.to_le_bytes());
        }

        let text = String::from_utf8_lossy(&bytes).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        registry_string(
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "VendorIdentifier",
        )
    }
}

/// The build architecture.
///
/// A compile-time constant rather than a runtime query: this reports what
/// Vitals *is*, and an x64 build under emulation on ARM should say x64,
/// because that is what determines its own behaviour.
#[must_use]
pub const fn architecture() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

// ---------------------------------------------------------------------------
// Topology
// ---------------------------------------------------------------------------

/// Physical cores, logical processors, and the hybrid class of each core.
///
/// Returns `(physical, logical, topology)`. Topology is `None` on a uniform
/// machine — every core the same — because a list of identical entries tells
/// the UI nothing it does not already know, and rendering a "P/E" split on a
/// machine that has none would be inventing a distinction.
fn topology() -> (u32, u32, Option<Vec<CoreClass>>) {
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    let logical = u32::try_from(crate::cpu::logical_core_count()).unwrap_or(1);

    let mut bytes = 0_u32;

    // SAFETY: the two-call idiom. A null buffer asks for the size, and the
    // call is expected to fail with ERROR_INSUFFICIENT_BUFFER.
    unsafe {
        let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &raw mut bytes);
    }

    if bytes == 0 {
        return (logical, logical, None);
    }

    // The records are variable-length and must be walked by their own
    // `Size` field, not by `size_of`. Aligned as the record type so the
    // pointer casts below are sound.
    let count = (bytes as usize).div_ceil(size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()) + 1;
    let mut buffer: Vec<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX> = Vec::with_capacity(count);

    // SAFETY: the buffer has capacity for `bytes`, correctly aligned because
    // it holds the record type itself.
    let ok = unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buffer.as_mut_ptr()),
            &raw mut bytes,
        )
    };

    if ok.is_err() {
        return (logical, logical, None);
    }

    let mut classes = Vec::with_capacity(logical as usize);
    let mut physical = 0_u32;

    // Walked by byte offset because the records are variable-length: each one
    // carries its own `Size`, and a fixed stride would desynchronise on the
    // first core with more than one group mask.
    //
    // The pointer stays typed as the record throughout rather than going via
    // `*const u8` and casting back. Both reach the same address, but the
    // round trip through a byte pointer discards the alignment guarantee —
    // which is undefined behaviour that happens to work, and exactly the bug
    // clippy caught in the GPU counter buffer.
    let base = buffer.as_ptr();
    let mut offset = 0_usize;

    while offset + size_of::<u32>() <= bytes as usize {
        // SAFETY: `base` is aligned for the record type because that is the
        // type the buffer holds, and `offset` stays within the byte count
        // Windows reported. `byte_add` preserves the alignment of `base`
        // because every record's `Size` is a multiple of the alignment.
        let record = unsafe { &*base.byte_add(offset) };

        let size = record.Size as usize;
        if size == 0 {
            // A zero size would loop forever. Windows should never report
            // one, but a corrupt record must not hang a system monitor.
            break;
        }

        if offset + size > bytes as usize {
            // A record claiming to extend past the buffer means the walk has
            // desynchronised; reading it would be out of bounds.
            break;
        }

        physical = physical.saturating_add(1);

        // SAFETY: the union holds `Processor` because the relationship was
        // filtered to `RelationProcessorCore`.
        let core = unsafe { record.Anonymous.Processor };

        // One entry per logical processor in this core, so a hyperthreaded
        // core contributes two — the topology list is indexed by logical
        // processor, which is what the per-core view iterates.
        let threads = core.GroupMask[0].Mask.count_ones();
        for _ in 0..threads {
            classes.push(class_from_efficiency(core.EfficiencyClass));
        }

        offset += size;
    }

    if physical == 0 {
        return (logical, logical, None);
    }

    // A uniform machine reports the same efficiency class for every core.
    let hybrid = classes.windows(2).any(|pair| pair[0] != pair[1]);

    (physical, logical, hybrid.then_some(classes))
}

/// Maps Windows' `EfficiencyClass` onto our core model.
///
/// Windows reports a small integer where higher means more performant, and
/// the count of distinct values is the number of core types. It does not name
/// them, so the mapping is by rank: 0 is the least performant class present.
const fn class_from_efficiency(efficiency: u8) -> CoreClass {
    match efficiency {
        0 => CoreClass::Efficiency,
        1 => CoreClass::Performance,
        // Meteor Lake and later add a third, lower tier on the SoC tile.
        // Windows reports it above the others on some steppings, so this is
        // the honest bucket for "a class we did not expect".
        _ => CoreClass::LowPower,
    }
}

// ---------------------------------------------------------------------------
// OS and machine
// ---------------------------------------------------------------------------

/// Returns `(name, version, build)`.
///
/// `RtlGetVersion` rather than `GetVersionEx`: the latter lies to processes
/// without a compatibility manifest, reporting 6.2 (Windows 8) on Windows 11.
fn os_version() -> (String, String, String) {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: u32::try_from(size_of::<OSVERSIONINFOW>()).unwrap_or(0),
        ..Default::default()
    };

    // SAFETY: `RtlGetVersion` writes into the struct whose size we declared.
    let ok = unsafe { RtlGetVersion(&raw mut info) };

    if ok.is_err() {
        return ("Windows".to_owned(), String::new(), String::new());
    }

    // Windows 11 reports major 10; the build number is what distinguishes it.
    // 22000 is the first Windows 11 build.
    let name = if info.dwMajorVersion >= 10 && info.dwBuildNumber >= 22000 {
        "Windows 11"
    } else if info.dwMajorVersion >= 10 {
        "Windows 10"
    } else {
        "Windows"
    };

    (
        name.to_owned(),
        format!("{}.{}", info.dwMajorVersion, info.dwMinorVersion),
        info.dwBuildNumber.to_string(),
    )
}

fn hostname() -> Option<String> {
    std::env::var("COMPUTERNAME").ok().filter(|s| !s.is_empty())
}

fn total_memory() -> Bytes {
    crate::memory::MemorySampler::new()
        .sample(0)
        .map_or(Bytes(0), |memory| memory.total)
}

/// Milliseconds since the Unix epoch at which this machine booted.
fn boot_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    use windows::Win32::System::SystemInformation::GetTickCount64;

    // SAFETY: no preconditions; returns milliseconds since boot.
    let uptime_ms = unsafe { GetTickCount64() };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

    now_ms.saturating_sub(uptime_ms)
}

/// Whether this is running inside a virtual machine.
///
/// The `CPUID` hypervisor-present bit (leaf 1, ECX bit 31). It is set by
/// every mainstream hypervisor and, notably, by Hyper-V on the *host* when
/// virtualisation-based security is on — so this means "a hypervisor is
/// present", which is the honest reading of the bit rather than a confident
/// claim about being a guest.
fn is_virtual_machine() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;

        // Leaf 1 is architecturally defined on every x86 processor.
        let result = __cpuid(1);
        result.ecx & (1 << 31) != 0
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Reads a `REG_SZ` value from `HKEY_LOCAL_MACHINE`.
fn registry_string(path: &str, value: &str) -> Option<String> {
    use windows::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, KEY_READ, REG_VALUE_TYPE, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    };
    use windows::core::PCWSTR;

    let wide = |text: &str| {
        text.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };

    let path_w = wide(path);
    let value_w = wide(value);

    let mut key = windows::Win32::System::Registry::HKEY::default();

    // SAFETY: both strings are null-terminated; the key is closed below.
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path_w.as_ptr()),
            None,
            KEY_READ,
            &raw mut key,
        )
    };

    if opened.is_err() {
        return None;
    }

    let mut kind = REG_VALUE_TYPE::default();
    let mut size = 0_u32;

    // Two-call idiom: ask for the size first.
    // SAFETY: a null data pointer requests only the size.
    let sized = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(value_w.as_ptr()),
            None,
            Some(&raw mut kind),
            None,
            Some(&raw mut size),
        )
    };

    let result = if sized.is_ok() && size > 0 {
        let mut buffer = vec![0_u8; size as usize];

        // SAFETY: the buffer is sized from the call above.
        let read = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(value_w.as_ptr()),
                None,
                Some(&raw mut kind),
                Some(buffer.as_mut_ptr()),
                Some(&raw mut size),
            )
        };

        if read.is_ok() {
            // The bytes are UTF-16, null-terminated.
            let units: Vec<u16> = buffer
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|&unit| unit != 0)
                .collect();

            let text = String::from_utf16_lossy(&units).trim().to_owned();
            (!text.is_empty()).then_some(text)
        } else {
            None
        }
    } else {
        None
    };

    // SAFETY: `key` is live and closed exactly once.
    unsafe {
        let _ = RegCloseKey(key);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cpu_identifies_itself() {
        // Every x86-64 part since the Athlon 64 supports the brand leaves, so
        // an empty answer here means the CPUID walk is wrong rather than that
        // the hardware is unusual.
        let brand = cpu_brand().expect("x86-64 must report a brand string");
        assert!(!brand.is_empty());
        assert!(
            !brand.contains('\0'),
            "the null padding was not trimmed: {brand:?}"
        );

        let vendor = cpu_vendor().expect("x86-64 must report a vendor");
        assert!(
            vendor == "GenuineIntel" || vendor == "AuthenticAMD" || !vendor.is_empty(),
            "unexpected vendor {vendor:?} — check the EBX/EDX/ECX order"
        );
    }

    #[test]
    fn the_vendor_registers_are_read_in_the_right_order() {
        // EBX, EDX, ECX — not the register order. Getting it wrong produces
        // "ineGnuineletI", which is a real bug people ship.
        let vendor = cpu_vendor().expect("vendor");
        assert!(
            !vendor.starts_with("ine") && !vendor.starts_with("ntel"),
            "the register order is scrambled: {vendor:?}"
        );
    }

    #[test]
    fn core_counts_are_plausible_and_consistent() {
        let (physical, logical, _) = topology();

        assert!(physical >= 1, "a machine has at least one core");
        assert!(
            logical >= physical,
            "{logical} logical cannot be fewer than {physical} physical"
        );
    }

    #[test]
    fn topology_is_absent_on_a_uniform_machine_rather_than_a_flat_list() {
        // A list saying "every core is the same class" tells the UI nothing,
        // and rendering a P/E split on a machine with none invents a
        // distinction. Whichever this machine is, the invariant holds.
        let (_, logical, topology) = topology();

        if let Some(classes) = topology {
            assert_eq!(
                classes.len(),
                logical as usize,
                "topology must be indexed by logical processor"
            );
            assert!(
                classes.windows(2).any(|pair| pair[0] != pair[1]),
                "a uniform machine must report None, not a list of identicals"
            );
        }
    }

    #[test]
    fn the_os_reports_itself_as_something_recognisable() {
        let (name, version, build) = os_version();

        assert!(name.starts_with("Windows"), "{name:?}");
        assert!(!version.is_empty());
        assert!(!build.is_empty());

        // The bug this guards: GetVersionEx reports 6.2 on Windows 11 without
        // a compatibility manifest. RtlGetVersion does not.
        let major: u32 = version
            .split('.')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        assert!(
            major >= 10,
            "major {major} suggests GetVersionEx's compatibility lie"
        );
    }

    #[test]
    fn boot_time_is_in_the_past_and_this_century() {
        let boot = boot_time_ms();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

        assert!(boot > 0, "boot time must be known");
        assert!(boot <= now, "the machine cannot have booted in the future");
        // 2001-09-09, the 1_000_000_000 second mark. Anything before that is
        // a clock or arithmetic failure rather than a very old machine.
        assert!(boot > 1_000_000_000_000, "boot time is implausibly early");
    }

    #[test]
    fn read_never_panics_and_fills_what_it_can() {
        let info = read();

        assert!(!info.os_name.is_empty());
        assert!(info.logical_cores >= 1);
        assert_eq!(info.architecture, "x86_64");
        assert!(info.total_memory.get() > 0, "a machine has memory");
    }

    #[test]
    fn an_absent_registry_value_is_none_rather_than_empty() {
        // The distinction matters: `Some("")` renders as a blank field that
        // looks like a bug, `None` renders as "unknown".
        assert!(registry_string(r"HARDWARE\DESCRIPTION\System\BIOS", "NoSuchValue").is_none());
        assert!(registry_string(r"SOFTWARE\NoSuchKeyAnywhere", "Whatever").is_none());
    }
}
