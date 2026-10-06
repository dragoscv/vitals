//! Directory traversal: parallel, and one system call per folder batch.
//!
//! The portable path: no elevation, works on every filesystem Windows can
//! mount, and correct on a subtree rather than only on a whole volume. See
//! [`super::mft`] for the elevated whole-volume path.
//!
//! # Why it is shaped like this
//!
//! The walker this replaced used `FindFirstFileExW`, opened every file to
//! read its identity for hard-link detection, and ran on one thread. Measured
//! on this machine (2026-09-28, release build, unelevated): 1,225 files a
//! second on `C:\Program Files`, and about 340 a second on the whole of `C:`,
//! which had not finished after seventeen minutes. Three changes, each for a
//! measured reason:
//!
//! - **`GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)`** with a
//!   256 KB buffer. One call returns hundreds of entries *with* their
//!   allocation size, file ID, attributes and reparse tag, so no file is ever
//!   opened. That removes the `CreateFileW` + `GetFileInformationByHandle` +
//!   `CloseHandle` per file the old hard-link check needed, and the
//!   `GetCompressedFileSizeW` per compressed file: NTFS reports compressed and
//!   sparse occupancy in `AllocationSize` directly.
//! - **Several threads.** Listing a directory is latency-bound, not
//!   CPU-bound, so an `NVMe` drive answers many at once. Workers pull
//!   directories from a shared queue and push the subdirectories they find;
//!   the calling thread alone owns the tree and merges each listing as it
//!   arrives. Nothing in the tree is shared, so nothing in it is locked.
//! - **No depth limit.** The old "quick" scan stopped three levels down and
//!   dropped every byte below from the totals, so the default scan of `C:`
//!   under-reported by hundreds of gigabytes while saying it was incomplete.
//!   A fast full scan makes the preview unnecessary.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES,
    ERROR_PATH_NOT_FOUND, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ID_EXTD_DIR_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileIdExtdDirectoryInfo, GetDiskFreeSpaceW, GetFileInformationByHandleEx,
};

use vitals_core::units::Bytes;

use super::ffi::create_file_read;
use super::sizing::{FileIdentity, LinkTracker, SharedFile, SkipReason, reconcile_reported};
use super::tree::{NodeId, SizeTree};

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
/// Directory listing access, plus the right to wait on the handle, which the
/// synchronous `GetFileInformationByHandleEx` needs.
const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
const SYNCHRONIZE: u32 = 0x0010_0000;
/// Required to open a *directory* handle at all.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// Opens a reparse point itself rather than whatever it points at.
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

/// Cloud-file reparse tags (`IO_REPARSE_TAG_CLOUD` .. `_CLOUD_F`). A folder
/// with one of these is an ordinary folder a sync engine manages, not a link
/// to somewhere else, so it is entered. The old walker skipped every
/// reparse-point folder, which removed the whole of `OneDrive` from the totals.
const IO_REPARSE_TAG_CLOUD_MASK: u32 = 0xFFFF_0FFF;
const IO_REPARSE_TAG_CLOUD: u32 = 0x9000_001A;

/// Bytes per directory query. 256 KB holds a few thousand entries; the gain
/// flattens beyond this and each worker keeps one.
const LIST_BUFFER_BYTES: usize = 256 * 1024;

/// How the scan should behave.
#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    /// Count a hard-linked file once. Costs about 20 bytes per file for the
    /// length of the scan and nothing else, since the file ID comes free with
    /// the directory listing.
    pub detect_hard_links: bool,
    /// Worker threads. `None` picks from the core count.
    pub threads: Option<usize>,
    /// Minimum time between progress callbacks.
    pub progress_interval: Duration,
    /// How many of the largest individual files to remember, by name.
    ///
    /// Files have no tree nodes (see [`SizeTree::add_file`]), so this bounded
    /// list is the only place a file's name survives the scan. Zero keeps
    /// none and costs nothing.
    pub largest_files: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            detect_hard_links: true,
            threads: None,
            progress_interval: Duration::from_millis(100),
            largest_files: 1000,
        }
    }
}

impl ScanOptions {
    fn worker_count(&self) -> usize {
        self.threads.unwrap_or_else(|| {
            // Listing is I/O latency, not CPU: more threads than cores keeps
            // an NVMe queue full. Capped because a spinning disk gains nothing
            // past a handful and every thread holds a 256 KB buffer.
            std::thread::available_parallelism()
                .map_or(4, std::num::NonZeroUsize::get)
                .clamp(2, 16)
        })
    }
}

/// What a running scan is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    /// Waiting for the user to answer the administrator prompt.
    Approval,
    /// Reading the drive's file table.
    Reading,
    /// Turning what was read into folders.
    Building,
    /// Reading the change journal to find what changed since a saved index.
    Journal,
    /// Listing folders.
    Walking,
}

impl ScanPhase {
    /// Stable key for the UI, never `{:?}`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Reading => "reading",
            Self::Building => "building",
            Self::Journal => "journal",
            Self::Walking => "walking",
        }
    }
}

/// A snapshot handed to the progress callback.
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub files_seen: u64,
    pub directories_seen: u64,
    pub bytes_seen: u64,
    pub elapsed_ms: u64,
    /// The directory most recently merged, for "now reading …".
    pub current_path: String,
    pub phase: ScanPhase,
    /// How far through, when that is known: reading a file table has a
    /// known length, a walk does not.
    pub fraction: Option<f64>,
}

impl ScanProgress {
    /// Files per second so far, or `None` before any measurable time has
    /// passed.
    #[must_use]
    pub fn files_per_second(&self) -> Option<f64> {
        if self.elapsed_ms == 0 {
            return None;
        }
        Some(self.files_seen as f64 * 1000.0 / self.elapsed_ms as f64)
    }
}

/// Cancellation and progress reporting for a running scan.
///
/// The scan runs on the caller's thread and spawns its own scoped workers,
/// which have all ended by the time [`scan_directory`] returns.
#[derive(Default)]
pub struct ScanControl<'a> {
    /// Checked by every worker before each directory and between listing
    /// batches, so a stop lands within one batch. Setting it yields a
    /// partial, honestly-flagged result.
    pub cancel: Option<&'a AtomicBool>,
    pub progress: Option<&'a mut dyn FnMut(ScanProgress)>,
}

// `dyn FnMut` cannot derive Debug, and the workspace denies missing Debug
// impls; the callback's identity is not interesting anyway.
impl std::fmt::Debug for ScanControl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanControl")
            .field("cancel", &self.cancel.is_some())
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

/// How a scan's figures were obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMethod {
    /// Every folder was listed.
    Walk,
    /// Read out of the NTFS file table by an elevated helper.
    Turbo,
    /// Folders unchanged since a saved index were taken from it; the rest
    /// were listed.
    Incremental,
}

/// The outcome of a scan.
#[derive(Debug)]
pub struct ScanResult {
    pub tree: SizeTree,
    /// Cluster size of the volume the root sits on, if it could be read.
    ///
    /// `None` disables rounding of the few sizes the filesystem does not
    /// report already aligned. Inventing a 4 KB cluster for an unmeasured
    /// volume would be a guess presented as a measurement.
    pub cluster_bytes: Option<u64>,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    /// Entries suppressed as repeat sightings of a hard-linked file.
    pub hard_link_duplicates: u64,
    /// Bytes those suppressions kept out of the total.
    pub hard_link_bytes_saved: u64,
    /// True when the cancellation token was set before traversal finished, so
    /// every figure here is a lower bound.
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// Worker threads the scan used.
    pub threads: usize,
    /// The largest individual files, largest first, at most
    /// [`ScanOptions::largest_files`].
    pub largest_files: Vec<LargeFile>,
    pub method: ScanMethod,
    /// Each folder's file ID, indexed by node: what the change journal
    /// names a folder by, so a saved index can tell which ones changed.
    /// Empty when the IDs were not recorded.
    pub dir_ids: Vec<u64>,
    /// Files seen under more than one name, by node.
    pub shared_files: Vec<SharedFile>,
    /// For [`ScanMethod::Incremental`]: folders taken from the saved index,
    /// and folders listed because they changed or are new.
    pub reused_directories: Option<u64>,
    pub relisted_directories: Option<u64>,
}

/// One of the largest files a scan saw.
///
/// Addressed by its folder's node plus its own name rather than a full path:
/// the path is rebuilt only for the rows a UI shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeFile {
    pub dir: NodeId,
    pub name: String,
    pub allocated: Bytes,
    pub logical: Bytes,
}

impl ScanResult {
    /// The full path of one of [`ScanResult::largest_files`].
    #[must_use]
    pub fn path_of_file(&self, file: &LargeFile) -> String {
        let dir = self.tree.path_of(file.dir);
        if dir.ends_with('\\') {
            format!("{dir}{}", file.name)
        } else {
            format!("{dir}\\{}", file.name)
        }
    }

    /// Files per second achieved, or `None` if the scan was too quick to
    /// measure.
    #[must_use]
    pub fn files_per_second(&self) -> Option<f64> {
        if self.elapsed_ms == 0 {
            return None;
        }
        Some(self.files_scanned as f64 * 1000.0 / self.elapsed_ms as f64)
    }

    /// Whether every figure can be trusted as complete.
    ///
    /// False when the scan was cancelled or any directory could not be read.
    /// Links that were recorded rather than followed do not count against
    /// it: they leave nothing out (see [`SkipReason::leaves_a_gap`]).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.cancelled && self.gaps() == 0
    }

    /// Directories whose contents are missing from the totals.
    #[must_use]
    pub fn gaps(&self) -> usize {
        self.tree
            .skipped()
            .iter()
            .filter(|s| s.reason.leaves_a_gap())
            .count()
    }

    /// On-disk size of the scanned root.
    #[must_use]
    pub fn allocated(&self) -> Bytes {
        self.tree
            .node(self.tree.root())
            .map_or(Bytes::ZERO, super::tree::Node::allocated)
    }

    /// Sum of file lengths under the scanned root.
    #[must_use]
    pub fn logical(&self) -> Bytes {
        self.tree
            .node(self.tree.root())
            .map_or(Bytes::ZERO, super::tree::Node::logical)
    }

    /// The folder at `path` in this scan, matched case-insensitively.
    #[must_use]
    pub fn find_directory(&self, path: &str) -> Option<NodeId> {
        let wanted = super::protect::normalise(path)?;
        let root = super::protect::normalise(&self.tree.path_of(self.tree.root()))?;
        if wanted == root {
            return Some(self.tree.root());
        }
        let rest = wanted.strip_prefix(&root)?.strip_prefix('\\')?;
        let mut cursor = self.tree.root();
        for part in rest.split('\\') {
            cursor = self
                .tree
                .children(cursor)
                .into_iter()
                .find(|&c| self.tree.name_of(c).to_lowercase() == part)?;
        }
        (!self.tree.is_detached(cursor)).then_some(cursor)
    }

    /// Takes an item that went to the Recycle Bin out of this scan.
    ///
    /// A folder is detached with everything in it; a file is found among
    /// [`ScanResult::largest_files`], the only files a scan knows by name.
    /// Every ancestor's total goes down by what the item held, and those
    /// bytes are added to the drive's `$Recycle.Bin` when this scan contains
    /// it: the space is still in use until the bin is emptied, and a drive
    /// total that dropped would say otherwise.
    ///
    /// Returns what was moved, or `None` when the scan does not contain the
    /// item (nothing changes then).
    pub fn forget_recycled(&mut self, path: &str) -> Option<super::tree::Amount> {
        use super::tree::Amount;
        let moved = if let Some(dir) = self.find_directory(path)
            && dir != self.tree.root()
        {
            let amount = self.tree.detach(dir)?;
            let tree = &self.tree;
            self.largest_files.retain(|f| !tree.is_detached(f.dir));
            amount
        } else {
            let wanted = super::protect::normalise(path)?;
            let index = self.largest_files.iter().position(|f| {
                super::protect::normalise(&self.path_of_file(f)).as_deref() == Some(&wanted)
            })?;
            let file = self.largest_files.remove(index);
            self.tree
                .remove_file(file.dir, file.allocated.get(), file.logical.get());
            Amount {
                allocated: file.allocated.get(),
                logical: file.logical.get(),
                files: 1,
            }
        };

        let root = self.tree.path_of(self.tree.root());
        let bin = format!("{}\\$Recycle.Bin", root.get(..2).unwrap_or(""));
        if let Some(bin) = self.find_directory(&bin) {
            self.tree.add_to(bin, moved);
        }
        Some(moved)
    }
}

/// A handle that closes itself.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful CreateFileW and is closed
        // exactly once, at drop.
        unsafe { CloseHandle(self.0) };
    }
}

/// Converts a Rust path into a NUL-terminated UTF-16 buffer.
///
/// Prefixed with `\\?\` when absolute: without it every path is capped at 260
/// characters, and a scanner that silently stops at `node_modules` five
/// levels down is not a scanner.
fn to_wide_extended(path: &str) -> Vec<u16> {
    let needs_prefix = !path.starts_with("\\\\?\\") && !path.starts_with("\\\\") && {
        let bytes = path.as_bytes();
        bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
    };

    let mut out: Vec<u16> = Vec::with_capacity(path.len() + 8);
    if needs_prefix {
        out.extend("\\\\?\\".encode_utf16());
    }
    out.extend(path.encode_utf16());
    out.push(0);
    out
}

/// Reads the cluster size of the volume holding `path`.
///
/// Returns `None` when the volume cannot be queried — a disconnected share,
/// a removed card.
fn cluster_size(path: &Path) -> Option<u64> {
    // GetDiskFreeSpaceW wants a root, not an arbitrary path.
    let root = path.components().next().map(|c| {
        let mut s = c.as_os_str().to_string_lossy().into_owned();
        if !s.ends_with('\\') {
            s.push('\\');
        }
        s
    })?;

    let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
    let mut sectors_per_cluster: u32 = 0;
    let mut bytes_per_sector: u32 = 0;
    let mut free_clusters: u32 = 0;
    let mut total_clusters: u32 = 0;

    // SAFETY: `wide` is a NUL-terminated UTF-16 root path that outlives the
    // call; the four out-pointers reference live u32s.
    let ok = unsafe {
        GetDiskFreeSpaceW(
            wide.as_ptr(),
            &raw mut sectors_per_cluster,
            &raw mut bytes_per_sector,
            &raw mut free_clusters,
            &raw mut total_clusters,
        )
    };

    if ok == 0 {
        return None;
    }

    let cluster = u64::from(sectors_per_cluster) * u64::from(bytes_per_sector);
    (cluster > 0).then_some(cluster)
}

/// Whether an entry is `.` or `..`, which must never be descended into.
fn is_dot_entry(name: &[u16]) -> bool {
    matches!(name, [0x2E] | [0x2E, 0x2E])
}

/// The cluster size of the volume holding `root`, for a scanner that did not
/// walk it.
pub(super) fn cluster_bytes_of(root: &str) -> Option<u64> {
    cluster_size(Path::new(root))
}

/// The root a caller meant, as a path that cannot be misread.
///
/// `C:` without a backslash is not the drive: Windows reads it as *the
/// current directory on C*, which for a GUI process is wherever it was
/// launched from. The drive list reports mounts as `C:`, so the Scan button
/// walked some unrelated folder and reported its eight files as the drive
/// (found live, 2026-09-29). Fixed here rather than in the UI so every caller
/// — CLI, LAN, a future one — gets the drive it asked for.
pub(super) fn normalise_root(root: &str) -> String {
    let bytes = root.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        format!("{root}\\")
    } else {
        root.to_owned()
    }
}

/// Whether a directory's reparse point should be traversed.
///
/// Only cloud-provider folders are: they hold this volume's own files. Every
/// other tag — junction, symlink, mount point, `AppExecLink`, WSL — either
/// points somewhere already counted, at another volume, or at an ancestor.
pub(super) fn is_traversable_reparse(tag: u32) -> bool {
    tag & IO_REPARSE_TAG_CLOUD_MASK == IO_REPARSE_TAG_CLOUD
}

/// One entry of a directory listing, already sized.
struct ListedFile {
    allocated: u64,
    logical: u64,
    /// Kept only when the file may be among the largest; see [`ListContext`].
    name: Option<Box<str>>,
}

/// What every worker needs to list a directory, shared read-only.
#[derive(Clone, Copy)]
struct ListContext<'a> {
    links: Option<&'a LinkTracker>,
    cluster: u64,
    cancel: Option<&'a AtomicBool>,
    /// The smallest size that can still enter the largest-files list.
    ///
    /// Raised by the merging thread as the list fills; workers read it with
    /// `Relaxed` because a stale, lower value only means one extra name is
    /// copied, never that a large file is missed — the floor only rises, so
    /// any file above the final floor was above every earlier one.
    name_floor: &'a AtomicU64,
}

/// A subdirectory found while listing.
struct ListedDir {
    name: String,
    /// Its file ID, low 64 bits: the whole NTFS file reference.
    id: u64,
    /// `Some` when it must be recorded rather than entered.
    skip: Option<SkipReason>,
}

/// What a worker found in one directory.
struct Listing {
    /// The tree node the directory belongs to, echoed back so the owner
    /// thread knows where to merge.
    node: NodeId,
    path: String,
    files: Vec<ListedFile>,
    dirs: Vec<ListedDir>,
    /// Set when the directory itself could not be read.
    failed: Option<SkipReason>,
    /// Set when the listing stopped part-way because the scan was cancelled.
    interrupted: bool,
}

/// Lists one directory with bulk directory-information queries.
fn list_directory(
    node: NodeId,
    path: String,
    buffer: &mut [u64],
    context: ListContext<'_>,
) -> Listing {
    let mut listing = Listing {
        node,
        path,
        files: Vec::new(),
        dirs: Vec::new(),
        failed: None,
        interrupted: false,
    };

    let wide = to_wide_extended(&listing.path);
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
    let handle = unsafe {
        create_file_read(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY | SYNCHRONIZE,
            // Sharing everything: a scanner must never block another process
            // from writing to, renaming or deleting what it merely listed.
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )
    };
    let Some(handle) = handle.filter(|h| *h != INVALID_HANDLE_VALUE && !h.is_null()) else {
        listing.failed = Some(last_error_reason());
        return listing;
    };
    let handle = OwnedHandle(handle);

    let byte_len = u32::try_from(std::mem::size_of_val(buffer)).unwrap_or(u32::MAX);
    loop {
        if context
            .cancel
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            listing.interrupted = true;
            break;
        }
        // SAFETY: `handle` is a live directory handle opened with
        // FILE_LIST_DIRECTORY; `buffer` is a live, 8-byte-aligned region of
        // `byte_len` bytes the kernel fills with FILE_ID_EXTD_DIR_INFO
        // records. Each call continues the enumeration where the last ended.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                handle.0,
                FileIdExtdDirectoryInfo,
                buffer.as_mut_ptr().cast(),
                byte_len,
            )
        };
        if ok == 0 {
            // SAFETY: no arguments, no preconditions.
            let code = unsafe { GetLastError() };
            if code != ERROR_NO_MORE_FILES {
                listing.failed = Some(reason_for(code));
            }
            break;
        }
        parse_batch(buffer, &mut listing, context);
    }

    listing
}

/// Walks the records of one filled buffer.
fn parse_batch(buffer: &[u64], listing: &mut Listing, context: ListContext<'_>) {
    let base = buffer.as_ptr().cast::<u8>();
    let limit = std::mem::size_of_val(buffer);
    let mut offset = 0_usize;

    loop {
        if offset + std::mem::size_of::<FILE_ID_EXTD_DIR_INFO>() > limit {
            break;
        }
        // SAFETY: the kernel wrote a chain of FILE_ID_EXTD_DIR_INFO records
        // starting at the buffer and linked by NextEntryOffset, each 8-byte
        // aligned; `offset` only ever follows that chain and is bounds-checked
        // against the buffer above. `read_unaligned` makes no alignment claim
        // regardless.
        let record = unsafe {
            base.add(offset)
                .cast::<FILE_ID_EXTD_DIR_INFO>()
                .read_unaligned()
        };
        let name_offset = offset + std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileName);
        let name_units = record.FileNameLength as usize / 2;
        if name_offset + name_units * 2 > limit {
            break;
        }
        // Copied out unit by unit with unaligned reads rather than viewed as
        // a `&[u16]` in place: the record chain is 8-byte aligned by
        // contract, but a slice would make that contract a soundness
        // requirement, and a name is at most 255 units.
        let name: Vec<u16> = (0..name_units)
            .map(|i| {
                // SAFETY: `name_offset + i * 2 + 2 <= limit` for every
                // `i < name_units`, checked above, so each read is in bounds.
                unsafe { base.add(name_offset + i * 2).cast::<u16>().read_unaligned() }
            })
            .collect();
        classify(&record, &name, listing, context);

        if record.NextEntryOffset == 0 {
            break;
        }
        offset += record.NextEntryOffset as usize;
    }
}

/// Sorts one record into a file or a subdirectory.
fn classify(
    record: &FILE_ID_EXTD_DIR_INFO,
    name: &[u16],
    listing: &mut Listing,
    context: ListContext<'_>,
) {
    if is_dot_entry(name) {
        return;
    }
    let attributes = record.FileAttributes;
    let is_reparse = attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;

    if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        let skip = (is_reparse && !is_traversable_reparse(record.ReparsePointTag))
            .then_some(SkipReason::ReparsePoint);
        listing.dirs.push(ListedDir {
            name: String::from_utf16_lossy(name),
            // Truncation intended: NTFS IDs are 64 bits, zero-extended.
            id: u128::from_le_bytes(record.FileId.Identifier) as u64,
            skip,
        });
        return;
    }

    let logical = u64::try_from(record.EndOfFile).unwrap_or(0);
    // A symbolic link to a file occupies nothing of its own; its target is
    // counted wherever it lives. A cloud placeholder reports what is
    // actually on disk here: zero for an online-only file.
    let allocated = if is_reparse && !is_traversable_reparse(record.ReparsePointTag) {
        0
    } else {
        // NTFS reports whole clusters already, including for compressed and
        // sparse files; ReFS and redirectors may not, hence the reconcile.
        reconcile_reported(
            u64::try_from(record.AllocationSize).unwrap_or(0),
            context.cluster,
        )
    };

    let counted = context.links.is_none_or(|tracker| {
        tracker.should_count(
            FileIdentity(u128::from_le_bytes(record.FileId.Identifier)),
            allocated,
            listing.node.0,
        )
    });
    // A repeat hard link is still a file the user can see; it is its bytes
    // that are not counted twice — and it cannot be listed as a large file,
    // or one file would fill the list under every name it has.
    listing.files.push(if counted {
        let candidate = allocated > 0 && allocated >= context.name_floor.load(Ordering::Relaxed);
        ListedFile {
            allocated,
            logical,
            name: candidate.then(|| String::from_utf16_lossy(name).into_boxed_str()),
        }
    } else {
        ListedFile {
            allocated: 0,
            logical: 0,
            name: None,
        }
    });
}

fn last_error_reason() -> SkipReason {
    // SAFETY: no arguments, no preconditions.
    reason_for(unsafe { GetLastError() })
}

fn reason_for(code: u32) -> SkipReason {
    match code {
        ERROR_ACCESS_DENIED => SkipReason::AccessDenied,
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => SkipReason::Vanished,
        other => SkipReason::OsError(other.cast_signed()),
    }
}

/// Directories waiting to be listed, shared by the workers.
///
/// A mutex and a condition variable rather than a lock-free deque: a
/// directory listing costs tens of microseconds of kernel time, so the queue
/// is touched a few thousand times a second at most and never contended
/// enough to matter. Measured, not assumed — see the module docs.
struct WorkQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

struct QueueState {
    pending: Vec<(NodeId, String)>,
    /// Directories handed out or queued and not yet merged. The scan is done
    /// when this reaches zero with the queue empty.
    outstanding: usize,
    closed: bool,
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                pending: Vec::new(),
                outstanding: 0,
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    fn push(&self, items: impl IntoIterator<Item = (NodeId, String)>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let before = state.pending.len();
        state.pending.extend(items);
        let added = state.pending.len() - before;
        state.outstanding += added;
        drop(state);
        for _ in 0..added {
            self.ready.notify_one();
        }
    }

    /// Takes the next directory, or `None` once the scan is over.
    fn pop(&self) -> Option<(NodeId, String)> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if state.closed {
                return None;
            }
            // Depth-first order: the newest directory is the deepest, which
            // keeps the queue small on a tree millions of entries wide.
            if let Some(item) = state.pending.pop() {
                return Some(item);
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Marks one directory merged; closes the queue when none remain.
    fn finish_one(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.outstanding = state.outstanding.saturating_sub(1);
        let done = state.outstanding == 0 && state.pending.is_empty();
        if done {
            state.closed = true;
        }
        drop(state);
        if done {
            self.ready.notify_all();
        }
        done
    }

    /// Stops every worker and returns what was never listed.
    fn close(&self) -> Vec<(NodeId, String)> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.closed = true;
        let left = std::mem::take(&mut state.pending);
        drop(state);
        self.ready.notify_all();
        left
    }
}

/// Running totals the owner thread keeps while merging.
#[derive(Default)]
struct Totals {
    files: u64,
    directories: u64,
    bytes: u64,
}

/// The `capacity` largest files seen so far, owned by the merging thread.
///
/// A min-heap of fixed size: each candidate costs O(log k), and the smallest
/// member is the floor workers compare against before copying a name.
struct LargestFiles<'a> {
    /// Ordered by size, then logical size, then name, so ties at the boundary
    /// resolve the same way whatever order the workers deliver listings in.
    /// The folder node is last and is the one key that depends on that order;
    /// it decides only between same-named files of identical size.
    heap: BinaryHeap<Reverse<(u64, u64, Box<str>, u32)>>,
    capacity: usize,
    floor: &'a AtomicU64,
}

impl<'a> LargestFiles<'a> {
    fn new(capacity: usize, floor: &'a AtomicU64) -> Self {
        // Nothing can enter a zero-capacity list, so no worker copies names.
        floor.store(if capacity == 0 { u64::MAX } else { 0 }, Ordering::Relaxed);
        Self {
            heap: BinaryHeap::with_capacity(capacity.min(4096) + 1),
            capacity,
            floor,
        }
    }

    fn offer(&mut self, dir: NodeId, allocated: u64, logical: u64, name: Box<str>) {
        if self.capacity == 0 {
            return;
        }
        let entry = (allocated, logical, name, dir.0);
        if self.heap.len() < self.capacity {
            self.heap.push(Reverse(entry));
        } else if self
            .heap
            .peek()
            .is_some_and(|Reverse(smallest)| entry > *smallest)
        {
            self.heap.pop();
            self.heap.push(Reverse(entry));
        } else {
            return;
        }
        if self.heap.len() == self.capacity
            && let Some(Reverse(smallest)) = self.heap.peek()
        {
            // Equal, not above: a file the same size can still win the tie.
            self.floor.store(smallest.0, Ordering::Relaxed);
        }
    }

    fn into_sorted(self) -> Vec<LargeFile> {
        // `into_sorted_vec` on `Reverse` is largest-first already.
        self.heap
            .into_sorted_vec()
            .into_iter()
            .map(|Reverse((allocated, logical, name, dir))| LargeFile {
                dir: NodeId(dir),
                name: name.into_string(),
                allocated: Bytes(allocated),
                logical: Bytes(logical),
            })
            .collect()
    }
}

/// Folders a walk may take from somewhere other than a listing.
///
/// The saved index implements it: an unchanged folder is copied from the
/// index instead of listed, with whatever below it is unchanged too.
pub(super) trait Reuse {
    /// Offers folder `id`, just added to `tree` as `node`. `None` when it
    /// must be listed. `Some(rest)` when it was filled in from elsewhere;
    /// `rest` are folders below it that changed and still need listing.
    fn graft(
        &mut self,
        id: u64,
        node: NodeId,
        tree: &mut SizeTree,
        dir_ids: &mut Vec<u64>,
    ) -> Option<Vec<(NodeId, String)>>;

    /// Adds everything the source knows that a listing cannot see again:
    /// the largest files and linked files of every folder it filled in.
    /// Called once, after the walk.
    fn finish(&mut self, result: &mut ScanResult);
}

/// Scans a directory tree.
///
/// # Errors
///
/// Never fails as a whole. An unreadable directory is recorded in
/// [`SizeTree::skipped`] with a reason and excluded from every total; a total
/// that quietly omits an unreadable folder is a wrong number presented as a
/// right one.
pub fn scan_directory(
    root: &Path,
    options: ScanOptions,
    control: &mut ScanControl<'_>,
) -> ScanResult {
    walk(root, options, control, None)
}

/// The file ID of a folder, as the change journal names it.
fn directory_id(path: &str) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let wide = to_wide_extended(path);
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
    let handle = unsafe {
        create_file_read(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_FLAG_BACKUP_SEMANTICS,
        )
    }
    .filter(|h| *h != INVALID_HANDLE_VALUE && !h.is_null())?;
    let handle = OwnedHandle(handle);
    // SAFETY: all-zero is a valid BY_HANDLE_FILE_INFORMATION.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `handle` is live and `info` is a live out-pointer.
    if unsafe { GetFileInformationByHandle(handle.0, &raw mut info) } == 0 {
        return None;
    }
    Some(u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow))
}

/// [`scan_directory`], taking unchanged folders from `reuse` where it can.
pub(super) fn walk(
    root: &Path,
    options: ScanOptions,
    control: &mut ScanControl<'_>,
    mut reuse: Option<&mut dyn Reuse>,
) -> ScanResult {
    let started = Instant::now();
    let root_display = normalise_root(&root.to_string_lossy());
    let root = Path::new(&root_display);
    let cluster_bytes = cluster_size(root);
    let cluster = cluster_bytes.unwrap_or(0);
    let threads = options.worker_count();

    let mut tree = SizeTree::new(&root_display);
    let links = LinkTracker::new();
    let queue = WorkQueue::new();
    let active = AtomicUsize::new(0);
    let listed = AtomicU64::new(0);
    let name_floor = AtomicU64::new(0);
    let mut largest = LargestFiles::new(options.largest_files, &name_floor);
    let cancel = control.cancel;
    let context = ListContext {
        links: options.detect_hard_links.then_some(&links),
        cluster,
        cancel,
        name_floor: &name_floor,
    };

    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        tree.mark_skipped(tree.root(), root_display, SkipReason::Cancelled);
        tree.aggregate();
        return ScanResult {
            tree,
            cluster_bytes,
            files_scanned: 0,
            directories_scanned: 0,
            hard_link_duplicates: 0,
            hard_link_bytes_saved: 0,
            cancelled: true,
            elapsed_ms: started.elapsed().as_millis() as u64,
            threads,
            largest_files: Vec::new(),
            method: ScanMethod::Walk,
            dir_ids: Vec::new(),
            shared_files: Vec::new(),
            reused_directories: None,
            relisted_directories: None,
        };
    }

    let root_id = directory_id(&root_display).unwrap_or(0);
    let mut dir_ids = vec![root_id];
    let first = match reuse.as_deref_mut() {
        Some(source) => source
            .graft(root_id, tree.root(), &mut tree, &mut dir_ids)
            .unwrap_or_else(|| vec![(tree.root(), root_display.clone())]),
        None => vec![(tree.root(), root_display.clone())],
    };
    let (sender, receiver) = mpsc::channel::<Listing>();
    let mut totals = Totals::default();
    let mut cancelled = false;
    // Nothing changed at all: every folder came from the index, and a queue
    // that starts empty would leave the workers waiting for work forever.
    let nothing_to_list = first.is_empty();
    queue.push(first);
    if nothing_to_list {
        queue.close();
    }

    std::thread::scope(|scope| {
        if nothing_to_list {
            return;
        }
        for _ in 0..threads {
            let sender = sender.clone();
            let queue = &queue;
            let active = &active;
            let listed = &listed;
            scope.spawn(move || {
                // u64 words so the buffer is 8-byte aligned, which the kernel
                // requires of FILE_ID_EXTD_DIR_INFO output.
                let mut buffer = vec![0_u64; LIST_BUFFER_BYTES / 8];
                while let Some((node, path)) = queue.pop() {
                    active.fetch_add(1, Ordering::Relaxed);
                    let listing = list_directory(node, path, &mut buffer, context);
                    listed.fetch_add(1, Ordering::Relaxed);
                    active.fetch_sub(1, Ordering::Relaxed);
                    if sender.send(listing).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);

        cancelled = merge_listings(
            &receiver,
            &queue,
            Sink {
                tree: &mut tree,
                totals: &mut totals,
                largest: &mut largest,
                dir_ids: &mut dir_ids,
                reuse: reuse.as_deref_mut(),
            },
            control,
            started,
            options.progress_interval,
        );
        // Everything still queued was never read. Recorded, not dropped: a
        // cancelled total is a lower bound and must say where it stops.
        for (node, path) in queue.close() {
            tree.mark_skipped(node, path, SkipReason::Cancelled);
        }
    });

    // After the scope, so every worker has joined: a listing sent after the
    // stop names a directory that was read but never merged, and it is
    // marked rather than silently absent.
    for listing in receiver.try_iter() {
        tree.mark_skipped(listing.node, listing.path, SkipReason::Cancelled);
    }

    tree.aggregate();
    dir_ids.resize(tree.len(), 0);

    let mut result = ScanResult {
        tree,
        cluster_bytes,
        files_scanned: totals.files,
        directories_scanned: totals.directories,
        hard_link_duplicates: links.duplicates(),
        hard_link_bytes_saved: links.duplicate_bytes(),
        cancelled,
        elapsed_ms: 0,
        threads,
        largest_files: largest.into_sorted(),
        method: ScanMethod::Walk,
        dir_ids,
        shared_files: links.shared(),
        reused_directories: None,
        relisted_directories: None,
    };
    if let Some(source) = reuse {
        source.finish(&mut result);
    }
    result.elapsed_ms = started.elapsed().as_millis() as u64;
    result
}

/// Everything a merged listing writes into, owned by the merging thread.
struct Sink<'s, 'f> {
    tree: &'s mut SizeTree,
    totals: &'s mut Totals,
    largest: &'s mut LargestFiles<'f>,
    dir_ids: &'s mut Vec<u64>,
    reuse: Option<&'s mut dyn Reuse>,
}

/// Merges listings into the tree until the walk ends or is cancelled.
///
/// Returns whether it was cancelled.
fn merge_listings(
    receiver: &mpsc::Receiver<Listing>,
    queue: &WorkQueue,
    sink: Sink<'_, '_>,
    control: &mut ScanControl<'_>,
    started: Instant,
    interval: Duration,
) -> bool {
    let Sink {
        tree,
        totals,
        largest,
        dir_ids,
        mut reuse,
    } = sink;
    let mut last_report = Instant::now();
    let mut last_path = String::new();

    loop {
        let listing = match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(listing) => listing,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if control
                    .cancel
                    .is_some_and(|flag| flag.load(Ordering::Relaxed))
                {
                    return true;
                }
                continue;
            }
            // Every worker exited: the queue closed because the walk is done.
            Err(mpsc::RecvTimeoutError::Disconnected) => return false,
        };

        let interrupted = listing.interrupted;
        merge_one(
            listing,
            queue,
            Sink {
                tree: &mut *tree,
                totals: &mut *totals,
                largest: &mut *largest,
                dir_ids: &mut *dir_ids,
                reuse: reuse.as_deref_mut(),
            },
            &mut last_path,
        );

        if interrupted
            || control
                .cancel
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return true;
        }
        if last_report.elapsed() >= interval
            && let Some(callback) = control.progress.as_mut()
        {
            last_report = Instant::now();
            callback(ScanProgress {
                files_seen: totals.files,
                directories_seen: totals.directories,
                bytes_seen: totals.bytes,
                elapsed_ms: started.elapsed().as_millis() as u64,
                current_path: last_path.clone(),
                phase: ScanPhase::Walking,
                fraction: None,
            });
        }
        if queue.finish_one() {
            return false;
        }
    }
}

/// Stores a folder's ID at its node's index.
pub(super) fn record_id(dir_ids: &mut Vec<u64>, node: NodeId, id: u64) {
    let index = node.0 as usize;
    if dir_ids.len() <= index {
        dir_ids.resize(index + 1, 0);
    }
    dir_ids[index] = id;
}

/// Adds one directory's listing to the tree and queues its subdirectories.
fn merge_one(listing: Listing, queue: &WorkQueue, sink: Sink<'_, '_>, last_path: &mut String) {
    let Sink {
        tree,
        totals,
        largest,
        dir_ids,
        mut reuse,
    } = sink;
    totals.directories += 1;

    if listing.interrupted {
        tree.mark_skipped(listing.node, listing.path, SkipReason::Cancelled);
        return;
    }
    if let Some(reason) = listing.failed
        && listing.files.is_empty()
        && listing.dirs.is_empty()
    {
        tree.mark_skipped(listing.node, listing.path, reason);
        return;
    }

    totals.files += listing.files.len() as u64;
    for file in listing.files {
        tree.add_file(listing.node, file.allocated, file.logical);
        totals.bytes = totals.bytes.saturating_add(file.allocated);
        if let Some(name) = file.name {
            largest.offer(listing.node, file.allocated, file.logical, name);
        }
    }

    let base = listing.path.trim_end_matches('\\');
    let mut next = Vec::with_capacity(listing.dirs.len());
    for dir in listing.dirs {
        let node = tree.add_child(listing.node, &dir.name);
        record_id(dir_ids, node, dir.id);
        let path = format!("{base}\\{}", dir.name);
        if let Some(reason) = dir.skip {
            tree.mark_skipped(node, path, reason);
            continue;
        }
        match reuse
            .as_deref_mut()
            .and_then(|source| source.graft(dir.id, node, tree, dir_ids))
        {
            Some(rest) => next.extend(rest),
            None => next.push((node, path)),
        }
    }
    // A listing that failed part-way still contributed what it read, and is
    // flagged so its total is shown as a lower bound.
    if let Some(reason) = listing.failed {
        tree.mark_skipped(listing.node, listing.path.clone(), reason);
    }
    queue.push(next);
    last_path.clone_from(&listing.path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn the_directory_record_layout_is_pinned() {
        // Records are read out of a kernel-filled buffer by offset; a layout
        // mismatch would read every file's size from the wrong bytes.
        assert_eq!(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, EndOfFile), 40);
        assert_eq!(
            std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, AllocationSize),
            48
        );
        assert_eq!(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileId), 72);
        assert_eq!(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileName), 88);
    }

    #[test]
    fn long_paths_get_the_extended_prefix() {
        let wide = to_wide_extended("C:\\Windows\\System32");
        let text = String::from_utf16_lossy(&wide[..wide.len() - 1]);
        assert_eq!(text, "\\\\?\\C:\\Windows\\System32");
    }

    #[test]
    fn unc_and_already_prefixed_paths_are_left_alone() {
        for path in ["\\\\?\\C:\\x", "\\\\server\\share"] {
            let wide = to_wide_extended(path);
            let text = String::from_utf16_lossy(&wide[..wide.len() - 1]);
            assert_eq!(text, path);
        }
    }

    #[test]
    fn relative_paths_are_not_prefixed() {
        // `\\?\` demands a fully-qualified path; applying it to a relative
        // one produces a path the kernel cannot resolve at all.
        let wide = to_wide_extended("subdir");
        assert_eq!(String::from_utf16_lossy(&wide[..wide.len() - 1]), "subdir");
    }

    #[test]
    fn dot_entries_are_recognised() {
        let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert!(is_dot_entry(&wide(".")));
        assert!(is_dot_entry(&wide("..")));
        assert!(!is_dot_entry(&wide("...weird but legal")));
        assert!(!is_dot_entry(&wide(".git")));
    }

    #[test]
    fn a_bare_drive_letter_means_the_drive_not_its_current_directory() {
        assert_eq!(normalise_root("C:"), "C:\\");
        assert_eq!(normalise_root("d:"), "d:\\");
        assert_eq!(normalise_root("C:\\"), "C:\\");
        assert_eq!(normalise_root("C:\\Users"), "C:\\Users");
        assert_eq!(normalise_root("\\\\server\\share"), "\\\\server\\share");
    }

    #[test]
    fn scanning_a_bare_drive_letter_walks_the_drive_root() {
        // The system drive's root holds `Windows`; the test process's current
        // directory on C: (the crate folder, or wherever cargo ran) does not.
        // Cancelled after the first listing so this reads one folder, not a
        // drive.
        let Ok(system) = std::env::var("SystemDrive") else {
            return;
        };
        let cancel = AtomicBool::new(false);
        let mut stop = |_: ScanProgress| cancel.store(true, Ordering::Relaxed);
        let mut control = ScanControl {
            cancel: Some(&cancel),
            progress: Some(&mut stop),
        };
        let options = ScanOptions {
            threads: Some(1),
            progress_interval: Duration::ZERO,
            ..ScanOptions::default()
        };
        let result = scan_directory(Path::new(&system), options, &mut control);
        let tree = &result.tree;
        assert_eq!(tree.path_of(tree.root()), format!("{system}\\"));
        let names: Vec<_> = tree
            .children(tree.root())
            .iter()
            .map(|id| tree.name_of(*id).to_ascii_lowercase())
            .collect();
        assert!(names.iter().any(|n| n == "windows"), "{names:?}");
    }

    #[test]
    fn only_cloud_folders_are_entered_among_reparse_points() {
        // OneDrive (CLOUD_6 is what current clients set) and the plain tag.
        assert!(is_traversable_reparse(0x9000_601A));
        assert!(is_traversable_reparse(0x9000_001A));
        // Junction, symlink and AppExecLink must never be followed.
        assert!(!is_traversable_reparse(0xA000_0003));
        assert!(!is_traversable_reparse(0xA000_000C));
        assert!(!is_traversable_reparse(0x8000_001B));
    }

    #[test]
    fn the_system_volume_reports_a_power_of_two_cluster() {
        let Some(cluster) = cluster_size(Path::new("C:\\")) else {
            // A machine without C: is unusual but not a test failure.
            return;
        };
        assert!(cluster.is_power_of_two(), "got {cluster}");
        assert!((512..=2 * 1024 * 1024).contains(&cluster), "got {cluster}");
    }

    /// A private directory tree with known contents, removed on drop.
    struct Fixture {
        root: std::path::PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("vitals-scan-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("a\\b\\c\\d\\e")).expect("create nested");
            std::fs::create_dir_all(root.join("wide")).expect("create wide");
            std::fs::write(root.join("top.bin"), vec![1_u8; 10_000]).expect("write top");
            std::fs::write(root.join("a\\b\\c\\d\\e\\deep.bin"), vec![2_u8; 5_000])
                .expect("write deep");
            for i in 0..300 {
                std::fs::write(root.join(format!("wide\\f{i}.txt")), b"x").expect("write wide");
            }
            Self { root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn scan(path: &Path, options: ScanOptions) -> ScanResult {
        let mut control = ScanControl::default();
        scan_directory(path, options, &mut control)
    }

    #[test]
    fn every_file_at_every_depth_is_counted() {
        let fixture = Fixture::new("depth");
        let result = scan(&fixture.root, ScanOptions::default());

        assert!(result.is_complete(), "skipped: {:?}", result.tree.skipped());
        assert_eq!(result.gaps(), 0);
        assert_eq!(result.files_scanned, 302);
        assert_eq!(
            result.logical().get(),
            10_000 + 5_000 + 300,
            "the file five levels down must be in the total"
        );
        assert!(result.allocated() >= result.logical());
    }

    #[test]
    fn a_recycled_folder_comes_out_of_the_kept_scan_without_a_rescan() {
        let fixture = Fixture::new("forget-dir");
        let mut result = scan(&fixture.root, ScanOptions::default());
        let before = result.logical().get();
        let wide = format!("{}\\WIDE", fixture.root.display());

        let moved = result.forget_recycled(&wide).expect("wide is in the scan");
        assert_eq!(moved.files, 300);
        assert_eq!(result.logical().get(), before - 300);
        assert!(result.find_directory(&wide).is_none());
        assert!(
            result.forget_recycled(&wide).is_none(),
            "a second report of the same item changes nothing"
        );
    }

    #[test]
    fn a_recycled_large_file_leaves_its_folder_and_the_list() {
        let fixture = Fixture::new("forget-file");
        let mut result = scan(&fixture.root, ScanOptions::default());
        let top = format!("{}\\top.bin", fixture.root.display());
        let before = result.logical().get();

        let moved = result.forget_recycled(&top).expect("top.bin is listed");
        assert_eq!(moved.logical, 10_000);
        assert_eq!(result.logical().get(), before - 10_000);
        assert!(result.largest_files.iter().all(|f| f.name != "top.bin"));
        assert!(
            result
                .forget_recycled(&format!("{}\\nope.bin", fixture.root.display()))
                .is_none()
        );
    }

    #[test]
    fn one_thread_and_many_threads_agree_exactly() {
        let fixture = Fixture::new("threads");
        let one = scan(
            &fixture.root,
            ScanOptions {
                threads: Some(1),
                ..Default::default()
            },
        );
        let many = scan(
            &fixture.root,
            ScanOptions {
                threads: Some(8),
                ..Default::default()
            },
        );
        assert_eq!(one.files_scanned, many.files_scanned);
        assert_eq!(one.allocated(), many.allocated());
        assert_eq!(one.logical(), many.logical());
        assert_eq!(one.tree.len(), many.tree.len());
        assert_eq!(
            one.largest_files
                .iter()
                .map(|f| one.path_of_file(f))
                .collect::<Vec<_>>(),
            many.largest_files
                .iter()
                .map(|f| many.path_of_file(f))
                .collect::<Vec<_>>(),
            "the largest-files list must not depend on which worker read what"
        );
    }

    #[test]
    fn the_largest_files_are_named_largest_first_and_bounded() {
        let fixture = Fixture::new("largest");
        let result = scan(
            &fixture.root,
            ScanOptions {
                largest_files: 2,
                ..Default::default()
            },
        );
        let paths: Vec<_> = result
            .largest_files
            .iter()
            .map(|f| result.path_of_file(f))
            .collect();
        assert_eq!(
            paths,
            [
                fixture.root.join("top.bin").to_string_lossy().into_owned(),
                fixture
                    .root
                    .join("a\\b\\c\\d\\e\\deep.bin")
                    .to_string_lossy()
                    .into_owned(),
            ],
            "the file five levels down must still be found"
        );
        assert_eq!(result.largest_files[0].logical.get(), 10_000);

        let none = scan(
            &fixture.root,
            ScanOptions {
                largest_files: 0,
                ..Default::default()
            },
        );
        assert!(none.largest_files.is_empty());
    }

    #[test]
    fn a_hard_linked_file_is_listed_under_one_name_only() {
        let fixture = Fixture::new("largest-links");
        std::fs::hard_link(
            fixture.root.join("top.bin"),
            fixture.root.join("wide\\link.bin"),
        )
        .expect("hard link");
        let result = scan(&fixture.root, ScanOptions::default());
        let big = result
            .largest_files
            .iter()
            .filter(|f| f.logical.get() == 10_000)
            .count();
        assert_eq!(big, 1, "{:?}", result.largest_files);
    }

    #[test]
    fn children_come_largest_first_and_ancestry_leads_back_to_the_root() {
        let fixture = Fixture::new("navigate");
        let result = scan(&fixture.root, ScanOptions::default());
        let tree = &result.tree;
        let children = tree.children_by_size(tree.root());
        let sizes: Vec<_> = children
            .iter()
            .map(|id| tree.node(*id).expect("child").allocated())
            .collect();
        // Tiny files may live inside the MFT record and occupy nothing, so
        // which of `a` and `wide` is larger depends on the volume; the order
        // must follow whatever the sizes are.
        assert_eq!(children.len(), 2);
        assert!(sizes[0] >= sizes[1], "{sizes:?}");
        let root = tree.node(tree.root()).expect("root");
        assert_eq!(root.own_files(), 1, "top.bin is the root's own file");

        let mut deepest = tree.root();
        while let Some(child) = tree
            .children_by_size(deepest)
            .into_iter()
            .find(|id| tree.name_of(*id) != "wide")
        {
            deepest = child;
        }
        let chain: Vec<_> = tree
            .ancestry(deepest)
            .iter()
            .skip(1)
            .map(|id| tree.name_of(*id))
            .collect();
        assert_eq!(chain, ["a", "b", "c", "d", "e"]);
        assert_eq!(tree.ancestry(tree.root()), [tree.root()]);
    }

    #[test]
    fn a_hard_link_is_counted_once() {
        let fixture = Fixture::new("links");
        std::fs::hard_link(
            fixture.root.join("top.bin"),
            fixture.root.join("wide\\link.bin"),
        )
        .expect("hard link");

        let with = scan(&fixture.root, ScanOptions::default());
        let without = scan(
            &fixture.root,
            ScanOptions {
                detect_hard_links: false,
                ..Default::default()
            },
        );

        assert_eq!(with.hard_link_duplicates, 1);
        assert_eq!(with.logical().get(), 10_000 + 5_000 + 300);
        assert_eq!(without.logical().get(), 2 * 10_000 + 5_000 + 300);
        assert_eq!(
            with.files_scanned, without.files_scanned,
            "both names are files"
        );
    }

    #[test]
    fn a_missing_root_is_recorded_as_skipped_not_reported_as_empty() {
        let result = scan(
            Path::new("C:\\this-path-does-not-exist-vitals-test"),
            ScanOptions::default(),
        );
        assert_eq!(result.files_scanned, 0);
        assert_eq!(result.tree.skipped().len(), 1);
        assert_eq!(result.tree.skipped()[0].reason, SkipReason::Vanished);
        assert!(!result.is_complete());
    }

    #[test]
    fn a_pre_set_cancel_token_stops_before_any_work() {
        let flag = AtomicBool::new(true);
        let mut control = ScanControl {
            cancel: Some(&flag),
            progress: None,
        };
        let result = scan_directory(
            Path::new("C:\\Windows"),
            ScanOptions::default(),
            &mut control,
        );

        assert!(result.cancelled);
        assert_eq!(result.files_scanned, 0);
        assert_eq!(result.tree.skipped()[0].reason, SkipReason::Cancelled);
    }

    #[test]
    fn a_cancel_mid_scan_marks_what_was_not_read_as_cancelled() {
        let flag = AtomicBool::new(false);
        let mut calls = 0_u32;
        let mut on_progress = |_: ScanProgress| {
            calls += 1;
            flag.store(true, Ordering::Relaxed);
        };
        let mut control = ScanControl {
            cancel: Some(&flag),
            progress: Some(&mut on_progress),
        };
        let result = scan_directory(
            Path::new("C:\\Windows"),
            ScanOptions {
                progress_interval: Duration::ZERO,
                ..Default::default()
            },
            &mut control,
        );

        assert!(result.cancelled);
        assert!(calls >= 1);
        assert!(
            result
                .tree
                .skipped()
                .iter()
                .any(|s| s.reason == SkipReason::Cancelled),
            "the unread part must be named, not silently absent"
        );
        assert!(!result.is_complete());
    }

    #[test]
    fn progress_carries_the_path_being_read() {
        let fixture = Fixture::new("progress");
        let mut last = None;
        {
            let mut callback = |p: ScanProgress| last = Some(p);
            let mut control = ScanControl {
                cancel: None,
                progress: Some(&mut callback),
            };
            scan_directory(
                &fixture.root,
                ScanOptions {
                    progress_interval: Duration::ZERO,
                    ..Default::default()
                },
                &mut control,
            );
        }
        let last = last.expect("a scan with a zero interval reports progress");
        assert!(
            last.current_path
                .starts_with(&*fixture.root.to_string_lossy())
        );
    }

    #[test]
    fn a_junction_back_to_its_parent_is_recorded_not_followed() {
        // A junction pointing at an ancestor is the case that never terminates
        // if followed. Made here with `mklink /J`, which needs no admin rights,
        // rather than borrowed from the user profile: scanning a real profile
        // took four minutes of the suite.
        let fixture = Fixture::new("junction");
        let link = fixture.root.join("a\\loop");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&fixture.root)
            .output()
            .is_ok_and(|out| out.status.success());
        if !made {
            // A filesystem without junctions (FAT, some network shares) is
            // not a failure of the scanner.
            return;
        }

        let result = scan(&fixture.root, ScanOptions::default());

        assert!(
            result
                .tree
                .skipped()
                .iter()
                .any(|s| s.reason == SkipReason::ReparsePoint && s.path.ends_with("loop")),
            "the junction must be listed as a reparse point: {:?}",
            result.tree.skipped()
        );
        assert_eq!(
            result.files_scanned, 302,
            "nothing behind the junction may be counted a second time"
        );
        assert!(
            result.is_complete(),
            "a link that was not followed leaves nothing out of the total"
        );
        // Removed before the fixture, so the fixture's remove_dir_all never
        // meets a junction pointing at the tree it is deleting.
        let _ = std::fs::remove_dir(&link);
    }
}
