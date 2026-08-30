//! Volume enumeration: capacity, free space and device kind.

use windows_sys::Win32::Foundation::{FALSE, MAX_PATH};
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};

use vitals_core::ids::DiskId;
use vitals_core::metrics::DiskKind;
use vitals_core::units::Bytes;

// `GetDriveTypeW` return values.
//
// Declared locally rather than pulled from a binding crate: `windows-sys`
// 0.61 does not export them at all, and reaching into `windows` for five
// integers would mean enabling a whole feature module for constants that
// have been fixed since Windows 95.
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

/// A mounted volume.
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub id: DiskId,
    /// Drive letter with colon, e.g. `C:`.
    pub mount: String,
    /// User-assigned volume label, if any.
    pub label: Option<String>,
    /// `NTFS`, `exFAT`, …
    pub file_system: Option<String>,
    pub kind: DiskKind,
    pub total: Bytes,
    /// Free space available to the calling user.
    ///
    /// Distinct from total free space when disk quotas are in force: a user
    /// under quota may see far less than the volume actually has spare, and
    /// reporting the larger figure would promise space they cannot use.
    pub available: Bytes,
}

impl VolumeInfo {
    #[must_use]
    pub fn used(&self) -> Bytes {
        Bytes(self.total.get().saturating_sub(self.available.get()))
    }
}

/// Enumerates mounted volumes.
///
/// Never fails as a whole: a volume that cannot be queried — an empty card
/// reader, a disconnected network share — is skipped rather than aborting the
/// enumeration and hiding every other disk.
#[must_use]
pub fn enumerate_volumes() -> Vec<VolumeInfo> {
    // SAFETY: no arguments, no preconditions.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(mask.count_ones() as usize);

    for bit in 0..26_u32 {
        if mask & (1 << bit) == 0 {
            continue;
        }

        let letter = char::from(b'A' + u8::try_from(bit).unwrap_or(0));
        if let Some(volume) = query_volume(letter, DiskId(bit)) {
            out.push(volume);
        }
    }

    out
}

/// Queries a single drive letter, returning `None` if it cannot be read.
fn query_volume(letter: char, id: DiskId) -> Option<VolumeInfo> {
    // Win32 wants a trailing backslash on the root path.
    let root: Vec<u16> = format!("{letter}:\\")
        .encode_utf16()
        .chain(Some(0))
        .collect();

    // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
    let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };

    let kind = match drive_type {
        DRIVE_REMOVABLE => DiskKind::Removable,
        DRIVE_REMOTE => DiskKind::Network,
        DRIVE_CDROM => DiskKind::Optical,
        // A fixed disk could be an HDD, SSD or NVMe; the drive type does not
        // say. The physical-disk layer refines this later via a seek-penalty
        // query. A RAM disk is likewise indistinguishable here.
        DRIVE_FIXED | DRIVE_RAMDISK => DiskKind::Unknown,
        // DRIVE_UNKNOWN / DRIVE_NO_ROOT_DIR: an empty card reader or a
        // stale mapping. Skipping keeps them out of the UI entirely, which
        // is better than a row reading "0 B of 0 B".
        _ => return None,
    };

    let mut available_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;

    // SAFETY: `root` is valid and NUL-terminated; the three out-pointers
    // reference live u64s.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            root.as_ptr(),
            &raw mut available_to_caller,
            &raw mut total,
            &raw mut total_free,
        )
    };

    // A removable drive with no medium fails here. Correct to skip.
    if ok == FALSE || total == 0 {
        return None;
    }

    let (label, file_system) = query_volume_names(&root);

    Some(VolumeInfo {
        id,
        mount: format!("{letter}:"),
        label,
        file_system,
        kind,
        total: Bytes(total),
        available: Bytes(available_to_caller),
    })
}

/// Reads the volume label and file system name.
fn query_volume_names(root: &[u16]) -> (Option<String>, Option<String>) {
    let mut label = [0_u16; MAX_PATH as usize + 1];
    let mut fs = [0_u16; MAX_PATH as usize + 1];

    // SAFETY: both buffers are MAX_PATH+1 elements and we pass those exact
    // lengths; `root` is a valid NUL-terminated string.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            label.as_mut_ptr(),
            u32::try_from(label.len()).unwrap_or(0),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs.as_mut_ptr(),
            u32::try_from(fs.len()).unwrap_or(0),
        )
    };

    if ok == FALSE {
        return (None, None);
    }

    (from_wide(&label), from_wide(&fs))
}

/// Converts a NUL-terminated UTF-16 buffer into a `String`.
///
/// Returns `None` for an empty result: an unlabelled volume is normal, and an
/// empty string in the UI is worse than no label at all.
fn from_wide(buffer: &[u16]) -> Option<String> {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    if end == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_at_least_one_volume() {
        let volumes = enumerate_volumes();
        assert!(
            !volumes.is_empty(),
            "a running Windows machine has a system volume"
        );
    }

    #[test]
    fn finds_the_system_drive() {
        let volumes = enumerate_volumes();
        assert!(
            volumes.iter().any(|v| v.mount == "C:"),
            "C: was not enumerated; got {:?}",
            volumes.iter().map(|v| &v.mount).collect::<Vec<_>>()
        );
    }

    #[test]
    fn capacities_are_plausible() {
        for v in enumerate_volumes() {
            assert!(v.total.get() > 0, "{} reported zero capacity", v.mount);
            assert!(
                v.available.get() <= v.total.get(),
                "{} reported more free ({}) than total ({})",
                v.mount,
                v.available,
                v.total
            );
        }
    }

    #[test]
    fn used_space_is_the_complement_of_available() {
        for v in enumerate_volumes() {
            assert_eq!(v.used().get() + v.available.get(), v.total.get());
        }
    }

    #[test]
    fn the_system_drive_reports_a_file_system() {
        let volumes = enumerate_volumes();
        let c = volumes.iter().find(|v| v.mount == "C:").expect("C:");
        assert!(c.file_system.is_some(), "C: reported no file system");
    }

    #[test]
    fn volume_ids_are_unique() {
        let volumes = enumerate_volumes();
        let mut ids: Vec<_> = volumes.iter().map(|v| v.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate DiskId across volumes");
    }

    #[test]
    fn empty_label_is_none_not_an_empty_string() {
        // An unlabelled volume is normal; the UI should fall back to the
        // mount point rather than render a blank cell.
        for v in enumerate_volumes() {
            if let Some(label) = &v.label {
                assert!(
                    !label.is_empty(),
                    "{} produced an empty label string",
                    v.mount
                );
            }
        }
    }

    #[test]
    fn from_wide_handles_an_empty_buffer() {
        assert!(from_wide(&[0]).is_none());
        assert!(from_wide(&[]).is_none());
    }

    #[test]
    fn from_wide_stops_at_the_terminator() {
        // Trailing garbage after the NUL must not leak into the name.
        let buffer = [
            u16::from(b'N'),
            u16::from(b'T'),
            u16::from(b'F'),
            u16::from(b'S'),
            0,
            u16::from(b'X'),
        ];
        assert_eq!(from_wide(&buffer).as_deref(), Some("NTFS"));
    }
}
