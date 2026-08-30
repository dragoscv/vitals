//! Pure classification and parsing logic.
//!
//! Deliberately free of any Windows call so it can be tested exhaustively
//! against hand-built entries. A raw dump of the `Uninstall` keys on a normal
//! desktop yields several hundred rows, the majority of which are runtime
//! redistributables, driver fragments, security updates and other people's
//! plumbing. Windows' own "Apps & Features" applies broadly the rules below,
//! and matching it is the point: a list nobody recognises is worse than no
//! list.

use vitals_core::units::Bytes;

use super::model::{InstallDate, InstalledApp, RawUninstallEntry};

/// Bytes in the kilobyte that `EstimatedSize` is measured in.
///
/// `EstimatedSize` is documented in KB, not bytes; treating it as bytes
/// under-reports every application by 1024x — and because a few hundred
/// "bytes" still renders as a small, plausible number rather than an obvious
/// zero, the mistake survives a casual glance at the UI.
const ESTIMATED_SIZE_UNIT: u64 = 1024;

/// Why an entry was excluded from the application list.
///
/// Enumerated rather than reduced to a boolean so the probe can report what
/// was discarded and in what proportion. "We show 180 of 640 rows" is only
/// defensible if the other 460 can be accounted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RejectReason {
    /// No `DisplayName`; there is nothing to show the user.
    NoDisplayName,
    /// `SystemComponent=1` — part of Windows or of another product's
    /// internals, and deliberately hidden by Windows itself.
    SystemComponent,
    /// A patch, hotfix or security update belonging to another product.
    UpdateOrHotfix,
    /// `ParentKeyName` present: a child entry whose parent is already listed.
    ChildOfAnotherEntry,
    /// An MSI patch key (`{GUID}` under a `Patches` grouping) with no
    /// uninstall path of its own.
    OrphanPatch,
}

/// Decides whether a raw entry represents an application a user would
/// recognise, returning the reason when it does not.
///
/// # Errors
///
/// Infallible; the `Err` side of the returned [`Result`] carries the
/// rejection reason rather than a failure.
pub fn classify(entry: &RawUninstallEntry) -> Result<(), RejectReason> {
    let name = entry
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or(RejectReason::NoDisplayName)?;

    if entry.system_component == Some(1) {
        return Err(RejectReason::SystemComponent);
    }

    // A `ParentKeyName` means the entry is a component of something already
    // in the list — an Office language pack, a Visual Studio workload. Listing
    // both duplicates the product and inflates the count.
    if entry
        .parent_key_name
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty())
    {
        return Err(RejectReason::ChildOfAnotherEntry);
    }

    if is_update_like(entry.release_type.as_deref(), name) {
        return Err(RejectReason::UpdateOrHotfix);
    }

    // An entry with neither a way to remove it nor a location on disk is not
    // an application; it is a breadcrumb another installer left behind.
    if entry.uninstall_string.is_none()
        && entry.quiet_uninstall_string.is_none()
        && entry.install_location.is_none()
    {
        return Err(RejectReason::OrphanPatch);
    }

    Ok(())
}

/// Whether the entry describes an update rather than a product.
///
/// Both signals are needed. `ReleaseType` is authoritative when present but
/// most installers never write it, so the name has to be inspected too —
/// hence the deliberately narrow patterns below. A broad substring match on
/// "update" would swallow legitimate products such as "Windows Update
/// Blocker" or "`UpdateStar`".
fn is_update_like(release_type: Option<&str>, display_name: &str) -> bool {
    if let Some(kind) = release_type {
        let kind = kind.trim().to_ascii_lowercase();
        if matches!(
            kind.as_str(),
            "security update" | "update" | "hotfix" | "update rollup" | "servicepack"
        ) {
            return true;
        }
    }

    let name = display_name.trim();

    // `KB` followed by digits, as its own token, is the unambiguous marker
    // of a Windows or Office patch.
    let has_kb_token = name.split(|c: char| !c.is_ascii_alphanumeric()).any(|tok| {
        let Some(rest) = tok
            .strip_prefix("KB")
            .or_else(|| tok.strip_prefix("kb"))
            .or_else(|| tok.strip_prefix("Kb"))
        else {
            return false;
        };
        rest.len() >= 6 && rest.bytes().all(|b| b.is_ascii_digit())
    });

    if has_kb_token {
        return true;
    }

    let lower = name.to_ascii_lowercase();
    lower.starts_with("security update for")
        || lower.starts_with("update for")
        || lower.starts_with("hotfix for")
}

/// Converts `EstimatedSize` from its documented kilobyte unit into bytes.
///
/// Returns `None` when the value is absent or zero: an installer that wrote
/// no size and one that wrote a genuine zero are indistinguishable here, and
/// "0 B" beside a product that plainly occupies disk space reads as a bug in
/// the monitor rather than a gap in the registry.
#[must_use]
pub fn estimated_size_to_bytes(kilobytes: Option<u32>) -> Option<Bytes> {
    match kilobytes {
        Some(0) | None => None,
        Some(kb) => Some(Bytes(u64::from(kb) * ESTIMATED_SIZE_UNIT)),
    }
}

/// Parses the registry's `YYYYMMDD` install date.
///
/// Returns `None` for anything that does not parse to a plausible calendar
/// date. Installers write this field freehand and get it wrong in every
/// imaginable way — `20240230`, `2024/03/07`, localised `07/03/2024`, or an
/// empty string — and guessing at a malformed value would put a confidently
/// wrong date in front of the user.
#[must_use]
pub fn parse_install_date(raw: Option<&str>) -> Option<InstallDate> {
    let raw = raw?.trim();
    if raw.len() != 8 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let year: u16 = raw.get(0..4)?.parse().ok()?;
    let month: u8 = raw.get(4..6)?.parse().ok()?;
    let day: u8 = raw.get(6..8)?.parse().ok()?;

    // Windows predates none of these entries; a year outside this range means
    // the field holds something other than a date.
    if !(1980..=2200).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    if day == 0 || u16::from(day) > days_in_month(year, month) {
        return None;
    }

    Some(InstallDate { year, month, day })
}

/// Days in a Gregorian month, leap years included.
const fn days_in_month(year: u16, month: u8) -> u16 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}

/// Whether the uninstall command is driven by Windows Installer.
///
/// The `WindowsInstaller` value is checked first, but plenty of MSI products
/// omit it, so the command line is inspected as a fallback.
#[must_use]
pub fn detect_msi(entry: &RawUninstallEntry) -> bool {
    if entry.windows_installer == Some(1) {
        return true;
    }

    entry
        .uninstall_string
        .as_deref()
        .is_some_and(|cmd| cmd.to_ascii_lowercase().contains("msiexec"))
}

/// Builds the domain type from a raw entry that has already been accepted by
/// [`classify`].
///
/// Returns `None` if the entry lacks a usable display name or source, which
/// [`classify`] would already have rejected.
#[must_use]
pub fn to_installed_app(entry: RawUninstallEntry) -> Option<InstalledApp> {
    let is_msi = detect_msi(&entry);
    let source = entry.source?;
    let display_name = entry.display_name.as_deref().map(str::trim)?.to_owned();
    if display_name.is_empty() {
        return None;
    }

    Some(InstalledApp {
        key_name: entry.key_name,
        source,
        display_name,
        publisher: non_empty(entry.publisher),
        version: non_empty(entry.display_version),
        install_date: parse_install_date(entry.install_date_raw.as_deref()),
        estimated_size: estimated_size_to_bytes(entry.estimated_size_kb),
        install_location: non_empty(entry.install_location),
        uninstall_string: non_empty(entry.uninstall_string),
        quiet_uninstall_string: non_empty(entry.quiet_uninstall_string),
        is_msi,
        per_user: source.is_per_user(),
    })
}

/// Discards strings that are present but blank.
///
/// Installers frequently write an empty `Publisher` rather than omitting it,
/// and an empty cell in the UI should mean "unknown", not "the publisher is
/// literally nothing".
fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// The identity used to detect the same product appearing in several views.
///
/// The registry key name alone is not enough: a 32-bit and 64-bit install of
/// the same product have different `{GUID}` keys, while per-user and
/// machine-wide installs of the same version share one. Name plus version is
/// what a user would call "the same app".
fn dedupe_key(app: &InstalledApp) -> (String, String) {
    (
        app.display_name.to_ascii_lowercase(),
        app.version
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    )
}

/// Collapses duplicates, keeping the entry from the highest-precedence view.
///
/// Order of the input is not significant; the output is sorted by display
/// name so successive runs are comparable.
#[must_use]
pub fn deduplicate(apps: Vec<InstalledApp>) -> Vec<InstalledApp> {
    let mut winners: std::collections::HashMap<(String, String), InstalledApp> =
        std::collections::HashMap::with_capacity(apps.len());

    for app in apps {
        let key = dedupe_key(&app);
        match winners.get(&key) {
            Some(existing) if existing.source.precedence() <= app.source.precedence() => {}
            _ => {
                winners.insert(key, app);
            }
        }
    }

    let mut out: Vec<InstalledApp> = winners.into_values().collect();
    out.sort_by(|a, b| {
        a.display_name
            .to_ascii_lowercase()
            .cmp(&b.display_name.to_ascii_lowercase())
            .then_with(|| a.key_name.cmp(&b.key_name))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::super::model::AppSource;
    use super::*;

    fn app_entry(name: &str) -> RawUninstallEntry {
        RawUninstallEntry {
            key_name: name.to_owned(),
            source: Some(AppSource::MachineNative),
            display_name: Some(name.to_owned()),
            uninstall_string: Some(r#""C:\Program Files\Thing\uninst.exe""#.to_owned()),
            ..RawUninstallEntry::default()
        }
    }

    #[test]
    fn kilobytes_become_bytes_not_the_other_way_round() {
        let size = estimated_size_to_bytes(Some(1024)).expect("1024 KB is a real size");
        assert_eq!(
            size,
            Bytes(1_048_576),
            "EstimatedSize is KB: 1024 KB must be 1 MiB, not 1 KiB"
        );
    }

    #[test]
    fn a_large_app_size_does_not_overflow_the_u32_source() {
        // 4 GiB expressed in KB still fits u32; the product must not wrap.
        let size = estimated_size_to_bytes(Some(4_194_304)).expect("4 GiB is a real size");
        assert_eq!(size, Bytes(4_294_967_296), "KB->bytes must widen to u64");
    }

    #[test]
    fn missing_or_zero_size_is_unknown_not_zero() {
        assert_eq!(
            estimated_size_to_bytes(None),
            None,
            "an absent EstimatedSize must not become 0 B"
        );
        assert_eq!(
            estimated_size_to_bytes(Some(0)),
            None,
            "a zero EstimatedSize is an installer that wrote nothing useful"
        );
    }

    #[test]
    fn well_formed_install_date_parses() {
        assert_eq!(
            parse_install_date(Some("20240307")),
            Some(InstallDate {
                year: 2024,
                month: 3,
                day: 7
            })
        );
    }

    #[test]
    fn malformed_install_dates_are_rejected_rather_than_guessed() {
        for bad in [
            "",
            "   ",
            "2024",
            "2024-03-07",
            "07/03/2024",
            "20241307", // month 13
            "20240230", // February 30th
            "20240000",
            "19700101", // before Windows existed
            "2024030x",
            "202403071", // nine digits
        ] {
            assert_eq!(
                parse_install_date(Some(bad)),
                None,
                "{bad:?} is not a valid YYYYMMDD date but was accepted"
            );
        }
    }

    #[test]
    fn leap_day_is_valid_only_in_a_leap_year() {
        assert!(
            parse_install_date(Some("20240229")).is_some(),
            "2024 is a leap year"
        );
        assert_eq!(
            parse_install_date(Some("20230229")),
            None,
            "2023 has no 29 February"
        );
        assert_eq!(
            parse_install_date(Some("19000229")),
            None,
            "1900 is divisible by 100 but not 400, so it is not a leap year"
        );
        assert!(
            parse_install_date(Some("20000229")).is_some(),
            "2000 is divisible by 400 and is a leap year"
        );
    }

    #[test]
    fn entries_without_a_display_name_are_dropped() {
        let mut entry = app_entry("x");
        entry.display_name = None;
        assert_eq!(classify(&entry), Err(RejectReason::NoDisplayName));

        entry.display_name = Some("   ".into());
        assert_eq!(
            classify(&entry),
            Err(RejectReason::NoDisplayName),
            "a whitespace-only name is as useless as no name"
        );
    }

    #[test]
    fn system_components_are_hidden_exactly_as_windows_hides_them() {
        let mut entry = app_entry("Some Runtime Fragment");
        entry.system_component = Some(1);
        assert_eq!(classify(&entry), Err(RejectReason::SystemComponent));

        entry.system_component = Some(0);
        assert_eq!(
            classify(&entry),
            Ok(()),
            "SystemComponent=0 is an explicit request to be shown"
        );
    }

    #[test]
    fn patches_belonging_to_a_parent_entry_are_not_listed_separately() {
        let mut entry = app_entry("Language Pack");
        entry.parent_key_name = Some("Office16".into());
        assert_eq!(classify(&entry), Err(RejectReason::ChildOfAnotherEntry));
    }

    #[test]
    fn updates_are_filtered_by_release_type_and_by_name() {
        let mut entry = app_entry("Something");
        entry.release_type = Some("Security Update".into());
        assert_eq!(classify(&entry), Err(RejectReason::UpdateOrHotfix));

        let named = app_entry("Security Update for Microsoft Office (KB5002623) 64-Bit Edition");
        assert_eq!(classify(&named), Err(RejectReason::UpdateOrHotfix));

        let hotfix = app_entry("Hotfix for Microsoft Visual Studio 2010");
        assert_eq!(classify(&hotfix), Err(RejectReason::UpdateOrHotfix));
    }

    #[test]
    fn products_whose_name_merely_contains_update_are_kept() {
        for keeper in [
            "UpdateStar 8",
            "Windows Update Blocker",
            "Driver Updater Pro",
        ] {
            assert_eq!(
                classify(&app_entry(keeper)),
                Ok(()),
                "{keeper:?} is a product, not a patch, and must remain listed"
            );
        }
    }

    #[test]
    fn a_short_kb_like_token_is_not_a_patch_number() {
        // "KB2000" is far too short to be a Microsoft KB article, and models
        // such as keyboards genuinely ship names of this shape.
        assert_eq!(
            classify(&app_entry("Logitech KB2000 Driver")),
            Ok(()),
            "a 4-digit KB-prefixed token is not a hotfix identifier"
        );
        assert_eq!(
            classify(&app_entry("Thing (KB5002623)")),
            Err(RejectReason::UpdateOrHotfix),
            "a 7-digit KB token is a patch"
        );
    }

    #[test]
    fn an_entry_with_no_uninstall_path_and_no_location_is_a_breadcrumb() {
        let mut entry = app_entry("Leftover");
        entry.uninstall_string = None;
        assert_eq!(classify(&entry), Err(RejectReason::OrphanPatch));

        entry.install_location = Some(r"C:\Program Files\Leftover".into());
        assert_eq!(
            classify(&entry),
            Ok(()),
            "a real install location makes it a genuine, if unremovable, product"
        );
    }

    #[test]
    fn msi_detected_from_the_flag_or_from_the_command_line() {
        let mut entry = app_entry("Flagged");
        entry.windows_installer = Some(1);
        assert!(detect_msi(&entry));

        let mut inferred = app_entry("Inferred");
        inferred.uninstall_string = Some("MsiExec.exe /X{1234-5678}".into());
        assert!(
            detect_msi(&inferred),
            "products omitting WindowsInstaller are still MSI if msiexec removes them"
        );

        let plain = app_entry("Plain");
        assert!(!detect_msi(&plain));
    }

    #[test]
    fn optional_values_stay_absent_rather_than_becoming_empty_strings() {
        let mut entry = app_entry("Sparse");
        entry.publisher = Some("   ".into());
        entry.display_version = None;

        let app = to_installed_app(entry).expect("a named entry converts");
        assert_eq!(
            app.publisher, None,
            "a blank Publisher must read as unknown, not as an empty publisher"
        );
        assert_eq!(app.version, None);
        assert_eq!(app.estimated_size, None);
        assert_eq!(app.install_date, None);
    }

    #[test]
    fn the_same_product_in_both_registry_views_is_listed_once() {
        let build = |source: AppSource, key: &str| InstalledApp {
            key_name: key.to_owned(),
            source,
            display_name: "Shared Product".into(),
            publisher: None,
            version: Some("1.0".into()),
            install_date: None,
            estimated_size: None,
            install_location: None,
            uninstall_string: None,
            quiet_uninstall_string: None,
            is_msi: false,
            per_user: source.is_per_user(),
        };

        let deduped = deduplicate(vec![
            build(AppSource::UserWow64, "user-32"),
            build(AppSource::MachineWow64, "machine-32"),
            build(AppSource::MachineNative, "machine-64"),
        ]);

        assert_eq!(deduped.len(), 1, "one product must yield one row");
        assert_eq!(
            deduped[0].source,
            AppSource::MachineNative,
            "the machine-wide 64-bit entry has the most reliable uninstall command"
        );
    }

    #[test]
    fn different_versions_of_one_product_remain_distinct_rows() {
        let build = |version: &str, key: &str| InstalledApp {
            key_name: key.to_owned(),
            source: AppSource::MachineNative,
            display_name: "Runtime".into(),
            publisher: None,
            version: Some(version.to_owned()),
            install_date: None,
            estimated_size: None,
            install_location: None,
            uninstall_string: None,
            quiet_uninstall_string: None,
            is_msi: true,
            per_user: false,
        };

        let deduped = deduplicate(vec![build("14.30", "a"), build("14.40", "b")]);
        assert_eq!(
            deduped.len(),
            2,
            "side-by-side versions are genuinely two installations"
        );
    }

    #[test]
    fn output_is_sorted_case_insensitively_by_name() {
        let build = |name: &str| InstalledApp {
            key_name: name.to_owned(),
            source: AppSource::MachineNative,
            display_name: name.to_owned(),
            publisher: None,
            version: None,
            install_date: None,
            estimated_size: None,
            install_location: None,
            uninstall_string: None,
            quiet_uninstall_string: None,
            is_msi: false,
            per_user: false,
        };

        let deduped = deduplicate(vec![build("zebra"), build("Apple"), build("beta")]);
        let names: Vec<&str> = deduped.iter().map(|a| a.display_name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Apple", "beta", "zebra"],
            "case-sensitive sorting would put every lowercase name after every uppercase one"
        );
    }
}
