//! Hardware inventory: what this computer is made of.
//!
//! # Why on demand, not on the tick
//!
//! Nothing here changes while the machine is running, and the cheapest WMI
//! read still costs tens of milliseconds for COM and `ConnectServer`. The
//! sampler budget is 30 ms for everything, so the inventory is read when a
//! screen asks for it and never from the sampling loop. The one live
//! reading — drive temperature — has its own path, [`read_drive_temperatures`],
//! which avoids WMI entirely.
//!
//! # Why every fact is optional
//!
//! Firmware lies in a small number of predictable ways: an empty string, an
//! OEM template left unfilled (`To Be Filled By O.E.M.`), a zero where a
//! number was never written, or a counter pinned at its type's maximum.
//! Each of those becomes `None` here rather than reaching a screen, because
//! a person reading "Version: x.x" or "Video memory: 4 GB" beside an 8 GB
//! card has been told something false. Each part of the inventory also
//! degrades on its own: a missing storage namespace leaves the CPU intact.

mod drive_temp;
mod wmi;

use std::time::{Duration, Instant};

use wmi::{Namespace, Row, Wmi, WmiValue};

pub use drive_temp::{DriveHealth, DriveReport, read_drive_reports, read_drive_temperatures};

/// Everything [`read_inventory`] found, plus how long it took.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HardwareInventory {
    pub cpu: Vec<CpuInfo>,
    pub memory: MemoryInfo,
    pub gpus: Vec<GpuInfo>,
    pub drives: Vec<DriveInfo>,
    pub board: BoardInfo,
    pub elapsed: Duration,
}

/// One processor package (`Win32_Processor`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CpuInfo {
    pub name: String,
    pub manufacturer: Option<String>,
    pub socket: Option<String>,
    pub cores: Option<u32>,
    pub logical_processors: Option<u32>,
    pub base_clock_mhz: Option<u32>,
    pub l2_cache_kb: Option<u32>,
    pub l3_cache_kb: Option<u32>,
    pub virtualization_enabled: Option<bool>,
}

/// Installed memory (`Win32_PhysicalMemoryArray`, `Win32_PhysicalMemory`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryInfo {
    /// Memory usable by Windows (`TotalPhysicalMemory`), which is slightly
    /// less than the sum of the modules: firmware reserves some of it.
    pub total_bytes: Option<u64>,
    pub slots_total: Option<u32>,
    pub max_capacity_bytes: Option<u64>,
    pub modules: Vec<MemoryModule>,
}

/// One memory module.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryModule {
    /// The slot label printed on the board (`DeviceLocator`).
    pub slot: Option<String>,
    pub bank: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub speed_mts: Option<u32>,
    pub configured_speed_mts: Option<u32>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    /// Identifies the physical part. Never log it.
    pub serial: Option<String>,
    /// `"DDR4"`, `"DDR5"`, … from `SMBIOSMemoryType`.
    pub kind: Option<String>,
    /// `"DIMM"` or `"SODIMM"`.
    pub form_factor: Option<String>,
    pub voltage_mv: Option<u32>,
}

/// One display adapter (`Win32_VideoController`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GpuInfo {
    pub name: String,
    pub manufacturer: Option<String>,
    pub driver_version: Option<String>,
    /// ISO `yyyy-mm-dd`.
    pub driver_date: Option<String>,
    /// From the driver's registry key where present, because `AdapterRAM`
    /// is a `uint32` and cannot describe more than 4 GiB.
    pub video_memory_bytes: Option<u64>,
    /// `"3440 × 1440"`.
    pub resolution: Option<String>,
    pub refresh_hz: Option<u32>,
    pub pnp_device_id: Option<String>,
}

/// One physical disk (`MSFT_PhysicalDisk`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DriveInfo {
    /// The `N` of `\\.\PhysicalDriveN`.
    pub index: u32,
    pub model: String,
    /// Identifies the physical part. Never log it.
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub size_bytes: Option<u64>,
    pub media: DriveMedia,
    /// `"NVMe"`, `"SATA"`, `"USB"`, …
    pub bus: Option<String>,
    /// `"Healthy"`, `"Warning"` or `"Unhealthy"`.
    pub health: Option<String>,
    pub spindle_rpm: Option<u32>,
    pub temperature_celsius: Option<f32>,
}

/// What a drive stores data on. `NVMe` is a bus, not a medium, and lives in
/// [`DriveInfo::bus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DriveMedia {
    Ssd,
    Hdd,
    #[default]
    Unknown,
}

/// Motherboard, firmware and system identity.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoardInfo {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub bios_vendor: Option<String>,
    pub bios_version: Option<String>,
    /// ISO `yyyy-mm-dd`.
    pub bios_date: Option<String>,
    pub system_manufacturer: Option<String>,
    pub system_model: Option<String>,
}

/// Reads the full inventory. Never fails: a part that cannot be read is
/// empty, and the rest is unaffected.
pub fn read_inventory() -> HardwareInventory {
    let started = Instant::now();
    let mut inventory = HardwareInventory::default();

    if let Some(wmi) = Wmi::new() {
        if let Some(cimv2) = wmi.connect(r"root\cimv2") {
            read_cimv2(&cimv2, &mut inventory);
        }
        if let Some(storage) = wmi.connect(r"root\Microsoft\Windows\Storage") {
            inventory.drives = read_drives(&storage);
        }
    }

    for (index, celsius) in read_drive_temperatures() {
        if let Some(drive) = inventory.drives.iter_mut().find(|d| d.index == index) {
            drive.temperature_celsius = Some(celsius);
        }
    }

    inventory.elapsed = started.elapsed();
    inventory
}

fn read_cimv2(cimv2: &Namespace<'_>, inventory: &mut HardwareInventory) {
    let system = first(
        cimv2,
        "Win32_ComputerSystem",
        &[
            "Manufacturer",
            "Model",
            "TotalPhysicalMemory",
            "HypervisorPresent",
        ],
    );
    let hypervisor = Fields(&system).flag("HypervisorPresent");

    inventory.cpu = read_cpus(cimv2, hypervisor);
    inventory.memory = read_memory(cimv2, &system);
    inventory.gpus = read_gpus(cimv2);
    inventory.board = read_board(cimv2, &system);
}

fn read_cpus(cimv2: &Namespace<'_>, hypervisor: Option<bool>) -> Vec<CpuInfo> {
    let props = [
        "Name",
        "Manufacturer",
        "SocketDesignation",
        "NumberOfCores",
        "NumberOfLogicalProcessors",
        "MaxClockSpeed",
        "L2CacheSize",
        "L3CacheSize",
        "VirtualizationFirmwareEnabled",
    ];
    cimv2
        .select("Win32_Processor", &props)
        .iter()
        .map(|row| {
            let f = Fields(row);
            CpuInfo {
                name: f.text("Name").unwrap_or_default(),
                manufacturer: f.text("Manufacturer"),
                socket: f.text("SocketDesignation"),
                cores: f.count("NumberOfCores"),
                logical_processors: f.count("NumberOfLogicalProcessors"),
                base_clock_mhz: f.count("MaxClockSpeed"),
                l2_cache_kb: f.count("L2CacheSize"),
                l3_cache_kb: f.count("L3CacheSize"),
                virtualization_enabled: virtualization(
                    f.flag("VirtualizationFirmwareEnabled"),
                    hypervisor,
                ),
            }
        })
        .collect()
}

fn read_memory(cimv2: &Namespace<'_>, system: &Row) -> MemoryInfo {
    let arrays = cimv2.select(
        "Win32_PhysicalMemoryArray",
        &["MemoryDevices", "MaxCapacityEx"],
    );
    let slots = sum_known(arrays.iter().map(|row| Fields(row).number("MemoryDevices")));
    let max_kb = sum_known(arrays.iter().map(|row| Fields(row).number("MaxCapacityEx")));

    let props = [
        "DeviceLocator",
        "BankLabel",
        "Capacity",
        "Speed",
        "ConfiguredClockSpeed",
        "Manufacturer",
        "PartNumber",
        "SerialNumber",
        "SMBIOSMemoryType",
        "FormFactor",
        "ConfiguredVoltage",
    ];
    let modules: Vec<MemoryModule> = cimv2
        .select("Win32_PhysicalMemory", &props)
        .iter()
        .map(|row| {
            let f = Fields(row);
            MemoryModule {
                slot: f.text("DeviceLocator"),
                bank: f.text("BankLabel"),
                capacity_bytes: f.number("Capacity"),
                speed_mts: f.count("Speed"),
                configured_speed_mts: f.count("ConfiguredClockSpeed"),
                manufacturer: f.text("Manufacturer"),
                part_number: f.text("PartNumber"),
                serial: f.text("SerialNumber").filter(|s| !is_blank_serial(s)),
                kind: f
                    .raw_u64("SMBIOSMemoryType")
                    .and_then(memory_kind)
                    .map(str::to_owned),
                form_factor: f
                    .raw_u64("FormFactor")
                    .and_then(form_factor)
                    .map(str::to_owned),
                voltage_mv: f.count("ConfiguredVoltage"),
            }
        })
        .collect();

    let installed = sum_known(modules.iter().map(|m| m.capacity_bytes));
    MemoryInfo {
        total_bytes: Fields(system).number("TotalPhysicalMemory"),
        slots_total: slots.and_then(|n| u32::try_from(n).ok()),
        max_capacity_bytes: plausible_max_capacity(
            max_kb.and_then(|kb| kb.checked_mul(1024)),
            installed,
        ),
        modules,
    }
}

/// The board's stated maximum, unless it is below what is installed.
///
/// SMBIOS type 16 is written once by the BIOS vendor and often never
/// updated: this Z790 reports 128 GiB with 192 GiB fitted. A maximum smaller
/// than the installed total is provably wrong, and printing it beside the
/// real figure would contradict the screen, so it is dropped.
fn plausible_max_capacity(stated: Option<u64>, installed: Option<u64>) -> Option<u64> {
    match (stated, installed) {
        (Some(max), Some(have)) if max < have => None,
        (max, _) => max,
    }
}

/// A serial of only zeros (or `0x` + zeros) is the SPD's "not programmed".
fn is_blank_serial(serial: &str) -> bool {
    let digits = serial.trim_start_matches("0x").trim_start_matches("0X");
    !digits.is_empty() && digits.chars().all(|c| c == '0')
}

fn read_gpus(cimv2: &Namespace<'_>) -> Vec<GpuInfo> {
    let props = [
        "Name",
        "AdapterCompatibility",
        "DriverVersion",
        "DriverDate",
        "AdapterRAM",
        "CurrentHorizontalResolution",
        "CurrentVerticalResolution",
        "CurrentRefreshRate",
        "PNPDeviceID",
    ];
    let registry = registry::display_adapter_memory();
    cimv2
        .select("Win32_VideoController", &props)
        .iter()
        .map(|row| {
            let f = Fields(row);
            let name = f.text("Name").unwrap_or_default();
            let from_registry = registry
                .iter()
                .find(|(desc, _)| desc.eq_ignore_ascii_case(&name))
                .map(|(_, bytes)| *bytes);
            GpuInfo {
                manufacturer: f.text("AdapterCompatibility"),
                driver_version: f.version("DriverVersion"),
                driver_date: f.text("DriverDate").as_deref().and_then(cim_date),
                video_memory_bytes: from_registry
                    .or_else(|| f.raw_u64("AdapterRAM").and_then(adapter_ram)),
                resolution: resolution(
                    f.raw_u64("CurrentHorizontalResolution"),
                    f.raw_u64("CurrentVerticalResolution"),
                ),
                refresh_hz: f.raw_u64("CurrentRefreshRate").and_then(refresh_rate),
                pnp_device_id: f.text("PNPDeviceID"),
                name,
            }
        })
        .collect()
}

fn read_board(cimv2: &Namespace<'_>, system: &Row) -> BoardInfo {
    let board = first(
        cimv2,
        "Win32_BaseBoard",
        &["Manufacturer", "Product", "Version"],
    );
    let bios = first(
        cimv2,
        "Win32_BIOS",
        &["Manufacturer", "SMBIOSBIOSVersion", "ReleaseDate"],
    );
    let (b, bios, s) = (Fields(&board), Fields(&bios), Fields(system));
    BoardInfo {
        manufacturer: b.text("Manufacturer"),
        product: b.text("Product"),
        version: b.version("Version"),
        bios_vendor: bios.text("Manufacturer"),
        bios_version: bios.version("SMBIOSBIOSVersion"),
        bios_date: bios.text("ReleaseDate").as_deref().and_then(cim_date),
        system_manufacturer: s.text("Manufacturer"),
        system_model: s.text("Model"),
    }
}

fn read_drives(storage: &Namespace<'_>) -> Vec<DriveInfo> {
    let props = [
        "DeviceId",
        "FriendlyName",
        "SerialNumber",
        "FirmwareVersion",
        "Size",
        "MediaType",
        "BusType",
        "HealthStatus",
        "SpindleSpeed",
    ];
    let mut drives: Vec<DriveInfo> = storage
        .select("MSFT_PhysicalDisk", &props)
        .iter()
        .filter_map(|row| {
            let f = Fields(row);
            // A disk without a drive number cannot be matched to anything
            // else on the machine, so it is not listed.
            let index = f.text("DeviceId")?.parse().ok()?;
            Some(DriveInfo {
                index,
                model: f.text("FriendlyName").unwrap_or_default(),
                serial: f.text("SerialNumber"),
                firmware: f.version("FirmwareVersion"),
                size_bytes: f.number("Size"),
                media: f.raw_u64("MediaType").map_or(DriveMedia::Unknown, media),
                bus: f.raw_u64("BusType").and_then(bus).map(str::to_owned),
                health: f
                    .raw_u64("HealthStatus")
                    .and_then(health)
                    .map(str::to_owned),
                spindle_rpm: f.raw_u64("SpindleSpeed").and_then(spindle),
                temperature_celsius: None,
            })
        })
        .collect();
    drives.sort_by_key(|drive| drive.index);
    drives
}

fn first(ns: &Namespace<'_>, class: &str, props: &[&str]) -> Row {
    ns.select(class, props)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Sums the known values; `None` only when none was known.
fn sum_known(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    values.flatten().reduce(u64::saturating_add)
}

/// Typed, sanitised access to one WMI row.
struct Fields<'a>(&'a Row);

impl Fields<'_> {
    fn text(&self, key: &str) -> Option<String> {
        self.0.get(key).and_then(WmiValue::text).and_then(sanitise)
    }

    fn version(&self, key: &str) -> Option<String> {
        self.text(key).filter(|v| !is_version_placeholder(v))
    }

    /// The number exactly as reported, zero included. For enumerations,
    /// where zero is a code rather than a missing value.
    fn raw_u64(&self, key: &str) -> Option<u64> {
        self.0.get(key).and_then(WmiValue::unsigned)
    }

    /// A quantity, where zero means "not written by the firmware".
    fn number(&self, key: &str) -> Option<u64> {
        self.raw_u64(key).filter(|&n| n != 0)
    }

    fn count(&self, key: &str) -> Option<u32> {
        self.number(key).and_then(|n| u32::try_from(n).ok())
    }

    fn flag(&self, key: &str) -> Option<bool> {
        self.0.get(key).and_then(WmiValue::flag)
    }
}

/// Firmware strings that mean "nobody filled this in".
const PLACEHOLDERS: &[&str] = &[
    "To Be Filled By O.E.M.",
    "Default string",
    "System Product Name",
    "System manufacturer",
    "System Serial Number",
    "Not Specified",
    "Not Available",
    "None",
    "N/A",
    "Undefined",
];

/// Trims and removes firmware placeholder text.
fn sanitise(raw: &str) -> Option<String> {
    let trimmed = raw.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    let junk = trimmed.is_empty() || PLACEHOLDERS.iter().any(|p| p.eq_ignore_ascii_case(trimmed));
    (!junk).then(|| trimmed.to_owned())
}

/// Version fields additionally use `x.x`, `0` and `0.0` for "unset".
fn is_version_placeholder(version: &str) -> bool {
    version.eq_ignore_ascii_case("x.x") || version.chars().all(|c| c == '0' || c == '.')
}

/// The date part of a CIM datetime (`20260904000000.000000-000`), as ISO.
fn cim_date(raw: &str) -> Option<String> {
    let digits = raw.get(..8)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (year, month, day) = (&digits[..4], &digits[4..6], &digits[6..8]);
    let month_ok = matches!(month.parse::<u8>(), Ok(1..=12));
    let day_ok = matches!(day.parse::<u8>(), Ok(1..=31));
    (month_ok && day_ok && year != "0000").then(|| format!("{year}-{month}-{day}"))
}

/// `Win32_Processor` reports virtualisation as disabled whenever a
/// hypervisor (Hyper-V, WSL 2, VBS) is running, because it reads the flag
/// from inside the hypervisor's guest. A running hypervisor is proof that
/// it is enabled.
fn virtualization(firmware: Option<bool>, hypervisor: Option<bool>) -> Option<bool> {
    if hypervisor == Some(true) {
        Some(true)
    } else {
        firmware
    }
}

fn memory_kind(smbios: u64) -> Option<&'static str> {
    Some(match smbios {
        24 => "DDR3",
        26 => "DDR4",
        29 => "LPDDR3",
        30 => "LPDDR4",
        34 => "DDR5",
        35 => "LPDDR5",
        _ => return None,
    })
}

fn form_factor(code: u64) -> Option<&'static str> {
    Some(match code {
        8 => "DIMM",
        12 => "SODIMM",
        _ => return None,
    })
}

/// `AdapterRAM` is a `uint32`: a card with 4 GiB or more reports the cap
/// (drivers write `0xFFF00000` or `0xFFFFFFFF`), which is not a measurement.
fn adapter_ram(bytes: u64) -> Option<u64> {
    (bytes != 0 && bytes < 0xFFF0_0000).then_some(bytes)
}

fn resolution(width: Option<u64>, height: Option<u64>) -> Option<String> {
    match (width?, height?) {
        (0, _) | (_, 0) => None,
        (w, h) => Some(format!("{w} × {h}")),
    }
}

/// 0 and 1 are "hardware default", not a rate.
fn refresh_rate(hz: u64) -> Option<u32> {
    u32::try_from(hz).ok().filter(|&hz| hz > 1)
}

fn media(code: u64) -> DriveMedia {
    match code {
        3 => DriveMedia::Hdd,
        // 5 is storage-class memory: solid state, with no moving parts.
        4 | 5 => DriveMedia::Ssd,
        _ => DriveMedia::Unknown,
    }
}

fn bus(code: u64) -> Option<&'static str> {
    Some(match code {
        1 => "SCSI",
        2 => "ATAPI",
        3 => "ATA",
        4 => "FireWire",
        5 => "SSA",
        6 => "Fibre Channel",
        7 => "USB",
        8 => "RAID",
        9 => "iSCSI",
        10 => "SAS",
        11 => "SATA",
        12 => "SD",
        13 => "MMC",
        15 => "Virtual",
        16 => "Storage Spaces",
        17 => "NVMe",
        _ => return None,
    })
}

fn health(code: u64) -> Option<&'static str> {
    Some(match code {
        0 => "Healthy",
        1 => "Warning",
        2 => "Unhealthy",
        _ => return None,
    })
}

/// Solid-state drives report 0, and "unknown" is `0xFFFFFFFF`.
fn spindle(rpm: u64) -> Option<u32> {
    u32::try_from(rpm)
        .ok()
        .filter(|&rpm| rpm != 0 && rpm != u32::MAX)
}

mod registry {
    //! Dedicated video memory from the display driver's own registry key.
    //!
    //! The display adapter class key holds one numbered subkey per adapter,
    //! and each driver writes its true memory size there as a 64-bit value.
    //! Matching on `DriverDesc` against the WMI name is imprecise for two
    //! identical cards, but they then have the same memory, so the answer
    //! is still right.

    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_ANY, RegGetValueW};
    use windows::core::PCWSTR;

    const CLASS_KEY: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    /// Adapter subkeys are `0000`, `0001`, …; the scan tolerates gaps.
    const MAX_SUBKEY: u32 = 32;

    /// `(DriverDesc, bytes)` for every adapter that records its memory.
    pub(super) fn display_adapter_memory() -> Vec<(String, u64)> {
        (0..MAX_SUBKEY)
            .filter_map(|n| {
                let key = format!(r"{CLASS_KEY}\{n:04}");
                let desc = read(&key, "DriverDesc").and_then(|b| utf16_string(&b))?;
                let bytes = read(&key, "HardwareInformation.qwMemorySize")
                    .or_else(|| read(&key, "HardwareInformation.MemorySize"))
                    .and_then(|b| memory_size(&b))?;
                Some((desc, bytes))
            })
            .collect()
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn read(key: &str, value: &str) -> Option<Vec<u8>> {
        let (key, value) = (wide(key), wide(value));
        let mut buffer = vec![0_u8; 512];
        let mut len = 512_u32;
        // SAFETY: both names are NUL-terminated; `buffer` is `len` bytes and
        // outlives the call, which writes at most `len` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_ANY,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&raw mut len),
            )
        };
        status.is_ok().then(|| {
            buffer.truncate(len as usize);
            buffer
        })
    }

    fn utf16_string(bytes: &[u8]) -> Option<String> {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|&unit| unit != 0)
            .collect();
        let text = String::from_utf16_lossy(&units).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }

    /// An 8-byte `QWORD`/binary or a 4-byte `DWORD`/binary; zero is absent.
    pub(super) fn memory_size(bytes: &[u8]) -> Option<u64> {
        let value = if let Ok(qword) = <[u8; 8]>::try_from(bytes) {
            u64::from_le_bytes(qword)
        } else if let Ok(dword) = <[u8; 4]>::try_from(bytes) {
            u64::from(u32::from_le_bytes(dword))
        } else {
            return None;
        };
        (value != 0).then_some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oem_placeholder_strings_are_absent_not_shown() {
        assert!(is_blank_serial("00000000"));
        assert!(is_blank_serial("0x00000000"));
        assert!(!is_blank_serial("0A1B0000"));
        assert_eq!(
            plausible_max_capacity(Some(128 << 30), Some(192 << 30)),
            None,
            "a maximum below the installed total is stale firmware"
        );
        assert_eq!(
            plausible_max_capacity(Some(256 << 30), Some(192 << 30)),
            Some(256 << 30)
        );
        assert_eq!(plausible_max_capacity(Some(64 << 30), None), Some(64 << 30));
        assert_eq!(sanitise("To Be Filled By O.E.M."), None);
        assert_eq!(sanitise("  Default string "), None);
        assert_eq!(sanitise("system product name"), None);
        assert_eq!(sanitise(""), None);
        assert_eq!(sanitise("   "), None);
    }

    #[test]
    fn real_strings_are_trimmed_and_kept() {
        assert_eq!(
            sanitise("CMH96GX5M2B5200C38   ").as_deref(),
            Some("CMH96GX5M2B5200C38"),
        );
    }

    #[test]
    fn unset_version_numbers_are_placeholders() {
        assert!(is_version_placeholder("x.x"));
        assert!(is_version_placeholder("0"));
        assert!(is_version_placeholder("0.0"));
        assert!(!is_version_placeholder("F10"));
        assert!(!is_version_placeholder("32.0.15.8157"));
    }

    #[test]
    fn a_cim_datetime_becomes_an_iso_date() {
        assert_eq!(
            cim_date("20260904000000.000000-000").as_deref(),
            Some("2026-09-04")
        );
    }

    #[test]
    fn a_malformed_cim_datetime_is_absent() {
        assert_eq!(cim_date("2026"), None);
        assert_eq!(cim_date("20261399000000.000000-000"), None);
        assert_eq!(cim_date("00000000000000.000000-000"), None);
        assert_eq!(cim_date("abcdefgh"), None);
    }

    #[test]
    fn smbios_memory_types_name_the_generation_or_nothing() {
        assert_eq!(memory_kind(34), Some("DDR5"));
        assert_eq!(memory_kind(26), Some("DDR4"));
        assert_eq!(memory_kind(24), Some("DDR3"));
        assert_eq!(memory_kind(30), Some("LPDDR4"));
        assert_eq!(memory_kind(35), Some("LPDDR5"));
        assert_eq!(memory_kind(0), None);
        assert_eq!(memory_kind(2), None);
        assert_eq!(form_factor(8), Some("DIMM"));
        assert_eq!(form_factor(12), Some("SODIMM"));
        assert_eq!(form_factor(0), None);
    }

    #[test]
    fn a_4_gib_adapter_ram_is_the_cap_not_a_measurement() {
        assert_eq!(adapter_ram(0xFFF0_0000), None);
        assert_eq!(adapter_ram(0xFFFF_FFFF), None);
        assert_eq!(adapter_ram(0), None);
        assert_eq!(adapter_ram(2 << 30), Some(2 << 30));
    }

    #[test]
    fn registry_memory_size_reads_qword_and_dword_forms() {
        let eight_gib = 8_u64 << 30;
        assert_eq!(
            registry::memory_size(&eight_gib.to_le_bytes()),
            Some(eight_gib)
        );
        assert_eq!(
            registry::memory_size(&(512_u32 << 20).to_le_bytes()),
            Some(512 << 20)
        );
        assert_eq!(registry::memory_size(&[0; 8]), None);
        assert_eq!(registry::memory_size(&[1, 2, 3]), None);
    }

    #[test]
    fn resolution_needs_both_dimensions() {
        assert_eq!(
            resolution(Some(3440), Some(1440)).as_deref(),
            Some("3440 × 1440")
        );
        assert_eq!(resolution(Some(0), Some(1440)), None);
        assert_eq!(resolution(None, Some(1440)), None);
    }

    #[test]
    fn a_default_refresh_rate_is_not_a_rate() {
        assert_eq!(refresh_rate(0), None);
        assert_eq!(refresh_rate(1), None);
        assert_eq!(refresh_rate(144), Some(144));
    }

    #[test]
    fn storage_codes_map_to_media_bus_and_health() {
        assert_eq!(media(3), DriveMedia::Hdd);
        assert_eq!(media(4), DriveMedia::Ssd);
        assert_eq!(media(0), DriveMedia::Unknown);
        assert_eq!(bus(17), Some("NVMe"));
        assert_eq!(bus(11), Some("SATA"));
        assert_eq!(bus(7), Some("USB"));
        assert_eq!(bus(99), None);
        assert_eq!(health(0), Some("Healthy"));
        assert_eq!(health(2), Some("Unhealthy"));
        assert_eq!(health(5), None);
    }

    #[test]
    fn a_solid_state_or_unknown_spindle_speed_is_absent() {
        assert_eq!(spindle(0), None);
        assert_eq!(spindle(0xFFFF_FFFF), None);
        assert_eq!(spindle(7200), Some(7200));
    }

    #[test]
    fn a_running_hypervisor_proves_virtualisation_is_enabled() {
        assert_eq!(virtualization(Some(false), Some(true)), Some(true));
        assert_eq!(virtualization(Some(false), Some(false)), Some(false));
        assert_eq!(virtualization(None, None), None);
    }

    #[test]
    fn a_sum_of_nothing_known_is_absent_not_zero() {
        assert_eq!(sum_known([None, None].into_iter()), None);
        assert_eq!(sum_known([Some(2), None, Some(2)].into_iter()), Some(4));
    }
}
