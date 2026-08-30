//! Registry `Run` keys and the Start Menu startup folders.
//!
//! ## Why six registry keys and not two
//!
//! On 64-bit Windows the `HKLM\Software` hive is *redirected*: a 32-bit
//! process asking for `Software\Microsoft\Windows\CurrentVersion\Run` is
//! silently given `Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run`
//! instead, and the two contain different values. A tool that opens the key
//! without specifying a view therefore sees exactly half the machine-wide
//! startup items, and which half depends on its own bitness rather than on
//! anything about the machine.
//!
//! Both views are read explicitly here, so the result does not change if this
//! crate is ever built for 32-bit. `HKCU\Software` is **not** redirected for
//! this path, so the per-user keys have one view only — reading a "32-bit
//! HKCU Run" would duplicate every entry.

use std::path::PathBuf;

use super::approved::ApprovalIndex;
use super::classify::{extract_image_path, file_name_of};
use super::registry::{
    FOLDERID_COMMON_STARTUP, FOLDERID_STARTUP, Hive, KEY_WOW64_32KEY, KEY_WOW64_64KEY, RegKey,
    decode_string, expand_environment, known_folder,
};
use super::{StartupEntry, StartupSource, StartupState};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";

/// Every registry location scanned, with the view flag each needs.
const REGISTRY_SOURCES: &[(StartupSource, Hive, &str, u32)] = &[
    (
        StartupSource::MachineRun,
        Hive::LocalMachine,
        RUN,
        KEY_WOW64_64KEY,
    ),
    (
        StartupSource::MachineRun32,
        Hive::LocalMachine,
        RUN,
        KEY_WOW64_32KEY,
    ),
    (
        StartupSource::MachineRunOnce,
        Hive::LocalMachine,
        RUN_ONCE,
        KEY_WOW64_64KEY,
    ),
    (
        StartupSource::MachineRunOnce32,
        Hive::LocalMachine,
        RUN_ONCE,
        KEY_WOW64_32KEY,
    ),
    (StartupSource::UserRun, Hive::CurrentUser, RUN, 0),
    (StartupSource::UserRunOnce, Hive::CurrentUser, RUN_ONCE, 0),
];

/// Reads every `Run`/`RunOnce` value across both registry views.
///
/// `approvals` supplies the enable/disable state Task Manager records
/// separately; pass a freshly loaded [`ApprovalIndex`] so a toggle made
/// moments ago is reflected.
#[must_use]
pub fn scan_run_keys(approvals: &ApprovalIndex) -> Vec<StartupEntry> {
    let mut out = Vec::new();

    for &(source, hive, path, view) in REGISTRY_SOURCES {
        let Ok(key) = RegKey::open(hive, path, view) else {
            // Absent RunOnce keys are the norm, and an HKLM key we cannot
            // read is a permission fact about this process, not a machine
            // fault. Either way, skipping beats failing the whole scan.
            continue;
        };

        for (name, value_type, data) in key.values() {
            let Some(command) = decode_string(value_type, &data) else {
                // A non-string value in a Run key is malformed; Windows
                // would not execute it either.
                continue;
            };

            // A Run value whose data is empty is inert. Listing it as a
            // startup item with a blank command implies we failed to read
            // something, when in fact there is nothing there.
            if command.trim().is_empty() {
                continue;
            }

            out.push(build_entry(source, name, command, approvals));
        }
    }

    // The 32-bit machine view is scanned after the 64-bit one, so identical
    // value names appear twice. They are genuinely two distinct entries in
    // two distinct keys and both really run, so both are kept — but the
    // ordering matters for the UI, which groups by source.
    out
}

/// Builds an entry from a raw registry value.
fn build_entry(
    source: StartupSource,
    name: String,
    command: String,
    approvals: &ApprovalIndex,
) -> StartupEntry {
    let image_path = extract_image_path(&command)
        .map(expand_environment)
        .map(PathBuf::from);

    // Absent approval record means the user never toggled it. The item is
    // enabled because it is present in a Run key — the key's own contents
    // are the evidence — not because the lookup defaulted to true.
    let state = approvals
        .lookup(source, &name)
        .unwrap_or(StartupState::Enabled);

    StartupEntry {
        name,
        display_name: None,
        source,
        state,
        command: Some(command),
        image_path,
        publisher: None,
        pid: None,
    }
}

/// Reads both Start Menu startup folders.
///
/// Shortcut targets are **not** resolved: parsing the `.lnk` shell-link
/// binary format correctly is a project of its own, and a wrong target is
/// worse than none — it would be shown as the measured command. The file name
/// is reported and [`StartupEntry::command`] is left `None`, which the UI
/// renders as "shortcut" rather than as a blank it might mistake for a
/// failure.
#[must_use]
pub fn scan_startup_folders(approvals: &ApprovalIndex) -> Vec<StartupEntry> {
    let mut out = Vec::new();

    for (source, folder) in [
        (StartupSource::UserStartupFolder, FOLDERID_STARTUP),
        (StartupSource::CommonStartupFolder, FOLDERID_COMMON_STARTUP),
    ] {
        let Some(dir) = known_folder(folder) else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
                continue;
            };

            // Explorer creates this in every startup folder and it is not a
            // startup item.
            if file_name.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }

            // Explorer's StartupApproved records are keyed by the file name
            // including its extension, so the stem must not be used here.
            let state = approvals
                .lookup(source, file_name)
                .unwrap_or(StartupState::Enabled);

            // A loose .exe dropped into the folder is its own image; a .lnk
            // is not, and claiming the shortcut file as the image would make
            // every entry report the wrong path.
            let is_shortcut = path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));

            out.push(StartupEntry {
                name: file_name.to_owned(),
                display_name: path
                    .file_stem()
                    .and_then(std::ffi::OsStr::to_str)
                    .map(str::to_owned),
                source,
                state,
                command: None,
                image_path: if is_shortcut { None } else { Some(path) },
                publisher: None,
                pid: None,
            });
        }
    }

    out
}

/// The image's file name, for classification.
///
/// A thin convenience over [`file_name_of`] that works on the resolved
/// [`StartupEntry::image_path`].
#[must_use]
pub fn image_file_name(entry: &StartupEntry) -> Option<&str> {
    entry
        .image_path
        .as_deref()
        .and_then(std::path::Path::to_str)
        .and_then(file_name_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_machine_registry_views_are_scanned() {
        let views: Vec<u32> = REGISTRY_SOURCES
            .iter()
            .filter(|(_, hive, path, _)| *hive == Hive::LocalMachine && *path == RUN)
            .map(|(_, _, _, view)| *view)
            .collect();

        assert!(
            views.contains(&KEY_WOW64_64KEY) && views.contains(&KEY_WOW64_32KEY),
            "missing a view means half the machine-wide entries are invisible, \
             and which half depends on our own bitness; got {views:?}"
        );
    }

    #[test]
    fn the_user_hive_is_scanned_once_per_key() {
        // HKCU\Software is not redirected for this path. Adding a 32-bit
        // view would list every per-user entry twice.
        let user_run = REGISTRY_SOURCES
            .iter()
            .filter(|(_, hive, path, _)| *hive == Hive::CurrentUser && *path == RUN)
            .count();
        assert_eq!(user_run, 1);
    }

    #[test]
    fn every_registry_source_is_listed_exactly_once() {
        let mut sources: Vec<_> = REGISTRY_SOURCES.iter().map(|(s, ..)| *s).collect();
        sources.sort_unstable();
        let before = sources.len();
        sources.dedup();
        assert_eq!(before, sources.len(), "a source is scanned twice");
        assert_eq!(
            before, 6,
            "six registry locations: Run and RunOnce, \
             HKLM in both views plus HKCU"
        );
    }

    #[test]
    fn entries_without_an_approval_record_are_enabled_by_presence() {
        let approvals = ApprovalIndex::default();
        let entry = build_entry(
            StartupSource::UserRun,
            "Thing".into(),
            r"C:\a\thing.exe -q".into(),
            &approvals,
        );
        assert_eq!(entry.state, StartupState::Enabled);
        assert_eq!(entry.image_path, Some(PathBuf::from(r"C:\a\thing.exe")));
        assert_eq!(entry.command.as_deref(), Some(r"C:\a\thing.exe -q"));
    }

    #[test]
    fn quoted_registry_commands_yield_the_full_path() {
        let entry = build_entry(
            StartupSource::MachineRun,
            "App".into(),
            r#""C:\Program Files\App\a.exe" /background"#.into(),
            &ApprovalIndex::default(),
        );
        assert_eq!(
            entry.image_path,
            Some(PathBuf::from(r"C:\Program Files\App\a.exe")),
            "the space in Program Files must not truncate the path"
        );
    }

    #[test]
    fn real_scan_produces_only_well_formed_entries() {
        let approvals = ApprovalIndex::load();
        let entries = scan_run_keys(&approvals);

        for entry in &entries {
            assert!(!entry.name.is_empty(), "a Run value with no name");
            let command = entry
                .command
                .as_deref()
                .expect("registry entries always carry their command");
            assert!(
                !command.trim().is_empty(),
                "{} has a blank command that should have been filtered",
                entry.name
            );
            assert!(
                matches!(
                    entry.source,
                    StartupSource::MachineRun
                        | StartupSource::MachineRun32
                        | StartupSource::MachineRunOnce
                        | StartupSource::MachineRunOnce32
                        | StartupSource::UserRun
                        | StartupSource::UserRunOnce
                ),
                "{:?} is not a registry source",
                entry.source
            );
        }
    }

    #[test]
    fn folder_scan_never_claims_a_shortcut_as_its_own_image() {
        for entry in scan_startup_folders(&ApprovalIndex::load()) {
            if entry.name.to_lowercase().ends_with(".lnk") {
                assert!(
                    entry.image_path.is_none(),
                    "{} is a shortcut; reporting the .lnk as the image would \
                     show the wrong executable and a wrong 'file missing' flag",
                    entry.name
                );
            }
            assert_ne!(entry.name.to_lowercase(), "desktop.ini");
        }
    }
}
