//! The unprivileged half of `vitals-sensors`: the pipe contract, the
//! register decoding, and a std-only pipe client for the desktop app.
//!
//! # Why a service at all
//!
//! Core temperature and package energy live in model-specific registers, and
//! `RDMSR` is ring 0. The signed `PawnIO` driver exposes a sandboxed module
//! interface to them, but opening its device needs administrator rights —
//! and running the whole app elevated would put a webview, the largest attack
//! surface in the product, at full privilege. So a small `LocalSystem`
//! service owns the device and publishes read-only numbers on a named pipe.
//! The app reads the pipe; it never touches the driver. Decision and trade-offs:
//! `docs/adr/0034-cpu-sensors-service.md`.
//!
//! Ported from codai's `codai-sensors` (same pipe shape, same module pins),
//! with package power added.

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// Named pipe served by the service.
pub const PIPE_NAME: &str = r"\\.\pipe\vitals-sensors";
/// Windows service name.
pub const SERVICE_NAME: &str = "vitals-sensors";
/// Name of the helper executable, bundled beside the app.
pub const HELPER_EXE: &str = "vitals-sensors.exe";
/// JSON contract version. Bump on any breaking change to [`Reading`].
pub const CONTRACT_VERSION: u32 = 1;

/// `PawnIO.Modules` 0.2.11 (signed release, LGPL-2.1) blobs. Embedded in the
/// helper and checked against these pins before loading, so a module swapped
/// on disk cannot be loaded into the driver.
pub const INTEL_MODULE_SHA256: &str =
    "D6ED85D65AB17A22F813EF98207D6D537155EE2DED5976A21CB48413C9B92E5F";
pub const AMD_MODULE_SHA256: &str =
    "DAE74615761B78BDF064DFB3E136252DDCC6FC727D88F14738D0E5800D427A91";

/// `PawnIO_setup.exe` 2.2.0, the only installer the helper will run.
pub const PAWNIO_SETUP_URL: &str =
    "https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe";
pub const PAWNIO_SETUP_SHA256: &str =
    "1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032";
/// Registry key `PawnIO`'s installer writes; `DisplayVersion` names the version.
pub const PAWNIO_UNINSTALL_KEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO";

pub const MSR_IA32_THERM_STATUS: u32 = 0x19C;
pub const MSR_IA32_TEMPERATURE_TARGET: u32 = 0x1A2;
pub const MSR_IA32_PACKAGE_THERM_STATUS: u32 = 0x1B1;
/// Intel RAPL: energy unit in bits 12:8.
pub const MSR_RAPL_POWER_UNIT: u32 = 0x606;
/// Intel RAPL: cumulative package energy, 32 bits, wraps.
pub const MSR_PKG_ENERGY_STATUS: u32 = 0x611;
/// AMD Zen RAPL equivalents (same bit layout as Intel's).
pub const MSR_AMD_RAPL_POWER_UNIT: u32 = 0xC001_0299;
pub const MSR_AMD_PKG_ENERGY_STATUS: u32 = 0xC001_029B;
/// AMD Zen `THM_TCON_CUR_TMP` (SMN address).
pub const AMD_SMN_THM_TCON_CUR_TMP: u32 = 0x0005_9800;

/// `PawnIO.Modules` 0.2.11 `LpcIO`: Super-I/O port access, restricted by the
/// module to the chip's config ports and the BARs it discovers.
pub const LPCIO_MODULE_SHA256: &str =
    "B3896A1CAB0D808FCA31FE2EBCAE045D59DAC690DA87B17C858BB8DA357EB45E";

/// One fan header's tachometer. `rpm` is `0.0` only when the chip says the
/// fan is stopped; a header the chip does not count is omitted entirely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fan {
    pub name: String,
    pub rpm: f32,
}

/// One snapshot, written as one JSON line on the pipe.
///
/// ok:    `{"v":1,"ok":true,"vendor":"intel","packageC":57.0,"hottestCoreC":61.0,"tjMaxC":100,"packageW":41.5,"superIo":"IT8689E","fans":[{"name":"Fan 1","rpm":812.0}],"source":"pawnio"}`
/// error: `{"v":1,"ok":false,"error":"..."}`
///
/// Every reading is optional on its own: a CPU whose module refuses the
/// energy register still reports its temperature, and the absent figure is
/// `null`, never `0`.
///
/// `superIo` and `fans` were added within v1: both are optional, so an older
/// reader ignores them and a newer reader of an older service sees none.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub v: u32,
    pub ok: bool,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default, rename = "packageC")]
    pub package_c: Option<f32>,
    #[serde(default, rename = "hottestCoreC")]
    pub hottest_core_c: Option<f32>,
    #[serde(default, rename = "tjMaxC")]
    pub tj_max_c: Option<u32>,
    #[serde(default, rename = "packageW")]
    pub package_w: Option<f32>,
    /// The board's monitoring chip, when one was recognised.
    #[serde(default, rename = "superIo")]
    pub super_io: Option<String>,
    #[serde(default)]
    pub fans: Vec<Fan>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl Reading {
    #[must_use]
    pub fn success(vendor: &str, package_c: Option<f32>, hottest_core_c: Option<f32>) -> Self {
        Self {
            v: CONTRACT_VERSION,
            ok: true,
            vendor: Some(vendor.to_owned()),
            package_c,
            hottest_core_c,
            tj_max_c: None,
            package_w: None,
            super_io: None,
            fans: Vec::new(),
            source: Some("pawnio".to_owned()),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            v: CONTRACT_VERSION,
            ok: false,
            vendor: None,
            package_c: None,
            hottest_core_c: None,
            tj_max_c: None,
            package_w: None,
            super_io: None,
            fans: Vec::new(),
            source: None,
            error: Some(error.into()),
        }
    }

    /// Serialised form without the trailing newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| r#"{"v":1,"ok":false,"error":"serialisation failed"}"#.to_owned())
    }

    /// Parses one pipe line.
    ///
    /// # Errors
    ///
    /// When the line is not a [`Reading`], or carries a contract version this
    /// build does not understand — a newer service must not be half-read.
    pub fn parse(line: &str) -> Result<Self, String> {
        let r: Self =
            serde_json::from_str(line.trim()).map_err(|e| format!("bad sensors JSON: {e}"))?;
        if r.v != CONTRACT_VERSION {
            return Err(format!(
                "sensors contract v{} (this build reads v{CONTRACT_VERSION})",
                r.v
            ));
        }
        Ok(r)
    }
}

impl Serialize for Reading {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("v", &self.v)?;
        m.serialize_entry("ok", &self.ok)?;
        if self.ok {
            m.serialize_entry("vendor", &self.vendor)?;
            m.serialize_entry("packageC", &self.package_c)?;
            m.serialize_entry("hottestCoreC", &self.hottest_core_c)?;
            m.serialize_entry("tjMaxC", &self.tj_max_c)?;
            m.serialize_entry("packageW", &self.package_w)?;
            if let Some(chip) = &self.super_io {
                m.serialize_entry("superIo", chip)?;
            }
            if !self.fans.is_empty() {
                m.serialize_entry("fans", &self.fans)?;
            }
            m.serialize_entry("source", &self.source)?;
        } else {
            m.serialize_entry("error", self.error.as_deref().unwrap_or("unknown error"))?;
        }
        m.end()
    }
}

/// `TjMax` from `IA32_TEMPERATURE_TARGET` (bits 23:16); 0 means "not reported"
/// and the architectural default of 100 °C applies.
#[must_use]
pub const fn intel_tjmax(tjmax_msr: u64) -> u32 {
    match ((tjmax_msr >> 16) & 0xFF) as u32 {
        0 => 100,
        t => t,
    }
}

/// Temperature from `IA32_(PACKAGE_)THERM_STATUS`: `TjMax` minus the digital
/// readout (bits 22:16).
///
/// The valid bit (31) is deliberately not required: some parts leave it
/// clear while reporting a correct readout. `None` when the readout exceeds
/// `TjMax`, which can only be garbage.
#[must_use]
pub fn intel_temp(tjmax_msr: u64, therm_msr: u64) -> Option<f32> {
    let tjmax = intel_tjmax(tjmax_msr);
    let distance = ((therm_msr >> 16) & 0x7F) as u32;
    (distance <= tjmax).then(|| (tjmax - distance) as f32)
}

/// AMD family 17h–1Ah `Tctl` from `THM_TCON_CUR_TMP`.
///
/// Bit 19 selects the −49 °C range some parts use; ignoring it reads 49 °C
/// hot on exactly those parts.
#[must_use]
pub fn amd_tctl(v: u32) -> f32 {
    let mut t = ((v >> 21) & 0x7FF) as f32 / 8.0;
    if v & 0x8_0000 != 0 {
        t -= 49.0;
    }
    t
}

/// CPU family from CPUID leaf 1 EAX (base + extended when base is 0xF).
#[must_use]
pub const fn cpu_family(leaf1_eax: u32) -> u32 {
    let base = (leaf1_eax >> 8) & 0xF;
    if base == 0xF {
        base + ((leaf1_eax >> 20) & 0xFF)
    } else {
        base
    }
}

/// True for the AMD families the `AMDFamily17` module serves (Zen 1–5).
#[must_use]
pub const fn amd_family_supported(family: u32) -> bool {
    family >= 0x17 && family <= 0x1A
}

/// Joules per RAPL energy count, from the power-unit MSR (bits 12:8 hold
/// `n` in `1 / 2^n` J). Intel and AMD Zen share the layout.
#[must_use]
pub fn rapl_energy_unit(power_unit_msr: u64) -> f64 {
    let n = ((power_unit_msr >> 8) & 0x1F) as i32;
    1.0 / f64::from(2_i32.pow(n.unsigned_abs()))
}

/// Average package power between two energy-counter reads.
///
/// The counter is 32 bits and wraps, so the difference is taken modulo 2^32:
/// at 250 W and the usual 61 µJ unit it wraps every ~17 minutes, well inside
/// any interval this is called with, but a plain subtraction across the wrap
/// would report a negative — or, cast, an enormous — wattage.
///
/// `None` for a non-positive interval, where no rate exists.
#[must_use]
pub fn rapl_watts(previous: u64, current: u64, unit_joules: f64, seconds: f64) -> Option<f32> {
    if seconds <= 0.0 || !seconds.is_finite() {
        return None;
    }
    let delta = (current as u32).wrapping_sub(previous as u32);
    Some((f64::from(delta) * unit_joules / seconds) as f32)
}

/// A Super-I/O chip family whose fan tachometers this build decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuperIoFamily {
    /// ITE IT86xx/IT87xx environment controller.
    Ite {
        fans: usize,
        /// IT8665E/IT8625E keep the sixth fan's counter at 0x93/0x94.
        alt_sixth: bool,
    },
    /// Nuvoton `NCT67xx` hardware monitor with banked registers.
    Nuvoton { fans: usize },
}

/// Identifies an ITE chip from its 16-bit ID (config registers 0x20/0x21).
///
/// Only chips with 16-bit fan counters are accepted: the older 8-bit
/// divisor path needs per-board divisor handling this build does not do,
/// and a guessed divisor produces a confident, wrong RPM.
#[must_use]
pub fn ite_chip(id: u16) -> Option<(&'static str, SuperIoFamily)> {
    let (name, fans, alt_sixth) = match id {
        0x8613 => ("IT8613E", 5, false),
        0x8620 => ("IT8620E", 5, false),
        0x8625 => ("IT8625E", 6, true),
        0x8628 => ("IT8628E", 6, false),
        0x8631 => ("IT8631E", 2, false),
        0x8638 => ("IT8638E", 2, false),
        0x8655 => ("IT8655E", 3, false),
        0x8665 => ("IT8665E", 6, true),
        0x8686 => ("IT8686E", 6, false),
        0x8688 => ("IT8688E", 6, false),
        0x8689 => ("IT8689E", 6, false),
        0x8696 => ("IT8696E", 6, false),
        0x8721 => ("IT8721F", 5, false),
        0x8728 => ("IT8728F", 5, false),
        0x8771 => ("IT8771E", 5, false),
        0x8772 => ("IT8772E", 5, false),
        0x8733 => ("IT8792E", 3, false),
        0x8695 => ("IT87952E", 3, false),
        _ => return None,
    };
    Some((name, SuperIoFamily::Ite { fans, alt_sixth }))
}

/// Identifies a Nuvoton chip from its ID and revision (0x20/0x21).
///
/// Only the NCT679x/NCT6799 line with the 13-bit fan count registers at
/// bank 4 (0x4B0..) is accepted; the `NCT668x` EC-space parts (MSI) use a
/// different access protocol entirely.
#[must_use]
pub fn nuvoton_chip(id: u8, revision: u8) -> Option<(&'static str, SuperIoFamily)> {
    let (name, fans) = match (id, revision) {
        (0xC8, 0x03) => ("NCT6791D", 6),
        (0xC9, 0x11) => ("NCT6792D", 6),
        (0xC9, 0x13) => ("NCT6792D-A", 6),
        (0xD1, 0x21) => ("NCT6793D", 6),
        (0xD3, 0x52) => ("NCT6795D", 6),
        (0xD4, 0x23) => ("NCT6796D", 6),
        (0xD4, 0x2A) => ("NCT6796D-R", 7),
        (0xD4, 0x51) => ("NCT6797D", 7),
        (0xD4, 0x2B) => ("NCT6798D", 7),
        (0xD8, 0x02) => ("NCT6799D", 7),
        _ => return None,
    };
    Some((name, SuperIoFamily::Nuvoton { fans }))
}

/// ITE: fan tachometer low bytes, one per header.
pub const ITE_FAN_LOW: [u8; 6] = [0x0D, 0x0E, 0x0F, 0x80, 0x82, 0x4C];
/// ITE: fan tachometer high bytes (16-bit mode).
pub const ITE_FAN_HIGH: [u8; 6] = [0x18, 0x19, 0x1A, 0x81, 0x83, 0x4D];
/// ITE IT8665E/IT8625E: the sixth fan lives elsewhere.
pub const ITE_FAN_LOW_ALT6: u8 = 0x93;
pub const ITE_FAN_HIGH_ALT6: u8 = 0x94;
/// ITE: bits 4/5 (fans 4/5) and 2 (fan 6) enable the 16-bit counters.
pub const ITE_FAN_16BIT_ENABLE: u8 = 0x0C;
/// Nuvoton: 13-bit fan count registers (bank in the high byte).
pub const NUVOTON_FAN_COUNT: [u16; 7] = [0x4B0, 0x4B2, 0x4B4, 0x4B6, 0x4B8, 0x4BA, 0x4CC];

/// RPM from an ITE 16-bit tachometer count.
///
/// `None` below 0x40 (no signal: the header is unconnected or the count is
/// noise); `Some(0.0)` at 0xFFFF, which the chip uses for a stopped fan.
#[must_use]
pub fn ite_rpm(count: u16) -> Option<f32> {
    match count {
        0..=0x3F => None,
        0xFFFF => Some(0.0),
        n => Some(1.35e6 / (f32::from(n) * 2.0)),
    }
}

/// Whether ITE header `index` has its 16-bit counter enabled.
///
/// Headers 1–3 always count in 16-bit mode on these chips; 4–6 each have an
/// enable bit, and a disabled header reads a stale count that must not be
/// reported as a fan.
#[must_use]
pub const fn ite_fan_enabled(index: usize, enable_register: u8) -> bool {
    match index {
        0..=2 => true,
        3 => enable_register & (1 << 4) != 0,
        4 => enable_register & (1 << 5) != 0,
        5 => enable_register & (1 << 2) != 0,
        _ => false,
    }
}

/// RPM from a Nuvoton 13-bit count (high byte, then low byte's 5 bits).
///
/// `Some(0.0)` at the counter's maximum (stopped); `None` below 0x15, where
/// the count cannot come from a spinning fan.
#[must_use]
pub fn nuvoton_rpm(high: u8, low: u8) -> Option<f32> {
    let count = (u32::from(high) << 5) | u32::from(low & 0x1F);
    match count {
        0x1FFF.. => Some(0.0),
        0..0x15 => None,
        n => Some(1.35e6 / n as f32),
    }
}

/// Uppercase hex SHA-256.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02X}");
            s
        })
}

/// Whether a `PawnIO` `DisplayVersion` is recent enough (major ≥ 2, the
/// release the pinned modules were signed for).
#[must_use]
pub fn pawnio_version_ok(version: Option<&str>) -> bool {
    version
        .and_then(|v| v.split('.').next())
        .and_then(|m| m.trim().parse::<u32>().ok())
        .is_some_and(|major| major >= 2)
}

/// Reads one [`Reading`] from the service pipe.
///
/// Retries for up to `timeout` while the pipe is busy or not yet created —
/// the service opens a fresh instance after each client, and a client that
/// lands in that gap would otherwise see "not found" from a running service.
///
/// # Errors
///
/// When the pipe does not exist (service not installed or stopped) within
/// `timeout`, or the line it serves does not parse.
#[cfg(windows)]
pub fn read_pipe(timeout: std::time::Duration) -> Result<Reading, String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match std::fs::File::open(PIPE_NAME) {
            Ok(mut f) => return Reading::parse(&read_line(&mut f)?),
            Err(e) => {
                if std::time::Instant::now() >= deadline {
                    return Err(format!("open {PIPE_NAME}: {e}"));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

/// Reads up to the first newline.
///
/// Not `read_to_string`: the server disconnects as soon as its line is
/// flushed, and the read after the line then fails with
/// `ERROR_PIPE_NOT_CONNECTED` (233), which std maps to an error rather than
/// end of file (it only treats `ERROR_BROKEN_PIPE` that way). The first
/// client here discarded every complete line for that reason — 0 of 20 reads
/// against the live service, while a .NET reader got all of them.
pub fn read_line(source: &mut impl std::io::Read) -> Result<String, String> {
    let mut buf = Vec::with_capacity(256);
    let mut chunk = [0u8; 256];
    loop {
        match source.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.contains(&b'\n') {
                    break;
                }
            }
            // A disconnect after data is the end of the message.
            Err(_) if !buf.is_empty() => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("read {PIPE_NAME}: {e}")),
        }
        if buf.len() > 64 * 1024 {
            return Err(format!("{PIPE_NAME} sent an oversized reading"));
        }
    }
    let text = String::from_utf8(buf).map_err(|e| format!("{PIPE_NAME}: not UTF-8: {e}"))?;
    Ok(text.lines().next().unwrap_or("").to_owned())
}

/// Helper command line for an elevated `install` / `uninstall`.
///
/// The installer path is quoted: it lives under the user's temp directory,
/// which contains a space on any account named "First Last".
#[must_use]
pub fn helper_args(install: bool, setup: Option<&std::path::Path>) -> String {
    match (install, setup) {
        (true, Some(s)) => format!("install --pawnio-setup \"{}\"", s.display()),
        (true, None) => "install".to_owned(),
        (false, _) => "uninstall".to_owned(),
    }
}

/// Exit codes of the helper, shared with the app that interprets them.
pub mod exit {
    pub const OK: i32 = 0;
    pub const FAILED: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const NO_PAWNIO: i32 = 3;
    pub const NOT_ELEVATED: i32 = 5;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_intel_readout_of_43_below_a_tjmax_of_100_is_57_degrees() {
        let tj = 100u64 << 16;
        let therm = (1u64 << 31) | (43u64 << 16);
        assert_eq!(intel_temp(tj, therm), Some(57.0));
    }

    #[test]
    fn an_unreported_tjmax_falls_back_to_100() {
        assert_eq!(intel_tjmax(0), 100);
        assert_eq!(intel_temp(0, 20u64 << 16), Some(80.0));
    }

    #[test]
    fn the_tcc_offset_and_bit_23_do_not_leak_into_the_temperature() {
        let tj = (5u64 << 24) | (105u64 << 16);
        assert_eq!(intel_tjmax(tj), 105);
        assert_eq!(intel_temp(tj, 0x0028_0000), Some(65.0));
        assert_eq!(
            intel_temp(100 << 16, (1u64 << 23) | (10u64 << 16)),
            Some(90.0)
        );
    }

    #[test]
    fn a_readout_above_tjmax_is_garbage_and_reported_as_absent() {
        assert_eq!(intel_temp(90u64 << 16, 100u64 << 16), None);
    }

    #[test]
    fn amd_tctl_honours_the_minus_49_range_bit() {
        assert!((amd_tctl(360 << 21) - 45.0).abs() < f32::EPSILON);
        assert!((amd_tctl((361 << 21) | 0x8_0000) - (45.125 - 49.0)).abs() < 1e-4);
        assert!((amd_tctl((880 << 21) | 0x8_0000) - 61.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cpu_family_adds_the_extended_family_only_for_base_0xf() {
        assert_eq!(cpu_family(0x00A2_0F10), 0x19); // Zen 4
        assert_eq!(cpu_family(0x0080_0F11), 0x17); // Zen 1
        assert_eq!(cpu_family(0x000B_0671), 6); // Raptor Lake
        assert!(amd_family_supported(0x1A));
        assert!(!amd_family_supported(0x16));
    }

    #[test]
    fn rapl_unit_decodes_the_common_61_microjoule_step() {
        // 0x000A0E03: energy status units = 0x0E -> 1/16384 J.
        let unit = rapl_energy_unit(0x000A_0E03);
        assert!((unit - 1.0 / 16384.0).abs() < 1e-12);
    }

    #[test]
    fn rapl_power_is_energy_over_time() {
        let unit = 1.0 / 16384.0;
        // 16384 counts = 1 J over 0.5 s = 2 W.
        let w = rapl_watts(1000, 1000 + 16384, unit, 0.5).expect("rate");
        assert!((w - 2.0).abs() < 1e-4);
    }

    #[test]
    fn rapl_power_survives_the_32_bit_counter_wrapping() {
        let unit = 1.0 / 16384.0;
        // 100 counts before the wrap, 16284 after: still 16384 counts.
        let w = rapl_watts(0xFFFF_FF9C, 16284, unit, 1.0).expect("rate");
        assert!((w - 1.0).abs() < 1e-4, "wrapped delta read as {w} W");
    }

    #[test]
    fn rapl_power_ignores_the_reserved_upper_half_of_the_msr() {
        let unit = 1.0 / 16384.0;
        let w = rapl_watts(0xABCD_0000_0000_0000, 0x1234_0000_0000_4000, unit, 1.0).expect("rate");
        assert!((w - 1.0).abs() < 1e-4);
    }

    #[test]
    fn rapl_power_has_no_rate_over_no_time() {
        assert_eq!(rapl_watts(0, 10, 1.0, 0.0), None);
        assert_eq!(rapl_watts(0, 10, 1.0, -1.0), None);
    }

    #[test]
    fn the_ok_contract_is_byte_exact() {
        let mut r = Reading::success("intel", Some(57.0), Some(61.0));
        r.tj_max_c = Some(100);
        r.package_w = Some(41.5);
        let j = r.to_json();
        assert_eq!(
            j,
            r#"{"v":1,"ok":true,"vendor":"intel","packageC":57.0,"hottestCoreC":61.0,"tjMaxC":100,"packageW":41.5,"source":"pawnio"}"#
        );
        assert_eq!(Reading::parse(&j).expect("parses"), r);
    }

    #[test]
    fn an_unmeasured_value_is_null_not_zero() {
        let r = Reading::success("amd", Some(48.5), None);
        let j = r.to_json();
        assert!(j.contains(r#""hottestCoreC":null"#), "{j}");
        assert!(j.contains(r#""packageW":null"#), "{j}");
    }

    #[test]
    fn the_error_contract_carries_only_the_message() {
        let e = Reading::failure("PawnIO not available");
        let j = e.to_json();
        assert_eq!(j, r#"{"v":1,"ok":false,"error":"PawnIO not available"}"#);
        assert_eq!(Reading::parse(&j).expect("parses"), e);
    }

    #[test]
    fn a_newer_contract_is_refused_rather_than_half_read() {
        let err = Reading::parse(r#"{"v":2,"ok":true,"packageC":50.0}"#).expect_err("v2");
        assert!(err.contains("v2"), "{err}");
    }

    #[test]
    fn sha256_is_uppercase_hex() {
        assert_eq!(
            sha256_hex(b""),
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        );
    }

    /// Hands out `data`, then fails the way a disconnected pipe does.
    struct DisconnectAfter(Vec<u8>, bool);

    impl std::io::Read for DisconnectAfter {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.1 {
                return Err(std::io::Error::from_raw_os_error(233));
            }
            self.1 = true;
            let n = self.0.len().min(out.len());
            out[..n].copy_from_slice(&self.0[..n]);
            Ok(n)
        }
    }

    #[test]
    fn a_disconnect_after_the_line_is_the_end_of_the_message_not_an_error() {
        let line = Reading::failure("x").to_json();
        // Without the trailing newline the reader must still stop at the
        // disconnect and keep what arrived.
        let mut src = DisconnectAfter(line.clone().into_bytes(), false);
        assert_eq!(read_line(&mut src).expect("line survives"), line);
    }

    #[test]
    fn a_disconnect_before_any_data_is_an_error() {
        let mut src = DisconnectAfter(Vec::new(), true);
        assert!(read_line(&mut src).is_err());
    }

    #[test]
    fn ite_rpm_is_half_the_tach_frequency_over_the_count() {
        // 1.35 MHz / (count * 2): 830 counts ≈ 813 RPM.
        let rpm = ite_rpm(830).expect("spinning");
        assert!((rpm - 813.25).abs() < 0.1, "{rpm}");
    }

    #[test]
    fn ite_distinguishes_no_signal_from_a_stopped_fan() {
        assert_eq!(ite_rpm(0), None, "unconnected header is not a fan");
        assert_eq!(ite_rpm(0x3F), None);
        assert_eq!(ite_rpm(0xFFFF), Some(0.0), "stopped fan is a real zero");
    }

    #[test]
    fn ite_headers_four_to_six_need_their_enable_bit() {
        assert!(ite_fan_enabled(0, 0));
        assert!(!ite_fan_enabled(3, 0));
        assert!(ite_fan_enabled(3, 1 << 4));
        assert!(ite_fan_enabled(4, 1 << 5));
        assert!(ite_fan_enabled(5, 1 << 2));
        assert!(!ite_fan_enabled(5, 1 << 4));
    }

    #[test]
    fn nuvoton_rpm_decodes_the_13_bit_count() {
        // count = (0x13 << 5) | 0x0A = 618 -> 2184.5 RPM
        let rpm = nuvoton_rpm(0x13, 0x0A).expect("spinning");
        assert!((rpm - 2184.47).abs() < 0.1, "{rpm}");
        assert_eq!(nuvoton_rpm(0xFF, 0x1F), Some(0.0));
        assert_eq!(nuvoton_rpm(0, 3), None);
    }

    #[test]
    fn only_chips_with_known_register_maps_are_accepted() {
        assert_eq!(ite_chip(0x8689).map(|c| c.0), Some("IT8689E"));
        assert_eq!(ite_chip(0x8705), None, "8-bit divisor chips are refused");
        assert_eq!(nuvoton_chip(0xD4, 0x2B).map(|c| c.0), Some("NCT6798D"));
        assert_eq!(
            nuvoton_chip(0xD4, 0x40),
            None,
            "NCT6686D EC space is refused"
        );
    }

    #[test]
    fn fans_are_omitted_from_the_line_when_there_are_none() {
        let r = Reading::success("intel", Some(50.0), None);
        assert!(!r.to_json().contains("fans"));
        let mut r = r;
        r.super_io = Some("IT8689E".to_owned());
        r.fans = vec![Fan {
            name: "Fan 1".to_owned(),
            rpm: 812.0,
        }];
        let j = r.to_json();
        assert!(
            j.contains(r#""superIo":"IT8689E","fans":[{"name":"Fan 1","rpm":812.0}]"#),
            "{j}"
        );
        assert_eq!(Reading::parse(&j).expect("parses"), r);
    }

    #[test]
    fn only_pawnio_2_or_newer_is_accepted() {
        assert!(pawnio_version_ok(Some("2.2.0.0")));
        assert!(pawnio_version_ok(Some("10.0")));
        assert!(!pawnio_version_ok(Some("1.9.9")));
        assert!(!pawnio_version_ok(Some("")));
        assert!(!pawnio_version_ok(None));
    }

    #[test]
    fn the_installer_path_is_quoted_because_temp_dirs_contain_spaces() {
        use std::path::Path;
        assert_eq!(helper_args(false, None), "uninstall");
        assert_eq!(helper_args(true, None), "install");
        assert_eq!(
            helper_args(true, Some(Path::new(r"C:\Users\A B\Temp\PawnIO_setup.exe"))),
            r#"install --pawnio-setup "C:\Users\A B\Temp\PawnIO_setup.exe""#
        );
    }

    #[test]
    fn the_embedded_modules_match_their_pins() {
        // The helper embeds these with include_bytes!; a module refreshed on
        // disk without updating the pin would be refused at load time on the
        // user's machine, so the mismatch must fail here instead.
        let intel = include_bytes!("../modules/IntelMSR.bin");
        let amd = include_bytes!("../modules/AMDFamily17.bin");
        assert_eq!(sha256_hex(intel), INTEL_MODULE_SHA256);
        assert_eq!(sha256_hex(amd), AMD_MODULE_SHA256);
    }
}
