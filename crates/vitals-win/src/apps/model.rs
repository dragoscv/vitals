//! The domain model for an installed application.
//!
//! Kept separate from the registry reader so the filtering and parsing logic
//! can be exercised without a live registry — see [`super::filter`].

use vitals_core::units::Bytes;

/// Which registry hive and architectural view an entry was read from.
///
/// Recorded rather than discarded because the same product routinely appears
/// in more than one view, and knowing which one won matters when a user
/// reports a duplicate or a missing entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppSource {
    /// `HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall` — 64-bit view.
    MachineNative,
    /// The same path under `WOW6432Node`. Skipping this view hides every
    /// 32-bit application on a 64-bit machine, which on a typical desktop is
    /// roughly half of everything installed.
    MachineWow64,
    /// `HKCU\...\Uninstall` — per-user installs, 64-bit view.
    UserNative,
    /// Per-user installs, 32-bit view.
    UserWow64,
}

impl AppSource {
    /// Every source that must be read for a complete picture.
    pub const ALL: [Self; 4] = [
        Self::MachineNative,
        Self::MachineWow64,
        Self::UserNative,
        Self::UserWow64,
    ];

    /// Whether this source describes a per-user rather than machine-wide
    /// installation.
    #[must_use]
    pub const fn is_per_user(self) -> bool {
        matches!(self, Self::UserNative | Self::UserWow64)
    }

    /// Whether this source is the 32-bit (`WOW6432Node`) registry view.
    #[must_use]
    pub const fn is_wow64(self) -> bool {
        matches!(self, Self::MachineWow64 | Self::UserWow64)
    }

    /// Ranking used to pick a winner when the same product appears in several
    /// views. Lower wins.
    ///
    /// Machine-wide native entries are preferred because they are the ones
    /// Windows itself surfaces first, and their `UninstallString` is the one
    /// that works regardless of which user is signed in.
    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::MachineNative => 0,
            Self::MachineWow64 => 1,
            Self::UserNative => 2,
            Self::UserWow64 => 3,
        }
    }
}

/// A calendar date with no time and no zone.
///
/// The registry stores `InstallDate` as a bare `YYYYMMDD` string with no
/// timezone, so anything richer would be inventing precision that was never
/// recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstallDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl InstallDate {
    /// Renders as `YYYY-MM-DD`.
    #[must_use]
    pub fn to_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl std::fmt::Display for InstallDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_iso())
    }
}

/// The raw values read from one `Uninstall` subkey, before any judgement is
/// applied about whether it represents a real application.
///
/// Every field is `Option` because every one of them is genuinely optional in
/// the registry. A missing value is recorded as missing; it is never
/// substituted with a zero or an empty string, because "0 bytes" and "unknown
/// size" render identically in a table and only one of them is true.
#[derive(Debug, Clone, Default)]
pub struct RawUninstallEntry {
    /// The subkey name, e.g. `{90160000-008C-0000-1000-0000000FF1CE}` or
    /// `Mozilla Firefox 121.0`.
    pub key_name: String,
    pub source: Option<AppSource>,
    pub display_name: Option<String>,
    pub display_version: Option<String>,
    pub publisher: Option<String>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    /// Raw `InstallDate`, normally `YYYYMMDD`.
    pub install_date_raw: Option<String>,
    /// Raw `EstimatedSize`. Documented as kilobytes, **not** bytes.
    pub estimated_size_kb: Option<u32>,
    /// `SystemComponent` — set to 1 by things that are part of Windows or of
    /// another product's plumbing and must not be listed on their own.
    pub system_component: Option<u32>,
    /// `WindowsInstaller` — set to 1 for MSI-managed products.
    pub windows_installer: Option<u32>,
    /// `ParentKeyName` / `ParentDisplayName` — present on patches that belong
    /// to another entry rather than standing alone.
    pub parent_key_name: Option<String>,
    /// `ReleaseType`, e.g. `Security Update`, `Hotfix`, `Update`.
    pub release_type: Option<String>,
    /// `NoRemove` — the entry declares itself non-uninstallable.
    pub no_remove: Option<u32>,
}

/// An installed application, after filtering and unit conversion.
#[derive(Debug, Clone)]
pub struct InstalledApp {
    /// Registry subkey name. Stable across runs; usable as a UI key.
    pub key_name: String,
    /// Which registry view this entry was read from.
    pub source: AppSource,
    /// `DisplayName`. Guaranteed non-empty — an entry without one is filtered
    /// out rather than shown as a blank row.
    pub display_name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub install_date: Option<InstallDate>,
    /// Disk footprint as reported by the installer.
    ///
    /// Advisory only: it is whatever the installer chose to write, is often
    /// absent, and is never recomputed by Windows afterwards.
    pub estimated_size: Option<Bytes>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
    /// A command line that uninstalls without prompting, when the installer
    /// published one. Only a minority of entries do.
    pub quiet_uninstall_string: Option<String>,
    /// Whether the product is managed by Windows Installer.
    ///
    /// Worth surfacing because uninstalling differs: an MSI product can be
    /// removed with `msiexec /x <ProductCode> /qn` even when no
    /// `QuietUninstallString` was published, whereas a bespoke uninstaller
    /// generally cannot be driven silently at all.
    pub is_msi: bool,
    /// Whether this was installed for the current user only.
    pub per_user: bool,
}

impl InstalledApp {
    /// Whether this application can be removed without user interaction.
    #[must_use]
    pub fn supports_silent_uninstall(&self) -> bool {
        self.quiet_uninstall_string.is_some() || self.is_msi
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates_are_zero_padded() {
        let date = InstallDate {
            year: 2024,
            month: 3,
            day: 7,
        };
        assert_eq!(
            date.to_iso(),
            "2024-03-07",
            "unpadded components sort incorrectly as strings"
        );
    }

    #[test]
    fn machine_native_outranks_every_other_view() {
        let mut ordered = AppSource::ALL;
        ordered.sort_by_key(|s| s.precedence());
        assert_eq!(
            ordered[0],
            AppSource::MachineNative,
            "the machine-wide 64-bit entry must win a duplicate contest"
        );
    }

    #[test]
    fn wow64_and_per_user_classification() {
        assert!(AppSource::MachineWow64.is_wow64());
        assert!(!AppSource::MachineWow64.is_per_user());
        assert!(AppSource::UserWow64.is_wow64() && AppSource::UserWow64.is_per_user());
        assert!(!AppSource::MachineNative.is_wow64());
    }

    #[test]
    fn msi_products_count_as_silently_removable_without_a_quiet_string() {
        let app = InstalledApp {
            key_name: "{GUID}".into(),
            source: AppSource::MachineNative,
            display_name: "Thing".into(),
            publisher: None,
            version: None,
            install_date: None,
            estimated_size: None,
            install_location: None,
            uninstall_string: Some("MsiExec.exe /X{GUID}".into()),
            quiet_uninstall_string: None,
            is_msi: true,
            per_user: false,
        };
        assert!(
            app.supports_silent_uninstall(),
            "an MSI product is removable with /qn even with no QuietUninstallString"
        );
    }
}
