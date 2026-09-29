//! File-level storage analysis: where the space went, and what could go.
//!
//! Distinct from [`crate::disk`], which answers "how full is this volume and
//! how fast is it moving bytes". This module answers "which folder is 40 GB
//! and why", which needs a walk of the namespace rather than a counter read.
//!
//! # The two scanners
//!
//! - [`scan`] — bulk directory-information queries on parallel workers.
//!   Works on any path, any filesystem, no elevation, and returns sizes.
//! - [`mft`] — `FSCTL_ENUM_USN_DATA`. Reads the entire NTFS namespace in
//!   seconds, but needs elevation and **returns no sizes at all**, because a
//!   USN record does not carry one. See that module for why, and for why
//!   inferring one would be fabrication.
//!
//! The consequence: the fast path accelerates *discovery*, not measurement.
//! [`scan`] remains the source of every byte figure, and nothing here
//! silently substitutes one for the other.
//!
//! # What the numbers mean
//!
//! Every total is **allocated** size — what the files occupy on the volume,
//! cluster-rounded — not the sum of their lengths. Logical size is carried
//! alongside because it is what Explorer's "Size:" line and
//! `Get-ChildItem | Measure-Object -Sum Length` report, and a user comparing
//! the two deserves to see both rather than conclude one of them is broken.
//!
//! Three things make the figures differ from a naive walk, all of them
//! deliberate: cluster rounding raises the total, hard-link deduplication
//! lowers it, and unreadable directories are excluded from it and listed
//! separately rather than counted as zero.

pub mod cleanup;
pub mod devclean;
mod ffi;
pub mod layout;
pub mod managed;
pub mod mft;
pub mod protect;
pub mod recycle;
pub mod scan;
pub mod sizing;
pub mod tree;

use std::path::Path;
use std::sync::atomic::AtomicBool;

use vitals_core::units::Bytes;

pub use cleanup::{
    CleanupCandidate, CleanupKind, Safety, candidate_locations, reclaimable_total, unmeasured_count,
};
pub use layout::{Cell, CellKind, Detail, icicle, treemap};
pub use managed::{ManagedTool, tool_for};
pub use mft::{MftEntry, VolumeNamespace};
pub use protect::{Protection, Rules, Vetted, vet};
pub use recycle::{Holder, HolderKind, Outcome, holders_of, recycle};
pub use scan::{LargeFile, ScanControl, ScanOptions, ScanProgress, ScanResult, scan_directory};
pub use sizing::{
    AllocationHints, FileIdentity, FileSize, LinkTracker, SkipReason, SkippedPath,
    round_up_to_cluster, top_n_by,
};
pub use tree::{Amount, Node, NodeId, SizeTree};

/// Which scanning strategy is usable right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStrategy {
    /// MFT enumeration is available for whole-volume discovery. Sizes still
    /// come from the directory walk.
    MftAssisted,
    /// Directory walk only.
    DirectoryWalk,
}

/// Reports which strategy applies to a drive letter.
///
/// Attempts the real volume open rather than inspecting the process token: an
/// elevated process on a non-NTFS volume still cannot enumerate an MFT, and a
/// capability check that answers a proxy question will eventually answer it
/// wrongly.
#[must_use]
pub fn strategy_for(letter: char) -> ScanStrategy {
    if mft::is_available(letter) {
        ScanStrategy::MftAssisted
    } else {
        ScanStrategy::DirectoryWalk
    }
}

/// One entry in a largest-folders list, ready for a treemap or table.
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub path: String,
    pub allocated: Bytes,
    pub logical: Bytes,
    pub files: u64,
    /// Set when this directory's contents could not be read, so the row can
    /// be rendered as incomplete rather than as a confident figure.
    pub incomplete: Option<SkipReason>,
}

/// The `n` largest directories in a completed scan.
#[must_use]
pub fn largest_directories(result: &ScanResult, n: usize) -> Vec<DirectoryEntry> {
    result
        .tree
        .largest_directories(n)
        .into_iter()
        .filter_map(|(id, allocated)| {
            let node = result.tree.node(id)?;
            Some(DirectoryEntry {
                path: result.tree.path_of(id),
                allocated,
                logical: node.logical(),
                files: node.file_count(),
                incomplete: node.skipped(),
            })
        })
        .collect()
}

/// Finds and sizes reclaimable locations.
///
/// Each candidate is sized with a bounded directory scan. `cancel` is
/// forwarded so a slow location — a multi-gigabyte package cache — does not
/// pin the caller.
///
/// A location that exists but cannot be measured is reported with a size of
/// `None`, not zero: those are different facts, and presenting the first as
/// the second is how a cleanup tool talks a user out of reclaiming 8 GB.
///
/// **Nothing here deletes anything.** Removal is a separate, explicitly
/// confirmed action: [`managed`] for Windows-managed space, the basket for
/// the rest.
#[must_use]
pub fn find_cleanup_candidates(cancel: Option<&AtomicBool>) -> Vec<CleanupCandidate> {
    let mut out = Vec::new();

    for (path, kind) in candidate_locations() {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            // Absent, or unreadable even to stat. Either way there is nothing
            // to report: claiming a cleanup opportunity we cannot see would
            // be inventing one.
            continue;
        };

        let size = if kind == CleanupKind::ComponentStore {
            // A walk would count 10 GB of hard links into System32 as the
            // store's own; what can go is known only to an elevated DISM.
            // Unknown, not a wrong number.
            None
        } else if metadata.is_file() {
            Some(Bytes(metadata.len()))
        } else {
            size_of_directory(&path, cancel)
        };

        out.push(cleanup::classify(path, kind, size));
    }

    out.sort_by_key(|candidate| std::cmp::Reverse(candidate.size));
    out
}

/// Sizes a directory, returning `None` if the scan could not complete it.
fn size_of_directory(path: &Path, cancel: Option<&AtomicBool>) -> Option<Bytes> {
    let mut control = ScanControl {
        cancel,
        progress: None,
    };
    let result = scan_directory(
        path,
        ScanOptions {
            // A cache directory has no hard links worth remembering IDs for.
            detect_hard_links: false,
            ..ScanOptions::default()
        },
        &mut control,
    );

    if result.cancelled {
        return None;
    }

    // The root itself being unreadable means the figure would be zero for the
    // wrong reason. A skipped *sub*directory is reported as a partial size,
    // which is still more useful than nothing.
    let root_unreadable = result
        .tree
        .node(result.tree.root())
        .is_some_and(|n| n.skipped().is_some());

    if root_unreadable {
        None
    } else {
        Some(result.allocated())
    }
}

/// Sizes a single known path, for a caller that already has one.
#[must_use]
pub fn size_of(path: &Path, cancel: Option<&AtomicBool>) -> Option<Bytes> {
    size_of_directory(path, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_is_reported_honestly_for_the_system_volume() {
        // Unelevated this must be DirectoryWalk. Asserting the *agreement*
        // rather than a fixed value keeps the test meaningful in both cases.
        let strategy = strategy_for('C');
        assert_eq!(
            strategy == ScanStrategy::MftAssisted,
            mft::is_available('C')
        );
    }

    #[test]
    fn largest_directories_are_returned_in_descending_order() {
        let mut control = ScanControl::default();
        let result = scan_directory(
            Path::new("C:\\Windows\\System32\\drivers"),
            ScanOptions {
                detect_hard_links: false,
                ..Default::default()
            },
            &mut control,
        );

        let top = largest_directories(&result, 5);
        for pair in top.windows(2) {
            assert!(pair[0].allocated >= pair[1].allocated);
        }
    }

    #[test]
    // Both properties below are about the same real discovery pass, and that
    // pass sizes every temp and cache directory on the machine — five seconds
    // of the test suite. Asserting both against one set of candidates costs
    // half as much and tests exactly the same things.
    fn cleanup_discovery_is_honest_about_what_it_found() {
        let candidates = find_cleanup_candidates(None);

        for candidate in &candidates {
            assert!(
                std::fs::symlink_metadata(&candidate.path).is_ok(),
                "reported a path that is not there: {}",
                candidate.path.display()
            );
            assert!(!candidate.reason.is_empty());
            if candidate.kind == CleanupKind::ComponentStore {
                assert_eq!(
                    candidate.size, None,
                    "a walk of WinSxS counts hard links into System32 as its own"
                );
            }
        }

        // A location whose size could not be measured must not silently
        // contribute zero to the headline "you can reclaim this much".
        let measured: u64 = candidates
            .iter()
            .filter(|c| c.safety <= Safety::Risky)
            .filter_map(|c| c.size)
            .map(Bytes::get)
            .sum();
        assert_eq!(
            reclaimable_total(&candidates, Safety::Risky).get(),
            measured
        );
    }

    #[test]
    fn a_cancelled_sizing_reports_unknown_rather_than_a_partial_total() {
        let flag = AtomicBool::new(true);
        assert_eq!(
            size_of_directory(Path::new("C:\\Windows"), Some(&flag)),
            None,
            "a truncated total must not be presented as a measurement"
        );
    }

    #[test]
    fn the_scan_of_a_known_directory_is_reproducible() {
        let path = Path::new("C:\\Windows\\System32\\drivers\\etc");
        let options = ScanOptions {
            detect_hard_links: false,
            ..Default::default()
        };
        let mut a_control = ScanControl::default();
        let a = scan_directory(path, options, &mut a_control);
        let mut b_control = ScanControl::default();
        let b = scan_directory(path, options, &mut b_control);

        assert_eq!(a.files_scanned, b.files_scanned);
        assert_eq!(a.allocated(), b.allocated());
    }
}
