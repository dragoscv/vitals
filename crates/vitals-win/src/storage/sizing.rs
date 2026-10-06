//! Pure size arithmetic: clusters, allocation and hard-link accounting.
//!
//! Everything here is deliberately free of Win32 so it can be tested without
//! a filesystem. The rules it encodes are the ones that decide whether a scan
//! agrees with the volume's own free-space figure or quietly disagrees with
//! it by tens of gigabytes.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

/// A file's two sizes, which are not the same number and must never be
/// conflated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileSize {
    /// The length the file reports — what `dir` prints and what
    /// `Get-ChildItem | Measure-Object -Sum Length` adds up.
    pub logical: u64,
    /// What the file actually occupies on the volume.
    pub allocated: u64,
}

impl FileSize {
    /// A file that occupies nothing.
    pub const ZERO: Self = Self {
        logical: 0,
        allocated: 0,
    };
}

/// Attributes that change how allocated size must be derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllocationHints {
    /// NTFS sparse file: unwritten ranges consume no clusters at all, so the
    /// logical length says nothing about occupancy.
    pub sparse: bool,
    /// NTFS transparent compression: occupancy is smaller than the logical
    /// length by an amount only the filesystem knows.
    pub compressed: bool,
    /// A reparse point whose data may not be present locally — a `OneDrive`
    /// placeholder is the common case. Its logical length is the size it
    /// *would* be once hydrated, which is not disk usage.
    pub reparse: bool,
}

impl AllocationHints {
    /// Whether the allocated size has to be asked of the filesystem rather
    /// than derived by rounding.
    ///
    /// Worth branching on: the query costs a syscall per file, and the
    /// overwhelming majority of files on a volume are plain and need none.
    #[must_use]
    pub const fn needs_filesystem_query(self) -> bool {
        self.sparse || self.compressed || self.reparse
    }
}

/// Rounds a logical length up to a whole number of clusters.
///
/// Allocated size is rounded up to the cluster, not the logical length: a
/// directory of 10,000 one-byte files occupies 40 MB on a 4 KB-cluster
/// volume, and reporting 10 KB makes the scan disagree with the volume's own
/// free space by a factor of four thousand.
///
/// A `cluster_bytes` of zero — which is what a failed
/// `GetDiskFreeSpaceW` leaves behind — returns the logical length unchanged
/// rather than dividing by zero. That is an under-report, but an honest one:
/// inventing a 4 KB cluster for a volume we could not measure would be a
/// guess dressed as a measurement.
#[must_use]
pub const fn round_up_to_cluster(logical: u64, cluster_bytes: u64) -> u64 {
    if cluster_bytes == 0 {
        return logical;
    }
    // Cannot overflow for any real file: `logical` is bounded by the volume
    // size and `cluster_bytes` by 2 MB, so the sum stays far below u64::MAX.
    // saturating_add keeps it total regardless.
    let bumped = logical.saturating_add(cluster_bytes - 1);
    (bumped / cluster_bytes) * cluster_bytes
}

/// Derives the on-disk size of a plain file.
///
/// Only correct when [`AllocationHints::needs_filesystem_query`] is false;
/// sparse, compressed and reparse-backed files must use the value the
/// filesystem reports instead.
#[must_use]
pub const fn plain_allocated(logical: u64, cluster_bytes: u64) -> u64 {
    round_up_to_cluster(logical, cluster_bytes)
}

/// Reconciles a filesystem-reported compressed size with the cluster grid.
///
/// `GetCompressedFileSizeW` already returns a cluster-aligned figure on NTFS,
/// but `ReFS` and network redirectors do not all honour that, and a value that
/// is not a whole number of clusters would make subtree totals drift away
/// from the volume figure. Rounding here keeps every leaf on the same grid.
///
/// A reported size of zero is kept as zero: that is a genuinely
/// unmaterialised file — a fully sparse file or a dehydrated cloud
/// placeholder — and rounding it up to one cluster would invent a cluster
/// that is not allocated.
#[must_use]
pub const fn reconcile_reported(reported: u64, cluster_bytes: u64) -> u64 {
    if reported == 0 {
        return 0;
    }
    round_up_to_cluster(reported, cluster_bytes)
}

/// A file's identity on a volume, as NTFS understands it.
///
/// Two directory entries with the same identity are the *same file* reached
/// by two names. `C:\Windows\WinSxS` is built almost entirely from hard links
/// into `System32`, so a scanner that adds both copies reports a Windows
/// directory roughly twice its true size.
///
/// The 128-bit file ID the directory query returns. It is unique only within
/// one volume, which is enough: a scan never crosses a volume boundary,
/// because mount points and symbolic links are recorded rather than entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity(pub u128);

/// Shards in [`LinkTracker`]. A power of two, and enough that 32 workers
/// rarely wait on the same lock.
const LINK_SHARDS: usize = 64;

/// Remembers which files have already been counted, from many threads.
///
/// The directory query that makes the scan fast returns every file's ID but
/// not its link count, so the only way to notice a second name is to have
/// seen the first. Every ID is therefore kept for the length of the scan:
/// about 20 bytes a file, 40 MB for two million, released when it ends. The
/// alternative — opening each file to read its link count — was what held
/// the old walker to a few hundred files a second.
///
/// Each first sighting also records the folder that was credited with the
/// bytes, and a file seen again keeps every folder it was seen in. A saved
/// index needs both: a rescan that re-reads only the changed folders must
/// re-read *all* the folders of a linked file together, or the bytes are
/// counted in a re-read folder and again in a reused one. Keyed by `u64`
/// where the ID fits (every NTFS ID does), so the owner costs no memory over
/// the plain set this was: a `(u64, u32)` entry is sixteen bytes, the same as
/// the `u128` alone.
#[derive(Debug)]
pub struct LinkTracker {
    shards: Vec<Mutex<Shard>>,
    duplicates: AtomicU64,
    duplicate_bytes: AtomicU64,
    /// Files seen under a second name.
    shared: Mutex<HashMap<u128, SharedFile>>,
}

/// A file seen under more than one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedFile {
    pub identity: FileIdentity,
    /// On-disk bytes, counted once, in `folders[0]`.
    pub allocated: u64,
    /// Every folder a name was seen in, the credited one first. A folder
    /// holding two names of the file appears twice.
    pub folders: Vec<u32>,
}

#[derive(Debug, Default)]
struct Shard {
    narrow: HashMap<u64, u32>,
    /// `ReFS` IDs that do not fit 64 bits. Rare enough that their owner is
    /// not worth a second map layout.
    wide: HashMap<u128, u32>,
}

impl Shard {
    /// Inserts and returns the owner already recorded, if any.
    fn claim(&mut self, identity: FileIdentity, owner: u32) -> Option<u32> {
        match u64::try_from(identity.0) {
            Ok(narrow) => match self.narrow.entry(narrow) {
                std::collections::hash_map::Entry::Occupied(e) => Some(*e.get()),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(owner);
                    None
                }
            },
            Err(_) => match self.wide.entry(identity.0) {
                std::collections::hash_map::Entry::Occupied(e) => Some(*e.get()),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(owner);
                    None
                }
            },
        }
    }

    fn len(&self) -> usize {
        self.narrow.len() + self.wide.len()
    }
}

impl Default for LinkTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            shards: (0..LINK_SHARDS)
                .map(|_| Mutex::new(Shard::default()))
                .collect(),
            duplicates: AtomicU64::new(0),
            duplicate_bytes: AtomicU64::new(0),
            shared: Mutex::new(HashMap::new()),
        }
    }

    /// Files seen under more than one name, in identity order.
    #[must_use]
    pub fn shared(&self) -> Vec<SharedFile> {
        let mut out: Vec<_> = self
            .shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        out.sort_unstable_by_key(|file| file.identity.0);
        out
    }

    /// Records a file and says whether its size should be counted.
    ///
    /// The first sighting counts; every later one returns `false`, so the
    /// bytes appear exactly once. Which name "owns" them depends on which
    /// worker got there first, so a hard-linked file's bytes may land under a
    /// different folder from one scan to the next. The volume total does not
    /// move.
    ///
    /// `owner` is the folder the bytes are credited to on a first sighting.
    pub fn should_count(&self, identity: FileIdentity, allocated: u64, owner: u32) -> bool {
        let shard = (identity.0 as usize) & (LINK_SHARDS - 1);
        let earlier = self.shards[shard]
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .claim(identity, owner);
        match earlier {
            None => true,
            Some(first_owner) => {
                self.duplicates.fetch_add(1, Ordering::Relaxed);
                self.duplicate_bytes.fetch_add(allocated, Ordering::Relaxed);
                self.shared
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .entry(identity.0)
                    .or_insert_with(|| SharedFile {
                        identity,
                        allocated,
                        folders: vec![first_owner],
                    })
                    .folders
                    .push(owner);
                false
            }
        }
    }

    /// How many directory entries were suppressed as repeat hard links.
    #[must_use]
    pub fn duplicates(&self) -> u64 {
        self.duplicates.load(Ordering::Relaxed)
    }

    /// How many bytes were *not* double-counted because of that suppression.
    ///
    /// Surfaced rather than hidden: it is the single largest reason a Vitals
    /// total will be smaller than one produced by `Get-ChildItem`, and a user
    /// comparing the two deserves the explanation.
    #[must_use]
    pub fn duplicate_bytes(&self) -> u64 {
        self.duplicate_bytes.load(Ordering::Relaxed)
    }

    /// Distinct files tracked so far.
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.lock().unwrap_or_else(PoisonError::into_inner).len())
            .sum()
    }
}

/// Why a directory contributed nothing to the totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The caller does not have permission to list it.
    AccessDenied,
    /// A junction, symlink or mount point. Following it would either count
    /// another volume's bytes against this one or, if it points at an
    /// ancestor, never terminate.
    ReparsePoint,
    /// The scan was stopped before this directory was read.
    Cancelled,
    /// It disappeared between being listed and being opened.
    Vanished,
    /// The OS refused for some other reason; the raw code is kept so the
    /// cause is diagnosable rather than merely "failed".
    OsError(i32),
}

impl SkipReason {
    /// A short phrase for the UI.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccessDenied => "access denied",
            Self::ReparsePoint => "reparse point not followed",
            Self::Cancelled => "scan stopped first",
            Self::Vanished => "removed during scan",
            Self::OsError(_) => "OS error",
        }
    }

    /// Whether running elevated would plausibly let this directory be read.
    #[must_use]
    pub const fn is_elevation_fixable(self) -> bool {
        matches!(self, Self::AccessDenied)
    }

    /// Whether this leaves bytes out of the totals.
    ///
    /// A link that was not followed does not: whatever it points at is
    /// counted where it really lives, or is on another volume and not this
    /// scan's to count. Treating links as gaps made a full scan of `C:` say
    /// "98,254 folders could not be read" when 961 could not — 95,160 of the
    /// rest were Windows container layer placeholders.
    #[must_use]
    pub const fn leaves_a_gap(self) -> bool {
        !matches!(self, Self::ReparsePoint)
    }
}

/// A directory that was not measured, and why.
///
/// Kept as data rather than silently swallowed. A total that omits an
/// unreadable folder without saying so is a wrong number presented as a right
/// one, and the user has no way to tell.
#[derive(Debug, Clone)]
pub struct SkippedPath {
    pub path: String,
    pub reason: SkipReason,
}

/// Returns the `n` largest items, in descending order.
///
/// A full sort is avoided: a treemap wants the top twenty of what can be a
/// million directories, and `select_nth_unstable` turns an O(n log n) sort
/// into an O(n) partition.
pub fn top_n_by<T, K, F>(items: &mut Vec<T>, n: usize, key: F) -> Vec<T>
where
    F: Fn(&T) -> K,
    K: Ord,
{
    if n == 0 || items.is_empty() {
        return Vec::new();
    }
    if n < items.len() {
        items.select_nth_unstable_by(n - 1, |a, b| key(b).cmp(&key(a)));
        items.truncate(n);
    }
    items.sort_unstable_by_key(|item| std::cmp::Reverse(key(item)));
    std::mem::take(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_byte_file_occupies_a_whole_cluster() {
        assert_eq!(round_up_to_cluster(1, 4096), 4096);
    }

    #[test]
    fn exact_multiples_are_not_bumped_to_the_next_cluster() {
        assert_eq!(round_up_to_cluster(4096, 4096), 4096);
        assert_eq!(round_up_to_cluster(8192, 4096), 8192);
    }

    #[test]
    fn empty_file_occupies_nothing() {
        assert_eq!(round_up_to_cluster(0, 4096), 0);
    }

    #[test]
    fn ten_thousand_tiny_files_cost_forty_megabytes() {
        // The headline case: 10 KB of data, 40 MB of disk. A scan that
        // reports the former disagrees with the volume by 4000x.
        let logical: u64 = 10_000;
        let allocated: u64 = (0..10_000).map(|_| round_up_to_cluster(1, 4096)).sum();
        assert_eq!(logical, 10_000);
        assert_eq!(allocated, 40_960_000);
    }

    #[test]
    fn unknown_cluster_size_reports_logical_rather_than_guessing() {
        assert_eq!(round_up_to_cluster(1, 0), 1);
        assert_eq!(round_up_to_cluster(123_456, 0), 123_456);
    }

    #[test]
    fn large_cluster_volumes_round_correctly() {
        // 64 KB clusters are normal on large exFAT and ReFS volumes.
        assert_eq!(round_up_to_cluster(1, 65_536), 65_536);
        assert_eq!(round_up_to_cluster(65_537, 65_536), 131_072);
    }

    #[test]
    fn rounding_saturates_instead_of_overflowing() {
        assert_eq!(round_up_to_cluster(u64::MAX, 4096), u64::MAX / 4096 * 4096);
    }

    #[test]
    fn plain_files_do_not_need_a_filesystem_query() {
        assert!(!AllocationHints::default().needs_filesystem_query());
    }

    #[test]
    fn sparse_compressed_and_reparse_files_do() {
        for hints in [
            AllocationHints {
                sparse: true,
                ..Default::default()
            },
            AllocationHints {
                compressed: true,
                ..Default::default()
            },
            AllocationHints {
                reparse: true,
                ..Default::default()
            },
        ] {
            assert!(hints.needs_filesystem_query(), "{hints:?}");
        }
    }

    #[test]
    fn a_dehydrated_cloud_file_stays_at_zero() {
        // A OneDrive placeholder reports a multi-gigabyte logical length and
        // occupies nothing. Rounding it up to a cluster would invent one.
        assert_eq!(reconcile_reported(0, 4096), 0);
    }

    #[test]
    fn reported_compressed_size_is_placed_on_the_cluster_grid() {
        assert_eq!(reconcile_reported(5000, 4096), 8192);
        assert_eq!(reconcile_reported(4096, 4096), 4096);
    }

    #[test]
    fn distinct_files_all_count() {
        let tracker = LinkTracker::new();
        for id in 0..1000_u128 {
            assert!(tracker.should_count(FileIdentity(id), 4096, 0));
        }
        assert_eq!(tracker.duplicates(), 0);
        assert_eq!(tracker.tracked(), 1000);
        assert!(tracker.shared().is_empty(), "no file had a second name");
    }

    #[test]
    fn a_hard_linked_file_counts_once_and_only_once() {
        let tracker = LinkTracker::new();
        let id = FileIdentity(900);
        assert!(tracker.should_count(id, 8192, 7), "first sighting counts");
        assert!(!tracker.should_count(id, 8192, 8), "second must not");
        assert!(!tracker.should_count(id, 8192, 9), "third must not");
        assert_eq!(tracker.duplicates(), 2);
        assert_eq!(tracker.duplicate_bytes(), 16_384);
        assert_eq!(
            tracker.shared(),
            [SharedFile {
                identity: id,
                allocated: 8192,
                folders: vec![7, 8, 9],
            }],
            "every folder a name was seen in, the credited one first"
        );
    }

    #[test]
    fn an_identity_wider_than_64_bits_is_tracked_like_any_other() {
        // ReFS IDs do not fit a u64; truncating one would merge two files.
        let tracker = LinkTracker::new();
        let wide = FileIdentity(u128::MAX - 3);
        let narrow = FileIdentity(u128::from(u64::MAX - 3));
        assert!(tracker.should_count(wide, 4096, 1));
        assert!(tracker.should_count(narrow, 4096, 2), "a different file");
        assert!(!tracker.should_count(wide, 4096, 3));
        assert_eq!(tracker.shared()[0].folders, [1, 3]);
        assert_eq!(tracker.tracked(), 2);
    }

    #[test]
    fn concurrent_sightings_of_one_file_count_it_exactly_once() {
        let tracker = LinkTracker::new();
        let counted = AtomicU64::new(0);
        std::thread::scope(|scope| {
            for _ in 0..16 {
                scope.spawn(|| {
                    for id in 0..500_u128 {
                        if tracker.should_count(FileIdentity(id), 1, 0) {
                            counted.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        assert_eq!(counted.load(Ordering::Relaxed), 500);
        assert_eq!(tracker.duplicates(), 15 * 500);
    }

    #[test]
    fn skip_reasons_distinguish_the_fixable_from_the_permanent() {
        assert!(SkipReason::AccessDenied.is_elevation_fixable());
        assert!(!SkipReason::ReparsePoint.is_elevation_fixable());
        assert!(!SkipReason::Cancelled.is_elevation_fixable());
    }

    #[test]
    fn a_link_not_followed_is_not_a_gap_in_the_totals() {
        assert!(!SkipReason::ReparsePoint.leaves_a_gap());
        for reason in [
            SkipReason::AccessDenied,
            SkipReason::Cancelled,
            SkipReason::Vanished,
            SkipReason::OsError(5),
        ] {
            assert!(reason.leaves_a_gap(), "{reason:?}");
        }
    }

    #[test]
    fn top_n_returns_the_largest_in_descending_order() {
        let mut items = vec![5_u64, 1, 9, 3, 7, 2];
        let top = top_n_by(&mut items, 3, |v| *v);
        assert_eq!(top, vec![9, 7, 5]);
    }

    #[test]
    fn top_n_handles_fewer_items_than_requested() {
        let mut items = vec![2_u64, 8];
        assert_eq!(top_n_by(&mut items, 10, |v| *v), vec![8, 2]);
    }

    #[test]
    fn top_zero_is_empty() {
        let mut items = vec![1_u64, 2];
        assert!(top_n_by(&mut items, 0, |v| *v).is_empty());
    }
}
