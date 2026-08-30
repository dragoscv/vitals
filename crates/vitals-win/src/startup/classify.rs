//! Which startup items are safe to switch off.
//!
//! Pure and platform-free, so the judgement is exhaustively testable without
//! breaking a machine.
//!
//! ## Why this exists
//!
//! Task Manager's Startup tab lets you disable anything it lists, with no
//! distinction between "a game launcher" and "the driver service your
//! trackpad needs". It gets away with it because it lists almost nothing
//! dangerous — it hides services and scheduled tasks entirely. Showing those,
//! as this module's callers do, means the safety judgement can no longer be
//! skipped: disabling `RpcSs` leaves a machine that will not boot to a
//! desktop, and there is no UI affordance to undo it from.
//!
//! The classification is deliberately conservative. A false "risky" costs the
//! user one extra confirmation; a false "safe" costs them a recovery console.

use super::{StartupEntry, StartupSource};

/// How safe it is to disable an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisableRisk {
    /// An ordinary third-party item. Disabling it delays nothing that
    /// matters.
    Safe,
    /// Something will visibly stop working — a vendor's update checker, a
    /// cloud-storage sync client, a peripheral's control panel.
    Degrades,
    /// Windows depends on it. Disabling breaks logon, networking or the
    /// desktop shell.
    SystemCritical,
    /// The OS will refuse regardless of privilege, so the UI must not offer
    /// the toggle at all.
    Forbidden,
}

impl DisableRisk {
    /// Whether the user must confirm before the toggle proceeds.
    #[must_use]
    pub const fn needs_confirmation(self) -> bool {
        matches!(self, Self::Degrades | Self::SystemCritical)
    }

    /// Whether the toggle can succeed at all.
    #[must_use]
    pub const fn is_possible(self) -> bool {
        !matches!(self, Self::Forbidden)
    }
}

/// Services that must never be presented as safely disableable.
///
/// These are the ones whose `Start` value Windows itself refuses to change
/// through `services.msc`, plus those where the change is *permitted* and
/// leaves an unbootable machine — the dangerous case, because nothing stops
/// you. `RpcSs` is the canonical example: setting it to Disabled is allowed
/// and the next boot reaches a black screen.
///
/// Matched case-insensitively against the service key name, never the display
/// name, which is localised and therefore useless as a key.
const CRITICAL_SERVICES: &[&str] = &[
    "rpcss", // COM/RPC endpoint mapper — nothing starts without it
    "rpcendpointmapper",
    "dcomlaunch", // hosts RpcSs; marked critical, bugchecks on failure
    "plugplay",
    "power",
    "brokerinfrastructure",
    "systemeventsbroker",
    "lsm",     // local session manager — no logon without it
    "profsvc", // user profile service — logon fails with a temp profile
    "eventlog",
    "nsi", // network store interface — no TCP/IP configuration
    "dhcp",
    "dnscache",
    "winmgmt", // WMI — half of Windows management stops
    "gpsvc",   // group policy — logon hangs on a domain machine
    "cryptsvc",
    "trustedinstaller",
    "msiserver",
    "schedule", // task scheduler; Windows Update depends on it
    "samss",
    "netlogon",
    "termservice", // disabling this locks out remote administration
    "audiosrv",
    "audioendpointbuilder",
    "wdiservicehost",
    "windefend", // Defender — silently reduces the machine's protection
    "wscsvc",
    "mpssvc", // Windows Firewall
    "bfe",    // base filtering engine; firewall cannot run without it
    "coremessagingregistrar",
    "usermanager",
    "statecoordinator",
    "tokenbroker",
    "storsvc",
    "wpndeps",
];

/// Executables that are part of Windows and legitimately auto-start.
///
/// Present so that a system component appearing in a `Run` key is not
/// reported as a third-party offender. `SecurityHealthSystray` is a real
/// example: it lives in a machine `Run` key and looks exactly like bloatware.
const SYSTEM_IMAGES: &[&str] = &[
    "securityhealthsystray.exe",
    "explorer.exe",
    "ctfmon.exe",
    "sihost.exe",
    "rundll32.exe",
    "userinit.exe",
    "onedrivesetup.exe",
];

/// Scheduled-task path prefixes owned by Windows.
///
/// Everything under `\Microsoft\Windows\` is a component task. Disabling
/// individual ones is a documented way to break Windows Update, defragging
/// and system restore, in each case with no error at the time.
const SYSTEM_TASK_PREFIXES: &[&str] = &[r"\Microsoft\Windows\", r"\Microsoft\XblGameSave\"];

/// Facts about an entry that the risk judgement needs.
///
/// A separate struct from [`StartupEntry`] so the decision can be tested
/// against synthetic inputs without constructing a plausible-looking entry
/// and, more importantly, so adding a field to the entry cannot silently
/// change the judgement.
#[derive(Debug, Clone, Copy)]
pub struct EntryFacts<'a> {
    pub source: StartupSource,
    /// Registry value name, service key name or task path.
    pub name: &'a str,
    /// File name of the image, lower-cased by the caller or not — the
    /// comparison is case-insensitive either way.
    pub image_file_name: Option<&'a str>,
    /// Whether the service is marked critical by its own configuration, when
    /// that could be read. Authoritative when present: a third-party service
    /// can be marked critical too, and no static list will know.
    pub kernel_critical: Option<bool>,
}

/// Judges how risky disabling an entry is.
///
/// The static lists are a fast pre-filter; `kernel_critical` overrides them
/// when the OS itself has an opinion.
#[must_use]
pub fn assess_disable(facts: EntryFacts<'_>) -> DisableRisk {
    // A driver or a service the kernel marks critical outranks every list
    // below, in both directions of surprise: a third-party filter driver can
    // be critical, and our list will never have heard of it.
    if facts.kernel_critical == Some(true) {
        return DisableRisk::Forbidden;
    }

    match facts.source {
        StartupSource::Service => {
            if CRITICAL_SERVICES
                .iter()
                .any(|s| facts.name.eq_ignore_ascii_case(s))
            {
                return DisableRisk::SystemCritical;
            }
            // An unrecognised automatic service is still more consequential
            // than a Run key: it may be a driver's user-mode half, and the
            // failure mode is a peripheral that stops working with no
            // message. Degrades, not Safe.
            DisableRisk::Degrades
        }

        StartupSource::ScheduledTask => {
            if SYSTEM_TASK_PREFIXES
                .iter()
                .any(|prefix| starts_with_ignore_case(facts.name, prefix))
            {
                return DisableRisk::SystemCritical;
            }
            DisableRisk::Safe
        }

        // A RunOnce value deletes itself after running. Offering to disable
        // it is offering to change something that is about to disappear, and
        // it is frequently the second half of an installer that has already
        // rebooted once — cancelling it can leave a half-installed product.
        StartupSource::MachineRunOnce
        | StartupSource::MachineRunOnce32
        | StartupSource::UserRunOnce => DisableRisk::Degrades,

        _ => {
            if is_system_image(facts.image_file_name) {
                DisableRisk::Degrades
            } else {
                DisableRisk::Safe
            }
        }
    }
}

/// Whether the image is a Windows component rather than third-party.
#[must_use]
pub fn is_system_image(file_name: Option<&str>) -> bool {
    file_name.is_some_and(|name| {
        SYSTEM_IMAGES
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
    })
}

/// Whether a service key name is on the never-disable list.
#[must_use]
pub fn is_critical_service(service_name: &str) -> bool {
    CRITICAL_SERVICES
        .iter()
        .any(|s| service_name.eq_ignore_ascii_case(s))
}

/// Extracts the executable path from a raw command line.
///
/// Two forms occur and they need opposite handling:
///
/// - `"C:\Program Files\App\a.exe" --flag` — quoted, so the path ends at the
///   closing quote and may contain spaces.
/// - `C:\Windows\system32\a.exe --flag` — unquoted, so the path ends at the
///   first space.
///
/// The unquoted form is genuinely ambiguous when the path contains spaces
/// (`C:\Program Files\a b.exe` with no quotes is a real, if broken, thing to
/// find in the registry). Rather than guess, the first token is returned and
/// the caller may find it does not exist — which is the truth, and is exactly
/// what Windows itself would do when trying to launch it.
///
/// Returns `None` for an empty or whitespace-only command, because there is
/// nothing to report and `Some("")` would render as a blank path that looks
/// measured.
#[must_use]
pub fn extract_image_path(command: &str) -> Option<&str> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix('"') {
        let end = rest.find('"')?;
        let path = &rest[..end];
        return if path.is_empty() { None } else { Some(path) };
    }

    // `rundll32.exe foo.dll,Entry` is one token by this rule, which is
    // correct: rundll32 is the image, the rest are arguments.
    Some(trimmed.split_whitespace().next().unwrap_or(trimmed))
}

/// The file name component of a path, without allocating.
#[must_use]
pub fn file_name_of(path: &str) -> Option<&str> {
    let name = path.rsplit(['\\', '/']).next()?;
    if name.is_empty() { None } else { Some(name) }
}

/// Convenience: judges a fully built entry.
///
/// Takes a reference rather than the value because [`StartupEntry`] owns
/// several `String`s and the judgement needs none of them afterwards.
#[must_use]
pub fn assess_entry(entry: &StartupEntry, kernel_critical: Option<bool>) -> DisableRisk {
    let image_file_name = entry
        .image_path
        .as_deref()
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str);

    assess_disable(EntryFacts {
        source: entry.source,
        name: &entry.name,
        image_file_name,
        kernel_critical,
    })
}

/// Case-insensitive prefix test that does not allocate.
fn starts_with_ignore_case(haystack: &str, prefix: &str) -> bool {
    haystack.len() >= prefix.len()
        && haystack
            .as_bytes()
            .iter()
            .zip(prefix.as_bytes())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(source: StartupSource, name: &str) -> EntryFacts<'_> {
        EntryFacts {
            source,
            name,
            image_file_name: None,
            kernel_critical: None,
        }
    }

    #[test]
    fn rpcss_is_never_offered_as_a_safe_toggle() {
        // Setting RpcSs to Disabled is permitted by services.msc and leaves
        // a machine that boots to a black screen with no way back short of
        // the recovery console. Nothing about the API stops you.
        let risk = assess_disable(facts(StartupSource::Service, "RpcSs"));
        assert_eq!(risk, DisableRisk::SystemCritical);
        assert!(risk.needs_confirmation());
    }

    #[test]
    fn critical_service_matching_ignores_case() {
        // Service key names come back from the SCM with the casing the
        // installer chose: "RpcSs", "rpcss" and "RPCSS" all occur.
        for spelling in ["RpcSs", "rpcss", "RPCSS", "RpCsS"] {
            assert!(
                is_critical_service(spelling),
                "{spelling} must match the critical list; a case-sensitive \
                 compare would let it through as merely Degrades"
            );
        }
    }

    #[test]
    fn kernel_flag_overrides_the_static_list_in_the_dangerous_direction() {
        // A third-party service can be marked critical. The static list will
        // never contain it, so the flag must win.
        let risk = assess_disable(EntryFacts {
            kernel_critical: Some(true),
            ..facts(StartupSource::Service, "SomeVendorFilter")
        });
        assert_eq!(risk, DisableRisk::Forbidden);
        assert!(
            !risk.is_possible(),
            "the UI must hide the toggle, not offer a UAC prompt that leads \
             to the same refusal"
        );
    }

    #[test]
    fn unknown_service_is_degrades_not_safe() {
        assert_eq!(
            assess_disable(facts(StartupSource::Service, "SomeVendorUpdater")),
            DisableRisk::Degrades,
            "an unrecognised service may be a driver's user-mode half; the \
             conservative default costs one confirmation, the optimistic one \
             costs a peripheral that stops working silently"
        );
    }

    #[test]
    fn windows_component_tasks_are_protected() {
        for task in [
            r"\Microsoft\Windows\UpdateOrchestrator\Schedule Scan",
            r"\microsoft\windows\Defrag\ScheduledDefrag",
            r"\Microsoft\XblGameSave\XblGameSaveTask",
        ] {
            assert_eq!(
                assess_disable(facts(StartupSource::ScheduledTask, task)),
                DisableRisk::SystemCritical,
                "{task} is a Windows component task"
            );
        }
    }

    #[test]
    fn third_party_task_is_safe() {
        assert_eq!(
            assess_disable(facts(
                StartupSource::ScheduledTask,
                r"\GoogleUpdateTaskMachineUA"
            )),
            DisableRisk::Safe
        );
        assert_eq!(
            assess_disable(facts(
                StartupSource::ScheduledTask,
                r"\NVIDIA\NvTmRepOnLogon"
            )),
            DisableRisk::Safe,
            "a vendor task outside \\Microsoft\\Windows is ordinary"
        );
    }

    #[test]
    fn task_name_that_merely_mentions_microsoft_is_not_protected() {
        // A prefix test, not a substring test: a third party may not claim
        // protection by putting the word in its task name.
        assert_eq!(
            assess_disable(facts(
                StartupSource::ScheduledTask,
                r"\Vendor\Microsoft\Windows\Updater"
            )),
            DisableRisk::Safe
        );
    }

    #[test]
    fn run_once_is_not_casually_disableable() {
        for source in [
            StartupSource::UserRunOnce,
            StartupSource::MachineRunOnce,
            StartupSource::MachineRunOnce32,
        ] {
            assert_eq!(
                assess_disable(facts(source, "SetupContinuation")),
                DisableRisk::Degrades,
                "{source:?} is often the second half of an installer that has \
                 already rebooted once"
            );
        }
    }

    #[test]
    fn windows_run_entries_are_flagged_as_components() {
        assert_eq!(
            assess_disable(EntryFacts {
                image_file_name: Some("SecurityHealthSystray.exe"),
                ..facts(StartupSource::MachineRun, "SecurityHealth")
            }),
            DisableRisk::Degrades,
            "it sits in a machine Run key and looks like bloatware, but it is \
             the Defender tray icon"
        );
    }

    #[test]
    fn ordinary_third_party_run_entry_is_safe() {
        assert_eq!(
            assess_disable(EntryFacts {
                image_file_name: Some("Discord.exe"),
                ..facts(StartupSource::UserRun, "Discord")
            }),
            DisableRisk::Safe
        );
    }

    #[test]
    fn quoted_command_keeps_spaces_in_the_path() {
        assert_eq!(
            extract_image_path(r#""C:\Program Files\App\a.exe" --minimised"#),
            Some(r"C:\Program Files\App\a.exe"),
            "splitting a quoted path on whitespace yields C:\\Program, which \
             does not exist and would be reported as a missing image"
        );
    }

    #[test]
    fn unquoted_command_stops_at_the_first_space() {
        assert_eq!(
            extract_image_path(r"C:\Windows\system32\cmd.exe /c foo"),
            Some(r"C:\Windows\system32\cmd.exe")
        );
    }

    #[test]
    fn rundll32_reports_the_host_as_the_image() {
        assert_eq!(
            extract_image_path(r"rundll32.exe shell32.dll,Control_RunDLL"),
            Some("rundll32.exe"),
            "the DLL is an argument; the process that starts is rundll32"
        );
    }

    #[test]
    fn empty_and_unterminated_commands_yield_nothing() {
        assert_eq!(extract_image_path(""), None);
        assert_eq!(extract_image_path("   "), None);
        assert_eq!(
            extract_image_path(r#""C:\a\b.exe --flag"#),
            None,
            "an unterminated quote is corrupt data; guessing where the path \
             ends would invent a measurement"
        );
        assert_eq!(extract_image_path(r#""" --flag"#), None);
    }

    #[test]
    fn file_name_extraction_handles_both_separators() {
        assert_eq!(file_name_of(r"C:\a\b\c.exe"), Some("c.exe"));
        assert_eq!(file_name_of("c.exe"), Some("c.exe"));
        assert_eq!(
            file_name_of(r"C:\a\"),
            None,
            "a trailing separator leaves no file name; Some(\"\") would \
             compare equal to nothing and read as a measured blank"
        );
    }

    #[test]
    fn risk_ordering_lets_the_ui_sort_by_severity() {
        assert!(DisableRisk::Safe < DisableRisk::Degrades);
        assert!(DisableRisk::Degrades < DisableRisk::SystemCritical);
        assert!(DisableRisk::SystemCritical < DisableRisk::Forbidden);
    }
}
