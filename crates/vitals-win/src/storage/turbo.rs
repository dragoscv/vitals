//! Turbo scan: the whole drive out of its file table, in seconds.
//!
//! # Shape
//!
//! The app never runs elevated. It creates a named pipe that only it can
//! serve (first instance, local clients only), relaunches itself under one
//! UAC prompt with [`TURBO_ARG`], the drive letter and the pipe name, and
//! waits. The elevated child checks the pipe's server is the process that
//! started it, reads `$MFT` off the volume, and streams back one compact
//! record per folder: its parent, name, file ID and the bytes and files
//! directly inside it. The parent checks the client is the child it
//! launched, rebuilds the tree, and the child exits. No path, file name or
//! byte figure is taken from anything but that pipe.
//!
//! Folder records rather than file records because a folder is what the
//! tree holds: `C:` here has 1.9 million folders and 8 million files, and
//! streaming 8 million records to rebuild 1.9 million nodes would be four
//! times the traffic for the same result.
//!
//! # Agreeing with the walk
//!
//! A file is counted in the folder of its first long name, once, however
//! many names it has — the walker's hard-link rule. A reparse-point file
//! occupies nothing unless it is a cloud placeholder, and a reparse-point
//! folder is recorded but not entered unless it is a cloud folder — the
//! walker's link rules. What differs is what an unelevated walk cannot open
//! (other users' profiles, `System Volume Information`): Turbo counts it,
//! which the user chose on 2026-09-30, and the result says so.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED,
    GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE,
    PIPE_ACCESS_INBOUND, ReadFile, SetFilePointerEx, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use vitals_core::error::{Error, Result};

use super::codec::{Reader, Writer};
use super::ffi::{create_file_read, device_io_control};
use super::ntfs::{self, RECORD_MASK, Record, VolumeData};
use super::scan::{
    ScanMethod, ScanPhase, ScanProgress, ScanResult, is_traversable_reparse, record_id,
};
use super::sizing::{FileIdentity, SharedFile, SkipReason};
use super::tree::{NodeId, SizeTree};

/// The argument the elevated reader is started with.
pub const TURBO_ARG: &str = "--elevated-storage-turbo";

const FSCTL_GET_NTFS_VOLUME_DATA: u32 = 0x0009_0064;
/// The root folder's record number on every NTFS volume.
const ROOT_RECORD: u64 = 5;
/// Records 0-23 are the filesystem's own (`$MFT`, `$LogFile`, `$Bitmap`,
/// ...). A directory listing never shows them, so neither does Turbo.
const FIRST_USER_RECORD: u64 = 24;
/// Records per read. 4 MB: large enough that the drive streams, small
/// enough to report progress several times a second.
const CHUNK_BYTES: usize = 4 * 1024 * 1024;

const STREAM_MAGIC: &[u8; 4] = b"VTTB";
const STREAM_VERSION: u8 = 1;
const STREAM_END: &[u8; 4] = b"DONE";
/// Message tags on the pipe.
const TAG_PROGRESS: u8 = 1;
const TAG_TABLE: u8 = 2;
const TAG_FAILED: u8 = 3;

/// Child exit codes that are Turbo's own.
mod exit {
    pub const OK: u32 = 0;
    pub const BAD_ARGS: u32 = 0xE5C1_0001;
    pub const PIPE: u32 = 0xE5C1_0002;
    pub const IMPOSTOR: u32 = 0xE5C1_0003;
    pub const READ: u32 = 0xE5C1_0004;
}

/// One file's contribution, merged across its base and extension records.
#[derive(Debug, Clone, Default)]
struct FileEntry {
    sequence: u16,
    directory: bool,
    /// Every long name, parent reference first.
    names: Vec<ntfs::Name>,
    data: Option<(u64, u64)>,
    reparse_tag: Option<u32>,
}

/// Folds records into files as they are read. Extension records can come
/// before or after their base, so both are merged by record number.
#[derive(Debug, Default)]
pub struct Table {
    files: HashMap<u64, FileEntry>,
}

impl Table {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, record: Record) {
        let owner = record.base.unwrap_or(record.number);
        let entry = self.files.entry(owner).or_default();
        if record.base.is_none() {
            entry.sequence = record.sequence;
            entry.directory = record.directory;
        }
        entry.names.extend(record.names);
        if record.data.is_some() {
            entry.data = record.data;
        }
        if record.reparse_tag.is_some() {
            entry.reparse_tag = record.reparse_tag;
        }
    }

    /// Resolves the table into per-folder figures, in tree order.
    #[must_use]
    pub fn folders(self) -> Folders {
        build(self)
    }
}

/// One folder, as Turbo sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// Index of the parent in the same list; the root is its own parent.
    pub parent: u32,
    pub name: String,
    pub reference: u64,
    pub own_allocated: u64,
    pub own_logical: u64,
    pub own_files: u64,
    /// A link recorded but not entered.
    pub link: bool,
}

/// Every folder of a volume, parents before children, plus what a tree
/// cannot hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folders {
    pub folders: Vec<Folder>,
    pub files: u64,
    pub duplicates: u64,
    pub duplicate_bytes: u64,
    /// The largest files: folder index, name, allocated, logical.
    pub largest: Vec<(u32, String, u64, u64)>,
    pub shared: Vec<SharedFile>,
    /// Names whose folder is not reachable from the root: the filesystem's
    /// own `$Extend` tree (which no listing shows either) and records torn
    /// by a rename that raced the read. Counted for the prover, never
    /// placed anywhere.
    pub orphans: u64,
}

const LARGEST: usize = 1000;

fn build(table: Table) -> Folders {
    let files = table.files;
    // Folder records by record number, excluding the filesystem's own (and
    // the root, which is placed first by hand).
    let mut index: HashMap<u64, u32> = HashMap::new();
    let mut out = Folders::default();
    let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
    for (&number, entry) in &files {
        if !entry.directory || number < FIRST_USER_RECORD {
            continue;
        }
        let Some(name) = entry.names.first() else {
            continue;
        };
        children
            .entry(name.parent & RECORD_MASK)
            .or_default()
            .push(number);
    }
    // Breadth-first from the root, so a parent always precedes its children
    // and a folder whose chain never reaches the root is never visited.
    let root_sequence = files.get(&ROOT_RECORD).map_or(0, |e| e.sequence);
    out.folders.push(Folder {
        parent: 0,
        name: String::new(),
        reference: u64::from(root_sequence) << 48 | ROOT_RECORD,
        own_allocated: 0,
        own_logical: 0,
        own_files: 0,
        link: false,
    });
    index.insert(ROOT_RECORD, 0);
    let mut queue = std::collections::VecDeque::from([ROOT_RECORD]);
    while let Some(parent) = queue.pop_front() {
        let Some(&parent_index) = index.get(&parent) else {
            continue;
        };
        let mut kids = children.remove(&parent).unwrap_or_default();
        kids.sort_unstable();
        for number in kids {
            let Some(entry) = files.get(&number) else {
                continue;
            };
            let Some(name) = entry.names.first() else {
                continue;
            };
            let link = entry
                .reparse_tag
                .is_some_and(|tag| !is_traversable_reparse(tag));
            let i = out.folders.len() as u32;
            out.folders.push(Folder {
                parent: parent_index,
                name: name.name.clone(),
                reference: u64::from(entry.sequence) << 48 | number,
                own_allocated: 0,
                own_logical: 0,
                own_files: 0,
                link,
            });
            index.insert(number, i);
            if !link {
                queue.push_back(number);
            }
        }
    }
    out.orphans = children.values().map(|v| v.len() as u64).sum();

    // Files: counted in their first reachable folder, once.
    let mut largest: std::collections::BinaryHeap<std::cmp::Reverse<(u64, u64, String, u32)>> =
        std::collections::BinaryHeap::new();
    for (&number, entry) in &files {
        if entry.directory || number < FIRST_USER_RECORD {
            continue;
        }
        let mut homes: Vec<(u32, &str)> = entry
            .names
            .iter()
            .filter_map(|n| {
                let at = *index.get(&(n.parent & RECORD_MASK))?;
                // A folder that is a link is not entered, so its files are
                // not this walk's either.
                (!out.folders[at as usize].link).then_some((at, n.name.as_str()))
            })
            .collect();
        if homes.is_empty() {
            if !entry.names.is_empty() {
                out.orphans += 1;
            }
            continue;
        }
        // The same folder the walker credits: whichever comes first in tree
        // order is not knowable in advance for the walker either, so the
        // lowest index — the shallowest, earliest-listed folder — is used.
        homes.sort_unstable_by_key(|(at, _)| *at);
        let (reported, logical) = entry.data.unwrap_or((0, 0));
        // The walker's rule: a link to a file occupies nothing of its own; a
        // cloud placeholder occupies what is really on disk.
        let allocated = match entry.reparse_tag {
            Some(tag) if !is_traversable_reparse(tag) => 0,
            _ => reported,
        };
        let (home, name) = homes[0];
        let folder = &mut out.folders[home as usize];
        folder.own_allocated += allocated;
        folder.own_logical += logical;
        out.files += homes.len() as u64;
        for &(other, _) in &homes[1..] {
            out.folders[other as usize].own_files += 1;
        }
        out.folders[home as usize].own_files += 1;
        if homes.len() > 1 {
            let repeats = homes.len() as u64 - 1;
            out.duplicates += repeats;
            out.duplicate_bytes += allocated * repeats;
            out.shared.push(SharedFile {
                identity: FileIdentity(u128::from(u64::from(entry.sequence) << 48 | number)),
                allocated,
                folders: homes.iter().map(|(at, _)| *at).collect(),
            });
        }
        if allocated > 0 {
            let item = std::cmp::Reverse((allocated, logical, name.to_owned(), home));
            if largest.len() < LARGEST {
                largest.push(item);
            } else if largest.peek().is_some_and(|smallest| item < *smallest) {
                largest.pop();
                largest.push(item);
            }
        }
    }
    out.largest = largest
        .into_sorted_vec()
        .into_iter()
        .map(|std::cmp::Reverse((a, l, n, d))| (d, n, a, l))
        .collect();
    out.shared.sort_unstable_by_key(|f| f.identity.0);
    out
}

/// Serialises what the child sends.
///
/// # Errors
///
/// When writing fails.
pub fn write_folders<W: Write>(out: W, folders: &Folders) -> io::Result<W> {
    let mut w = Writer::new(out);
    w.bytes(STREAM_MAGIC)?;
    w.u8(STREAM_VERSION)?;
    w.varint(folders.folders.len() as u64)?;
    for (i, f) in folders.folders.iter().enumerate() {
        w.varint((i as u64).saturating_sub(u64::from(f.parent)))?;
        w.str(&f.name)?;
        w.varint(f.reference)?;
        w.varint(f.own_allocated)?;
        w.varint(f.own_logical)?;
        w.varint(f.own_files)?;
        w.u8(u8::from(f.link))?;
    }
    w.varint(folders.files)?;
    w.varint(folders.duplicates)?;
    w.varint(folders.duplicate_bytes)?;
    w.varint(folders.orphans)?;
    w.varint(folders.largest.len() as u64)?;
    for (dir, name, a, l) in &folders.largest {
        w.varint(u64::from(*dir))?;
        w.str(name)?;
        w.varint(*a)?;
        w.varint(*l)?;
    }
    w.varint(folders.shared.len() as u64)?;
    for s in &folders.shared {
        w.varint(s.identity.0 as u64)?;
        w.varint(s.allocated)?;
        w.varint(s.folders.len() as u64)?;
        for f in &s.folders {
            w.varint(u64::from(*f))?;
        }
    }
    w.bytes(STREAM_END)?;
    w.flush()?;
    Ok(w.into_inner())
}

fn corrupt(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

/// Reads what [`write_folders`] wrote, refusing anything malformed: this is
/// the one place data crosses from an elevated process into the app.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidData`] for a malformed stream.
pub fn read_folders<R: Read>(input: R) -> io::Result<Folders> {
    const MAX: u64 = 64 * 1024 * 1024;
    let mut r = Reader::new(input);
    if &r.bytes::<4>()? != STREAM_MAGIC || r.u8()? != STREAM_VERSION {
        return Err(corrupt("not a Turbo table"));
    }
    let count = r.varint()?;
    if count == 0 || count > MAX {
        return Err(corrupt("an implausible number of folders"));
    }
    let mut out = Folders::default();
    for i in 0..count {
        let back = r.varint()?;
        if (i > 0 && back == 0) || back > i {
            return Err(corrupt("a folder whose parent comes after it"));
        }
        out.folders.push(Folder {
            parent: (i - back) as u32,
            name: r.string()?,
            reference: r.varint()?,
            own_allocated: r.varint()?,
            own_logical: r.varint()?,
            own_files: r.varint()?,
            link: match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(corrupt("a malformed link flag")),
            },
        });
    }
    out.files = r.varint()?;
    out.duplicates = r.varint()?;
    out.duplicate_bytes = r.varint()?;
    out.orphans = r.varint()?;
    let largest = r.varint()?;
    if largest > MAX {
        return Err(corrupt("an implausible number of large files"));
    }
    for _ in 0..largest {
        let dir = r.varint_u32()?;
        if u64::from(dir) >= count {
            return Err(corrupt("a large file in a folder that does not exist"));
        }
        out.largest.push((dir, r.string()?, r.varint()?, r.varint()?));
    }
    let shared = r.varint()?;
    if shared > MAX {
        return Err(corrupt("an implausible number of linked files"));
    }
    for _ in 0..shared {
        let identity = FileIdentity(u128::from(r.varint()?));
        let allocated = r.varint()?;
        let n = r.varint()?;
        if n > 1024 {
            return Err(corrupt("a file with more names than NTFS allows"));
        }
        let mut folders = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let f = r.varint_u32()?;
            if u64::from(f) >= count {
                return Err(corrupt("a linked file in a folder that does not exist"));
            }
            folders.push(f);
        }
        out.shared.push(SharedFile {
            identity,
            allocated,
            folders,
        });
    }
    if &r.bytes::<4>()? != STREAM_END {
        return Err(corrupt("a Turbo table that ends early"));
    }
    Ok(out)
}

/// Turns received folders into a scan result for `letter`.
#[must_use]
pub fn into_result(folders: Folders, letter: char, cluster: Option<u64>, elapsed: Duration) -> ScanResult {
    let root = format!("{letter}:\\");
    let mut tree = SizeTree::new(&root);
    let mut dir_ids = Vec::with_capacity(folders.folders.len());
    let mut nodes = Vec::with_capacity(folders.folders.len());
    for (i, folder) in folders.folders.iter().enumerate() {
        let node = if i == 0 {
            tree.root()
        } else {
            let parent = nodes
                .get(folder.parent as usize)
                .copied()
                .unwrap_or_else(|| tree.root());
            tree.add_child(parent, &folder.name)
        };
        nodes.push(node);
        record_id(&mut dir_ids, node, folder.reference);
        tree.add_files(node, folder.own_allocated, folder.own_logical, folder.own_files);
        if folder.link {
            let path = tree.path_of(node);
            tree.mark_skipped(node, path, SkipReason::ReparsePoint);
        }
    }
    tree.aggregate();
    let at = |i: u32| nodes.get(i as usize).copied().unwrap_or(NodeId(0));
    ScanResult {
        largest_files: folders
            .largest
            .into_iter()
            .map(|(dir, name, a, l)| super::scan::LargeFile {
                dir: at(dir),
                name,
                allocated: vitals_core::units::Bytes(a),
                logical: vitals_core::units::Bytes(l),
            })
            .collect(),
        shared_files: folders
            .shared
            .into_iter()
            .map(|s| SharedFile {
                folders: s.folders.iter().map(|f| at(*f).0).collect(),
                ..s
            })
            .collect(),
        directories_scanned: tree.len() as u64,
        tree,
        cluster_bytes: cluster,
        files_scanned: folders.files,
        hard_link_duplicates: folders.duplicates,
        hard_link_bytes_saved: folders.duplicate_bytes,
        cancelled: false,
        elapsed_ms: elapsed.as_millis() as u64,
        threads: 1,
        method: ScanMethod::Turbo,
        dir_ids,
        reused_directories: None,
        relisted_directories: None,
    }
}

// ---------------------------------------------------------------------------
// The elevated child: read the table off the volume.
// ---------------------------------------------------------------------------

/// A handle that closes itself.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: a live handle, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

fn last_os(context: &str) -> Error {
    // SAFETY: no arguments, no preconditions.
    let code = unsafe { GetLastError() };
    if code == ERROR_ACCESS_DENIED {
        return Error::AccessDenied {
            operation: context.to_owned(),
        };
    }
    Error::Os {
        context: context.to_owned(),
        code: code.cast_signed(),
    }
}

fn open_volume(letter: char) -> Result<Owned> {
    let path: Vec<u16> = format!("\\\\.\\{letter}:").encode_utf16().chain(Some(0)).collect();
    // SAFETY: NUL-terminated device path, alive for the call.
    let handle = unsafe {
        create_file_read(path.as_ptr(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, 0)
    };
    match handle.filter(|h| *h != INVALID_HANDLE_VALUE && !h.is_null()) {
        Some(h) => Ok(Owned(h)),
        None => Err(last_os(&format!("open \\\\.\\{letter}: for reading"))),
    }
}

fn read_at(volume: &Owned, offset: u64, buf: &mut [u8]) -> Result<usize> {
    // SAFETY: `volume` is live; the out-pointer is null, which is allowed.
    if unsafe { SetFilePointerEx(volume.0, offset.cast_signed(), std::ptr::null_mut(), 0) } == 0 {
        return Err(last_os("seek on the volume"));
    }
    let mut read = 0_u32;
    // SAFETY: `buf` is a live buffer of the length passed; synchronous.
    let ok = unsafe {
        ReadFile(volume.0, buf.as_mut_ptr(), buf.len() as u32, &raw mut read, std::ptr::null_mut())
    };
    if ok == 0 {
        return Err(last_os("read the volume"));
    }
    Ok(read as usize)
}

/// The cluster runs of `$MFT`'s own data, following its attribute list when
/// it is fragmented past what one record holds.
fn mft_runs(volume: &Owned, data: &VolumeData) -> Result<Vec<ntfs::Run>> {
    let record = data.bytes_per_record as usize;
    let cluster = u64::from(data.bytes_per_cluster);
    // Sector-aligned read of a whole cluster, which holds record 0.
    let mut first = vec![0_u8; record.max(data.bytes_per_cluster as usize)];
    read_at(volume, data.mft_start_lcn * cluster, &mut first)?;
    let mut base = first[..record].to_vec();
    if !ntfs::apply_fixup(&mut base) {
        return Err(Error::Os {
            context: "the first file record failed its integrity check".into(),
            code: 0,
        });
    }
    let mut segments: Vec<(u64, Vec<ntfs::Run>)> = Vec::new();
    let mut list = None;
    for a in ntfs::attributes(&base) {
        if a.kind == ntfs::ATTR_DATA && a.name().is_empty() {
            if let (Some(vcn), Some(runs)) = (a.lowest_vcn(), a.runs()) {
                segments.push((vcn, runs));
            }
        } else if a.kind == ntfs::ATTR_ATTRIBUTE_LIST {
            list = a.value().map(<[u8]>::to_vec);
        }
    }
    // The rest of `$MFT`'s runs live in extension records named by its
    // attribute list. Those records are within the first segment (NTFS
    // keeps them there so the table can always be bootstrapped).
    if let Some(list) = list {
        let first_runs = segments.first().map(|(_, r)| r.clone()).unwrap_or_default();
        for (_, number) in ntfs::data_extensions(&list, 0) {
            let Some(offset) = offset_of_record(&first_runs, number, record as u64, cluster) else {
                continue;
            };
            let mut raw = vec![0_u8; record];
            read_at(volume, offset, &mut raw)?;
            if !ntfs::apply_fixup(&mut raw) {
                continue;
            }
            for a in ntfs::attributes(&raw) {
                if a.kind == ntfs::ATTR_DATA
                    && a.name().is_empty()
                    && let (Some(vcn), Some(runs)) = (a.lowest_vcn(), a.runs())
                {
                    segments.push((vcn, runs));
                }
            }
        }
    }
    segments.sort_by_key(|(vcn, _)| *vcn);
    Ok(segments.into_iter().flat_map(|(_, r)| r).collect())
}

fn offset_of_record(runs: &[ntfs::Run], number: u64, record: u64, cluster: u64) -> Option<u64> {
    let mut byte = number * record;
    for run in runs {
        let len = run.clusters * cluster;
        if byte < len {
            return Some(run.lcn? * cluster + byte);
        }
        byte -= len;
    }
    None
}

/// Reads and resolves the whole table. `on_progress` gets the fraction read.
///
/// # Errors
///
/// When the volume is not NTFS, cannot be opened, or a read fails.
pub fn read_volume(
    letter: char,
    cancel: Option<&AtomicBool>,
    on_progress: &mut dyn FnMut(ScanPhase, f64),
) -> Result<(Folders, u64)> {
    let volume = open_volume(letter)?;
    let mut raw = [0_u8; 128];
    let mut returned = 0_u32;
    // SAFETY: `volume` is live; `raw` is a live buffer of the size passed.
    let ok = unsafe {
        device_io_control(
            volume.0,
            FSCTL_GET_NTFS_VOLUME_DATA,
            std::ptr::null(),
            0,
            raw.as_mut_ptr().cast(),
            raw.len() as u32,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        // Not NTFS: no file table to read. The UI only offers Turbo for
        // NTFS drives, so reaching this is a drive that changed under it.
        return Err(Error::Unsupported(vitals_core::capability::Capability::DiskCleanup));
    }
    let data = VolumeData::parse(&raw).ok_or_else(|| Error::Os {
        context: "the drive reported an implausible file table".into(),
        code: 0,
    })?;
    let cluster = u64::from(data.bytes_per_cluster);
    let record = data.bytes_per_record as usize;
    let runs = mft_runs(&volume, &data)?;
    let total = data.mft_valid_length;
    let mut table = Table::new();
    let mut buf = vec![0_u8; CHUNK_BYTES];
    let mut number = 0_u64;
    let mut done = 0_u64;
    'runs: for run in runs {
        let Some(lcn) = run.lcn else {
            // A sparse stretch of the table holds no records, but it still
            // occupies record numbers.
            number += run.clusters * cluster / record as u64;
            continue;
        };
        let mut offset = lcn * cluster;
        let mut left = run.clusters * cluster;
        while left > 0 && done < total {
            if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Err(Error::Refused("the scan was stopped".into()));
            }
            let want = (left.min(CHUNK_BYTES as u64).min(total - done)) as usize;
            let want = want - want % record;
            if want == 0 {
                break 'runs;
            }
            let got = read_at(&volume, offset, &mut buf[..want])?;
            for chunk in buf[..got - got % record].chunks_exact_mut(record) {
                if let Some(parsed) = ntfs::parse_record(chunk, number, cluster) {
                    table.add(parsed);
                }
                number += 1;
            }
            offset += got as u64;
            left -= got as u64;
            done += got as u64;
            on_progress(ScanPhase::Reading, done as f64 / total.max(1) as f64);
            if got < want {
                break 'runs;
            }
        }
    }
    on_progress(ScanPhase::Building, 1.0);
    Ok((table.folders(), cluster))
}

// ---------------------------------------------------------------------------
// The pipe between the two.
// ---------------------------------------------------------------------------

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// A pipe name nobody can predict: a guessable one could be created first by
/// another process. `FILE_FLAG_FIRST_PIPE_INSTANCE` would then make our
/// create fail rather than join theirs, and both ends check the peer's PID,
/// so a guess costs a failed scan, never data — but unpredictable is cheap.
fn pipe_name() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|err| Error::Os {
        context: format!("no randomness for the Turbo pipe name: {err}"),
        code: 0,
    })?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("\\\\.\\pipe\\vitals-turbo-{hex}"))
}

/// The server end the app creates before launching the child.
struct Server {
    pipe: Owned,
    event: Owned,
}

impl Server {
    fn create(name: &str) -> Result<Self> {
        let name_w = wide(name);
        // SAFETY: `name_w` is NUL-terminated; default security (the creator
        // and administrators); one instance only, so nobody can open a
        // second one under the same name.
        let pipe = unsafe {
            CreateNamedPipeW(
                name_w.as_ptr(),
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                1 << 20,
                0,
                std::ptr::null(),
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            return Err(last_os("create the Turbo pipe"));
        }
        let pipe = Owned(pipe);
        // SAFETY: manual-reset, unsignalled, unnamed.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(last_os("create an event for the Turbo pipe"));
        }
        Ok(Self {
            pipe,
            event: Owned(event),
        })
    }

    /// Runs one overlapped operation to completion, giving up when `alive`
    /// says the child has gone or the scan was cancelled.
    fn complete(
        &self,
        started: i32,
        overlapped: &mut OVERLAPPED,
        alive: &mut dyn FnMut() -> bool,
    ) -> Result<u32> {
        if started == 0 {
            // SAFETY: no arguments.
            let code = unsafe { GetLastError() };
            if code == ERROR_PIPE_CONNECTED {
                return Ok(0);
            }
            if code != ERROR_IO_PENDING {
                return Err(if code == ERROR_BROKEN_PIPE {
                    Error::Os {
                        context: "the Turbo reader closed the pipe early".into(),
                        code: code.cast_signed(),
                    }
                } else {
                    last_os("use the Turbo pipe")
                });
            }
        }
        loop {
            // SAFETY: the event is live.
            let waited = unsafe { WaitForSingleObject(self.event.0, 200) };
            if waited == 0 {
                break;
            }
            if !alive() {
                // SAFETY: the pipe and `overlapped` are live; cancelling and
                // then waiting keeps the kernel from writing into a dropped
                // buffer.
                unsafe {
                    CancelIoEx(self.pipe.0, &raw const *overlapped);
                    let mut n = 0;
                    GetOverlappedResult(self.pipe.0, &raw const *overlapped, &raw mut n, 1);
                }
                return Err(Error::Os {
                    context: "the Turbo reader stopped before it finished".into(),
                    code: 0,
                });
            }
        }
        let mut n = 0_u32;
        // SAFETY: the operation has completed; `overlapped` is live.
        if unsafe { GetOverlappedResult(self.pipe.0, &raw const *overlapped, &raw mut n, 0) } == 0 {
            // SAFETY: no arguments.
            let code = unsafe { GetLastError() };
            return Err(Error::Os {
                context: "the Turbo pipe".into(),
                code: code.cast_signed(),
            });
        }
        Ok(n)
    }

    fn overlapped(&self) -> OVERLAPPED {
        OVERLAPPED {
            hEvent: self.event.0,
            ..OVERLAPPED::default()
        }
    }

    fn accept(&self, alive: &mut dyn FnMut() -> bool) -> Result<u32> {
        let mut o = self.overlapped();
        // SAFETY: the pipe and `o` are live until `complete` returns.
        let started = unsafe { ConnectNamedPipe(self.pipe.0, &raw mut o) };
        self.complete(started, &mut o, alive)?;
        let mut pid = 0_u32;
        // SAFETY: the pipe is connected; `pid` is a live out-pointer.
        if unsafe { GetNamedPipeClientProcessId(self.pipe.0, &raw mut pid) } == 0 {
            return Err(last_os("identify the Turbo reader"));
        }
        Ok(pid)
    }
}

/// Reads the connected pipe as a byte stream.
struct PipeReader<'a> {
    server: &'a Server,
    alive: &'a mut dyn FnMut() -> bool,
}

impl Read for PipeReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut o = self.server.overlapped();
        let mut n = 0_u32;
        // SAFETY: `buf` and `o` live until `complete` returns.
        let started = unsafe {
            ReadFile(self.server.pipe.0, buf.as_mut_ptr(), buf.len().min(1 << 20) as u32, &raw mut n, &raw mut o)
        };
        match self.server.complete(started, &mut o, self.alive) {
            Ok(n) => Ok(n as usize),
            Err(Error::Os { code, .. }) if code == ERROR_BROKEN_PIPE.cast_signed() => Ok(0),
            Err(err) => Err(io::Error::other(err.to_string())),
        }
    }
}

/// A running elevated reader, as [`run_with`] needs it.
pub trait Child {
    fn id(&self) -> u32;
    /// `Some(code)` once exited.
    fn exited(&mut self) -> Option<u32>;
}

impl Child for crate::actions::taskmgr::ElevatedProcess {
    fn id(&self) -> u32 {
        Self::id(self)
    }

    fn exited(&mut self) -> Option<u32> {
        self.wait(Some(Duration::ZERO)).ok().flatten()
    }
}

/// Runs a Turbo scan of `letter`: one UAC prompt, then the table.
///
/// # Errors
///
/// - [`Error::Refused`] when the prompt was declined. Nothing was read.
/// - [`Error::Os`] when the reader failed, with what it reported.
pub fn run(
    letter: char,
    cancel: Option<&AtomicBool>,
    on_progress: &mut dyn FnMut(ScanProgress),
) -> Result<ScanResult> {
    let exe = std::env::current_exe()?;
    run_with(letter, cancel, on_progress, |args| {
        crate::actions::taskmgr::spawn_program_elevated(&exe, args, "nothing was scanned")
    })
}

/// [`run`] with the elevation step injected, so a declined prompt and a
/// reader that dies are testable without UAC.
///
/// # Errors
///
/// As [`run`].
pub fn run_with<C, L>(
    letter: char,
    cancel: Option<&AtomicBool>,
    on_progress: &mut dyn FnMut(ScanProgress),
    launch: L,
) -> Result<ScanResult>
where
    C: Child,
    L: FnOnce(&str) -> Result<C>,
{
    let letter = letter.to_ascii_uppercase();
    if !letter.is_ascii_uppercase() {
        return Err(Error::NotFound(format!("{letter} is not a drive letter")));
    }
    let started = Instant::now();
    let progress = |phase, fraction| ScanProgress {
        files_seen: 0,
        directories_seen: 0,
        bytes_seen: 0,
        elapsed_ms: started.elapsed().as_millis() as u64,
        current_path: format!("{letter}:\\"),
        phase,
        fraction,
    };
    let name = pipe_name()?;
    let server = Server::create(&name)?;
    on_progress(progress(ScanPhase::Approval, None));
    let mut child = launch(&format!("{TURBO_ARG} {letter} {name}"))?;
    let child_id = child.id();
    let cancelled = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    let mut exit_code = None;
    let mut alive = || {
        if exit_code.is_none() {
            exit_code = child.exited();
        }
        exit_code.is_none() && !cancelled()
    };
    let peer = server.accept(&mut alive)?;
    if peer != child_id {
        return Err(Error::Refused(format!(
            "process {peer} connected to the Turbo pipe instead of the reader Vitals started"
        )));
    }
    // Buffered: each unbuffered read would be one overlapped ReadFile, and
    // the table is tens of megabytes read a varint at a time.
    let mut reader = Reader::new(io::BufReader::with_capacity(
        1 << 20,
        PipeReader {
            server: &server,
            alive: &mut alive,
        },
    ));
    loop {
        match reader.u8()? {
            TAG_PROGRESS => {
                let phase = match reader.u8()? {
                    0 => ScanPhase::Reading,
                    _ => ScanPhase::Building,
                };
                let permille = reader.varint()?.min(1000);
                on_progress(progress(phase, Some(permille as f64 / 1000.0)));
            }
            TAG_TABLE => break,
            TAG_FAILED => {
                let message = reader.string()?;
                return Err(Error::Os {
                    context: message,
                    code: 0,
                });
            }
            _ => return Err(Error::Os {
                context: "the Turbo reader sent something unexpected".into(),
                code: 0,
            }),
        }
    }
    let folders = read_folders(reader.into_inner())?;
    let cluster = super::scan::cluster_bytes_of(&format!("{letter}:\\"));
    Ok(into_result(folders, letter, cluster, started.elapsed()))
}

/// Writes to the pipe the parent created.
struct PipeWriter(Owned);

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut n = 0_u32;
        // SAFETY: the handle is live; synchronous write.
        let ok = unsafe {
            WriteFile(self.0.0, buf.as_ptr(), buf.len().min(1 << 20) as u32, &raw mut n, std::ptr::null_mut())
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Reads the arguments after [`TURBO_ARG`].
#[must_use]
pub fn parse_args<I, S>(rest: I) -> Option<(char, String)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut rest = rest.into_iter();
    let letter = rest.next()?;
    let pipe = rest.next()?;
    if rest.next().is_some() {
        return None;
    }
    let mut chars = letter.as_ref().chars();
    let (Some(l), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let pipe = pipe.as_ref();
    let valid = l.is_ascii_alphabetic()
        && pipe.strip_prefix("\\\\.\\pipe\\vitals-turbo-").is_some_and(|hex| {
            hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit())
        });
    valid.then(|| (l.to_ascii_uppercase(), pipe.to_owned()))
}

/// The elevated child's whole job.
#[must_use]
pub fn perform<I, S>(rest: I) -> u32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some((letter, pipe)) = parse_args(rest) else {
        return exit::BAD_ARGS;
    };
    let name = wide(&pipe);
    // SAFETY: NUL-terminated pipe name.
    let handle = unsafe {
        create_file_read(name.as_ptr(), GENERIC_WRITE, 0, 0)
    };
    let Some(handle) = handle.filter(|h| *h != INVALID_HANDLE_VALUE && !h.is_null()) else {
        return exit::PIPE;
    };
    let handle = Owned(handle);
    // The pipe must be served by the process that launched us. Anything
    // else is a process that raced to create the name, and gets nothing.
    let mut server = 0_u32;
    // SAFETY: the pipe is open; `server` is a live out-pointer.
    let served = unsafe { GetNamedPipeServerProcessId(handle.0, &raw mut server) } != 0;
    if !served || Some(server) != parent_process_id() {
        return exit::IMPOSTOR;
    }
    let mut out = Writer::new(io::BufWriter::with_capacity(1 << 20, PipeWriter(handle)));
    let mut last = Instant::now();
    let mut report = |phase: ScanPhase, fraction: f64| {
        if last.elapsed() < Duration::from_millis(100) && fraction < 1.0 {
            return;
        }
        last = Instant::now();
        let _ = out.u8(TAG_PROGRESS);
        let _ = out.u8(u8::from(phase != ScanPhase::Reading));
        let _ = out.varint((fraction * 1000.0) as u64);
        let _ = out.flush();
    };
    match read_volume(letter, None, &mut report) {
        Ok((folders, _)) => {
            let sent = out
                .u8(TAG_TABLE)
                .and_then(|()| write_folders(out.into_inner(), &folders))
                .and_then(|mut w| w.flush());
            if sent.is_ok() { exit::OK } else { exit::PIPE }
        }
        Err(err) => {
            let _ = out.u8(TAG_FAILED);
            let _ = out.str(&err.to_string());
            let _ = out.flush();
            exit::READ
        }
    }
}

/// The process that started this one, from our own basic information.
///
/// `ShellExecuteExW(runas)` makes the requesting app the parent of the
/// elevated child (the `AppInfo` service re-parents it), which is what lets
/// the child check that the pipe's server is the app that asked.
fn parent_process_id() -> Option<u32> {
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, PROCESS_BASIC_INFORMATION};
    // SAFETY: all-zero is a valid PROCESS_BASIC_INFORMATION.
    let mut info: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let mut len = 0_u32;
    // SAFETY: the pseudo-handle is always valid; `info` is a live buffer of
    // exactly the size passed.
    let status = unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            ProcessBasicInformation,
            (&raw mut info).cast(),
            size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            &raw mut len,
        )
    };
    (status >= 0).then(|| info.InheritedFromUniqueProcessId as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ntfs::synth::Builder;

    const DIR: u16 = 0x3;
    const FILE: u16 = 0x1;

    fn reference(number: u64, sequence: u64) -> u64 {
        sequence << 48 | number
    }

    /// Root (5), `Users` (40), `Users\me` (41), a junction folder (42),
    /// a file with two names, a file in the junction, an extension record.
    fn table() -> Table {
        let mut t = Table::new();
        let mut add = |number: u64, raw: Vec<u8>| {
            let mut raw = raw;
            t.add(ntfs::parse_record(&mut raw, number, 4096).expect("record"));
        };
        add(5, Builder::new(5, DIR, 0).file_name(reference(5, 5), ".", 1).finish());
        add(40, Builder::new(1, DIR, 0).file_name(reference(5, 5), "Users", 1).finish());
        add(41, Builder::new(1, DIR, 0).file_name(reference(40, 1), "me", 1).finish());
        add(
            42,
            Builder::new(1, DIR, 0)
                .file_name(reference(5, 5), "Documents and Settings", 1)
                .plain(ntfs::ATTR_REPARSE_POINT, &0xA000_0003_u32.to_le_bytes())
                .finish(),
        );
        add(
            100,
            Builder::new(2, FILE, 0)
                .file_name(reference(41, 1), "BIG~1.BIN", 2)
                .file_name(reference(41, 1), "big.bin", 1)
                .file_name(reference(40, 1), "big-link.bin", 1)
                .nonresident_data("", 0, 0, 1 << 20, 1_000_000, 0, &[0x21, 0x00, 0x01, 0x10])
                .finish(),
        );
        add(
            101,
            Builder::new(1, FILE, 0)
                .file_name(reference(42, 1), "hidden.txt", 1)
                .resident_data("", 10)
                .finish(),
        );
        add(102, Builder::new(1, FILE, 0).file_name(reference(40, 1), "small.txt", 1).finish());
        // Extension record of 102 carrying its data.
        add(
            103,
            Builder::new(1, FILE, 102)
                .nonresident_data("", 0, 0, 8192, 5000, 0, &[0x11, 0x02, 0x20])
                .finish(),
        );
        // A system record that must never show.
        add(6, Builder::new(1, FILE, 0).file_name(reference(5, 5), "$Bitmap", 3).finish());
        t
    }

    #[test]
    fn folders_come_out_parents_first_and_links_are_not_entered() {
        let f = table().folders();
        let names: Vec<_> = f.folders.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["", "Users", "Documents and Settings", "me"]);
        for (i, folder) in f.folders.iter().enumerate().skip(1) {
            assert!((folder.parent as usize) < i, "{folder:?}");
        }
        assert!(f.folders[2].link, "a junction is recorded, not entered");
        assert_eq!(f.folders[2].own_files, 0, "its contents belong elsewhere");
    }

    #[test]
    fn a_file_with_two_names_is_counted_once_in_the_first_folder() {
        let f = table().folders();
        let users = &f.folders[1];
        let me = &f.folders[3];
        // Users (index 1) precedes me (index 3), so it is credited.
        assert_eq!(users.own_allocated, (1 << 20) + 8192);
        assert_eq!(me.own_allocated, 0);
        assert_eq!(users.own_files, 2, "big-link.bin and small.txt");
        assert_eq!(me.own_files, 1, "big.bin is still a file there");
        assert_eq!(f.files, 3);
        assert_eq!(f.duplicates, 1);
        assert_eq!(f.duplicate_bytes, 1 << 20);
        assert_eq!(f.shared.len(), 1);
        assert_eq!(f.shared[0].folders, [1, 3]);
    }

    #[test]
    fn an_extension_record_adds_its_data_to_its_base_file() {
        let f = table().folders();
        assert!(
            f.largest.iter().any(|(_, n, a, _)| n == "small.txt" && *a == 8192),
            "{:?}",
            f.largest
        );
    }

    #[test]
    fn a_file_whose_folder_never_reaches_the_root_is_counted_as_an_orphan() {
        let mut t = table();
        let mut raw = Builder::new(1, FILE, 0).file_name(reference(9999, 1), "lost.bin", 1).finish();
        t.add(ntfs::parse_record(&mut raw, 200, 4096).expect("record"));
        assert_eq!(t.folders().orphans, 1);
    }

    #[test]
    fn the_stream_round_trips_and_a_truncated_one_is_refused() {
        let f = table().folders();
        let bytes = write_folders(Vec::new(), &f).expect("write");
        assert_eq!(read_folders(bytes.as_slice()).expect("read"), f);
        for cut in [3, 10, bytes.len() - 1] {
            assert!(read_folders(&bytes[..cut]).is_err(), "cut {cut}");
        }
    }

    #[test]
    fn a_stream_naming_a_parent_after_its_child_is_refused() {
        let mut f = table().folders();
        f.folders[1].parent = 3;
        let bytes = write_folders(Vec::new(), &f).expect("write");
        assert!(read_folders(bytes.as_slice()).is_err());
    }

    #[test]
    fn the_result_tree_has_the_same_totals_as_the_folders() {
        let f = table().folders();
        let result = into_result(f.clone(), 'C', Some(4096), Duration::from_millis(5));
        let own: u64 = f.folders.iter().map(|x| x.own_allocated).sum();
        assert_eq!(result.allocated().get(), own);
        assert_eq!(result.tree.path_of(NodeId(3)), "C:\\Users\\me");
        assert_eq!(result.method, ScanMethod::Turbo);
        assert_eq!(result.gaps(), 0, "a junction is a link, not a gap");
        assert_eq!(result.dir_ids[1], reference(40, 1));
    }

    #[test]
    fn only_a_letter_and_a_vitals_pipe_name_are_accepted() {
        let pipe = format!("\\\\.\\pipe\\vitals-turbo-{}", "a".repeat(32));
        assert_eq!(parse_args(["c", pipe.as_str()]), Some(('C', pipe.clone())));
        assert_eq!(parse_args(["C:", pipe.as_str()]), None);
        assert_eq!(parse_args(["C", "\\\\.\\pipe\\evil"]), None);
        assert_eq!(parse_args(["C", pipe.as_str(), "extra"]), None);
        assert_eq!(parse_args(["C"]), None);
        assert_eq!(perform(["C", "\\\\server\\pipe\\x"]), exit::BAD_ARGS);
    }

    #[test]
    fn two_pipe_names_are_never_the_same_and_the_child_accepts_them() {
        let a = pipe_name().expect("random");
        assert_ne!(a, pipe_name().expect("random"));
        assert!(parse_args(["C", a.as_str()]).is_some(), "{a}");
    }

    struct Never;
    impl Child for Never {
        fn id(&self) -> u32 {
            0
        }
        fn exited(&mut self) -> Option<u32> {
            Some(exit::READ)
        }
    }

    #[test]
    fn a_declined_prompt_reads_nothing_and_says_so() {
        let mut phases = Vec::new();
        let err = run_with('C', None, &mut |p| phases.push(p.phase), |_| -> Result<Never> {
            Err(Error::Refused(
                "administrator approval was declined, so nothing was scanned".into(),
            ))
        })
        .expect_err("declined");
        assert!(matches!(err, Error::Refused(_)), "{err:?}");
        assert_eq!(phases, [ScanPhase::Approval]);
    }

    #[test]
    fn a_reader_that_dies_before_connecting_fails_instead_of_hanging() {
        let started = Instant::now();
        let err = run_with('C', None, &mut |_| {}, |args| {
            assert!(args.starts_with(TURBO_ARG));
            Ok(Never)
        })
        .expect_err("the reader exited");
        assert!(matches!(err, Error::Os { .. }), "{err:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn an_impostor_on_the_pipe_is_refused() {
        // The launch connects to the pipe from this process, whose ID is not
        // the one the fake child reports.
        struct Liar;
        impl Child for Liar {
            fn id(&self) -> u32 {
                u32::MAX - 1
            }
            fn exited(&mut self) -> Option<u32> {
                None
            }
        }
        let mut holder = None;
        let err = run_with('C', None, &mut |_| {}, |args| {
            let pipe = args.rsplit(' ').next().expect("pipe").to_owned();
            let name = wide(&pipe);
            // SAFETY: NUL-terminated name.
            let h = unsafe { create_file_read(name.as_ptr(), GENERIC_WRITE, 0, 0) }
                .expect("connect");
            holder = Some(Owned(h));
            Ok(Liar)
        })
        .expect_err("refused");
        assert!(matches!(err, Error::Refused(_)), "{err:?}");
    }
}
