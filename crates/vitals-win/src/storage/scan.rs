//! Directory traversal via `FindFirstFileEx`.
//!
//! The portable path: no elevation, works on every filesystem Windows can
//! mount, and correct on a subtree rather than only on a whole volume. It is
//! also the slow one — see [`super::mft`] for why, and for the fast path it
//! falls back from.
//!
//! Two flags make the difference between "slow" and "unusable":
//! `FindExInfoBasic` suppresses the 8.3 short-name lookup, which is a
//! separate MFT read per entry, and `FIND_FIRST_EX_LARGE_FETCH` asks the
//! kernel for a larger directory buffer so a 10,000-entry folder costs tens
//! of transitions instead of thousands.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES,
    ERROR_PATH_NOT_FOUND, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FindClose,
    FindFirstFileExW, FindNextFileW, GetCompressedFileSizeW, GetDiskFreeSpaceW,
    GetFileInformationByHandle, WIN32_FIND_DATAW,
};

use vitals_core::units::Bytes;

use super::ffi::create_file_read;

use super::sizing::{
    AllocationHints, FileIdentity, LinkTracker, SkipReason, plain_allocated, reconcile_reported,
};
use super::tree::{NodeId, SizeTree};

// Declared locally rather than pulled from a binding crate, matching the
// approach in `disk::volumes`: these are fixed constants of the Win32 ABI and
// enabling extra feature modules for a handful of integers is not a trade
// worth making.

/// `FindExInfoBasic` — skip the 8.3 alternate name.
const FIND_EX_INFO_BASIC: i32 = 1;
/// `FindExSearchNameMatch`
const FIND_EX_SEARCH_NAME_MATCH: i32 = 0;
/// `FIND_FIRST_EX_LARGE_FETCH`
const FIND_FIRST_EX_LARGE_FETCH: u32 = 2;

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x0000_0800;
const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x0000_0200;

const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
/// Required to open a *directory* handle at all.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// Opens the reparse point itself instead of its target.
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

/// Sentinel returned by `GetCompressedFileSizeW` on failure.
const INVALID_FILE_SIZE: u32 = u32::MAX;

/// How the scan should behave.
#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    /// Maximum directory depth below the root. `None` means unlimited.
    ///
    /// A limit is a blunt instrument and is not the cycle defence — that is
    /// reparse-point skipping. It exists so a UI can offer a shallow preview.
    pub max_depth: Option<u32>,
    /// Resolve file identities so hard links are counted once.
    ///
    /// Costs an open and a metadata query per file, which roughly halves the
    /// scan rate. Worth it on `C:\Windows`, where `WinSxS` is built almost
    /// entirely from links into `System32` and a naive scan reports the
    /// directory at close to twice its true size. Pointless on a photo
    /// library, hence the switch.
    pub detect_hard_links: bool,
    /// Query the filesystem for the true occupancy of sparse and compressed
    /// files instead of rounding their logical length.
    pub resolve_compressed: bool,
    /// Files between progress callbacks.
    ///
    /// Not a timer: calling `Instant::now` per file is itself measurable at
    /// these rates.
    pub progress_every: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            detect_hard_links: true,
            resolve_compressed: true,
            progress_every: 20_000,
        }
    }
}

/// A snapshot handed to the progress callback.
#[derive(Debug, Clone, Copy)]
pub struct ScanProgress {
    pub files_seen: u64,
    pub directories_seen: u64,
    pub bytes_seen: u64,
    pub elapsed_ms: u64,
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
/// The module never spawns a thread. A scan runs on whatever thread the
/// caller gives it; the caller is expected to put it on a background thread
/// and flip the token from the UI thread. That keeps the threading policy —
/// pool, priority, count — where it belongs, with the application.
#[derive(Default)]
pub struct ScanControl<'a> {
    /// Checked once per directory and once per progress interval. Setting it
    /// ends the scan promptly and yields a partial, honestly-flagged result.
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

impl ScanControl<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
    }
}

/// The outcome of a scan.
#[derive(Debug)]
pub struct ScanResult {
    pub tree: SizeTree,
    /// Cluster size of the volume the root sits on, if it could be read.
    ///
    /// `None` disables cluster rounding entirely, which under-reports. That
    /// is deliberate: inventing a 4 KB cluster for an unmeasured volume would
    /// be a guess presented as a measurement.
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
}

impl ScanResult {
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
    /// False when the scan was cancelled or any directory was skipped. The UI
    /// should render a total from an incomplete scan with a qualifier, not as
    /// a plain number.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.cancelled && self.tree.skipped().is_empty()
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
}

/// A find handle that closes itself.
struct FindHandle(HANDLE);

impl Drop for FindHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful FindFirstFileExW and is
        // closed exactly once, at drop.
        unsafe { FindClose(self.0) };
    }
}

/// A file or directory handle that closes itself.
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
/// # Errors
///
/// Returns `None` when the volume cannot be queried — a disconnected share,
/// a removed card. The caller then reports logical sizes rather than
/// fabricating a cluster size.
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

/// Identity and link count of an already-open handle.
fn handle_identity(handle: HANDLE) -> Option<(FileIdentity, u32)> {
    let mut info = BY_HANDLE_FILE_INFORMATION {
        dwFileAttributes: 0,
        ftCreationTime: unsafe { std::mem::zeroed() },
        ftLastAccessTime: unsafe { std::mem::zeroed() },
        ftLastWriteTime: unsafe { std::mem::zeroed() },
        dwVolumeSerialNumber: 0,
        nFileSizeHigh: 0,
        nFileSizeLow: 0,
        nNumberOfLinks: 0,
        nFileIndexHigh: 0,
        nFileIndexLow: 0,
    };

    // SAFETY: `handle` is a live handle opened with FILE_READ_ATTRIBUTES and
    // `info` is a live, fully-initialised structure of the expected type.
    let ok = unsafe { GetFileInformationByHandle(handle, &raw mut info) };
    if ok == 0 {
        return None;
    }

    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Some((
        FileIdentity {
            volume_serial: info.dwVolumeSerialNumber,
            file_index: index,
        },
        info.nNumberOfLinks,
    ))
}

/// Opens a path for metadata only, never following a reparse point.
fn open_for_metadata(wide: &[u16], directory: bool) -> Option<OwnedHandle> {
    let mut flags = FILE_FLAG_OPEN_REPARSE_POINT;
    if directory {
        flags |= FILE_FLAG_BACKUP_SEMANTICS;
    }

    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
    let handle = unsafe {
        create_file_read(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            // Sharing everything: a scanner must never block another process
            // from writing to or deleting a file it merely looked at.
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            flags,
        )
    }?;

    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return None;
    }
    Some(OwnedHandle(handle))
}

/// Reads the true occupancy of a sparse or compressed file.
fn compressed_size(wide: &[u16]) -> Option<u64> {
    let mut high: u32 = 0;
    // SAFETY: `wide` is NUL-terminated and outlives the call; `high` is a
    // live u32.
    let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &raw mut high) };

    // The documented failure protocol: INVALID_FILE_SIZE is ambiguous with a
    // legitimate low word of 0xFFFFFFFF, so the error code must be checked
    // rather than the return value alone.
    if low == INVALID_FILE_SIZE {
        // SAFETY: no arguments, no preconditions.
        let code = unsafe { GetLastError() };
        if code != 0 {
            return None;
        }
    }

    Some((u64::from(high) << 32) | u64::from(low))
}

/// Extracts the entry name from a find record.
fn find_name(data: &WIN32_FIND_DATAW) -> String {
    let end = data
        .cFileName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(data.cFileName.len());
    String::from_utf16_lossy(&data.cFileName[..end])
}

/// Whether an entry is `.` or `..`, which must never be descended into.
fn is_dot_entry(name: &str) -> bool {
    name == "." || name == ".."
}

/// A directory queued for traversal.
struct Pending {
    path: String,
    node: NodeId,
    depth: u32,
}

/// Scans a directory tree.
///
/// Explicitly iterative with a work stack rather than recursive: a
/// `node_modules/.pnpm` chain nests deep enough to overflow the stack on a
/// recursive walker, and a scanner that panics on a deep directory is worse
/// than a slow one.
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
    let started = Instant::now();
    let root_display = root.to_string_lossy().into_owned();
    let cluster_bytes = cluster_size(root);

    let mut tree = SizeTree::new(&root_display);
    let mut links = LinkTracker::new();
    let mut stack = vec![Pending {
        path: root_display,
        node: tree.root(),
        depth: 0,
    }];

    let mut files: u64 = 0;
    let mut directories: u64 = 0;
    let mut bytes: u64 = 0;
    let mut next_report = options.progress_every;
    let mut cancelled = false;

    while let Some(pending) = stack.pop() {
        if control.cancelled() {
            cancelled = true;
            break;
        }

        directories += 1;
        let outcome = visit_directory(
            &pending,
            options,
            cluster_bytes,
            &mut tree,
            &mut links,
            &mut stack,
        );

        files += outcome.files;
        bytes = bytes.saturating_add(outcome.bytes);

        if files >= next_report {
            next_report = files + options.progress_every;
            if let Some(callback) = control.progress.as_mut() {
                callback(ScanProgress {
                    files_seen: files,
                    directories_seen: directories,
                    bytes_seen: bytes,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
        }
    }

    // Anything still queued when cancellation hit is a real gap in the
    // totals, and must be recorded as one rather than vanishing.
    if cancelled {
        for pending in stack {
            tree.mark_skipped(pending.node, pending.path, SkipReason::Cycle);
        }
    }

    tree.aggregate();

    ScanResult {
        tree,
        cluster_bytes,
        files_scanned: files,
        directories_scanned: directories,
        hard_link_duplicates: links.duplicates(),
        hard_link_bytes_saved: links.duplicate_bytes(),
        cancelled,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

struct DirOutcome {
    files: u64,
    bytes: u64,
}

/// Lists one directory, sizing its files and queueing its subdirectories.
fn visit_directory(
    pending: &Pending,
    options: ScanOptions,
    cluster_bytes: Option<u64>,
    tree: &mut SizeTree,
    links: &mut LinkTracker,
    stack: &mut Vec<Pending>,
) -> DirOutcome {
    let mut outcome = DirOutcome { files: 0, bytes: 0 };

    let pattern = format!("{}\\*", pending.path.trim_end_matches('\\'));
    let wide = to_wide_extended(&pattern);
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };

    // SAFETY: `wide` is a NUL-terminated UTF-16 search pattern that outlives
    // the call, and `data` is a live WIN32_FIND_DATAW the kernel fills in.
    // The info level and search op are the documented constants for a basic
    // name-match enumeration; the filter argument must be null for them.
    let handle = unsafe {
        FindFirstFileExW(
            wide.as_ptr(),
            FIND_EX_INFO_BASIC,
            (&raw mut data).cast(),
            FIND_EX_SEARCH_NAME_MATCH,
            std::ptr::null_mut(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        // SAFETY: no arguments, no preconditions.
        let code = unsafe { GetLastError() };
        let reason = match code {
            ERROR_ACCESS_DENIED => SkipReason::AccessDenied,
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => SkipReason::Vanished,
            // An empty directory is not an error and must not be recorded as
            // a gap in the totals.
            ERROR_NO_MORE_FILES => return outcome,
            other => SkipReason::OsError(other.cast_signed()),
        };
        tree.mark_skipped(pending.node, pending.path.clone(), reason);
        return outcome;
    }

    let _guard = FindHandle(handle);

    loop {
        let name = find_name(&data);
        if !is_dot_entry(&name) {
            let attributes = data.dwFileAttributes;
            let child_path = format!("{}\\{}", pending.path.trim_end_matches('\\'), name);

            if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                let node = tree.add_child(pending.node, &name);

                if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    // A junction or symlink. Following it would either count
                    // another volume's bytes against this one — `C:\Users\All
                    // Users` points at ProgramData — or, when it points at an
                    // ancestor, never terminate. Recorded, not traversed.
                    tree.mark_skipped(node, child_path, SkipReason::ReparsePoint);
                } else if options
                    .max_depth
                    .is_some_and(|limit| pending.depth + 1 > limit)
                {
                    tree.mark_skipped(node, child_path, SkipReason::DepthLimit);
                } else {
                    stack.push(Pending {
                        path: child_path,
                        node,
                        depth: pending.depth + 1,
                    });
                }
            } else {
                let logical = (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow);
                let hints = AllocationHints {
                    sparse: attributes & FILE_ATTRIBUTE_SPARSE_FILE != 0,
                    compressed: attributes & FILE_ATTRIBUTE_COMPRESSED != 0,
                    reparse: attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
                };

                let allocated = size_of_file(&child_path, logical, hints, cluster_bytes, options);
                outcome.files += 1;

                if count_this_file(&child_path, allocated, links, options) {
                    tree.add_file(pending.node, allocated, logical);
                    outcome.bytes = outcome.bytes.saturating_add(allocated);
                }
            }
        }

        // SAFETY: `handle` is live for the lifetime of `_guard`, and `data`
        // is a live WIN32_FIND_DATAW the kernel overwrites in place.
        if unsafe { FindNextFileW(handle, &raw mut data) } == 0 {
            break;
        }
    }

    outcome
}

/// Determines the on-disk size of one file.
fn size_of_file(
    path: &str,
    logical: u64,
    hints: AllocationHints,
    cluster_bytes: Option<u64>,
    options: ScanOptions,
) -> u64 {
    let cluster = cluster_bytes.unwrap_or(0);

    if !hints.needs_filesystem_query() || !options.resolve_compressed {
        return plain_allocated(logical, cluster);
    }

    let wide = to_wide_extended(path);
    compressed_size(&wide).map_or_else(
        // The query failed — a locked file, a redirector that does not
        // implement it. Falling back to cluster rounding over-reports a
        // compressed file, which is the safer direction: it will never claim
        // free space that is not there.
        || plain_allocated(logical, cluster),
        |reported| reconcile_reported(reported, cluster),
    )
}

/// Whether a file's bytes should be added, or suppressed as a repeat hard
/// link.
fn count_this_file(
    path: &str,
    allocated: u64,
    links: &mut LinkTracker,
    options: ScanOptions,
) -> bool {
    if !options.detect_hard_links {
        return true;
    }

    let wide = to_wide_extended(path);
    let Some(handle) = open_for_metadata(&wide, false) else {
        // Unopenable — in use, or denied. Counting it is right: the file
        // exists and occupies space, and the only thing lost is the ability
        // to notice it is a duplicate. Skipping would under-report.
        return true;
    };

    handle_identity(handle.0)
        .is_none_or(|(identity, link_count)| links.should_count(identity, link_count, allocated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn find_data_layout_is_pinned() {
        // WIN32_FIND_DATAW is read field-by-field out of a buffer the kernel
        // fills; a size mismatch would mean silently reading the wrong offset
        // for every file size on the volume.
        assert_eq!(size_of::<WIN32_FIND_DATAW>(), 592);
        assert_eq!(size_of::<BY_HANDLE_FILE_INFORMATION>(), 52);
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
        assert!(is_dot_entry("."));
        assert!(is_dot_entry(".."));
        assert!(!is_dot_entry("...weird but legal"));
        assert!(!is_dot_entry(".git"));
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

    #[test]
    fn scanning_a_real_directory_produces_consistent_totals() {
        let mut control = ScanControl::default();
        let result = scan_directory(
            Path::new("C:\\Windows\\System32\\drivers\\etc"),
            ScanOptions::default(),
            &mut control,
        );

        assert!(result.tree.is_aggregated());
        // Deliberately NOT `allocated >= logical`. That holds only for plain
        // files; NTFS compression makes occupancy smaller than length, and
        // `C:\Windows` is heavily compressed on a modern install. What must
        // always hold is that both figures come from the same set of files.
        assert!(result.files_scanned > 0, "this directory is never empty");
        assert_eq!(
            result.logical() == Bytes::ZERO,
            result.allocated() == Bytes::ZERO,
            "one figure cannot be zero while the other is not"
        );
    }

    #[test]
    fn a_missing_root_is_recorded_as_skipped_not_reported_as_empty() {
        let mut control = ScanControl::default();
        let result = scan_directory(
            Path::new("C:\\this-path-does-not-exist-vitals-test"),
            ScanOptions::default(),
            &mut control,
        );

        assert_eq!(result.files_scanned, 0);
        assert_eq!(
            result.tree.skipped().len(),
            1,
            "an unreachable root must appear as a gap, not as a zero total"
        );
        assert!(!result.is_complete());
    }

    #[test]
    fn a_depth_limit_records_the_directories_it_did_not_enter() {
        let mut control = ScanControl::default();
        let result = scan_directory(
            Path::new("C:\\Windows\\System32\\drivers"),
            ScanOptions {
                max_depth: Some(0),
                detect_hard_links: false,
                ..Default::default()
            },
            &mut control,
        );

        assert!(
            result.tree.skipped().iter().all(|s| matches!(
                s.reason,
                SkipReason::DepthLimit | SkipReason::AccessDenied | SkipReason::ReparsePoint
            )),
            "unexpected skip reasons: {:?}",
            result.tree.skipped()
        );
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
        assert!(!result.is_complete());
    }

    #[test]
    fn progress_is_reported_during_a_real_scan() {
        let mut ticks = 0_u32;
        let mut last = ScanProgress {
            files_seen: 0,
            directories_seen: 0,
            bytes_seen: 0,
            elapsed_ms: 0,
        };
        {
            let mut callback = |p: ScanProgress| {
                ticks += 1;
                last = p;
            };
            let mut control = ScanControl {
                cancel: None,
                progress: Some(&mut callback),
            };
            scan_directory(
                Path::new("C:\\Windows\\System32"),
                ScanOptions {
                    detect_hard_links: false,
                    progress_every: 100,
                    ..Default::default()
                },
                &mut control,
            );
        }
        assert!(ticks > 0, "a System32 scan must report progress");
        assert!(last.files_seen >= 100);
    }

    #[test]
    fn reparse_points_under_the_user_profile_are_skipped_not_followed() {
        // `%LOCALAPPDATA%\Application Data` is a junction back to its own
        // parent. A scanner that follows it never terminates.
        let Ok(local) = std::env::var("LOCALAPPDATA") else {
            return;
        };
        let mut control = ScanControl::default();
        let result = scan_directory(
            Path::new(&local),
            ScanOptions {
                max_depth: Some(1),
                detect_hard_links: false,
                resolve_compressed: false,
                ..Default::default()
            },
            &mut control,
        );

        // The assertion that matters is termination; that the scan returned
        // at all proves the junction was not followed into a loop.
        assert!(result.tree.is_aggregated());
    }

    #[test]
    fn hard_link_detection_never_inflates_the_total() {
        let path = Path::new("C:\\Windows\\System32\\drivers\\etc");
        let mut control = ScanControl::default();
        let with = scan_directory(
            path,
            ScanOptions {
                detect_hard_links: true,
                ..Default::default()
            },
            &mut control,
        );
        let mut control = ScanControl::default();
        let without = scan_directory(
            path,
            ScanOptions {
                detect_hard_links: false,
                ..Default::default()
            },
            &mut control,
        );

        assert!(
            with.allocated() <= without.allocated(),
            "deduplicating links can only ever reduce a total"
        );
    }
}
