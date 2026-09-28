//! Per-volume device queries: activity counters and media kind.
//!
//! Both go through a volume handle opened with zero access rights. Neither
//! ioctl needs read access — `IOCTL_DISK_PERFORMANCE` and
//! `IOCTL_STORAGE_QUERY_PROPERTY` are both `FILE_ANY_ACCESS` — and asking for
//! read access would demand administrator rights for no benefit. That is why
//! the dashboard's disk figures were zero for so long: the counter arithmetic
//! in `rate.rs` existed, but nothing ever fed it.

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    BusTypeFileBackedVirtual, BusTypeNvme, BusTypeSd, BusTypeSpaces, BusTypeUsb, BusTypeVirtual,
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, DISK_PERFORMANCE, IOCTL_DISK_PERFORMANCE,
    IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR,
    STORAGE_PROPERTY_QUERY, StorageDeviceProperty, StorageDeviceSeekPenaltyProperty,
};

use vitals_core::metrics::DiskKind;

use super::rate::DiskCounters;

/// An open, zero-access handle to `\\.\X:`, closed on drop.
struct VolumeHandle(HANDLE);

impl VolumeHandle {
    fn open(letter: char) -> Option<Self> {
        let path: Vec<u16> = format!("\\\\.\\{letter}:")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: `path` is NUL-terminated and outlives the call; a null
        // security descriptor and template handle are permitted.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(Self(handle))
    }

    /// Issues an ioctl whose output is a single fixed-size struct.
    fn query<I, O: Default>(&self, code: u32, input: Option<&I>) -> Option<O> {
        let mut out = O::default();
        let mut returned = 0_u32;
        let (in_ptr, in_len) = input.map_or((std::ptr::null(), 0), |i| {
            (
                std::ptr::from_ref(i).cast::<core::ffi::c_void>(),
                u32::try_from(size_of::<I>()).unwrap_or(0),
            )
        });
        // SAFETY: the handle is open; `out` is a live `O` and its exact size
        // is passed; the input pointer, when present, is a live `I`.
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                in_ptr,
                in_len,
                std::ptr::from_mut(&mut out).cast(),
                u32::try_from(size_of::<O>()).unwrap_or(0),
                &raw mut returned,
                std::ptr::null_mut(),
            )
        };
        (ok != FALSE && returned > 0).then_some(out)
    }
}

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by CreateFileW and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Reads the cumulative activity counters of one volume.
///
/// `None` when the volume will not answer — a network share, or a machine
/// where `diskperf -n` has switched the counters off. That becomes an absent
/// reading upstream, never a zero.
#[must_use]
pub fn volume_counters(letter: char) -> Option<DiskCounters> {
    let handle = VolumeHandle::open(letter)?;
    let perf: DISK_PERFORMANCE = handle.query::<(), _>(IOCTL_DISK_PERFORMANCE, None)?;
    Some(DiskCounters {
        bytes_read: u64::try_from(perf.BytesRead).unwrap_or(0),
        bytes_written: u64::try_from(perf.BytesWritten).unwrap_or(0),
        read_count: u64::from(perf.ReadCount),
        write_count: u64::from(perf.WriteCount),
        read_time: u64::try_from(perf.ReadTime).unwrap_or(0),
        write_time: u64::try_from(perf.WriteTime).unwrap_or(0),
        idle_time: u64::try_from(perf.IdleTime).unwrap_or(0),
        query_time: u64::try_from(perf.QueryTime).unwrap_or(0),
    })
}

/// Works out whether a fixed volume sits on `NVMe`, an SSD or a spinning disk.
///
/// Bus type first: `NVMe` is unambiguous there, and USB or SD media are
/// removable in every sense a user cares about even when Windows calls them
/// fixed. Otherwise the seek-penalty property separates rotational from
/// solid-state media. Returns `fallback` when the device answers neither,
/// which is the case for virtual disks and Storage Spaces.
#[must_use]
pub fn refine_kind(letter: char, fallback: DiskKind) -> DiskKind {
    let Some(handle) = VolumeHandle::open(letter) else {
        return fallback;
    };

    let device_query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    if let Some(device) = handle
        .query::<_, STORAGE_DEVICE_DESCRIPTOR>(IOCTL_STORAGE_QUERY_PROPERTY, Some(&device_query))
    {
        // Compared, not matched: the binding names are Win32's CamelCase
        // constants, which a pattern would read as a fresh binding lint.
        let bus = device.BusType;
        if bus == BusTypeNvme {
            return DiskKind::Nvme;
        }
        if bus == BusTypeUsb || bus == BusTypeSd {
            return DiskKind::Removable;
        }
    }

    let seek_query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    handle
        .query::<_, DEVICE_SEEK_PENALTY_DESCRIPTOR>(IOCTL_STORAGE_QUERY_PROPERTY, Some(&seek_query))
        .map_or(fallback, |seek| {
            if seek.IncursSeekPenalty {
                DiskKind::Hdd
            } else {
                DiskKind::Ssd
            }
        })
}

/// Whether the volume sits on a virtual, file-backed or Storage Spaces bus —
/// the devices that answer neither classification query, so `Unknown` is
/// the truthful kind for them rather than a failure to look.
#[must_use]
pub fn is_virtual_bus(letter: char) -> bool {
    let Some(handle) = VolumeHandle::open(letter) else {
        return false;
    };
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    handle
        .query::<_, STORAGE_DEVICE_DESCRIPTOR>(IOCTL_STORAGE_QUERY_PROPERTY, Some(&query))
        .is_some_and(|d| {
            let bus = d.BusType;
            bus == BusTypeVirtual || bus == BusTypeFileBackedVirtual || bus == BusTypeSpaces
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_volume_reports_activity_counters() {
        // Every Windows install since 8 has disk performance counters on by
        // default; if this fails, the dashboard's disk rates are fiction.
        let counters = volume_counters('C').expect("C: answered IOCTL_DISK_PERFORMANCE");
        assert!(
            counters.bytes_read > 0,
            "a booted system volume has been read from"
        );
    }

    #[test]
    fn the_system_volume_is_classified_rather_than_left_unknown() {
        // A hypervisor's virtual disk (CI runners, most VMs) truthfully
        // answers neither query; the guarantee is about physical media.
        if is_virtual_bus('C') {
            return;
        }
        let kind = refine_kind('C', DiskKind::Unknown);
        assert_ne!(
            kind,
            DiskKind::Unknown,
            "C: answered neither the bus-type nor the seek-penalty query"
        );
    }

    #[test]
    fn a_letter_with_no_volume_answers_nothing() {
        // No machine has every letter mounted; walk from the end to find a gap.
        let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
        let free = (0..26_u8).rev().find(|bit| mask & (1 << bit) == 0);
        if let Some(bit) = free {
            let letter = char::from(b'A' + bit);
            assert!(volume_counters(letter).is_none());
            assert_eq!(refine_kind(letter, DiskKind::Unknown), DiskKind::Unknown);
        }
    }
}
