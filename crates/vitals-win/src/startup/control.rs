//! Changing what runs: enable/disable a startup item, start/stop a service,
//! change a service's start type.
//!
//! ## The same mechanism Task Manager uses, not a deletion
//!
//! Disabling a `Run` value or a Startup-folder shortcut writes a
//! `StartupApproved` record (see [`super::approved`]) and leaves the value or
//! shortcut exactly where it was. That is what Task Manager does, so the two
//! tools agree about the state afterwards, and it is reversible without
//! knowing what the command line used to be. Deleting the value — which some
//! "startup managers" do — loses the command, and "enable it again" then has
//! nothing to restore. A scheduled task is disabled through its own flag, and
//! a service through its start type, for the same reason.
//!
//! ## One UAC prompt per change
//!
//! Machine-wide entries and every service change need administrator rights.
//! As with process actions ([`crate::actions::elevated`]), the running app
//! never elevates: [`apply_or_elevate`] tries unelevated, and on a denial
//! re-launches this executable under `runas` with [`STARTUP_ACTION_ARG`] to
//! make exactly that one change. The child re-checks the risk itself, so the
//! elevated pass cannot be used to make a change the desktop would refuse.

use vitals_core::error::{Error, Result};

use super::StartupSource;
use super::approved::{FOLDER, RUN, RUN32};
use super::classify::{DisableRisk, EntryFacts, assess_disable};
use super::services::{ServiceControl, SettableStartType, control_service, set_start_type};
use crate::actions::Consent;

/// The argument the elevated instance is started with.
pub const STARTUP_ACTION_ARG: &str = "--elevated-startup-action";

/// One change the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupChange {
    /// Enable or disable a registry, folder or scheduled-task entry.
    SetEnabled {
        source: StartupSource,
        name: String,
        enabled: bool,
    },
    /// Start, stop or restart a service.
    Service {
        name: String,
        control: ServiceControl,
    },
    /// Change when a service starts.
    StartType {
        name: String,
        start_type: SettableStartType,
    },
}

impl StartupChange {
    /// Whether this change makes something run less.
    ///
    /// Only those need the user's confirmation on a risky item: turning a
    /// critical service back *on* is the fix, not the hazard, and putting a
    /// warning in front of it would train people to click through warnings.
    #[must_use]
    pub const fn reduces(&self) -> bool {
        match self {
            Self::SetEnabled { enabled, .. } => !*enabled,
            Self::Service { control, .. } => {
                matches!(control, ServiceControl::Stop | ServiceControl::Restart)
            }
            Self::StartType { start_type, .. } => {
                matches!(
                    start_type,
                    SettableStartType::Manual | SettableStartType::Disabled
                )
            }
        }
    }

    /// How risky the target is, from facts the change itself carries.
    ///
    /// The image name is not known here, so a `Run` value pointing at a
    /// Windows component is judged `Safe` rather than `Degrades`. That only
    /// ever loosens a confirmation the desktop already asked for; the two
    /// cases that can break a machine — critical services and Windows
    /// scheduled tasks — are decided by name alone and are caught.
    #[must_use]
    pub fn risk(&self) -> DisableRisk {
        let (source, name) = match self {
            Self::SetEnabled { source, name, .. } => (*source, name.as_str()),
            Self::Service { name, .. } | Self::StartType { name, .. } => {
                (StartupSource::Service, name.as_str())
            }
        };
        assess_disable(EntryFacts {
            source,
            name,
            image_file_name: None,
            kernel_critical: None,
        })
    }
}

/// Makes the change in this process, without elevating.
///
/// # Errors
///
/// - [`Error::Refused`] when the target is one Windows will not let be
///   changed, when a run-once value is targeted, or when a risky reduction
///   arrives without [`Consent::Confirmed`].
/// - [`Error::AccessDenied`] when administrator rights are needed.
/// - [`Error::NotFound`] when the entry, task or service does not exist.
/// - [`Error::Os`] for anything else.
pub fn apply(change: &StartupChange, consent: Consent) -> Result<()> {
    check(change, consent)?;

    match change {
        StartupChange::SetEnabled {
            source,
            name,
            enabled,
        } => {
            if *source == StartupSource::ScheduledTask {
                set_task_enabled(name, *enabled)
            } else {
                write_approval(*source, name, *enabled)
            }
        }
        StartupChange::Service { name, control } => control_service(name, *control),
        StartupChange::StartType { name, start_type } => set_start_type(name, *start_type),
    }
}

/// [`apply`], retried behind a UAC prompt when it is denied.
///
/// Blocks for as long as the prompt is on screen; run it off the UI thread.
///
/// # Errors
///
/// As [`apply`]; a dismissed prompt is [`Error::Refused`].
pub fn apply_or_elevate(change: &StartupChange, consent: Consent) -> Result<()> {
    match apply(change, consent) {
        Err(Error::AccessDenied { .. }) => {
            let code = crate::actions::taskmgr::run_elevated(
                &format_args(change, consent),
                "nothing was changed",
            )?;
            from_exit_code(code)
        }
        other => other,
    }
}

fn check(change: &StartupChange, consent: Consent) -> Result<()> {
    if let StartupChange::SetEnabled { source, .. } = change
        && source.is_run_once()
    {
        return Err(Error::Refused(
            "run-once entries delete themselves after running, so there is nothing to switch off"
                .to_owned(),
        ));
    }
    if let StartupChange::SetEnabled {
        source: StartupSource::Service,
        ..
    } = change
    {
        return Err(Error::Refused(
            "a service is switched off by its start type, not by an enabled flag".to_owned(),
        ));
    }

    let risk = change.risk();
    if risk == DisableRisk::Forbidden {
        return Err(Error::Refused(
            "Windows does not allow this to be changed".to_owned(),
        ));
    }
    if change.reduces() && risk.needs_confirmation() && consent != Consent::Confirmed {
        return Err(Error::Refused(
            "this needs to be confirmed first, because something depends on it".to_owned(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// StartupApproved
// ---------------------------------------------------------------------------

/// Where the approval record for `source` lives.
fn approval_location(source: StartupSource) -> Option<(bool, &'static str)> {
    // (machine hive?, sub-key)
    match source {
        StartupSource::MachineRun => Some((true, RUN)),
        StartupSource::MachineRun32 => Some((true, RUN32)),
        StartupSource::UserRun => Some((false, RUN)),
        StartupSource::CommonStartupFolder => Some((true, FOLDER)),
        StartupSource::UserStartupFolder => Some((false, FOLDER)),
        _ => None,
    }
}

/// The twelve-byte record Explorer writes: state byte, padding, `FILETIME`.
///
/// `02` enabled with a zero timestamp, `03` disabled with the time of the
/// change — the exact bytes Task Manager produces, so it reads our record
/// back as its own.
#[must_use]
pub fn approval_blob(enabled: bool, now_filetime: u64) -> [u8; 12] {
    let mut blob = [0_u8; 12];
    blob[0] = if enabled { 0x02 } else { 0x03 };
    let stamp = if enabled { 0 } else { now_filetime };
    blob[4..].copy_from_slice(&stamp.to_le_bytes());
    blob
}

fn write_approval(source: StartupSource, name: &str, enabled: bool) -> Result<()> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_BINARY,
        REG_OPTION_NON_VOLATILE, RegCloseKey, RegCreateKeyExW, RegSetValueExW,
    };
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

    const ERROR_ACCESS_DENIED: u32 = 5;

    let Some((machine, sub_key)) = approval_location(source) else {
        return Err(Error::Refused(format!(
            "{} entries cannot be switched off this way",
            source.as_str()
        )));
    };

    let mut now = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: `now` is a live out-parameter.
    unsafe { GetSystemTimeAsFileTime(&raw mut now) };
    let ticks = (u64::from(now.dwHighDateTime) << 32) | u64::from(now.dwLowDateTime);
    let blob = approval_blob(enabled, ticks);

    let root = if machine {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    };
    let sub: Vec<u16> = sub_key.encode_utf16().chain(Some(0)).collect();
    let value: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();

    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `sub` is NUL-terminated; `key` is an out-parameter closed below.
    // Created rather than opened: on a machine where nothing was ever
    // toggled the `StartupApproved` key does not exist yet.
    let status = unsafe {
        RegCreateKeyExW(
            root,
            sub.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_WOW64_64KEY,
            std::ptr::null(),
            &raw mut key,
            std::ptr::null_mut(),
        )
    };
    if status == ERROR_ACCESS_DENIED {
        return Err(Error::AccessDenied {
            operation: format!("change the startup state of {name}"),
        });
    }
    if status != 0 {
        return Err(Error::Os {
            context: format!("RegCreateKeyExW({sub_key})"),
            code: status.cast_signed(),
        });
    }

    // SAFETY: `key` is open for writing; `blob` is 12 live bytes.
    let status = unsafe { RegSetValueExW(key, value.as_ptr(), 0, REG_BINARY, blob.as_ptr(), 12) };
    // SAFETY: `key` came from RegCreateKeyExW and is closed exactly once.
    unsafe { RegCloseKey(key) };

    match status {
        0 => Ok(()),
        ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
            operation: format!("change the startup state of {name}"),
        }),
        code => Err(Error::Os {
            context: format!("RegSetValueExW({name})"),
            code: code.cast_signed(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Scheduled tasks
// ---------------------------------------------------------------------------

fn set_task_enabled(path: &str, enabled: bool) -> Result<()> {
    use windows::Win32::Foundation::{VARIANT_FALSE, VARIANT_TRUE};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::System::TaskScheduler::{ITaskService, TaskScheduler};
    use windows::Win32::System::Variant::VARIANT;
    use windows::core::BSTR;

    /// `E_ACCESSDENIED` as an HRESULT.
    const E_ACCESSDENIED: i32 = 0x8007_0005_u32.cast_signed();
    /// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)`.
    const E_NOT_FOUND: i32 = 0x8007_0002_u32.cast_signed();

    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            // SAFETY: paired with the successful CoInitializeEx below.
            unsafe { CoUninitialize() };
        }
    }

    // SAFETY: no preconditions; see `taskcom::ComScope` for why MTA.
    if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
        return Err(Error::Os {
            context: "CoInitializeEx".to_owned(),
            code: 0,
        });
    }
    let _com = Com;

    let map = |context: &str, err: &windows::core::Error| -> Error {
        match err.code().0 {
            E_ACCESSDENIED => Error::AccessDenied {
                operation: format!("change the scheduled task {path}"),
            },
            E_NOT_FOUND => Error::NotFound(format!("no scheduled task is at {path}")),
            code => Error::Os {
                context: format!("{context}({path})"),
                code,
            },
        }
    };

    // SAFETY: the apartment is initialised for the lifetime of `_com`.
    let service: ITaskService =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| map("CoCreateInstance", &e))?;
    // SAFETY: empty variants mean the local machine and current user.
    unsafe {
        service.Connect(
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
        )
    }
    .map_err(|e| map("Connect", &e))?;
    // SAFETY: the root folder always exists once connected.
    let root = unsafe { service.GetFolder(&BSTR::from("\\")) }.map_err(|e| map("GetFolder", &e))?;
    // SAFETY: `GetTask` on the root accepts the task's full path.
    let task = unsafe { root.GetTask(&BSTR::from(path)) }.map_err(|e| map("GetTask", &e))?;
    let flag = if enabled { VARIANT_TRUE } else { VARIANT_FALSE };
    // SAFETY: `task` is live.
    unsafe { task.SetEnabled(flag) }.map_err(|e| map("SetEnabled", &e))
}

// ---------------------------------------------------------------------------
// The elevated pass
// ---------------------------------------------------------------------------

mod exit {
    pub const OK: u32 = 0;
    pub const NOT_FOUND: u32 = 10;
    pub const ACCESS_DENIED: u32 = 11;
    pub const REFUSED: u32 = 12;
    pub const BAD_ARGS: u32 = 13;
    pub const FAILED: u32 = 14;
}

/// Hex of the UTF-8 bytes.
///
/// Names carry spaces, quotes and backslashes — `\Microsoft\Windows\…`,
/// "Adobe Acrobat Update" — and they cross a `ShellExecuteExW` command line
/// that Windows re-parses with its own quoting rules. Hex has no characters
/// that any of those rules touch, so the child receives exactly the bytes
/// the parent sent.
fn encode(text: &str) -> String {
    use std::fmt::Write as _;
    text.bytes()
        .fold(String::with_capacity(text.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

fn decode(hex: &str) -> Option<String> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    String::from_utf8(bytes?).ok()
}

const fn control_arg(control: ServiceControl) -> &'static str {
    match control {
        ServiceControl::Start => "start",
        ServiceControl::Stop => "stop",
        ServiceControl::Restart => "restart",
    }
}

const fn start_type_arg(start_type: SettableStartType) -> &'static str {
    match start_type {
        SettableStartType::Automatic => "automatic",
        SettableStartType::Manual => "manual",
        SettableStartType::Disabled => "disabled",
    }
}

/// The arguments for one elevated change, in the order [`parse_args`] reads.
#[must_use]
pub fn format_args(change: &StartupChange, consent: Consent) -> String {
    let consent = match consent {
        Consent::Confirmed => "confirmed",
        Consent::Unconfirmed => "unconfirmed",
    };
    let body = match change {
        StartupChange::SetEnabled {
            source,
            name,
            enabled,
        } => format!(
            "entry {} {} {}",
            source.as_str(),
            encode(name),
            if *enabled { "on" } else { "off" }
        ),
        StartupChange::Service { name, control } => {
            format!("service {} {}", encode(name), control_arg(*control))
        }
        StartupChange::StartType { name, start_type } => {
            format!(
                "start-type {} {}",
                encode(name),
                start_type_arg(*start_type)
            )
        }
    };
    format!("{STARTUP_ACTION_ARG} {body} {consent}")
}

/// Reads the arguments after [`STARTUP_ACTION_ARG`]. `None` for anything
/// malformed: the caller is our own parent, and guessing would change the
/// wrong thing.
#[must_use]
pub fn parse_args<I, S>(rest: I) -> Option<(StartupChange, Consent)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = rest.into_iter().map(|a| a.as_ref().to_owned()).collect();
    let (consent, body) = args.split_last()?;
    let consent = match consent.as_str() {
        "confirmed" => Consent::Confirmed,
        "unconfirmed" => Consent::Unconfirmed,
        _ => return None,
    };

    let change = match body {
        [kind, source, name, state] if kind == "entry" => StartupChange::SetEnabled {
            source: StartupSource::from_key(source)?,
            name: decode(name)?,
            enabled: match state.as_str() {
                "on" => true,
                "off" => false,
                _ => return None,
            },
        },
        [kind, name, control] if kind == "service" => StartupChange::Service {
            name: decode(name)?,
            control: match control.as_str() {
                "start" => ServiceControl::Start,
                "stop" => ServiceControl::Stop,
                "restart" => ServiceControl::Restart,
                _ => return None,
            },
        },
        [kind, name, start_type] if kind == "start-type" => StartupChange::StartType {
            name: decode(name)?,
            start_type: match start_type.as_str() {
                "automatic" => SettableStartType::Automatic,
                "manual" => SettableStartType::Manual,
                "disabled" => SettableStartType::Disabled,
                _ => return None,
            },
        },
        _ => return None,
    };
    Some((change, consent))
}

/// The elevated instance's whole job: one change, then an exit code.
#[must_use]
pub fn perform<I, S>(rest: I) -> u32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some((change, consent)) = parse_args(rest) else {
        return exit::BAD_ARGS;
    };
    match apply(&change, consent) {
        Ok(()) => exit::OK,
        Err(Error::NotFound(_)) => exit::NOT_FOUND,
        Err(Error::AccessDenied { .. }) => exit::ACCESS_DENIED,
        Err(Error::Refused(_)) => exit::REFUSED,
        Err(_) => exit::FAILED,
    }
}

fn from_exit_code(code: u32) -> Result<()> {
    match code {
        exit::OK => Ok(()),
        exit::NOT_FOUND => Err(Error::NotFound(
            "the item disappeared before the change could be made".to_owned(),
        )),
        exit::ACCESS_DENIED => Err(Error::AccessDenied {
            operation: "make this change, even as administrator".to_owned(),
        }),
        exit::REFUSED => Err(Error::Refused(
            "Windows refused the change, even as administrator".to_owned(),
        )),
        other => Err(Error::Os {
            context: "the elevated startup change".to_owned(),
            code: other.cast_signed(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_change() -> Vec<StartupChange> {
        vec![
            StartupChange::SetEnabled {
                source: StartupSource::MachineRun32,
                name: "Adobe \"Acrobat\" Update, 2".into(),
                enabled: false,
            },
            StartupChange::SetEnabled {
                source: StartupSource::ScheduledTask,
                name: r"\Microsoft\Windows\Defrag\ScheduledDefrag".into(),
                enabled: true,
            },
            StartupChange::Service {
                name: "Spooler".into(),
                control: ServiceControl::Restart,
            },
            StartupChange::StartType {
                name: "Șofer ünïcode".into(),
                start_type: SettableStartType::Disabled,
            },
        ]
    }

    #[test]
    fn every_change_survives_the_elevated_command_line_unchanged() {
        for change in every_change() {
            for consent in [Consent::Confirmed, Consent::Unconfirmed] {
                let line = format_args(&change, consent);
                let mut words = line.split(' ');
                assert_eq!(words.next(), Some(STARTUP_ACTION_ARG));
                assert!(
                    !line.contains('"') && !line.contains('\\'),
                    "{line} carries a character the shell would re-parse"
                );
                assert_eq!(
                    parse_args(words),
                    Some((change.clone(), consent)),
                    "round trip of {line}"
                );
            }
        }
    }

    #[test]
    fn a_malformed_elevated_argument_changes_nothing() {
        assert_eq!(
            parse_args(["entry", "hkcu-run", "zz", "off", "confirmed"]),
            None
        );
        assert_eq!(parse_args(["service", "53", "pause", "confirmed"]), None);
        assert_eq!(parse_args(["service", "53", "stop"]), None);
        assert_eq!(perform(Vec::<String>::new()), exit::BAD_ARGS);
    }

    #[test]
    fn a_critical_service_is_not_stopped_or_disabled_without_confirmation() {
        // `check`, never `apply`: if this guard ever regressed on an elevated
        // runner — GitHub's Windows images run as administrator — `apply`
        // would really stop RpcSs and take the machine down with the test.
        for change in [
            StartupChange::Service {
                name: "RpcSs".into(),
                control: ServiceControl::Stop,
            },
            StartupChange::StartType {
                name: "Dnscache".into(),
                start_type: SettableStartType::Disabled,
            },
            StartupChange::SetEnabled {
                source: StartupSource::ScheduledTask,
                name: r"\Microsoft\Windows\UpdateOrchestrator\Schedule Scan".into(),
                enabled: false,
            },
        ] {
            assert!(
                matches!(check(&change, Consent::Unconfirmed), Err(Error::Refused(_))),
                "{change:?} ran without confirmation"
            );
            assert!(check(&change, Consent::Confirmed).is_ok());
        }
    }

    #[test]
    fn turning_a_critical_service_back_on_needs_no_confirmation() {
        let change = StartupChange::StartType {
            name: "RpcSs".into(),
            start_type: SettableStartType::Automatic,
        };
        assert!(!change.reduces());
        assert!(check(&change, Consent::Unconfirmed).is_ok());
    }

    #[test]
    fn run_once_entries_are_refused_rather_than_disabled() {
        let change = StartupChange::SetEnabled {
            source: StartupSource::UserRunOnce,
            name: "x".into(),
            enabled: false,
        };
        assert!(matches!(
            check(&change, Consent::Confirmed),
            Err(Error::Refused(_))
        ));
    }

    #[test]
    fn the_approval_blob_is_what_task_manager_reads_back() {
        let off = approval_blob(false, 0x01DA_0000_0000_0001);
        let on = approval_blob(true, 0x01DA_0000_0000_0001);
        assert_eq!(
            super::super::approved::interpret(&off),
            Some(super::super::StartupState::Disabled)
        );
        assert_eq!(
            super::super::approved::interpret(&on),
            Some(super::super::StartupState::Enabled)
        );
        assert_eq!(
            super::super::approved::toggled_at(&off),
            Some(0x01DA_0000_0000_0001)
        );
        assert_eq!(
            super::super::approved::toggled_at(&on),
            None,
            "Explorer zeroes the timestamp on enable; so must we"
        );
    }

    #[test]
    fn every_source_key_parses_back() {
        for source in [
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
        ] {
            assert_eq!(StartupSource::from_key(source.as_str()), Some(source));
        }
    }
}
