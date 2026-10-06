//! Drive temperatures without elevation and without WMI.
//!
//! # Why a zero-access handle
//!
//! Opening `\\.\PhysicalDriveN` for read needs administrator rights, but a
//! handle with an access mask of zero does not, and the storage stack still
//! answers `IOCTL_STORAGE_QUERY_PROPERTY` on it. That is the whole trick:
//! the same two queries `smartctl` would make elevated, answered for an
//! ordinary user. Verified on this project's reference machine against
//! SATA and `NVMe` drives (see the probes under `.copilot-tmp`).
//!
//! # Why two queries
//!
//! `StorageDeviceTemperatureProperty` is the generic path and works for
//! SATA, but the in-box `NVMe` driver rejects it (error 1117). `NVMe` drives
//! instead answer the protocol-specific query for the SMART / health log
//! page, whose composite temperature is in kelvin. USB bridges generally
//! reject both, which is reported as absent — never as zero.
//!
//! # Why this is cheap enough for the sensors table
//!
//! Two `DeviceIoControl` calls per drive and no COM. The driver serves the
//! temperature from its cache; it does not spin up a sleeping disk.

use std::ffi::c_void;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::IOCTL_STORAGE_QUERY_PROPERTY;

/// Highest `PhysicalDriveN` probed. Windows numbers disks densely from zero,
/// but removing a disk leaves a gap, so the scan does not stop at the first
/// failed open.
const MAX_DRIVE_INDEX: u32 = 31;

/// `StorageDeviceTemperatureProperty`.
const PROPERTY_TEMPERATURE: u32 = 52;
/// `StorageDeviceProtocolSpecificProperty`.
const PROPERTY_PROTOCOL_SPECIFIC: u32 = 50;
/// `ProtocolTypeNvme`.
const PROTOCOL_NVME: u32 = 3;
/// `NVMeDataTypeLogPage`.
const NVME_DATA_LOG_PAGE: u32 = 2;
/// `NVME_LOG_PAGE_HEALTH_INFO`.
const NVME_LOG_HEALTH: u32 = 2;
/// `ProtocolTypeAta`.
const PROTOCOL_ATA: u32 = 2;
/// `AtaDataTypeLogPage`.
const ATA_DATA_LOG_PAGE: u32 = 2;
/// ATA Device Statistics log and its Solid State Device Statistics page.
const ATA_LOG_DEVICE_STATISTICS: u32 = 0x04;
const ATA_PAGE_SSD_STATISTICS: u32 = 0x07;
/// One ATA log page.
const ATA_PAGE_LEN: usize = 512;
const ATA_BUFFER_LEN: usize = QUERY_HEADER + PROTOCOL_DATA_HEADER + ATA_PAGE_LEN;

/// `STORAGE_PROPERTY_QUERY` up to (not including) `AdditionalParameters`.
const QUERY_HEADER: usize = 8;
/// `sizeof(STORAGE_PROTOCOL_SPECIFIC_DATA)`.
const PROTOCOL_DATA_HEADER: usize = 40;
/// The `NVMe` health log page is 512 bytes.
const NVME_LOG_LEN: usize = 512;
const NVME_BUFFER_LEN: usize = QUERY_HEADER + PROTOCOL_DATA_HEADER + NVME_LOG_LEN;

/// `STORAGE_TEMPERATURE_DATA_DESCRIPTOR`: offset of `InfoCount` and of the
/// first `STORAGE_TEMPERATURE_INFO`.
const TEMP_INFO_COUNT_OFFSET: usize = 12;
const TEMP_INFO_OFFSET: usize = 24;

/// The range a working drive can report. Outside it the value is a sensor
/// fault or an uninitialised field, and showing it would be a lie.
const PLAUSIBLE_CELSIUS: std::ops::RangeInclusive<f32> = -20.0..=120.0;

/// `(physical drive index, °C)` for every drive that reports a temperature.
/// Drives that exist but cannot report are simply absent.
pub fn read_drive_temperatures() -> Vec<(u32, f32)> {
    (0..=MAX_DRIVE_INDEX)
        .filter_map(|index| {
            let drive = Drive::open(index)?;
            drive.temperature().map(|celsius| (index, celsius))
        })
        .collect()
}

/// What a physical drive reports about itself without elevation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DriveReport {
    pub index: u32,
    pub celsius: Option<f32>,
    pub health: Option<DriveHealth>,
}

/// Wear and error figures a drive reports about itself: the `NVMe` SMART /
/// health log, or for a SATA SSD the ATA Device Statistics page, which has
/// only the wear figure — the others stay `None` there, never `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DriveHealth {
    /// `100 - Percentage Used`, floored at 0 (the field may exceed 100).
    pub life_remaining: u8,
    pub power_on_hours: Option<u64>,
    /// Data Units Written × 512 000 bytes.
    pub bytes_written: Option<u64>,
    pub media_errors: Option<u64>,
    /// Any bit of Critical Warning set: spare below threshold, temperature,
    /// reliability degraded, read-only, or volatile backup failed.
    pub critical: bool,
}

/// Temperature and, for `NVMe`, health of every physical drive that answers.
#[must_use]
pub fn read_drive_reports() -> Vec<DriveReport> {
    (0..=MAX_DRIVE_INDEX)
        .filter_map(|index| {
            let drive = Drive::open(index)?;
            let log = drive.nvme_health_log();
            let health = log.as_deref().and_then(decode_nvme_wear).or_else(|| {
                drive
                    .ata_ssd_statistics()
                    .as_deref()
                    .and_then(decode_ata_wear)
            });
            let celsius = drive
                .descriptor_temperature()
                .or_else(|| log.as_deref().and_then(decode_nvme_health));
            (celsius.is_some() || health.is_some()).then_some(DriveReport {
                index,
                celsius,
                health,
            })
        })
        .collect()
}

/// A zero-access handle to a physical drive, closed on drop.
struct Drive(HANDLE);

impl Drive {
    fn open(index: u32) -> Option<Self> {
        let path: Vec<u16> = format!(r"\\.\PhysicalDrive{index}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: `path` is NUL-terminated and outlives the call; a zero
        // access mask asks for no rights, only a handle for IOCTLs.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        (!handle.is_null() && handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
    }

    fn temperature(&self) -> Option<f32> {
        if let Some(celsius) = self.descriptor_temperature() {
            return Some(celsius);
        }
        decode_nvme_health(&self.nvme_health_log()?)
    }

    /// The generic temperature property (SATA and most others).
    fn descriptor_temperature(&self) -> Option<f32> {
        let mut query = [0_u8; 12];
        query[..4].copy_from_slice(&PROPERTY_TEMPERATURE.to_le_bytes());
        let mut out = [0_u8; 1024];
        let len = self.query(&query, &mut out)?;
        decode_temperature_descriptor(&out[..len])
    }

    /// The raw `NVMe` health log response, header included.
    fn nvme_health_log(&self) -> Option<Vec<u8>> {
        let request = nvme_health_request();
        let mut out = [0_u8; NVME_BUFFER_LEN];
        let len = self.query(&request, &mut out)?;
        Some(out[..len].to_vec())
    }

    /// The ATA Solid State Device Statistics page, header included.
    fn ata_ssd_statistics(&self) -> Option<Vec<u8>> {
        let request = ata_statistics_request();
        let mut out = [0_u8; ATA_BUFFER_LEN];
        let len = self.query(&request, &mut out)?;
        Some(out[..len].to_vec())
    }

    /// Runs `IOCTL_STORAGE_QUERY_PROPERTY`, returning the bytes written.
    fn query(&self, input: &[u8], output: &mut [u8]) -> Option<usize> {
        let mut returned = 0_u32;
        // SAFETY: both buffers are live for the call and their lengths are
        // passed exactly; the call is synchronous (no OVERLAPPED).
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                IOCTL_STORAGE_QUERY_PROPERTY,
                input.as_ptr().cast::<c_void>(),
                u32::try_from(input.len()).ok()?,
                output.as_mut_ptr().cast::<c_void>(),
                u32::try_from(output.len()).ok()?,
                &raw mut returned,
                ptr::null_mut(),
            )
        };
        (ok != 0).then(|| (returned as usize).min(output.len()))
    }
}

impl Drop for Drive {
    fn drop(&mut self) {
        // SAFETY: a valid handle from `CreateFileW`, closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

/// `STORAGE_PROPERTY_QUERY` for the `NVMe` health log, followed by the
/// `STORAGE_PROTOCOL_SPECIFIC_DATA` that selects it, followed by room for
/// the 512-byte page the driver writes back in place.
fn nvme_health_request() -> [u8; NVME_BUFFER_LEN] {
    let mut buffer = [0_u8; NVME_BUFFER_LEN];
    let mut put = |offset: usize, value: u32| {
        buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    };
    put(0, PROPERTY_PROTOCOL_SPECIFIC);
    put(4, 0); // PropertyStandardQuery
    put(QUERY_HEADER, PROTOCOL_NVME);
    put(QUERY_HEADER + 4, NVME_DATA_LOG_PAGE);
    put(QUERY_HEADER + 8, NVME_LOG_HEALTH);
    put(QUERY_HEADER + 12, 0); // ProtocolDataRequestSubValue
    put(QUERY_HEADER + 16, PROTOCOL_DATA_HEADER as u32);
    put(QUERY_HEADER + 20, NVME_LOG_LEN as u32);
    buffer
}

/// The protocol-specific query for ATA log 04h page 07h.
fn ata_statistics_request() -> [u8; ATA_BUFFER_LEN] {
    let mut buffer = [0_u8; ATA_BUFFER_LEN];
    let mut put = |offset: usize, value: u32| {
        buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    };
    put(0, PROPERTY_PROTOCOL_SPECIFIC);
    put(4, 0); // PropertyStandardQuery
    put(QUERY_HEADER, PROTOCOL_ATA);
    put(QUERY_HEADER + 4, ATA_DATA_LOG_PAGE);
    put(QUERY_HEADER + 8, ATA_LOG_DEVICE_STATISTICS);
    put(QUERY_HEADER + 12, ATA_PAGE_SSD_STATISTICS);
    put(QUERY_HEADER + 16, PROTOCOL_DATA_HEADER as u32);
    put(QUERY_HEADER + 20, ATA_PAGE_LEN as u32);
    buffer
}

/// Wear from the ATA Solid State Device Statistics page.
///
/// Each statistic is a 64-bit little-endian quadword whose top byte holds
/// flags; bit 63 "supported" and bit 62 "valid" must both be set, or the
/// value means nothing. Percentage Used Endurance Indicator is at offset 8.
/// Only life remaining exists on this page; the other fields stay absent.
pub(super) fn decode_ata_wear(bytes: &[u8]) -> Option<DriveHealth> {
    let data_offset = read_u32(bytes, QUERY_HEADER + 16)? as usize;
    let start = QUERY_HEADER.checked_add(data_offset)?;
    let page = bytes.get(start..start + ATA_PAGE_LEN)?;
    // Header: revision (u16), page number (byte 2) must be 07h.
    if page.get(2).copied()? != 0x07 {
        return None;
    }
    let field = page.get(8..16)?;
    let flags = field[7];
    if flags & 0xC0 != 0xC0 {
        return None;
    }
    let used = field[0];
    Some(DriveHealth {
        life_remaining: 100_u8.saturating_sub(used),
        ..DriveHealth::default()
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}

fn read_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    read_u16(bytes, offset).map(|value| i16::from_le_bytes(value.to_le_bytes()))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let quad = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

fn plausible(celsius: f32) -> Option<f32> {
    PLAUSIBLE_CELSIUS.contains(&celsius).then_some(celsius)
}

/// The first sensor of a `STORAGE_TEMPERATURE_DATA_DESCRIPTOR`, in °C.
///
/// Sensor 0 is the device's own composite reading; any further entries are
/// secondary sensors a caller would have to label, so they are not used.
pub(super) fn decode_temperature_descriptor(bytes: &[u8]) -> Option<f32> {
    let count = read_u16(bytes, TEMP_INFO_COUNT_OFFSET)?;
    if count == 0 {
        return None;
    }
    let celsius = read_i16(bytes, TEMP_INFO_OFFSET + 2)?;
    plausible(f32::from(celsius))
}

/// The composite temperature from an `NVMe` health log response, in °C.
///
/// The log reports kelvin; zero means the controller does not report it.
pub(super) fn decode_nvme_health(bytes: &[u8]) -> Option<f32> {
    let data_offset = read_u32(bytes, QUERY_HEADER + 16)? as usize;
    let start = QUERY_HEADER.checked_add(data_offset)?;
    let kelvin = read_u16(bytes, start + 1)?;
    if kelvin == 0 {
        return None;
    }
    plausible(f32::from(kelvin) - 273.0)
}

/// Little-endian unsigned of up to 16 bytes, saturated to `u64`. The SMART
/// counters are 128-bit; none will exceed 64 bits in a drive's lifetime, and
/// a corrupt high half saturates rather than wrapping to a small number.
fn read_u128_saturating(bytes: &[u8], offset: usize) -> Option<u64> {
    let field = bytes.get(offset..offset + 16)?;
    let (low, high) = field.split_at(8);
    let mut low_bytes = [0_u8; 8];
    low_bytes.copy_from_slice(low);
    if high.iter().any(|&b| b != 0) {
        return Some(u64::MAX);
    }
    Some(u64::from_le_bytes(low_bytes))
}

/// Wear and error figures from an `NVMe` health log response.
///
/// Offsets are from the `NVMe` base specification, SMART / Health
/// Information log (page 02h): Critical Warning at 0, Percentage Used at 5,
/// Data Units Written at 48, Power On Hours at 128, Media Errors at 160.
pub(super) fn decode_nvme_wear(bytes: &[u8]) -> Option<DriveHealth> {
    let data_offset = read_u32(bytes, QUERY_HEADER + 16)? as usize;
    let start = QUERY_HEADER.checked_add(data_offset)?;
    let page = bytes.get(start..start + NVME_LOG_LEN)?;
    // An all-zero page is a controller that answered without filling it.
    if page.iter().all(|&b| b == 0) {
        return None;
    }
    let used = page.get(5).copied()?;
    Some(DriveHealth {
        life_remaining: 100_u8.saturating_sub(used),
        power_on_hours: read_u128_saturating(page, 128),
        bytes_written: read_u128_saturating(page, 48).map(|units| units.saturating_mul(512_000)),
        media_errors: read_u128_saturating(page, 160),
        critical: page.first().is_some_and(|&w| w != 0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sizeof(STORAGE_TEMPERATURE_INFO)`.
    const TEMP_INFO_SIZE: usize = 16;

    fn descriptor(count: u16, celsius: i16) -> Vec<u8> {
        let mut bytes = vec![0_u8; TEMP_INFO_OFFSET + TEMP_INFO_SIZE];
        bytes[TEMP_INFO_COUNT_OFFSET..TEMP_INFO_COUNT_OFFSET + 2]
            .copy_from_slice(&count.to_le_bytes());
        bytes[TEMP_INFO_OFFSET + 2..TEMP_INFO_OFFSET + 4].copy_from_slice(&celsius.to_le_bytes());
        bytes
    }

    fn nvme_response(kelvin: u16) -> Vec<u8> {
        let mut bytes = nvme_health_request().to_vec();
        let start = QUERY_HEADER + PROTOCOL_DATA_HEADER;
        bytes[start + 1..start + 3].copy_from_slice(&kelvin.to_le_bytes());
        bytes
    }

    #[test]
    fn a_sata_descriptor_reports_its_first_sensor_in_celsius() {
        assert_eq!(
            decode_temperature_descriptor(&descriptor(1, 40)),
            Some(40.0)
        );
    }

    #[test]
    fn a_descriptor_with_no_sensors_is_absent_not_zero() {
        assert_eq!(decode_temperature_descriptor(&descriptor(0, 0)), None);
    }

    #[test]
    fn a_truncated_descriptor_is_absent() {
        assert_eq!(
            decode_temperature_descriptor(&descriptor(1, 40)[..20]),
            None
        );
    }

    #[test]
    fn nvme_composite_temperature_is_kelvin_minus_273() {
        assert_eq!(decode_nvme_health(&nvme_response(318)), Some(45.0));
    }

    #[test]
    fn an_nvme_drive_reporting_zero_kelvin_has_no_temperature() {
        assert_eq!(decode_nvme_health(&nvme_response(0)), None);
    }

    #[test]
    fn the_nvme_request_points_the_driver_at_the_health_page() {
        let request = nvme_health_request();
        assert_eq!(read_u32(&request, 0), Some(PROPERTY_PROTOCOL_SPECIFIC));
        assert_eq!(read_u32(&request, 8), Some(PROTOCOL_NVME));
        assert_eq!(read_u32(&request, 12), Some(NVME_DATA_LOG_PAGE));
        assert_eq!(read_u32(&request, 16), Some(NVME_LOG_HEALTH));
        assert_eq!(read_u32(&request, 24), Some(40));
        assert_eq!(read_u32(&request, 28), Some(512));
        assert_eq!(request.len(), 560);
    }

    #[test]
    fn implausible_readings_are_absent_rather_than_shown() {
        assert_eq!(decode_temperature_descriptor(&descriptor(1, -273)), None);
        assert_eq!(decode_temperature_descriptor(&descriptor(1, 200)), None);
        // 0xFFFF K is an uninitialised field, not a drive at 65 000 °C.
        assert_eq!(decode_nvme_health(&nvme_response(u16::MAX)), None);
    }

    fn nvme_page(fill: impl Fn(&mut [u8])) -> Vec<u8> {
        let mut bytes = nvme_health_request().to_vec();
        let start = QUERY_HEADER + PROTOCOL_DATA_HEADER;
        fill(&mut bytes[start..start + NVME_LOG_LEN]);
        bytes
    }

    #[test]
    fn life_remaining_is_one_hundred_minus_percentage_used() {
        let bytes = nvme_page(|page| {
            page[1..3].copy_from_slice(&318_u16.to_le_bytes());
            page[5] = 7;
            page[48..56].copy_from_slice(&40_000_000_u64.to_le_bytes());
            page[128..136].copy_from_slice(&12_345_u64.to_le_bytes());
            page[160..168].copy_from_slice(&3_u64.to_le_bytes());
        });
        let wear = decode_nvme_wear(&bytes).expect("a filled page decodes");
        assert_eq!(wear.life_remaining, 93);
        assert_eq!(wear.power_on_hours, Some(12_345));
        // 40 M data units of 512 000 bytes = 20.48 TB.
        assert_eq!(wear.bytes_written, Some(40_000_000 * 512_000));
        assert_eq!(wear.media_errors, Some(3));
        assert!(!wear.critical);
    }

    fn ata_page(fill: impl Fn(&mut [u8])) -> Vec<u8> {
        let mut bytes = ata_statistics_request().to_vec();
        let start = QUERY_HEADER + PROTOCOL_DATA_HEADER;
        fill(&mut bytes[start..start + ATA_PAGE_LEN]);
        bytes
    }

    #[test]
    fn a_sata_ssd_reports_wear_and_nothing_it_did_not_measure() {
        let bytes = ata_page(|page| {
            page[2] = 0x07;
            page[8] = 12; // 12 % used
            page[15] = 0xC0; // supported + valid
        });
        let wear = decode_ata_wear(&bytes).expect("a valid statistic");
        assert_eq!(wear.life_remaining, 88);
        assert_eq!(wear.power_on_hours, None);
        assert_eq!(wear.bytes_written, None);
    }

    #[test]
    fn an_ata_statistic_not_flagged_valid_is_ignored() {
        let bytes = ata_page(|page| {
            page[2] = 0x07;
            page[8] = 12;
            page[15] = 0x80; // supported, but not valid
        });
        assert_eq!(decode_ata_wear(&bytes), None);
    }

    #[test]
    fn a_drive_past_its_rated_endurance_reads_zero_life_not_a_wrapped_number() {
        // Percentage Used may legitimately exceed 100 (up to 255).
        let bytes = nvme_page(|page| {
            page[1..3].copy_from_slice(&318_u16.to_le_bytes());
            page[5] = 140;
        });
        assert_eq!(decode_nvme_wear(&bytes).map(|w| w.life_remaining), Some(0));
    }

    #[test]
    fn any_critical_warning_bit_marks_the_drive_critical() {
        let bytes = nvme_page(|page| {
            page[0] = 0b0000_0100; // reliability degraded
            page[1..3].copy_from_slice(&318_u16.to_le_bytes());
        });
        assert_eq!(decode_nvme_wear(&bytes).map(|w| w.critical), Some(true));
    }

    #[test]
    fn an_empty_page_is_no_health_report_rather_than_a_new_drive() {
        // All zeros would otherwise read as "100 % life, 0 hours".
        assert_eq!(decode_nvme_wear(&nvme_page(|_| {})), None);
    }
}
