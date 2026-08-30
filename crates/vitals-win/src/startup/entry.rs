//! The shape of a startup item, independent of where it was found.
//!
//! One type covers registry Run values, Start Menu shortcuts, logon-triggered
//! scheduled tasks and automatic services, because from the user's point of
//! view they are the same thing: something that runs without being asked.
//! Task Manager splits them across three separate consoles (Startup Apps,
//! `services.msc`, `taskschd.msc`) and shows only the first, which is why
//! anything hiding in the other two stays hidden.

use std::path::PathBuf;

/// Where an entry was discovered.
///
/// Kept precise rather than collapsed to "registry"/"folder": the source
/// decides which enable/disable mechanism applies, and a 32-bit registry
/// entry needs a different view flag from a 64-bit one to write back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StartupSource {
    /// `HKLM\...\CurrentVersion\Run`, 64-bit view.
    MachineRun,
    /// `HKLM\...\CurrentVersion\Run` seen through the 32-bit view, which the
    /// kernel redirects to `WOW6432Node`. A distinct key with distinct
    /// contents, invisible to any tool that forgets to ask for it.
    MachineRun32,
    /// `HKLM\...\CurrentVersion\RunOnce`, 64-bit view.
    MachineRunOnce,
    /// `HKLM\...\CurrentVersion\RunOnce`, 32-bit view.
    MachineRunOnce32,
    /// `HKCU\...\CurrentVersion\Run`.
    UserRun,
    /// `HKCU\...\CurrentVersion\RunOnce`.
    UserRunOnce,
    /// `%ProgramData%\Microsoft\Windows\Start Menu\Programs\StartUp`.
    CommonStartupFolder,
    /// `%AppData%\Microsoft\Windows\Start Menu\Programs\Startup`.
    UserStartupFolder,
    /// A scheduled task with a logon or boot trigger.
    ScheduledTask,
    /// A service configured to start automatically.
    Service,
}

impl StartupSource {
    /// Whether the entry applies to every user of the machine.
    ///
    /// Drives the "affects all users" badge, and more importantly tells the
    /// UI that disabling will need elevation.
    #[must_use]
    pub const fn is_machine_wide(self) -> bool {
        matches!(
            self,
            Self::MachineRun
                | Self::MachineRun32
                | Self::MachineRunOnce
                | Self::MachineRunOnce32
                | Self::CommonStartupFolder
                | Self::Service
                | Self::ScheduledTask
        )
    }

    /// Whether the entry is consumed after a single run.
    ///
    /// `RunOnce` values delete themselves once executed, so presenting them
    /// as persistent startup items — as several third-party tools do — is
    /// wrong: the user disables something that was going to vanish anyway.
    #[must_use]
    pub const fn is_run_once(self) -> bool {
        matches!(
            self,
            Self::MachineRunOnce | Self::MachineRunOnce32 | Self::UserRunOnce
        )
    }

    /// Whether the value lives in the 32-bit registry view.
    #[must_use]
    pub const fn is_wow64_view(self) -> bool {
        matches!(self, Self::MachineRun32 | Self::MachineRunOnce32)
    }

    /// A stable identifier for logs and tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MachineRun => "hklm-run",
            Self::MachineRun32 => "hklm-run-wow64",
            Self::MachineRunOnce => "hklm-runonce",
            Self::MachineRunOnce32 => "hklm-runonce-wow64",
            Self::UserRun => "hkcu-run",
            Self::UserRunOnce => "hkcu-runonce",
            Self::CommonStartupFolder => "common-startup-folder",
            Self::UserStartupFolder => "user-startup-folder",
            Self::ScheduledTask => "scheduled-task",
            Self::Service => "service",
        }
    }
}

/// Whether an entry will actually run at the next logon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupState {
    /// It will run.
    Enabled,
    /// Explicitly switched off — by Task Manager's `StartupApproved` record,
    /// by a disabled task, or by a service set to `Disabled`.
    Disabled,
    /// The state could not be determined.
    ///
    /// Present deliberately. A permission failure reading `StartupApproved`
    /// must not be rendered as "enabled": that is a fabricated fact, and it
    /// is the reading the user would act on.
    Unknown,
}

impl StartupState {
    /// Whether the item runs, when that is known.
    ///
    /// Returns `None` for [`Unknown`](Self::Unknown) so a caller cannot
    /// accidentally treat "we could not tell" as "off".
    #[must_use]
    pub const fn runs(self) -> Option<bool> {
        match self {
            Self::Enabled => Some(true),
            Self::Disabled => Some(false),
            Self::Unknown => None,
        }
    }
}

/// A single thing that runs at logon or boot.
#[derive(Debug, Clone)]
pub struct StartupEntry {
    /// Registry value name, shortcut file stem, task name or service name.
    pub name: String,
    /// A friendlier label when one exists — a service's display name, or a
    /// task's URI. `None` when the source offers nothing better than `name`;
    /// the UI falls back rather than us duplicating the field.
    pub display_name: Option<String>,
    pub source: StartupSource,
    pub state: StartupState,
    /// The raw command line exactly as configured, quoting and arguments
    /// intact.
    ///
    /// `None` for a Start Menu shortcut whose target we could not resolve:
    /// `.lnk` parsing is not attempted here, and inventing the path from the
    /// file name would be a guess presented as a measurement.
    pub command: Option<String>,
    /// The resolved executable, when it could be extracted from `command`
    /// with confidence.
    pub image_path: Option<PathBuf>,
    /// The publisher, if known. Always `None` today — Authenticode
    /// verification belongs to the signature module and is not yet wired in.
    /// Kept in the shape so the UI column exists from the start and is
    /// honestly blank rather than absent.
    pub publisher: Option<String>,
    /// Live PID, for services that are currently running.
    pub pid: Option<u32>,
}

impl StartupEntry {
    /// The best human label available.
    #[must_use]
    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }

    /// Whether the configured image is missing from disk.
    ///
    /// An orphaned Run value — the uninstaller removed the binary but left
    /// the registry entry — costs a failed process creation at every logon
    /// and is worth surfacing. Returns `None` when there is no path to
    /// check, which is not the same as "the file is there".
    #[must_use]
    pub fn image_missing(&self) -> Option<bool> {
        self.image_path.as_ref().map(|p| !p.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_state_is_not_reported_as_disabled() {
        assert_eq!(
            StartupState::Unknown.runs(),
            None,
            "Unknown must stay unknown; collapsing it to false or true \
             invents a fact the UI will display as measured"
        );
        assert_eq!(StartupState::Enabled.runs(), Some(true));
        assert_eq!(StartupState::Disabled.runs(), Some(false));
    }

    #[test]
    fn wow64_sources_are_flagged_and_machine_wide() {
        for source in [StartupSource::MachineRun32, StartupSource::MachineRunOnce32] {
            assert!(source.is_wow64_view(), "{source:?} reads the 32-bit view");
            assert!(source.is_machine_wide(), "{source:?} lives under HKLM");
        }
        assert!(
            !StartupSource::UserRun.is_wow64_view(),
            "HKCU\\Software is not redirected, so there is no separate \
             32-bit view of the per-user Run key"
        );
    }

    #[test]
    fn run_once_entries_are_distinguished() {
        assert!(StartupSource::UserRunOnce.is_run_once());
        assert!(!StartupSource::UserRun.is_run_once());
        assert!(
            !StartupSource::Service.is_run_once(),
            "a service is never consumed after one start"
        );
    }

    #[test]
    fn source_identifiers_are_unique() {
        let all = [
            StartupSource::MachineRun,
            StartupSource::MachineRun32,
            StartupSource::MachineRunOnce,
            StartupSource::MachineRunOnce32,
            StartupSource::UserRun,
            StartupSource::UserRunOnce,
            StartupSource::CommonStartupFolder,
            StartupSource::UserStartupFolder,
            StartupSource::ScheduledTask,
            StartupSource::Service,
        ];
        let mut seen: Vec<&str> = all.iter().map(|s| s.as_str()).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(
            before,
            seen.len(),
            "two sources share an identifier, so log lines and persisted \
             per-entry settings would collide"
        );
    }

    #[test]
    fn label_falls_back_to_name() {
        let entry = StartupEntry {
            name: "Spooler".into(),
            display_name: None,
            source: StartupSource::Service,
            state: StartupState::Enabled,
            command: None,
            image_path: None,
            publisher: None,
            pid: None,
        };
        assert_eq!(entry.label(), "Spooler");
        assert_eq!(
            entry.image_missing(),
            None,
            "no path means unknown, not present"
        );
    }
}
