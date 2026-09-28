//! Installed application enumeration.
//!
//! ## Why three sources
//!
//! There is no single list of installed software on Windows. The `Uninstall`
//! key exists in three places and each holds products the others do not:
//!
//! * `HKLM\...\Uninstall` — machine-wide 64-bit products.
//! * `HKLM\Software\WOW6432Node\...\Uninstall` — machine-wide 32-bit
//!   products. On a typical desktop this is roughly half of everything
//!   installed, and a reader that omits it looks correct while hiding
//!   hundreds of megabytes of software.
//! * `HKCU\...\Uninstall` — per-user installs. Anything installed without
//!   elevation (Chrome, VS Code, Slack, most Electron applications) lives
//!   only here and is invisible to a machine-wide scan.
//!
//! The 32-bit view is selected with `KEY_WOW64_32KEY` rather than by writing
//! `WOW6432Node` into the path, because the flag is the documented mechanism
//! and behaves correctly under registry redirection regardless of the
//! bitness of this process.
//!
//! ## What is deliberately not listed
//!
//! A raw dump of these keys yields several hundred rows on an ordinary
//! machine, most of which are runtime redistributables' internal fragments,
//! driver components, security updates and child entries of products already
//! present. [`filter::classify`] discards them and records why; the
//! `apps_probe` example prints the breakdown so the difference between the
//! raw count and the shown count is always accountable.
//!
//! ## What is not covered
//!
//! MSIX/Store packages are not in the registry at all — see
//! [`enumerate_store_apps`].

pub mod filter;
pub mod model;
mod registry;

pub use filter::{RejectReason, deduplicate, estimated_size_to_bytes, parse_install_date};
pub use model::{AppSource, InstallDate, InstalledApp, RawUninstallEntry};

use std::collections::BTreeMap;

use vitals_core::capability::Capability;
use vitals_core::error::{Error, Result};

use registry::{Hive, RegKey};

/// The `Uninstall` key path, identical under HKLM and HKCU.
pub(crate) const UNINSTALL_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

/// The result of a scan: the applications, plus what was discarded.
///
/// The rejection tally is part of the public result rather than a debug
/// aside. Any filter aggressive enough to be useful is also aggressive enough
/// to hide something a user expected, and without the counts there is no way
/// to tell a good filter from a broken scan — both produce a short list.
#[derive(Debug, Clone, Default)]
pub struct AppScan {
    /// Applications, deduplicated and sorted by display name.
    pub apps: Vec<InstalledApp>,
    /// Total subkeys examined across all four registry views.
    pub examined: usize,
    /// How many entries each rejection reason accounted for.
    pub rejected: BTreeMap<RejectReason, usize>,
    /// Entries removed because the same product appeared in more than one
    /// view.
    pub duplicates_collapsed: usize,
}

impl AppScan {
    /// Total entries discarded by the "is this a real application?" filter.
    #[must_use]
    pub fn rejected_total(&self) -> usize {
        self.rejected.values().sum()
    }
}

/// Enumerates installed applications from every registry view.
///
/// Never fails as a whole. A view that is absent — `WOW6432Node` does not
/// exist on a 32-bit-only system — or unreadable is skipped, because one
/// inaccessible hive must not blank the entire list.
#[must_use]
pub fn enumerate_installed_apps() -> AppScan {
    let mut scan = AppScan::default();
    let mut accepted = Vec::new();

    for source in AppSource::ALL {
        let hive = if source.is_per_user() {
            Hive::CurrentUser
        } else {
            Hive::LocalMachine
        };

        let Ok(root) = RegKey::open(hive, UNINSTALL_PATH, source.is_wow64()) else {
            continue;
        };

        for name in root.child_names() {
            // A product uninstalled between enumeration and open is normal
            // churn, not a fault.
            let Ok(key) = root.open_child(&name) else {
                continue;
            };

            scan.examined += 1;
            let entry = read_entry(&key, name, source);

            match filter::classify(&entry) {
                Ok(()) => {
                    if let Some(app) = filter::to_installed_app(entry) {
                        accepted.push(app);
                    }
                }
                Err(reason) => *scan.rejected.entry(reason).or_default() += 1,
            }
        }
    }

    let before = accepted.len();
    scan.apps = deduplicate(accepted);
    scan.duplicates_collapsed = before - scan.apps.len();
    scan
}

/// Reads every value of interest from one `Uninstall` subkey.
fn read_entry(key: &RegKey, key_name: String, source: AppSource) -> RawUninstallEntry {
    RawUninstallEntry {
        key_name,
        source: Some(source),
        display_name: key.string_value("DisplayName"),
        display_version: key.string_value("DisplayVersion"),
        publisher: key.string_value("Publisher"),
        install_location: key.string_value("InstallLocation"),
        uninstall_string: key.string_value("UninstallString"),
        quiet_uninstall_string: key.string_value("QuietUninstallString"),
        install_date_raw: key.string_value("InstallDate"),
        estimated_size_kb: key.dword_value("EstimatedSize"),
        system_component: key.dword_value("SystemComponent"),
        windows_installer: key.dword_value("WindowsInstaller"),
        // Either value marks the entry as a child; installers write one or
        // the other and rarely both.
        parent_key_name: key
            .string_value("ParentKeyName")
            .or_else(|| key.string_value("ParentDisplayName")),
        release_type: key.string_value("ReleaseType"),
        no_remove: key.dword_value("NoRemove"),
    }
}

/// The uninstall command an entry published, read fresh from the registry.
///
/// `uninstall_app` used to execute whatever string the webview sent through
/// `cmd /c` — every other mutating command takes an identity the backend
/// verifies, and this one took a command line. Now the webview names the
/// entry (`key_name` + the view it came from) and the command is re-read
/// here, so nothing but a vendor's own published uninstaller can be run.
///
/// # Errors
///
/// - [`Error::NotFound`] when the entry is gone (already uninstalled) or
///   published no `UninstallString`.
/// - [`Error::Refused`] when the entry sets `NoRemove` or `SystemComponent`:
///   it declares itself not removable, and the list never offered it.
pub fn uninstall_command(key_name: &str, source: AppSource) -> Result<String> {
    // A subkey name never contains a separator; one that does is trying to
    // walk out of the Uninstall key.
    if key_name.is_empty() || key_name.contains(['\\', '/']) {
        return Err(Error::NotFound(format!(
            "no uninstall entry named {key_name:?}"
        )));
    }
    let hive = if source.is_per_user() {
        Hive::CurrentUser
    } else {
        Hive::LocalMachine
    };
    let key = RegKey::open(hive, UNINSTALL_PATH, source.is_wow64())
        .and_then(|root| root.open_child(key_name))
        .map_err(|_| Error::NotFound(format!("{key_name} is no longer installed")))?;

    if key.dword_value("NoRemove") == Some(1) || key.dword_value("SystemComponent") == Some(1) {
        return Err(Error::Refused(format!(
            "{key_name} declares that it cannot be uninstalled"
        )));
    }

    key.string_value("UninstallString")
        .map(|command| command.trim().to_owned())
        .filter(|command| !command.is_empty())
        .ok_or_else(|| Error::NotFound(format!("{key_name} published no uninstall command")))
}

/// Enumerates MSIX / Microsoft Store packages.
///
/// **Not implemented — always returns [`Error::Unsupported`].**
///
/// Store packages are not registered under `Uninstall` and cannot be read
/// from the registry at all. The supported route is the `WinRT`
/// `Windows.Management.Deployment.PackageManager` API (what `Get-AppxPackage`
/// wraps), which needs the `windows` crate's `WinRT` feature set enabled on
/// this crate. Shelling out to `PowerShell` would work but costs several
/// hundred milliseconds per call and would make the applications list depend
/// on an external interpreter and its execution policy.
///
/// Returning an error rather than an empty list is deliberate: an empty
/// `Vec` would be indistinguishable from "this machine has no Store apps",
/// and the UI would silently show an incomplete list as though it were
/// complete.
///
/// # Errors
///
/// Always [`Error::Unsupported`].
pub fn enumerate_store_apps() -> Result<Vec<InstalledApp>> {
    Err(Error::Unsupported(Capability::UninstallApplications))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uninstall_command_is_only_ever_read_from_a_real_registry_entry() {
        // The webview can name an entry, nothing more. A name that is not
        // an entry, or that tries to climb out of the Uninstall key, yields
        // no command at all.
        for bad in [
            "",
            "..\\..\\Run",
            "a/b",
            "definitely-not-an-installed-product-7f3a",
        ] {
            assert!(
                matches!(
                    uninstall_command(bad, AppSource::UserNative),
                    Err(Error::NotFound(_))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_listed_app_resolves_to_exactly_the_command_the_list_showed() {
        let scan = enumerate_installed_apps();
        let Some(app) = scan.apps.iter().find(|a| a.uninstall_string.is_some()) else {
            eprintln!("no uninstallable app on this machine; nothing to compare");
            return;
        };
        let resolved = uninstall_command(&app.key_name, app.source).expect("resolves");
        assert_eq!(
            Some(resolved.as_str()),
            app.uninstall_string.as_deref().map(str::trim)
        );
    }

    #[test]
    fn a_real_machine_has_installed_applications() {
        let scan = enumerate_installed_apps();
        assert!(
            !scan.apps.is_empty(),
            "no applications found at all; examined {} registry entries, rejected {}",
            scan.examined,
            scan.rejected_total()
        );
    }

    #[test]
    fn the_scan_reads_more_entries_than_it_shows() {
        let scan = enumerate_installed_apps();
        assert!(
            scan.examined >= scan.apps.len(),
            "shown {} exceeds examined {}, so entries are being invented",
            scan.apps.len(),
            scan.examined
        );
    }

    #[test]
    fn every_listed_application_has_a_usable_name() {
        let scan = enumerate_installed_apps();
        for app in &scan.apps {
            assert!(
                !app.display_name.trim().is_empty(),
                "entry {} passed the filter with a blank name",
                app.key_name
            );
        }
    }

    #[test]
    fn no_application_is_listed_twice() {
        let scan = enumerate_installed_apps();
        let mut seen = std::collections::HashSet::new();
        for app in &scan.apps {
            let key = (
                app.display_name.to_ascii_lowercase(),
                app.version.clone().unwrap_or_default(),
            );
            assert!(
                seen.insert(key),
                "{} {:?} appears more than once after deduplication",
                app.display_name,
                app.version
            );
        }
    }

    #[test]
    fn sizes_are_plausible_rather_than_off_by_a_factor_of_1024() {
        // Every non-trivial application occupies more than 64 KiB. A reader
        // that forgot EstimatedSize is in kilobytes would report almost
        // everything below that line while still looking like real data.
        let scan = enumerate_installed_apps();
        let sized: Vec<_> = scan
            .apps
            .iter()
            .filter_map(|a| a.estimated_size.map(|s| (a, s)))
            .collect();

        if sized.is_empty() {
            return;
        }

        let largest = sized
            .iter()
            .map(|(_, size)| size.get())
            .max()
            .unwrap_or_default();

        assert!(
            largest > 64 * 1024,
            "the largest reported application is only {largest} bytes, which means \
             EstimatedSize was read as bytes instead of kilobytes"
        );
    }

    #[test]
    fn no_size_is_ever_reported_as_zero_bytes() {
        let scan = enumerate_installed_apps();
        for app in &scan.apps {
            if let Some(size) = app.estimated_size {
                assert!(
                    size.get() > 0,
                    "{} reports 0 B, which must be represented as an unknown size instead",
                    app.display_name
                );
            }
        }
    }

    #[test]
    fn install_dates_that_are_present_are_within_a_believable_range() {
        let scan = enumerate_installed_apps();
        for app in &scan.apps {
            if let Some(date) = app.install_date {
                assert!(
                    (1990..=2100).contains(&date.year),
                    "{} claims an install date of {}, which is not a real date",
                    app.display_name,
                    date
                );
            }
        }
    }

    #[test]
    fn both_32_and_64_bit_products_are_discovered() {
        // A machine with no 32-bit software at all is possible but very
        // unusual; the assertion is written so its failure names the real
        // cause rather than just reporting a count.
        let scan = enumerate_installed_apps();
        let wow64 = scan.apps.iter().filter(|a| a.source.is_wow64()).count();
        let native = scan.apps.iter().filter(|a| !a.source.is_wow64()).count();

        assert!(
            native > 0,
            "no 64-bit applications found; the native registry view was not read"
        );
        assert!(
            wow64 > 0,
            "no 32-bit applications found across {} total: KEY_WOW64_32KEY is \
             probably being ignored, hiding every WOW6432Node product",
            scan.apps.len()
        );
    }

    #[test]
    fn the_filter_actually_discards_something() {
        let scan = enumerate_installed_apps();
        assert!(
            scan.rejected_total() > 0,
            "nothing was filtered out of {} examined entries; a raw dump of the \
             Uninstall keys always contains system components and updates",
            scan.examined
        );
    }

    #[test]
    fn store_apps_report_unsupported_rather_than_an_empty_list() {
        let err = enumerate_store_apps().expect_err("MSIX enumeration is not implemented");
        assert!(
            matches!(err, Error::Unsupported(_)),
            "an unimplemented source must be Unsupported so the UI can hide it, \
             not an empty Vec that reads as 'no Store apps installed'; got {err:?}"
        );
    }
}
