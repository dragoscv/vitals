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
        let mut query = [0_u8; 12];
        query[..4].copy_from_slice(&PROPERTY_TEMPERATURE.to_le_bytes());
        let mut out = [0_u8; 1024];
        if let Some(len) = self.query(&query, &mut out)
            && let Some(celsius) = decode_temperature_descriptor(&out[..len])
        {
            return Some(celsius);
        }

        let request = nvme_health_request();
        let mut out = [0_u8; NVME_BUFFER_LEN];
        let len = self.query(&request, &mut out)?;
        decode_nvme_health(&out[..len])
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
}
