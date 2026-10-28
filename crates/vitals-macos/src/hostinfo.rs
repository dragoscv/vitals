//! Static machine facts, read once per launch.

use sysinfo::System;
use vitals_core::provider::{CoreClass, HostInfo};
use vitals_core::units::Bytes;

use crate::mach::{sysctl_string, sysctl_u64};

/// Reads every static fact about this Mac. Never fails: an unreadable field
/// degrades on its own (`None` or empty), never to a plausible placeholder.
#[must_use]
pub fn read() -> HostInfo {
    let logical = sysctl_u64(c"hw.logicalcpu").unwrap_or(0) as u32;
    let physical = sysctl_u64(c"hw.physicalcpu").unwrap_or(0) as u32;
    let brand = sysctl_string(c"machdep.cpu.brand_string").unwrap_or_default();
    let vendor = if cfg!(target_arch = "aarch64") {
        "Apple".to_owned()
    } else {
        sysctl_string(c"machdep.cpu.vendor").unwrap_or_default()
    };

    HostInfo {
        hostname: System::host_name().unwrap_or_default(),
        os_name: "macOS".to_owned(),
        os_version: System::os_version().unwrap_or_default(),
        kernel_version: System::kernel_version().unwrap_or_default(),
        architecture: std::env::consts::ARCH.to_owned(),
        cpu_model: brand,
        cpu_vendor: vendor,
        physical_cores: physical,
        logical_cores: logical,
        core_topology: topology(logical),
        total_memory: Bytes(sysctl_u64(c"hw.memsize").unwrap_or(0)),
        boot_time_ms: System::boot_time() * 1000,
        is_virtual_machine: is_virtual_machine(),
        motherboard: sysctl_string(c"hw.model"),
        bios_version: None,
    }
}

/// Performance and efficiency cores on Apple Silicon, from the
/// `hw.perflevelN` sysctls (perflevel0 = performance). Logical CPUs are
/// numbered performance-first on every shipping M-series part. `None` on
/// Intel Macs, which have one core class.
fn topology(logical: u32) -> Option<Vec<CoreClass>> {
    let levels = sysctl_u64(c"hw.nperflevels")?;
    if levels < 2 {
        return None;
    }
    let p = sysctl_u64(c"hw.perflevel0.logicalcpu")? as u32;
    let e = sysctl_u64(c"hw.perflevel1.logicalcpu")? as u32;
    if p + e != logical {
        return None;
    }
    let mut out = vec![CoreClass::Performance; p as usize];
    out.extend(std::iter::repeat_n(CoreClass::Efficiency, e as usize));
    Some(out)
}

/// `kern.hv_vmm_present` is 1 inside Apple's Virtualization framework and
/// under most hypervisors; Intel QEMU guests also expose the `VMM` CPU feature.
fn is_virtual_machine() -> bool {
    sysctl_u64(c"kern.hv_vmm_present").is_some_and(|v| v == 1)
        || sysctl_string(c"machdep.cpu.features").is_some_and(|f| f.contains("VMM"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_info_names_the_os_cpu_and_memory() {
        let h = read();
        assert_eq!(h.os_name, "macOS");
        assert!(!h.os_version.is_empty());
        assert!(h.logical_cores >= 1);
        assert!(h.physical_cores >= 1 && h.physical_cores <= h.logical_cores);
        assert!(h.total_memory.get() > 1 << 30);
        assert!(!h.cpu_model.is_empty());
        assert!(h.boot_time_ms > 1_600_000_000_000);
        if let Some(t) = &h.core_topology {
            assert_eq!(t.len() as u32, h.logical_cores);
        }
    }
}
