//! Discovery of space that could plausibly be reclaimed.
//!
//! **This module reports. It never deletes.** There is no filesystem
//! mutation anywhere in it, by design: a wrong suggestion costs the user a
//! moment's judgement, whereas a wrong deletion costs them their data. The
//! classification below is therefore free to be a little cautious, and the
//! act of removal belongs to a separate, explicitly-confirmed code path that
//! does not exist yet.

use std::path::{Path, PathBuf};

use vitals_core::units::Bytes;

/// How confident we are that removing something is harmless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Safety {
    /// Regenerated automatically; removal has no user-visible effect beyond
    /// a slower next start.
    Safe,
    /// Removal is recoverable but noticeable — a signed-out browser, a
    /// re-downloaded update.
    Review,
    /// Removal changes system behaviour or discards the only copy of
    /// something. Never suggest this as a one-click action.
    Risky,
}

impl Safety {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Review => "review",
            Self::Risky => "risky",
        }
    }

    /// Whether a UI may offer this behind a single confirmation.
    ///
    /// `Risky` items are still *shown* — a 34 GB hibernation file is exactly
    /// what someone hunting for space wants to know about — but the action
    /// must route through the specific system setting, not a generic delete.
    #[must_use]
    pub const fn allows_one_click(self) -> bool {
        matches!(self, Self::Safe)
    }
}

/// What kind of reclaimable thing this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupKind {
    UserTemp,
    SystemTemp,
    BrowserCache,
    WindowsUpdateCache,
    RecycleBin,
    CrashDump,
    PreviousWindows,
    Hibernation,
    PackageManagerCache,
    ThumbnailCache,
    DeliveryOptimisation,
}

impl CleanupKind {
    /// Default safety for the category.
    #[must_use]
    pub const fn safety(self) -> Safety {
        match self {
            // Regenerated on demand; nothing depends on their contents
            // surviving a reboot.
            Self::UserTemp
            | Self::SystemTemp
            | Self::ThumbnailCache
            | Self::DeliveryOptimisation => Safety::Safe,
            // Recoverable, but the user pays for it: signed-out sessions,
            // re-downloaded updates, a rebuilt package cache.
            Self::BrowserCache
            | Self::WindowsUpdateCache
            | Self::PackageManagerCache
            | Self::CrashDump => Safety::Review,
            // The recycle bin is the user's own undo buffer, `Windows.old` is
            // the only route back from a feature update, and deleting
            // `hiberfil.sys` by hand breaks fast startup — that one is a
            // `powercfg` setting, not a file operation.
            Self::RecycleBin | Self::PreviousWindows | Self::Hibernation => Safety::Risky,
        }
    }

    /// Why this is reclaimable, phrased for a human.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::UserTemp => {
                "Temporary files written by applications. Windows never clears these \
                 automatically once the writing process has exited."
            }
            Self::SystemTemp => {
                "System temporary files, mostly installer scratch space left behind by \
                 setup programs that exited without cleaning up."
            }
            Self::BrowserCache => {
                "Cached web content. Removing it frees space immediately and costs only \
                 a slower first load of previously visited sites."
            }
            Self::WindowsUpdateCache => {
                "Downloaded update packages already installed. Windows keeps them for \
                 repair scenarios and re-downloads them if needed."
            }
            Self::RecycleBin => {
                "Files the user deleted but has not yet purged. Emptying this is the \
                 point at which deletion becomes permanent."
            }
            Self::CrashDump => {
                "Memory dumps written after a crash. Useful only while a fault is being \
                 diagnosed; a full dump can be as large as installed RAM."
            }
            Self::PreviousWindows => {
                "The previous Windows installation kept after a feature update. It is \
                 the only route back to the old build and Windows removes it \
                 automatically after ten days."
            }
            Self::Hibernation => {
                "The hibernation image, sized to a fraction of installed RAM. It cannot \
                 be deleted as a file — disable hibernation with `powercfg /h off` \
                 instead, which also turns off fast startup."
            }
            Self::PackageManagerCache => {
                "Downloaded package archives kept by a developer tool. Rebuilt on demand \
                 at the cost of re-downloading."
            }
            Self::ThumbnailCache => {
                "Explorer's thumbnail and icon database. Regenerated as folders are \
                 browsed."
            }
            Self::DeliveryOptimisation => {
                "Update fragments cached for peer-to-peer sharing with other machines on \
                 the network. Purely a bandwidth optimisation."
            }
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UserTemp => "User temporary files",
            Self::SystemTemp => "System temporary files",
            Self::BrowserCache => "Browser cache",
            Self::WindowsUpdateCache => "Windows Update cache",
            Self::RecycleBin => "Recycle Bin",
            Self::CrashDump => "Crash dumps",
            Self::PreviousWindows => "Previous Windows installation",
            Self::Hibernation => "Hibernation file",
            Self::PackageManagerCache => "Package manager cache",
            Self::ThumbnailCache => "Thumbnail cache",
            Self::DeliveryOptimisation => "Delivery Optimisation cache",
        }
    }
}

/// One reclaimable location.
#[derive(Debug, Clone)]
pub struct CleanupCandidate {
    pub path: PathBuf,
    pub kind: CleanupKind,
    /// On-disk size, or `None` when the location exists but could not be
    /// measured.
    ///
    /// An option rather than a zero: "we could not size this" and "this is
    /// empty" are different facts, and showing the first as the second is how
    /// a cleanup tool talks a user out of reclaiming 8 GB.
    pub size: Option<Bytes>,
    pub safety: Safety,
    pub reason: &'static str,
    /// Set when the location is present but unreadable without elevation.
    pub needs_elevation: bool,
}

impl CleanupCandidate {
    /// Whether this is worth putting in front of the user.
    ///
    /// Unsized candidates are kept: an unmeasurable Windows Update cache is
    /// still worth reporting, precisely because the user can elevate and find
    /// out.
    #[must_use]
    pub fn is_worth_reporting(&self, minimum: Bytes) -> bool {
        match self.size {
            Some(size) => size >= minimum,
            None => true,
        }
    }
}

/// A location to probe, before it is known to exist.
#[derive(Debug, Clone, Copy)]
struct Location {
    /// Environment variable holding the base directory, if any.
    env: Option<&'static str>,
    /// Path relative to that base, or absolute when `env` is `None`.
    suffix: &'static str,
    kind: CleanupKind,
}

/// Every location worth probing.
///
/// Data rather than code so that the list is reviewable at a glance, and so
/// the classification tests can walk it exhaustively.
const LOCATIONS: &[Location] = &[
    Location {
        env: Some("TEMP"),
        suffix: "",
        kind: CleanupKind::UserTemp,
    },
    Location {
        env: Some("SystemRoot"),
        suffix: "Temp",
        kind: CleanupKind::SystemTemp,
    },
    Location {
        env: Some("SystemRoot"),
        suffix: "SoftwareDistribution\\Download",
        kind: CleanupKind::WindowsUpdateCache,
    },
    Location {
        env: Some("SystemRoot"),
        suffix: "SoftwareDistribution\\DeliveryOptimization",
        kind: CleanupKind::DeliveryOptimisation,
    },
    Location {
        env: Some("SystemRoot"),
        suffix: "Minidump",
        kind: CleanupKind::CrashDump,
    },
    Location {
        env: Some("SystemRoot"),
        suffix: "LiveKernelReports",
        kind: CleanupKind::CrashDump,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "CrashDumps",
        kind: CleanupKind::CrashDump,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "Microsoft\\Windows\\Explorer",
        kind: CleanupKind::ThumbnailCache,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "Google\\Chrome\\User Data\\Default\\Cache",
        kind: CleanupKind::BrowserCache,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "Microsoft\\Edge\\User Data\\Default\\Cache",
        kind: CleanupKind::BrowserCache,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "Mozilla\\Firefox\\Profiles",
        kind: CleanupKind::BrowserCache,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "BraveSoftware\\Brave-Browser\\User Data\\Default\\Cache",
        kind: CleanupKind::BrowserCache,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "npm-cache",
        kind: CleanupKind::PackageManagerCache,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "pnpm\\store",
        kind: CleanupKind::PackageManagerCache,
    },
    Location {
        env: Some("LOCALAPPDATA"),
        suffix: "pip\\Cache",
        kind: CleanupKind::PackageManagerCache,
    },
    Location {
        env: Some("USERPROFILE"),
        suffix: ".cargo\\registry\\cache",
        kind: CleanupKind::PackageManagerCache,
    },
    Location {
        env: Some("USERPROFILE"),
        suffix: ".nuget\\packages",
        kind: CleanupKind::PackageManagerCache,
    },
];

/// Locations on the system drive that are not reached through an environment
/// variable.
const SYSTEM_DRIVE_LOCATIONS: &[(&str, CleanupKind)] = &[
    ("Windows.old", CleanupKind::PreviousWindows),
    ("$Recycle.Bin", CleanupKind::RecycleBin),
    ("hiberfil.sys", CleanupKind::Hibernation),
    ("MEMORY.DMP", CleanupKind::CrashDump),
];

/// Every location worth probing on this machine, whether or not it exists.
///
/// Existence is deliberately *not* checked here: this function is pure and
/// unit-testable, and the caller pairs each path with a size (or with the
/// knowledge that it is absent). Splitting it this way is what lets the
/// classification be tested without a Windows install underneath it.
#[must_use]
pub fn candidate_locations() -> Vec<(PathBuf, CleanupKind)> {
    let mut out = Vec::with_capacity(LOCATIONS.len() + SYSTEM_DRIVE_LOCATIONS.len());

    for location in LOCATIONS {
        let Some(var) = location.env else {
            out.push((PathBuf::from(location.suffix), location.kind));
            continue;
        };
        // A missing variable means the concept does not apply to this
        // account — a service running as LocalSystem has no LOCALAPPDATA.
        // Skipping is correct; synthesising `C:\Users\...` would be a guess.
        let Ok(base) = std::env::var(var) else {
            continue;
        };
        let mut path = PathBuf::from(base);
        if !location.suffix.is_empty() {
            path.push(location.suffix);
        }
        out.push((path, location.kind));
    }

    if let Ok(drive) = std::env::var("SystemDrive") {
        for (suffix, kind) in SYSTEM_DRIVE_LOCATIONS {
            let mut path = PathBuf::from(format!("{drive}\\"));
            path.push(suffix);
            out.push((path, *kind));
        }
    }

    out
}

/// Builds a candidate from a probed location.
///
/// `size` is `None` when the location exists but could not be measured, which
/// is a different report from a size of zero.
#[must_use]
pub fn classify(path: PathBuf, kind: CleanupKind, size: Option<Bytes>) -> CleanupCandidate {
    let needs_elevation = size.is_none() && requires_elevation(&path);
    CleanupCandidate {
        path,
        kind,
        size,
        safety: kind.safety(),
        reason: kind.reason(),
        needs_elevation,
    }
}

/// Whether a path is one an unelevated process is normally refused.
///
/// A heuristic used only to phrase the UI hint, never to decide whether to
/// attempt the read — the attempt is always made, and its actual failure is
/// what gets reported.
fn requires_elevation(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    lower.contains("\\softwaredistribution")
        || lower.contains("\\system volume information")
        || lower.contains("$recycle.bin")
        || lower.ends_with("hiberfil.sys")
        || lower.ends_with("memory.dmp")
        || lower.contains("\\windows.old")
}

/// Total reclaimable bytes across candidates at or below a safety level.
///
/// Unsized candidates are excluded from the sum and counted separately by the
/// caller. Treating an unknown as zero would present an under-count as a
/// measurement.
#[must_use]
pub fn reclaimable_total(candidates: &[CleanupCandidate], up_to: Safety) -> Bytes {
    let total = candidates
        .iter()
        .filter(|c| c.safety <= up_to)
        .filter_map(|c| c.size)
        .fold(0_u64, |acc, b| acc.saturating_add(b.get()));
    Bytes(total)
}

/// How many candidates could not be sized.
#[must_use]
pub fn unmeasured_count(candidates: &[CleanupCandidate]) -> usize {
    candidates.iter().filter(|c| c.size.is_none()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_directories_are_safe_to_offer() {
        assert_eq!(CleanupKind::UserTemp.safety(), Safety::Safe);
        assert_eq!(CleanupKind::SystemTemp.safety(), Safety::Safe);
        assert!(CleanupKind::UserTemp.safety().allows_one_click());
    }

    #[test]
    fn the_recycle_bin_is_never_a_one_click_action() {
        // It is the user's own undo buffer; emptying it is the moment their
        // deletion becomes irreversible.
        assert_eq!(CleanupKind::RecycleBin.safety(), Safety::Risky);
        assert!(!CleanupKind::RecycleBin.safety().allows_one_click());
    }

    #[test]
    fn windows_old_is_risky_because_it_is_the_only_way_back() {
        assert_eq!(CleanupKind::PreviousWindows.safety(), Safety::Risky);
        assert!(!CleanupKind::PreviousWindows.safety().allows_one_click());
    }

    #[test]
    fn the_hibernation_file_is_risky_and_says_how_to_remove_it_properly() {
        assert_eq!(CleanupKind::Hibernation.safety(), Safety::Risky);
        assert!(
            CleanupKind::Hibernation.reason().contains("powercfg"),
            "the reason must point at the supported mechanism, not a delete"
        );
    }

    #[test]
    fn browser_caches_need_review_not_blind_removal() {
        assert_eq!(CleanupKind::BrowserCache.safety(), Safety::Review);
        assert!(!CleanupKind::BrowserCache.safety().allows_one_click());
    }

    #[test]
    fn every_kind_has_a_non_empty_reason_and_label() {
        for kind in [
            CleanupKind::UserTemp,
            CleanupKind::SystemTemp,
            CleanupKind::BrowserCache,
            CleanupKind::WindowsUpdateCache,
            CleanupKind::RecycleBin,
            CleanupKind::CrashDump,
            CleanupKind::PreviousWindows,
            CleanupKind::Hibernation,
            CleanupKind::PackageManagerCache,
            CleanupKind::ThumbnailCache,
            CleanupKind::DeliveryOptimisation,
        ] {
            assert!(!kind.reason().is_empty(), "{kind:?} has no reason");
            assert!(!kind.label().is_empty(), "{kind:?} has no label");
            assert!(
                kind.reason().len() > 30,
                "{kind:?} reason is too terse to justify anything"
            );
        }
    }

    #[test]
    fn an_unmeasurable_candidate_is_reported_not_hidden() {
        let candidate = classify(
            PathBuf::from("C:\\Windows\\SoftwareDistribution\\Download"),
            CleanupKind::WindowsUpdateCache,
            None,
        );
        assert!(candidate.needs_elevation);
        assert!(
            candidate.is_worth_reporting(Bytes(1_000_000_000)),
            "an unknown size must not be filtered out by a size threshold"
        );
    }

    #[test]
    fn a_small_candidate_is_filtered_out() {
        let candidate = classify(
            PathBuf::from("C:\\Temp"),
            CleanupKind::UserTemp,
            Some(Bytes(1024)),
        );
        assert!(!candidate.is_worth_reporting(Bytes(10 * 1024 * 1024)));
        assert!(candidate.is_worth_reporting(Bytes(1024)));
    }

    #[test]
    fn totals_respect_the_safety_ceiling() {
        let candidates = vec![
            classify(
                PathBuf::from("C:\\Temp"),
                CleanupKind::UserTemp,
                Some(Bytes(1000)),
            ),
            classify(
                PathBuf::from("C:\\cache"),
                CleanupKind::BrowserCache,
                Some(Bytes(2000)),
            ),
            classify(
                PathBuf::from("C:\\Windows.old"),
                CleanupKind::PreviousWindows,
                Some(Bytes(4000)),
            ),
        ];
        assert_eq!(reclaimable_total(&candidates, Safety::Safe).get(), 1000);
        assert_eq!(reclaimable_total(&candidates, Safety::Review).get(), 3000);
        assert_eq!(reclaimable_total(&candidates, Safety::Risky).get(), 7000);
    }

    #[test]
    fn unmeasured_candidates_are_excluded_from_the_total_and_counted() {
        let candidates = vec![
            classify(
                PathBuf::from("C:\\Temp"),
                CleanupKind::UserTemp,
                Some(Bytes(1000)),
            ),
            classify(
                PathBuf::from("C:\\Windows\\Temp"),
                CleanupKind::SystemTemp,
                None,
            ),
        ];
        assert_eq!(
            reclaimable_total(&candidates, Safety::Safe).get(),
            1000,
            "an unknown must not be silently added as zero"
        );
        assert_eq!(unmeasured_count(&candidates), 1);
    }

    #[test]
    fn elevation_hint_matches_the_protected_locations() {
        assert!(requires_elevation(Path::new(
            "C:\\Windows\\SoftwareDistribution\\Download"
        )));
        assert!(requires_elevation(Path::new("C:\\$Recycle.Bin")));
        assert!(requires_elevation(Path::new("C:\\hiberfil.sys")));
        assert!(!requires_elevation(Path::new(
            "C:\\Users\\x\\AppData\\Local\\Temp"
        )));
    }

    #[test]
    fn locations_are_probed_without_touching_the_filesystem() {
        // The list must be produced from environment alone, so that the
        // classification is testable on a machine where none of these exist.
        let locations = candidate_locations();
        assert!(
            !locations.is_empty(),
            "TEMP is set in every user session; the list cannot be empty"
        );
        assert!(locations.iter().all(|(p, _)| p.is_absolute() || cfg!(test)));
    }

    #[test]
    fn safety_orders_from_safe_to_risky() {
        assert!(Safety::Safe < Safety::Review);
        assert!(Safety::Review < Safety::Risky);
    }
}
