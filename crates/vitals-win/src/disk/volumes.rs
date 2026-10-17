//! Volume enumeration: capacity, free space and device kind.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

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

/// How old a network drive's cached capacity may be before the sampler asks
/// for a fresh read. Free space on a share moves slowly; the cost of asking
/// is a round-trip that can take seconds when the server is asleep.
const REMOTE_REFRESH: Duration = Duration::from_secs(30);

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
///
/// Blocking: a network drive is queried in-line, and `GetDiskFreeSpaceExW`
/// against an unreachable server waits out the SMB timeout. Call it from a
/// worker thread; the sampler uses [`enumerate_volumes_cached`] instead.
#[must_use]
pub fn enumerate_volumes() -> Vec<VolumeInfo> {
    enumerate(RemotePolicy::Blocking)
}

/// Enumerates mounted volumes without ever waiting on the network.
///
/// Local volumes are read as in [`enumerate_volumes`]. A network drive is
/// served from a cache refreshed on a background thread: the sampler runs
/// every two seconds on the thread that feeds the UI, and one sleeping NAS
/// used to stall every frame for the length of an SMB timeout. A share not
/// yet read, or whose last read failed, is omitted — the same answer the
/// blocking path gives for a disconnected share — rather than shown as
/// zero bytes, which would be a measurement nobody took.
#[must_use]
pub fn enumerate_volumes_cached() -> Vec<VolumeInfo> {
    enumerate(RemotePolicy::Cached)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemotePolicy {
    Blocking,
    Cached,
}

fn enumerate(policy: RemotePolicy) -> Vec<VolumeInfo> {
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
        if let Some(volume) = query_volume(letter, DiskId(bit), policy) {
            out.push(volume);
        }
    }

    out
}

/// Root path in the form Win32 wants: trailing backslash, NUL-terminated.
fn root_of(letter: char) -> Vec<u16> {
    format!("{letter}:\\")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

/// Queries a single drive letter, returning `None` if it cannot be read.
fn query_volume(letter: char, id: DiskId, policy: RemotePolicy) -> Option<VolumeInfo> {
    let root = root_of(letter);

    // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
    // `GetDriveTypeW` reads the mount table only; it does not touch the
    // server behind a mapped drive, so it is safe on the hot path.
    let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };

    let kind = match drive_type {
        DRIVE_REMOVABLE => DiskKind::Removable,
        DRIVE_REMOTE => DiskKind::Network,
        DRIVE_CDROM => DiskKind::Optical,
        // A fixed disk could be an HDD, SSD or NVMe; the drive type does not
        // say, so the device itself is asked (bus type, then seek penalty).
        // Done here rather than in the sampler so the Storage screen, which
        // lists volumes directly, does not show "Unknown type" beside a drive
        // the dashboard calls NVMe — which is exactly what it did.
        DRIVE_FIXED | DRIVE_RAMDISK => super::device::refine_kind(letter, DiskKind::Unknown),
        // DRIVE_UNKNOWN / DRIVE_NO_ROOT_DIR: an empty card reader or a
        // stale mapping. Skipping keeps them out of the UI entirely, which
        // is better than a row reading "0 B of 0 B".
        _ => return None,
    };

    let capacity = if drive_type == DRIVE_REMOTE {
        match policy {
            RemotePolicy::Blocking => {
                let fresh = read_capacity(&root);
                remember_remote(letter, fresh.clone());
                fresh
            }
            RemotePolicy::Cached => cached_remote(letter),
        }
    } else {
        read_capacity(&root)
    }?;

    Some(VolumeInfo {
        id,
        mount: format!("{letter}:"),
        label: capacity.label,
        file_system: capacity.file_system,
        kind,
        total: Bytes(capacity.total),
        available: Bytes(capacity.available),
    })
}

/// What one capacity read found.
#[derive(Debug, Clone)]
struct Capacity {
    total: u64,
    available: u64,
    label: Option<String>,
    file_system: Option<String>,
}

/// Reads capacity and names. May block for seconds on a network drive.
fn read_capacity(root: &[u16]) -> Option<Capacity> {
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

    let (label, file_system) = query_volume_names(root);

    Some(Capacity {
        total,
        available: available_to_caller,
        label,
        file_system,
    })
}

/// One network drive's last read.
#[derive(Debug)]
struct RemoteEntry {
    /// `None` when the last read failed (server unreachable).
    capacity: Option<Capacity>,
    read_at: Instant,
    /// A refresh thread is running. Stops a hung server from collecting a
    /// new blocked thread every two seconds.
    refreshing: bool,
}

fn remote_cache() -> &'static Mutex<HashMap<char, RemoteEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<char, RemoteEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether a refresh should start for this entry now.
fn needs_refresh(entry: Option<&RemoteEntry>, now: Instant) -> bool {
    entry.is_none_or(|e| !e.refreshing && now.duration_since(e.read_at) >= REMOTE_REFRESH)
}

fn remember_remote(letter: char, capacity: Option<Capacity>) {
    // A poisoned lock only means another thread panicked mid-update of a
    // cache entry; the map itself is still a valid map, and losing network
    // drive figures over it would be worse than a stale entry.
    let mut cache = remote_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    cache.insert(
        letter,
        RemoteEntry {
            capacity,
            read_at: Instant::now(),
            refreshing: false,
        },
    );
}

/// Returns the cached read, starting a background refresh when it is stale.
fn cached_remote(letter: char) -> Option<Capacity> {
    let now = Instant::now();
    let mut cache = remote_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let entry = cache.get(&letter);
    let current = entry.and_then(|e| e.capacity.clone());

    if needs_refresh(entry, now) {
        let spawned = std::thread::Builder::new()
            .name(format!("vitals-volume-{letter}"))
            .spawn(move || remember_remote(letter, read_capacity(&root_of(letter))));
        if spawned.is_ok() {
            cache
                .entry(letter)
                .and_modify(|e| e.refreshing = true)
                .or_insert(RemoteEntry {
                    capacity: None,
                    read_at: now,
                    refreshing: true,
                });
        }
    }

    current
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
    fn the_system_drive_is_not_left_unknown() {
        // The Storage screen showed "Unknown type" for every drive because
        // only the sampler refined the kind; the volume list must carry it.
        // Not on a device that refuses to be classified (an Azure VM disk),
        // which truthfully has no kind to report.
        if !super::super::device::is_classifiable('C') {
            return;
        }
        let volumes = enumerate_volumes();
        let c = volumes.iter().find(|v| v.mount == "C:").expect("C:");
        assert_ne!(c.kind, DiskKind::Unknown, "C: kind was not refined");
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

    fn entry(age: Duration, refreshing: bool) -> RemoteEntry {
        RemoteEntry {
            capacity: None,
            read_at: Instant::now().checked_sub(age).unwrap_or_else(Instant::now),
            refreshing,
        }
    }

    #[test]
    fn a_network_drive_never_read_is_queued_for_a_background_read() {
        assert!(needs_refresh(None, Instant::now()));
    }

    #[test]
    fn a_hung_share_does_not_collect_a_new_blocked_thread_every_tick() {
        // The refresh is still in flight because the server has not
        // answered; a second one would block the same way.
        let stale_but_busy = entry(REMOTE_REFRESH * 10, true);
        assert!(!needs_refresh(Some(&stale_but_busy), Instant::now()));
    }

    #[test]
    fn a_fresh_read_is_reused_and_a_stale_one_is_refreshed() {
        assert!(!needs_refresh(
            Some(&entry(Duration::ZERO, false)),
            Instant::now()
        ));
        assert!(needs_refresh(
            Some(&entry(REMOTE_REFRESH + Duration::from_secs(1), false)),
            Instant::now()
        ));
    }

    #[test]
    fn the_cached_enumeration_agrees_with_the_blocking_one_on_local_drives() {
        // Only network drives take the cached path; a fixed disk must be
        // reported identically by both, or the dashboard and the Storage
        // screen would disagree about C:.
        let blocking = enumerate_volumes();
        let cached = enumerate_volumes_cached();
        let local = |v: &&VolumeInfo| v.kind != DiskKind::Network;
        let mounts = |list: &[VolumeInfo]| {
            list.iter()
                .filter(local)
                .map(|v| (v.mount.clone(), v.total.get()))
                .collect::<Vec<_>>()
        };
        assert_eq!(mounts(&blocking), mounts(&cached));
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
