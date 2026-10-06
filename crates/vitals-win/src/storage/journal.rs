//! What changed on a volume since a point in time, without elevation.
//!
//! NTFS keeps a change journal: one record per change, naming the file and
//! the folder it is in. `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL` reads it for a
//! normal user — names are left out, which does not matter here, because the
//! saved index only needs *which folders* changed, and every record carries
//! its folder's file reference.
//!
//! Measured on this machine (2026-09-29): both calls succeed unelevated on a
//! handle to `C:\` opened with backup semantics and no access rights; on the
//! volume device `\\.\C:` the query fails with `ERROR_INVALID_FUNCTION`, and
//! the privileged `FSCTL_READ_USN_JOURNAL` fails with access denied.

use std::collections::HashSet;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetVolumeInformationW,
};

use vitals_core::error::{Error, Result};

use super::ffi::{create_file_read, device_io_control};

const FSCTL_QUERY_USN_JOURNAL: u32 = 0x0009_00F4;
const FSCTL_READ_UNPRIVILEGED_USN_JOURNAL: u32 = 0x0009_03AB;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;

/// Reasons that can change what a directory listing reports. Everything
/// else — timestamps, security, object IDs, named streams — leaves every
/// size and name the walker reads as it was, and relisting for it would
/// throw the saving away on a machine where antivirus touches every file.
const SIZE_OR_NAME_REASONS: u32 = 0x0000_0001 // DATA_OVERWRITE
    | 0x0000_0002 // DATA_EXTEND
    | 0x0000_0004 // DATA_TRUNCATION
    | 0x0000_0100 // FILE_CREATE
    | 0x0000_0200 // FILE_DELETE
    | 0x0000_1000 // RENAME_OLD_NAME
    | 0x0000_2000 // RENAME_NEW_NAME
    | 0x0001_0000 // HARD_LINK_CHANGE
    | 0x0002_0000 // COMPRESSION_CHANGE
    | 0x0010_0000 // REPARSE_POINT_CHANGE
    | 0x0020_0000; // STREAM_CHANGE
pub const REASON_HARD_LINK_CHANGE: u32 = 0x0001_0000;

/// Where the journal stood at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkpoint {
    /// Identifies this instance of the journal. A journal deleted and
    /// recreated gets a new ID, and USNs from the old one mean nothing.
    pub journal_id: u64,
    /// The next USN to be written: every change after this moment has a
    /// USN at least this large.
    pub next_usn: i64,
    /// The oldest USN still in the journal. Older changes were discarded to
    /// keep the journal at its size limit.
    pub first_usn: i64,
    /// The volume's serial number, so an index is never applied to a
    /// different drive that got the same letter.
    pub volume_serial: u32,
}

/// A handle that closes itself.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a live handle from CreateFileW, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// `C:\` for any path on C:, as a NUL-terminated wide string.
fn root_of(path: &str) -> Option<Vec<u16>> {
    let bytes = path.as_bytes();
    (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':').then(|| {
        format!("{}:\\", char::from(bytes[0]).to_ascii_uppercase())
            .encode_utf16()
            .chain(Some(0))
            .collect()
    })
}

fn open_root(path: &str) -> Result<Handle> {
    let wide = root_of(path).ok_or_else(|| {
        Error::NotFound(format!("{path} is not on a lettered drive"))
    })?;
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    let handle = unsafe {
        create_file_read(
            wide.as_ptr(),
            // No access at all: the journal FSCTLs need only a handle on the
            // volume, and asking for more would fail on locked-down roots.
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_FLAG_BACKUP_SEMANTICS,
        )
    };
    match handle.filter(|h| *h != INVALID_HANDLE_VALUE && !h.is_null()) {
        Some(h) => Ok(Handle(h)),
        None => Err(os_error("open the drive root for its change journal")),
    }
}

fn os_error(context: &str) -> Error {
    // SAFETY: no arguments, no preconditions.
    let code = unsafe { GetLastError() };
    Error::Os {
        context: context.to_owned(),
        code: code.cast_signed(),
    }
}

/// The volume serial number of the drive holding `path`.
#[must_use]
pub fn volume_serial(path: &str) -> Option<u32> {
    let root = root_of(path)?;
    let mut serial = 0_u32;
    // SAFETY: `root` is NUL-terminated; every other buffer is omitted.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            &raw mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    (ok != 0).then_some(serial)
}

/// Where the journal of `path`'s drive stands now.
///
/// # Errors
///
/// [`Error::Os`] when the drive has no active journal (`FAT`, `exFAT`, or an
/// NTFS volume with it switched off) or cannot be opened.
pub fn checkpoint(path: &str) -> Result<Checkpoint> {
    let root = open_root(path)?;
    let mut data = [0_u8; 80];
    let mut returned = 0_u32;
    // SAFETY: `root` is live; `data` is a live 80-byte buffer, large enough
    // for USN_JOURNAL_DATA_V2.
    let ok = unsafe {
        device_io_control(
            root.0,
            FSCTL_QUERY_USN_JOURNAL,
            std::ptr::null(),
            0,
            data.as_mut_ptr().cast(),
            data.len() as u32,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || returned < 24 {
        return Err(os_error("query the change journal"));
    }
    let read = |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().unwrap_or([0; 8]));
    Ok(Checkpoint {
        journal_id: read(0),
        first_usn: read(8).cast_signed(),
        next_usn: read(16).cast_signed(),
        volume_serial: volume_serial(path).unwrap_or(0),
    })
}

/// What the journal says changed between a checkpoint and now.
#[derive(Debug, Default)]
pub struct Changes {
    /// File references of folders whose listing may differ.
    pub folders: HashSet<u64>,
    /// Files that gained or lost a name, whose other folders the journal
    /// does not say.
    pub relinked: HashSet<u64>,
    pub records: u64,
}

/// Why the journal cannot answer "what changed".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stale {
    /// Another drive, or the same letter on a reformatted volume.
    OtherVolume,
    /// The journal was deleted and recreated since.
    NewJournal,
    /// Changes since the checkpoint have already been discarded.
    Wrapped,
}

/// Whether `saved` can still be brought up to date from `now`.
///
/// # Errors
///
/// The reason it cannot.
pub fn usable(saved: &Checkpoint, now: &Checkpoint) -> std::result::Result<(), Stale> {
    if saved.volume_serial != now.volume_serial {
        return Err(Stale::OtherVolume);
    }
    if saved.journal_id != now.journal_id {
        return Err(Stale::NewJournal);
    }
    if saved.next_usn < now.first_usn {
        return Err(Stale::Wrapped);
    }
    Ok(())
}

/// Reads every change from `since` up to `until`.
///
/// # Errors
///
/// [`Error::Os`] when the journal cannot be read.
pub fn changes_since(path: &str, since: &Checkpoint, until: &Checkpoint) -> Result<Changes> {
    let root = open_root(path)?;
    let mut changes = Changes::default();
    let mut start = since.next_usn;
    let mut buffer = vec![0_u64; 64 * 1024 / 8];
    let byte_len = (buffer.len() * 8) as u32;

    while start < until.next_usn {
        let mut query = [0_u8; 48];
        query[0..8].copy_from_slice(&start.to_le_bytes());
        query[8..12].copy_from_slice(&SIZE_OR_NAME_REASONS.to_le_bytes());
        query[32..40].copy_from_slice(&since.journal_id.to_le_bytes());
        query[40..42].copy_from_slice(&2_u16.to_le_bytes());
        query[42..44].copy_from_slice(&2_u16.to_le_bytes());
        let mut returned = 0_u32;
        // SAFETY: `root` is live; `query` is a READ_USN_JOURNAL_DATA_V1 of
        // the size passed; `buffer` is an 8-byte-aligned live allocation of
        // `byte_len` bytes.
        let ok = unsafe {
            device_io_control(
                root.0,
                FSCTL_READ_UNPRIVILEGED_USN_JOURNAL,
                query.as_ptr().cast(),
                query.len() as u32,
                buffer.as_mut_ptr().cast(),
                byte_len,
                &raw mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(os_error("read the change journal"));
        }
        let bytes: Vec<u8> = buffer
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .take(returned as usize)
            .collect();
        let Some(next) = bytes.get(0..8).and_then(|b| b.try_into().ok()) else {
            break;
        };
        let next = i64::from_le_bytes(next);
        parse_records(&bytes[8..], &mut changes);
        // The driver says where to resume; no progress means the end.
        if next <= start {
            break;
        }
        start = next;
    }
    Ok(changes)
}

/// Folds a buffer of `USN_RECORD_V2`s into `changes`.
fn parse_records(bytes: &[u8], changes: &mut Changes) {
    let mut at = 0_usize;
    while let Some(len) = bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
    {
        let len = len as usize;
        if len < 60 || at + len > bytes.len() {
            break;
        }
        let record = &bytes[at..at + len];
        let field = |from: usize| u64::from_le_bytes(record[from..from + 8].try_into().unwrap_or([0; 8]));
        let word = |from: usize| u32::from_le_bytes(record[from..from + 4].try_into().unwrap_or([0; 4]));
        // Version 2 only: the offsets below are V2's.
        if record[4] == 2 {
            let file = field(8);
            let parent = field(16);
            let reason = word(40);
            let attributes = word(52);
            changes.records += 1;
            changes.folders.insert(parent);
            if reason & REASON_HARD_LINK_CHANGE != 0 && attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                changes.relinked.insert(file);
            }
        }
        at += len;
    }
}

/// Every folder that holds a name of the file with reference `file`, as
/// paths relative to the drive root (`\Windows\System32`).
///
/// For a file that gained or lost a name: its bytes are counted once, in
/// whichever folder the walk meets first, so every folder with a name of it
/// must be read again together.
///
/// # Errors
///
/// When the file cannot be opened — it may be gone, which is fine (the
/// folder it was removed from is in the journal already), or unreadable,
/// which the caller must treat as "cannot tell".
pub fn folders_of(path: &str, file: u64) -> Result<Vec<String>> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FileIdType, FindClose, FindFirstFileNameW,
        FindNextFileNameW, GetFinalPathNameByHandleW, OpenFileById,
    };
    let root = open_root(path)?;
    let descriptor = FILE_ID_DESCRIPTOR {
        dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32,
        Type: FileIdType,
        Anonymous: FILE_ID_DESCRIPTOR_0 {
            FileId: file.cast_signed(),
        },
    };
    // SAFETY: `root` is a live handle on the volume; `descriptor` is fully
    // initialised; no security attributes.
    let handle = unsafe {
        OpenFileById(
            root.0,
            &raw const descriptor,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            FILE_FLAG_BACKUP_SEMANTICS,
        )
    };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(os_error("open a relinked file by its ID"));
    }
    let file_handle = Handle(handle);
    let mut buf = vec![0_u16; 32 * 1024];
    // SAFETY: `file_handle` is live; `buf` is a live buffer of the length
    // passed. Flags 0: the normalised DOS path.
    let len = unsafe {
        GetFinalPathNameByHandleW(file_handle.0, buf.as_mut_ptr(), buf.len() as u32, 0)
    } as usize;
    drop(file_handle);
    if len == 0 || len >= buf.len() {
        return Err(os_error("name a relinked file"));
    }
    let mut name: Vec<u16> = buf[..len].to_vec();
    name.push(0);

    let mut out = Vec::new();
    let mut link = vec![0_u16; 32 * 1024];
    let mut link_len = link.len() as u32;
    // SAFETY: `name` is NUL-terminated; `link` is a live buffer of
    // `link_len` units.
    let find = unsafe { FindFirstFileNameW(name.as_ptr(), 0, &raw mut link_len, link.as_mut_ptr()) };
    if find == INVALID_HANDLE_VALUE || find.is_null() {
        return Err(os_error("list the names of a relinked file"));
    }
    loop {
        let units = (link_len as usize).saturating_sub(1).min(link.len());
        let full = String::from_utf16_lossy(&link[..units]);
        if let Some((folder, _)) = full.rsplit_once('\\') {
            out.push(folder.to_owned());
        }
        link_len = link.len() as u32;
        // SAFETY: `find` is a live find handle; `link` as above.
        if unsafe { FindNextFileNameW(find, &raw mut link_len, link.as_mut_ptr()) } == 0 {
            break;
        }
    }
    // SAFETY: `find` came from FindFirstFileNameW and is closed once.
    unsafe { FindClose(find) };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(file: u64, parent: u64, reason: u32, attributes: u32) -> Vec<u8> {
        let mut r = vec![0_u8; 64];
        r[0..4].copy_from_slice(&64_u32.to_le_bytes());
        r[4..6].copy_from_slice(&2_u16.to_le_bytes());
        r[8..16].copy_from_slice(&file.to_le_bytes());
        r[16..24].copy_from_slice(&parent.to_le_bytes());
        r[40..44].copy_from_slice(&reason.to_le_bytes());
        r[52..56].copy_from_slice(&attributes.to_le_bytes());
        r[58..60].copy_from_slice(&60_u16.to_le_bytes());
        r
    }

    #[test]
    fn every_change_marks_the_folder_it_happened_in() {
        let mut bytes = record(100, 10, 0x2, 0);
        bytes.extend(record(101, 11, 0x100, FILE_ATTRIBUTE_DIRECTORY));
        bytes.extend(record(102, 12, REASON_HARD_LINK_CHANGE, 0));
        bytes.extend(record(103, 12, REASON_HARD_LINK_CHANGE, FILE_ATTRIBUTE_DIRECTORY));
        let mut changes = Changes::default();
        parse_records(&bytes, &mut changes);
        assert_eq!(changes.records, 4);
        assert_eq!(changes.folders, HashSet::from([10, 11, 12]));
        assert_eq!(
            changes.relinked,
            HashSet::from([102]),
            "only a file's new name leaves its other folders unknown"
        );
    }

    #[test]
    fn a_malformed_record_ends_the_walk() {
        let mut bytes = record(1, 2, 1, 0);
        bytes.extend([0_u8; 64]);
        let mut changes = Changes::default();
        parse_records(&bytes, &mut changes);
        assert_eq!(changes.records, 1);
    }

    #[test]
    fn a_checkpoint_from_another_volume_or_journal_or_before_the_oldest_change_is_refused() {
        let saved = Checkpoint {
            journal_id: 7,
            next_usn: 1000,
            first_usn: 0,
            volume_serial: 42,
        };
        let now = Checkpoint {
            next_usn: 5000,
            first_usn: 500,
            ..saved
        };
        assert_eq!(usable(&saved, &now), Ok(()));
        assert_eq!(
            usable(&saved, &Checkpoint { volume_serial: 43, ..now }),
            Err(Stale::OtherVolume)
        );
        assert_eq!(
            usable(&saved, &Checkpoint { journal_id: 8, ..now }),
            Err(Stale::NewJournal)
        );
        assert_eq!(
            usable(&saved, &Checkpoint { first_usn: 1001, ..now }),
            Err(Stale::Wrapped)
        );
    }

    #[test]
    fn the_system_drive_journal_is_readable_without_elevation() {
        let Ok(drive) = std::env::var("SystemDrive") else {
            return;
        };
        let Ok(before) = checkpoint(&drive) else {
            // A machine with the journal off is allowed; the scan then walks.
            return;
        };
        assert_ne!(before.volume_serial, 0);
        let dir = std::env::temp_dir().join(format!("vitals-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("a.bin"), [1_u8; 100]).expect("write");
        let after = checkpoint(&drive).expect("still there");
        let changes = changes_since(&drive, &before, &after).expect("read");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(changes.records > 0, "the write must be in the journal");
    }
}
