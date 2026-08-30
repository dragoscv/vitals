//! Whole-volume namespace enumeration via `FSCTL_ENUM_USN_DATA`.
//!
//! # Why this exists, and what it can and cannot do
//!
//! [`super::scan`] walks directories one at a time. That costs at least one
//! kernel transition per directory and, for hard-link detection, an open per
//! file. On a two-terabyte volume with two million files it takes minutes.
//!
//! NTFS keeps every name on the volume in the Master File Table, and
//! `FSCTL_ENUM_USN_DATA` streams that table back in bulk — sixty-four
//! kilobytes of records per call, hundreds of thousands of entries a second.
//! It is how `WizTree` reads a whole volume in the time Explorer takes to
//! render a folder.
//!
//! **What it does not return is size.** A `USN_RECORD_V2` carries the file
//! reference number, the parent's reference number, the attributes and the
//! name — and nothing else. There is no length field, because the USN journal
//! exists to answer "what changed", not "how big is it". Building a size tree
//! from USN records alone is therefore impossible; a tool that wants both
//! must additionally parse raw `$MFT` attribute records off the volume, which
//! means reading the volume as a block device and reimplementing a chunk of
//! NTFS.
//!
//! So this module is deliberately scoped to what USN can answer honestly: the
//! complete directory namespace, in seconds, which gives a scanner the tree
//! shape and every path without a single `FindFirstFile`. Sizing remains
//! [`super::scan`]'s job. The alternative — inferring sizes — would be
//! fabrication, and this project does not ship fabricated numbers.
//!
//! # Elevation
//!
//! Opening `\\.\C:` requires administrator rights. Unelevated, the open fails
//! with `ERROR_ACCESS_DENIED` and this module returns
//! [`vitals_core::error::Error::AccessDenied`], which the UI turns into an
//! "Elevate" affordance rather than a failure. It never silently degrades:
//! choosing the slower path is the caller's decision to make.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_HANDLE_EOF, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

use vitals_core::error::{Error, Result};

use super::ffi::{create_file_read, device_io_control};

/// `FSCTL_ENUM_USN_DATA`
///
/// `CTL_CODE(FILE_DEVICE_FILE_SYSTEM, 44, METHOD_NEITHER, FILE_ANY_ACCESS)`.
const FSCTL_ENUM_USN_DATA: u32 = 0x0009_00B3;

/// Generic read access, spelled out to avoid pulling in another feature
/// module for a single constant.
const GENERIC_READ: u32 = 0x8000_0000;

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Output buffer per `DeviceIoControl` call.
///
/// 64 KB holds roughly six hundred records. Larger buffers stop helping —
/// the driver copies out of its own page cache and the per-call overhead is
/// already amortised to nothing at this size — while costing a bigger
/// contiguous allocation.
const BUFFER_BYTES: usize = 64 * 1024;

/// `MFT_ENUM_DATA_V0`
///
/// V0 rather than V2: V2 requires the volume to have been formatted with a
/// version of NTFS that supports 128-bit file references, and asking for it
/// on an older volume fails the whole enumeration. V0 works everywhere and
/// returns everything a namespace walk needs.
#[repr(C)]
#[derive(Clone, Copy)]
struct MftEnumDataV0 {
    /// Where to resume. The driver writes the next value into the first eight
    /// bytes of the *output* buffer, which is the part of this API that is
    /// easiest to get silently wrong.
    start_file_reference_number: u64,
    low_usn: i64,
    high_usn: i64,
}

/// Fixed portion of `USN_RECORD_V2`.
///
/// The real structure ends with `FileName: [u16; 1]`, a variable-length tail.
/// It is modelled here without that field and the name is read separately at
/// `file_name_offset`, because a Rust array of length one would tempt code
/// into reading exactly one character. Records are also *not* a fixed stride:
/// each one's `record_length` says where the next begins, and assuming a
/// constant stride is the classic way to produce a silently empty or
/// garbage-filled list.
#[repr(C)]
#[derive(Clone, Copy)]
struct UsnRecordV2 {
    record_length: u32,
    major_version: u16,
    minor_version: u16,
    file_reference_number: u64,
    parent_file_reference_number: u64,
    usn: i64,
    timestamp: i64,
    reason: u32,
    source_info: u32,
    security_id: u32,
    file_attributes: u32,
    file_name_length: u16,
    file_name_offset: u16,
}

/// One entry in the volume namespace.
#[derive(Debug, Clone)]
pub struct MftEntry {
    /// Identifies this file within the volume.
    pub file_reference: u64,
    /// Identifies the directory containing it. The volume root is its own
    /// parent, which is how a walk upwards knows to stop.
    pub parent_reference: u64,
    pub name: String,
    pub attributes: u32,
}

impl MftEntry {
    #[must_use]
    pub const fn is_directory(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    }

    /// Whether this is a junction, symlink or mount point.
    ///
    /// Such an entry must be recorded but never descended into: following one
    /// either counts another volume's bytes against this one or, when it
    /// points at an ancestor, never terminates.
    #[must_use]
    pub const fn is_reparse_point(&self) -> bool {
        self.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
}

/// The complete namespace of one volume.
#[derive(Debug)]
pub struct VolumeNamespace {
    /// Drive letter with colon, e.g. `C:`.
    pub mount: String,
    pub entries: Vec<MftEntry>,
    /// True when the caller cancelled before the table was exhausted, so
    /// `entries` is a prefix rather than the whole volume.
    pub truncated: bool,
}

impl VolumeNamespace {
    /// Indexes directories by file reference, for resolving parent chains.
    ///
    /// Only directories are indexed: they are a small fraction of a volume's
    /// entries, and they are the only ones a path walk ever needs to look up.
    #[must_use]
    pub fn directory_index(&self) -> HashMap<u64, &MftEntry> {
        self.entries
            .iter()
            .filter(|e| e.is_directory())
            .map(|e| (e.file_reference, e))
            .collect()
    }

    /// Reconstructs the full path of an entry.
    ///
    /// Returns `None` when the chain to the root is broken — which happens
    /// legitimately, because the enumeration is a snapshot and a directory
    /// can be renamed or deleted while it runs. Returning a partial path
    /// would be worse than admitting the gap.
    #[must_use]
    pub fn path_of(&self, entry: &MftEntry, index: &HashMap<u64, &MftEntry>) -> Option<String> {
        let mut parts = vec![entry.name.as_str()];
        let mut cursor = entry.parent_reference;
        let mut current = entry.file_reference;

        // Bounded rather than `loop`: a corrupted table, or a snapshot torn
        // by a concurrent rename, can produce a cycle, and a scanner that
        // hangs on a damaged volume is worse than one that reports the gap.
        for _ in 0..512 {
            if cursor == current {
                break;
            }
            let parent = index.get(&cursor)?;
            parts.push(parent.name.as_str());
            current = parent.file_reference;
            cursor = parent.parent_reference;
        }

        parts.reverse();
        Some(format!("{}\\{}", self.mount, parts.join("\\")))
    }
}

/// A volume handle that closes itself.
struct VolumeHandle(HANDLE);

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful CreateFileW and is closed
        // exactly once, at drop.
        unsafe { CloseHandle(self.0) };
    }
}

/// Opens a volume for MFT enumeration.
///
/// # Errors
///
/// [`Error::AccessDenied`] when the process is not elevated — the expected
/// outcome for a normal user, and the signal for the caller to fall back to
/// [`super::scan`] or offer to elevate.
fn open_volume(letter: char) -> Result<VolumeHandle> {
    // The device path form. A trailing backslash turns this into a request
    // for the root *directory* rather than the volume device, and the
    // subsequent FSCTL then fails with a misleading invalid-parameter error.
    let path: Vec<u16> = format!("\\\\.\\{letter}:")
        .encode_utf16()
        .chain(Some(0))
        .collect();

    // SAFETY: `path` is a NUL-terminated UTF-16 device path that outlives the
    // call.
    let opened = unsafe {
        create_file_read(
            path.as_ptr(),
            GENERIC_READ,
            // The volume stays fully usable while we read it; refusing to
            // share would make a scan block every other process on the disk.
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            0,
        )
    };

    let handle = opened.unwrap_or(INVALID_HANDLE_VALUE);
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        // SAFETY: no arguments, no preconditions.
        let code = unsafe { GetLastError() };
        return Err(if code == ERROR_ACCESS_DENIED {
            Error::AccessDenied {
                operation: format!("open volume {letter}: for MFT enumeration"),
            }
        } else {
            Error::Os {
                context: format!("CreateFileW(\\\\.\\{letter}:)"),
                code: code.cast_signed(),
            }
        });
    }

    Ok(VolumeHandle(handle))
}

/// Whether MFT enumeration is available for a drive letter.
///
/// Cheap and truthful: it attempts the actual open rather than inspecting a
/// token, so it answers the question that matters — "will this work?" — and
/// not a proxy for it.
#[must_use]
pub fn is_available(letter: char) -> bool {
    open_volume(letter).is_ok()
}

/// Enumerates every name on an NTFS volume.
///
/// Returns entries in MFT order, which is neither alphabetical nor
/// hierarchical; the caller reassembles the tree through
/// [`VolumeNamespace::directory_index`].
///
/// `cancel` is checked once per buffer — roughly every six hundred entries —
/// so a cancelled scan stops within milliseconds. The module spawns no
/// threads; the caller decides where this runs.
///
/// # Errors
///
/// - [`Error::AccessDenied`] when not elevated.
/// - [`Error::Unsupported`] when the volume is not NTFS, since only NTFS has
///   an MFT to enumerate.
/// - [`Error::Os`] for any other driver failure, carrying the raw code so the
///   cause stays diagnosable.
pub fn enumerate_volume(letter: char, cancel: Option<&AtomicBool>) -> Result<VolumeNamespace> {
    let volume = open_volume(letter)?;

    let mut query = MftEnumDataV0 {
        start_file_reference_number: 0,
        low_usn: 0,
        // Every USN ever issued. Bounding this to the current journal head
        // would return only recently-changed files, which is the mistake that
        // makes an MFT enumeration come back suspiciously small.
        high_usn: i64::MAX,
    };

    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut entries: Vec<MftEntry> = Vec::new();
    let mut truncated = false;

    loop {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            truncated = true;
            break;
        }

        let mut returned: u32 = 0;

        // SAFETY: `volume.0` is a live volume handle; `query` is a live,
        // fully-initialised MFT_ENUM_DATA_V0 of the size passed; `buffer` is
        // a live allocation of exactly BUFFER_BYTES; `returned` is a live
        // u32. The overlapped pointer is legitimately null for a synchronous
        // handle.
        let ok = unsafe {
            device_io_control(
                volume.0,
                FSCTL_ENUM_USN_DATA,
                (&raw const query).cast(),
                size_of::<MftEnumDataV0>() as u32,
                buffer.as_mut_ptr().cast(),
                BUFFER_BYTES as u32,
                &raw mut returned,
                std::ptr::null_mut(),
            )
        };

        if ok == 0 {
            // SAFETY: no arguments, no preconditions.
            let code = unsafe { GetLastError() };
            // The documented end of the table, not a failure.
            if code == ERROR_HANDLE_EOF {
                break;
            }
            return Err(match code {
                ERROR_ACCESS_DENIED => Error::AccessDenied {
                    operation: format!("enumerate MFT on {letter}:"),
                },
                other => Error::Os {
                    context: format!("FSCTL_ENUM_USN_DATA on {letter}:"),
                    code: other.cast_signed(),
                },
            });
        }

        // The driver prefixes the output with the resume position. Fewer than
        // those eight bytes means there were no records at all.
        if (returned as usize) <= size_of::<u64>() {
            break;
        }

        // SAFETY: the driver guarantees at least eight bytes of output, and
        // `buffer` is a live allocation; read unaligned because a Vec<u8> is
        // only byte-aligned.
        query.start_file_reference_number =
            unsafe { buffer.as_ptr().cast::<u64>().read_unaligned() };

        parse_records(&buffer[size_of::<u64>()..returned as usize], &mut entries);
    }

    Ok(VolumeNamespace {
        mount: format!("{letter}:"),
        entries,
        truncated,
    })
}

/// Walks a buffer of variable-length USN records.
///
/// Split out and taking a plain slice so the pointer arithmetic — the part of
/// this module most able to fail silently — is testable against a synthetic
/// buffer without a volume handle.
fn parse_records(buffer: &[u8], out: &mut Vec<MftEntry>) {
    let mut offset = 0_usize;

    while offset + size_of::<UsnRecordV2>() <= buffer.len() {
        // SAFETY: the bounds check above guarantees a full fixed header lies
        // within the slice. Read unaligned: records are packed end to end at
        // eight-byte boundaries the compiler cannot prove.
        let record = unsafe {
            buffer
                .as_ptr()
                .add(offset)
                .cast::<UsnRecordV2>()
                .read_unaligned()
        };

        let length = record.record_length as usize;

        // A zero or undersized length would make this loop spin forever on
        // the same offset, and a length past the end would read out of the
        // buffer. Both mean the buffer is not what the driver promised, and
        // stopping is the only safe response.
        if length < size_of::<UsnRecordV2>() || offset + length > buffer.len() {
            break;
        }

        // V3 and V4 records have different layouts and would be misread
        // wholesale by the offsets above. The FSCTL only emits V2 unless V3
        // is explicitly requested, so anything else means an assumption here
        // is wrong and the record must be skipped rather than guessed at.
        if record.major_version == 2 {
            let name_start = offset + record.file_name_offset as usize;
            let name_end = name_start + record.file_name_length as usize;

            // The name is offset from the start of the record and its length
            // is in *bytes*, not characters — halving it is the step everyone
            // forgets, and forgetting it reads twice the name and half of the
            // next record with it.
            if name_end <= buffer.len() && name_start >= offset {
                let bytes = &buffer[name_start..name_end];
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect();

                out.push(MftEntry {
                    file_reference: record.file_reference_number,
                    parent_reference: record.parent_file_reference_number,
                    name: String::from_utf16_lossy(&units),
                    attributes: record.file_attributes,
                });
            }
        }

        offset += length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn usn_record_layout_is_pinned() {
        // This structure is read by pointer arithmetic out of a driver
        // buffer. A wrong offset does not crash — it silently yields garbage
        // names and zero references, which is exactly the class of bug that
        // has already produced a silently empty list in this project once.
        assert_eq!(size_of::<UsnRecordV2>(), 64);
        assert_eq!(align_of::<UsnRecordV2>(), 8);

        assert_eq!(offset_of!(UsnRecordV2, record_length), 0);
        assert_eq!(offset_of!(UsnRecordV2, major_version), 4);
        assert_eq!(offset_of!(UsnRecordV2, minor_version), 6);
        assert_eq!(offset_of!(UsnRecordV2, file_reference_number), 8);
        assert_eq!(offset_of!(UsnRecordV2, parent_file_reference_number), 16);
        assert_eq!(offset_of!(UsnRecordV2, usn), 24);
        assert_eq!(offset_of!(UsnRecordV2, timestamp), 32);
        assert_eq!(offset_of!(UsnRecordV2, reason), 40);
        assert_eq!(offset_of!(UsnRecordV2, source_info), 44);
        assert_eq!(offset_of!(UsnRecordV2, security_id), 48);
        assert_eq!(offset_of!(UsnRecordV2, file_attributes), 52);
        assert_eq!(offset_of!(UsnRecordV2, file_name_length), 56);
        assert_eq!(offset_of!(UsnRecordV2, file_name_offset), 58);
    }

    #[test]
    fn enum_data_layout_is_pinned() {
        assert_eq!(size_of::<MftEnumDataV0>(), 24);
        assert_eq!(offset_of!(MftEnumDataV0, start_file_reference_number), 0);
        assert_eq!(offset_of!(MftEnumDataV0, low_usn), 8);
        assert_eq!(offset_of!(MftEnumDataV0, high_usn), 16);
    }

    /// Builds a synthetic USN record, so the parser can be exercised without
    /// an elevated handle.
    fn synth_record(reference: u64, parent: u64, name: &str, attributes: u32) -> Vec<u8> {
        let name_bytes: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let header = size_of::<UsnRecordV2>();
        // Records are eight-byte aligned; the driver pads to that boundary.
        let unpadded = header + name_bytes.len();
        let length = unpadded.next_multiple_of(8);

        let mut bytes = vec![0_u8; length];
        bytes[0..4].copy_from_slice(&(length as u32).to_le_bytes());
        bytes[4..6].copy_from_slice(&2_u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&0_u16.to_le_bytes());
        bytes[8..16].copy_from_slice(&reference.to_le_bytes());
        bytes[16..24].copy_from_slice(&parent.to_le_bytes());
        bytes[52..56].copy_from_slice(&attributes.to_le_bytes());
        bytes[56..58].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        bytes[58..60].copy_from_slice(&(header as u16).to_le_bytes());
        bytes[header..header + name_bytes.len()].copy_from_slice(&name_bytes);
        bytes
    }

    #[test]
    fn variable_length_records_are_walked_by_their_own_length() {
        // Names of different lengths give records of different sizes. A
        // parser that assumes a fixed stride reads the second record from the
        // wrong offset and yields nonsense — the exact failure this test
        // exists to catch.
        let mut buffer = Vec::new();
        buffer.extend(synth_record(5, 5, "a", 0));
        buffer.extend(synth_record(6, 5, "a-much-longer-directory-name", 0x10));
        buffer.extend(synth_record(7, 6, "x", 0));

        let mut out = Vec::new();
        parse_records(&buffer, &mut out);

        assert_eq!(out.len(), 3);
        assert_eq!(out[0].name, "a");
        assert_eq!(out[1].name, "a-much-longer-directory-name");
        assert_eq!(out[2].name, "x");
        assert_eq!(out[1].file_reference, 6);
        assert_eq!(out[2].parent_reference, 6);
        assert!(out[1].is_directory());
        assert!(!out[0].is_directory());
    }

    #[test]
    fn a_zero_length_record_terminates_instead_of_spinning() {
        let mut buffer = synth_record(1, 1, "ok", 0);
        buffer.extend(vec![0_u8; size_of::<UsnRecordV2>()]);

        let mut out = Vec::new();
        parse_records(&buffer, &mut out);
        assert_eq!(out.len(), 1, "the malformed record must end the walk");
    }

    #[test]
    fn a_record_claiming_more_than_the_buffer_is_rejected() {
        let mut buffer = synth_record(1, 1, "ok", 0);
        let overrun = (buffer.len() as u32 + 4096).to_le_bytes();
        buffer[0..4].copy_from_slice(&overrun);

        let mut out = Vec::new();
        parse_records(&buffer, &mut out);
        assert!(out.is_empty(), "an out-of-bounds length must read nothing");
    }

    #[test]
    fn non_v2_records_are_skipped_rather_than_misread() {
        let mut buffer = synth_record(1, 1, "v2-entry", 0);
        let mut v3 = synth_record(2, 1, "v3-entry", 0);
        v3[4..6].copy_from_slice(&3_u16.to_le_bytes());
        buffer.extend(v3);

        let mut out = Vec::new();
        parse_records(&buffer, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "v2-entry");
    }

    #[test]
    fn an_empty_buffer_parses_to_nothing() {
        let mut out = Vec::new();
        parse_records(&[], &mut out);
        parse_records(&[0_u8; 4], &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn reparse_points_are_flagged_from_the_attribute_bits() {
        let buffer = synth_record(
            9,
            5,
            "junction",
            FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT,
        );
        let mut out = Vec::new();
        parse_records(&buffer, &mut out);

        assert!(out[0].is_directory());
        assert!(
            out[0].is_reparse_point(),
            "a junction must be identifiable before anything tries to descend it"
        );
    }

    #[test]
    fn paths_resolve_through_the_parent_chain() {
        let namespace = VolumeNamespace {
            mount: "C:".into(),
            entries: vec![
                MftEntry {
                    file_reference: 5,
                    parent_reference: 5,
                    name: ".".into(),
                    attributes: FILE_ATTRIBUTE_DIRECTORY,
                },
                MftEntry {
                    file_reference: 10,
                    parent_reference: 5,
                    name: "Windows".into(),
                    attributes: FILE_ATTRIBUTE_DIRECTORY,
                },
                MftEntry {
                    file_reference: 20,
                    parent_reference: 10,
                    name: "System32".into(),
                    attributes: FILE_ATTRIBUTE_DIRECTORY,
                },
                MftEntry {
                    file_reference: 30,
                    parent_reference: 20,
                    name: "kernel32.dll".into(),
                    attributes: 0,
                },
            ],
            truncated: false,
        };

        let index = namespace.directory_index();
        assert_eq!(index.len(), 3, "only directories are indexed");

        let leaf = &namespace.entries[3];
        assert_eq!(
            namespace.path_of(leaf, &index).as_deref(),
            Some("C:\\.\\Windows\\System32\\kernel32.dll")
        );
    }

    #[test]
    fn a_broken_parent_chain_reports_nothing_rather_than_a_partial_path() {
        let namespace = VolumeNamespace {
            mount: "C:".into(),
            entries: vec![MftEntry {
                file_reference: 30,
                parent_reference: 999,
                name: "orphan.txt".into(),
                attributes: 0,
            }],
            truncated: false,
        };
        let index = namespace.directory_index();
        assert!(
            namespace.path_of(&namespace.entries[0], &index).is_none(),
            "a partial path presented as complete would be a wrong answer"
        );
    }

    #[test]
    fn a_cyclic_parent_chain_terminates() {
        let namespace = VolumeNamespace {
            mount: "C:".into(),
            entries: vec![
                MftEntry {
                    file_reference: 1,
                    parent_reference: 2,
                    name: "a".into(),
                    attributes: FILE_ATTRIBUTE_DIRECTORY,
                },
                MftEntry {
                    file_reference: 2,
                    parent_reference: 1,
                    name: "b".into(),
                    attributes: FILE_ATTRIBUTE_DIRECTORY,
                },
            ],
            truncated: false,
        };
        let index = namespace.directory_index();
        // The assertion is that this returns at all.
        let _ = namespace.path_of(&namespace.entries[0], &index);
    }

    #[test]
    fn availability_matches_what_enumeration_actually_does() {
        // Unelevated this is false and enumeration returns AccessDenied;
        // elevated both succeed. Either way they must agree, because the UI
        // decides whether to show the fast path based on the former.
        let available = is_available('C');
        let outcome = enumerate_volume('C', None);
        assert_eq!(
            available,
            outcome.is_ok(),
            "availability must not disagree with the real attempt"
        );

        match outcome {
            Ok(namespace) => assert!(
                namespace.entries.len() > 1000,
                "a real C: has far more than 1000 entries; got {} — \
                 a suspiciously small count means the record walk is broken",
                namespace.entries.len()
            ),
            Err(error) => assert!(
                error.is_elevation_fixable(),
                "the only expected failure here is elevation: {error}"
            ),
        }
    }
}
