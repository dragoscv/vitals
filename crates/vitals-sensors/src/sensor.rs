//! CPU temperature and package power through `PawnIO`: vendor detection,
//! module selection, register reads.

use std::time::Instant;

use vitals_sensors::{
    AMD_MODULE_SHA256, AMD_SMN_THM_TCON_CUR_TMP, INTEL_MODULE_SHA256, MSR_AMD_PKG_ENERGY_STATUS,
    MSR_AMD_RAPL_POWER_UNIT, MSR_IA32_PACKAGE_THERM_STATUS, MSR_IA32_TEMPERATURE_TARGET,
    MSR_IA32_THERM_STATUS, MSR_PKG_ENERGY_STATUS, MSR_RAPL_POWER_UNIT, Reading,
    amd_family_supported, amd_tctl, cpu_family, intel_temp, intel_tjmax, rapl_energy_unit,
    rapl_watts, sha256_hex,
};
use windows::Win32::System::SystemInformation::GROUP_AFFINITY;
use windows::Win32::System::Threading::{
    GetActiveProcessorCount, GetActiveProcessorGroupCount, GetCurrentThread, SetThreadGroupAffinity,
};

use crate::pawnio::PawnIo;

/// Embedded rather than shipped beside the exe: one file to copy, and no
/// window in which a module on disk can be swapped between the hash check and
/// the load. The pin is still checked, so a rebuilt helper with a refreshed
/// module and a stale pin fails loudly instead of loading unknown code.
const INTEL_MODULE: &[u8] = include_bytes!("../modules/IntelMSR.bin");
const AMD_MODULE: &[u8] = include_bytes!("../modules/AMDFamily17.bin");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Intel,
    Amd,
}

impl Vendor {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Intel => "intel",
            Self::Amd => "amd",
        }
    }

    const fn module(self) -> (&'static [u8], &'static str) {
        match self {
            Self::Intel => (INTEL_MODULE, INTEL_MODULE_SHA256),
            Self::Amd => (AMD_MODULE, AMD_MODULE_SHA256),
        }
    }

    const fn rapl_msrs(self) -> (u32, u32) {
        match self {
            Self::Intel => (MSR_RAPL_POWER_UNIT, MSR_PKG_ENERGY_STATUS),
            Self::Amd => (MSR_AMD_RAPL_POWER_UNIT, MSR_AMD_PKG_ENERGY_STATUS),
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub fn detect_vendor() -> Result<Vendor, String> {
    use std::arch::x86_64::__cpuid;
    // CPUID is always available on x86_64 and has no preconditions.
    let (l0, l1) = (__cpuid(0), __cpuid(1));
    let mut v = Vec::with_capacity(12);
    for r in [l0.ebx, l0.edx, l0.ecx] {
        v.extend_from_slice(&r.to_le_bytes());
    }
    match &v[..] {
        b"GenuineIntel" => Ok(Vendor::Intel),
        b"AuthenticAMD" | b"HygonGenuine" => {
            let fam = cpu_family(l1.eax);
            if amd_family_supported(fam) {
                Ok(Vendor::Amd)
            } else {
                Err(format!(
                    "unsupported AMD CPU family {fam:#x} (need 17h-1Ah)"
                ))
            }
        }
        other => Err(format!(
            "unsupported CPU vendor {:?}",
            String::from_utf8_lossy(other)
        )),
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_vendor() -> Result<Vendor, String> {
    Err("unsupported CPU architecture (x86_64 only)".to_owned())
}

/// The previous package-energy read, for turning a counter into a rate.
#[derive(Debug, Clone, Copy)]
struct EnergyMark {
    at: Instant,
    counter: u64,
}

#[derive(Debug)]
pub struct Sensor {
    dev: PawnIo,
    vendor: Vendor,
    /// `None` when the module refused the power-unit register: package power
    /// is then absent for the life of the service, not retried every second.
    energy_unit: Option<f64>,
    last_energy: Option<EnergyMark>,
}

impl Sensor {
    pub fn open() -> Result<Self, String> {
        let vendor = detect_vendor()?;
        let (blob, pin) = vendor.module();
        let got = sha256_hex(blob);
        if got != pin {
            return Err(format!(
                "{} module sha256 mismatch ({got})",
                vendor.as_str()
            ));
        }
        let dev = PawnIo::open()?;
        dev.load(blob)?;
        let mut sensor = Self {
            dev,
            vendor,
            energy_unit: None,
            last_energy: None,
        };
        sensor.energy_unit = sensor
            .msr(vendor.rapl_msrs().0)
            .ok()
            .map(rapl_energy_unit)
            .filter(|u| *u > 0.0 && u.is_finite());
        Ok(sensor)
    }

    fn msr(&self, msr: u32) -> Result<u64, String> {
        self.dev
            .execute("ioctl_read_msr", &[u64::from(msr)], 1)?
            .first()
            .copied()
            .ok_or_else(|| format!("MSR {msr:#x}: empty result"))
    }

    pub fn read(&mut self) -> Result<Reading, String> {
        let mut reading = match self.vendor {
            Vendor::Intel => self.read_intel()?,
            Vendor::Amd => self.read_amd()?,
        };
        reading.package_w = self.package_power();
        Ok(reading)
    }

    /// Average package power since the previous read.
    ///
    /// The first read only records a baseline and reports `None`: a rate
    /// needs two points in time, and the alternative — dividing the whole
    /// counter since boot by nothing — is a fabricated number.
    fn package_power(&mut self) -> Option<f32> {
        let unit = self.energy_unit?;
        let counter = self.msr(self.vendor.rapl_msrs().1).ok()?;
        let now = Instant::now();
        let previous = self.last_energy.replace(EnergyMark { at: now, counter })?;
        rapl_watts(
            previous.counter,
            counter,
            unit,
            now.duration_since(previous.at).as_secs_f64(),
        )
    }

    fn read_amd(&self) -> Result<Reading, String> {
        let v = self
            .dev
            .execute("ioctl_read_smn", &[u64::from(AMD_SMN_THM_TCON_CUR_TMP)], 1)?
            .first()
            .copied()
            .ok_or("SMN THM_TCON_CUR_TMP: empty result")?;
        Ok(Reading::success(
            self.vendor.as_str(),
            Some(amd_tctl(v as u32)),
            None,
        ))
    }

    fn read_intel(&self) -> Result<Reading, String> {
        let tj = self.msr(MSR_IA32_TEMPERATURE_TARGET)?;
        let package = intel_temp(tj, self.msr(MSR_IA32_PACKAGE_THERM_STATUS)?);
        let hottest = self.hottest_core(tj);
        if package.is_none() && hottest.is_none() {
            return Err("no valid Intel thermal readout".to_owned());
        }
        let mut r = Reading::success(self.vendor.as_str(), package, hottest);
        r.tj_max_c = Some(intel_tjmax(tj));
        Ok(r)
    }

    /// `IA32_THERM_STATUS` on every logical CPU.
    ///
    /// The register is per core, so the reading thread is pinned to each
    /// processor in turn — across every processor group, or a machine with
    /// more than 64 logical CPUs would only ever report the first group.
    fn hottest_core(&self, tj: u64) -> Option<f32> {
        let mut hottest: Option<f32> = None;
        let mut original: Option<GROUP_AFFINITY> = None;
        // SAFETY: the affinity calls only affect the current thread, and the
        // original affinity is restored before returning.
        unsafe {
            let thread = GetCurrentThread();
            for group in 0..GetActiveProcessorGroupCount() {
                let count = GetActiveProcessorCount(group).min(usize::BITS);
                for cpu in 0..count {
                    let ga = GROUP_AFFINITY {
                        Mask: 1usize << cpu,
                        Group: group,
                        Reserved: [0; 3],
                    };
                    let mut prev = GROUP_AFFINITY::default();
                    if !SetThreadGroupAffinity(thread, &raw const ga, Some(&raw mut prev)).as_bool()
                    {
                        continue;
                    }
                    if original.is_none() {
                        original = Some(prev);
                    }
                    // Let the scheduler actually move us before reading.
                    std::thread::yield_now();
                    if let Some(t) = self
                        .msr(MSR_IA32_THERM_STATUS)
                        .ok()
                        .and_then(|v| intel_temp(tj, v))
                    {
                        hottest = Some(hottest.map_or(t, |h| h.max(t)));
                    }
                }
            }
            if let Some(orig) = original {
                let _ = SetThreadGroupAffinity(thread, &raw const orig, None);
            }
        }
        hottest
    }
}
