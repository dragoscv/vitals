//! On-demand inventory commands: connections, startup, services, apps.
//!
//! # Why these are `invoke` and the metrics are not
//!
//! The rule stated in [`crate::commands`] — high-frequency data is pushed,
//! never polled — is about *cadence*, not about the bridge. These four are the
//! opposite case in every respect:
//!
//! - They are **slow**. Enumerating installed applications walks four registry
//!   views; reading service configuration opens the SCM once per service. Both
//!   are tens of milliseconds to a second, which is far outside the sampler's
//!   30 ms budget and would blow it on every tick.
//! - They **rarely change**. Installed programs and startup entries change
//!   when the user installs something, not sixty times a minute. Pushing them
//!   at 1 Hz would be sending an unchanged payload over and over.
//! - They are **only wanted while their screen is open**. Nobody needs the
//!   startup inventory refreshed while looking at Processes.
//!
//! So: request/response, initiated by the screen that needs it, with a manual
//! refresh. That is the shape the data actually has.
//!
//! # Everything here is serialised for the webview, not reused from core
//!
//! The Rust types carry things JSON cannot hold — `BTreeSet<IpAddr>`, a
//! `BTreeMap` keyed by an enum, `SystemTime`. Rather than derive `Serialize`
//! onto the domain types and let their internal shape become a wire contract
//! that cannot be changed without breaking the UI, each command has an
//! explicit DTO. The conversion is the boundary, and it is visible.

use serde::Serialize;

#[cfg(windows)]
use crate::commands::CommandError;

#[cfg(windows)]
type CommandResult<T> = std::result::Result<T, CommandError>;

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

// There are two `Connection` types, and this converts between them.
//
// `vitals_win::connections::Connection` is the platform shape: a `SocketAddr`
// pair, an enum carrying the TCP peer and state together, IPv6 scope ids. It
// is `Copy` and allocation-free because it is built for tens of thousands of
// rows on a hot path.
//
// `vitals_core::provider::Connection` is the wire shape, and it is the one
// ts-rs already exports to TypeScript — so the frontend type is generated
// from it and cannot drift. Converting here rather than declaring a third
// struct is what keeps the number of contracts at two instead of three.

/// Per-process rollup, so the UI can group by application.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessConnectionsDto {
    pub pid: u32,
    pub total: usize,
    pub tcp: usize,
    pub udp: usize,
    pub active: usize,
    pub listening: usize,
    /// Count, not the set: the addresses are already in `connections`, and
    /// sending them twice would double the payload for a figure the UI only
    /// displays as a number.
    pub remote_hosts: usize,
    pub public: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsSnapshot {
    pub connections: Vec<vitals_core::provider::Connection>,
    pub by_process: Vec<ProcessConnectionsDto>,
}

/// Reads the TCP and UDP tables.
///
/// Cheap enough to call on a timer from the screen (single-digit milliseconds
/// for a few hundred rows), but still not on the sampler tick — the tables are
/// only interesting while the Network screen is open.
///
/// Off the main thread anyway: measured at 27 ms and ~320 KB on a busy
/// machine, every two seconds while the screen is open, and a synchronous
/// command blocks the window for that long each time.
#[tauri::command]
#[cfg(windows)]
pub async fn get_connections() -> CommandResult<ConnectionsSnapshot> {
    tauri::async_runtime::spawn_blocking(collect_connections)
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the connections read was abandoned: {err}"),
        })
}

#[cfg(windows)]
fn collect_connections() -> ConnectionsSnapshot {
    use vitals_win::connections;

    let (rows, by_process) = connections::snapshot();

    ConnectionsSnapshot {
        connections: rows.iter().map(to_wire).collect(),
        by_process: by_process
            .iter()
            .map(|p| ProcessConnectionsDto {
                pid: p.pid.0,
                total: p.total,
                tcp: p.tcp,
                udp: p.udp,
                active: p.active,
                listening: p.listening,
                remote_hosts: p.remote_hosts.len(),
                public: p.public,
            })
            .collect(),
    }
}

#[cfg(windows)]
fn to_wire(row: &vitals_win::connections::Connection) -> vitals_core::provider::Connection {
    use vitals_core::provider::{Connection, Protocol};
    use vitals_win::connections::Transport;

    let remote = row.remote();

    Connection {
        protocol: match row.transport {
            Transport::Tcp { .. } => Protocol::Tcp,
            Transport::Udp => Protocol::Udp,
        },
        local_address: row.local.ip().to_string(),
        local_port: row.local.port(),
        remote_address: remote.map(|addr| addr.ip().to_string()),
        remote_port: remote.map(|addr| addr.port()),
        state: to_wire_state(row.state()),
        // PID 0 is Windows' answer for kernel-owned sockets, not a missing
        // value, so it is passed through rather than mapped to `None`.
        owner_pid: Some(row.pid),
        // Resolved by the frontend against the process snapshot it already
        // holds. Looking it up here would mean an OpenProcess per row on a
        // list that can exceed a thousand entries.
        owner_name: None,
        // Both require outbound lookups, which are opt-in and not wired yet.
        remote_host: None,
        country: None,
        // Per-connection byte counters need ETW; the tables do not carry them.
        bytes_sent: None,
        bytes_received: None,
    }
}

#[cfg(windows)]
const fn to_wire_state(
    state: Option<vitals_win::connections::TcpState>,
) -> vitals_core::provider::ConnectionState {
    use vitals_core::provider::ConnectionState as Wire;
    use vitals_win::connections::TcpState as Tcp;

    match state {
        Some(Tcp::Closed) => Wire::Closed,
        Some(Tcp::Listen) => Wire::Listen,
        Some(Tcp::SynSent) => Wire::SynSent,
        Some(Tcp::SynReceived) => Wire::SynReceived,
        Some(Tcp::Established) => Wire::Established,
        Some(Tcp::FinWait1) => Wire::FinWait1,
        Some(Tcp::FinWait2) => Wire::FinWait2,
        Some(Tcp::CloseWait) => Wire::CloseWait,
        Some(Tcp::Closing) => Wire::Closing,
        Some(Tcp::LastAck) => Wire::LastAck,
        Some(Tcp::TimeWait) => Wire::TimeWait,
        Some(Tcp::DeleteTcb) => Wire::DeleteTcb,
        // Two cases, one honest answer.
        //
        // `None` is UDP, which genuinely has no connection state.
        // `Unknown(_)` is a raw value Windows returned that this build cannot
        // name. The wire enum is closed and has no "unknown" member, and
        // `Stateless` is its own word for "no meaningful TCP state here" —
        // true of both. Mapping the unknown case to `Closed` instead would be
        // a specific claim about the socket that nothing supports.
        //
        // The row is kept either way: dropping it would understate every
        // count on the screen, and an unrecognised state is still a real
        // connection holding a real port.
        None | Some(Tcp::Unknown(_)) => Wire::Stateless,
    }
}

// ---------------------------------------------------------------------------
// Startup and services
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupEntryDto {
    pub name: String,
    /// A friendlier label when the source offers one. The UI falls back to
    /// `name`, which is why this is not pre-resolved here.
    pub display_name: Option<String>,
    pub command: Option<String>,
    pub image_path: Option<String>,
    pub publisher: Option<String>,
    /// Translation keys, not display text. See `startup_source`.
    pub source: &'static str,
    pub state: &'static str,
    pub pid: Option<u32>,
    /// What this item measurably cost during the boot window, from our own
    /// sampler. `None` when nothing was measured for it — the app was not
    /// running in the first two minutes after boot, or the executable never
    /// ran while it was. Never a zero standing in for either.
    pub impact: Option<vitals_core::startup::StartupImpact>,
    /// `CompanyName` from the image's version resource — a claim the file
    /// makes, not a verified signer, which is why it is not `publisher`.
    pub company: Option<String>,
    /// Drives "Hide Microsoft entries". True only when confirmed; an entry
    /// whose company could not be read stays visible.
    pub microsoft: bool,
    /// How risky switching this off is. Translation key.
    pub risk: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDto {
    pub name: String,
    pub display_name: Option<String>,
    pub state: &'static str,
    pub start_type: &'static str,
    pub pid: Option<u32>,
    pub binary_path: Option<String>,
    /// The svchost group this service shares, when it shares one.
    ///
    /// Grouped services run in one process, so their CPU and memory cannot be
    /// attributed individually. The UI needs this to say so, rather than
    /// showing the host's whole footprint against each of a dozen services —
    /// which is what Task Manager's Details tab appears to do.
    pub svchost_group: Option<String>,
    /// The executable `binary_path` resolves to, for the shell actions.
    pub image_path: Option<String>,
    pub company: Option<String>,
    pub microsoft: bool,
    pub risk: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupSnapshot {
    pub entries: Vec<StartupEntryDto>,
    pub services: Vec<ServiceDto>,
    /// Task definitions that existed but could not be read.
    ///
    /// Surfaced so the UI can say "and 40 we could not read" rather than
    /// presenting a partial list as complete — unelevated, some definitions
    /// under `\Microsoft\Windows` are ACL'd to SYSTEM.
    pub unreadable_tasks: usize,
    /// When the boot window whose figures fill `impact` closed, in Unix
    /// milliseconds. `None` when no window has ever been measured on this
    /// machine, so the screen can say so instead of showing a column of
    /// dashes with no explanation.
    pub impact_measured_at_ms: Option<u64>,
}

/// Collects startup entries and services.
///
/// `with_service_config` opens each service to read its start type and binary
/// path, which costs an SCM round trip per service — on a machine with ~370
/// services that is the difference between a fast list and a slow one. The
/// screen asks for it only when the user opens the Services tab.
///
/// `async` + `spawn_blocking`: 230–320 ms measured, and a synchronous command
/// runs on the main thread, so it froze the window and queued every other
/// command behind it. The webview now reads this in the background at
/// launch, which is only harmless if it cannot block a click.
#[tauri::command]
#[cfg(windows)]
pub async fn get_startup(
    with_service_config: bool,
    app: tauri::AppHandle,
) -> CommandResult<StartupSnapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let impact = app.state::<crate::startup_impact::StartupImpactStore>();
        collect_startup(with_service_config, &impact)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the startup read was abandoned: {err}"),
    })?
}

#[cfg(windows)]
fn collect_startup(
    with_service_config: bool,
    impact: &crate::startup_impact::StartupImpactStore,
) -> CommandResult<StartupSnapshot> {
    use vitals_win::startup;

    let inventory = startup::collect(with_service_config)?;
    let mut companies = startup::CompanyCache::default();

    Ok(StartupSnapshot {
        entries: inventory
            .entries
            .iter()
            .map(|entry| {
                let image_path = entry
                    .image_path
                    .as_ref()
                    .map(|path| path.display().to_string());
                // Keyed on the resolved image only. The raw command carries
                // arguments, and an entry whose image could not be extracted
                // from it has no reliable executable to match — guessing one
                // would attach another program's figures to this row.
                let impact = image_path.as_deref().and_then(|key| impact.lookup(key));
                let company = image_path.as_deref().and_then(|path| companies.get(path));
                // A task under `\Microsoft\` is Windows' own even when its
                // action is `rundll32` or a COM handler with no image to read.
                let microsoft = startup::is_microsoft(company.as_deref())
                    || (entry.source == startup::StartupSource::ScheduledTask
                        && entry.name.to_ascii_lowercase().starts_with(r"\microsoft\"));
                StartupEntryDto {
                    name: entry.name.clone(),
                    display_name: entry.display_name.clone(),
                    command: entry.command.clone(),
                    image_path,
                    publisher: entry.publisher.clone(),
                    source: startup_source(entry.source),
                    state: startup_state(entry.state),
                    pid: entry.pid,
                    impact,
                    company,
                    microsoft,
                    risk: disable_risk(startup::assess_entry(entry, None)),
                }
            })
            .collect(),
        services: inventory
            .services
            .iter()
            .map(|service| {
                let image_path = service
                    .binary_path
                    .as_deref()
                    .and_then(startup::extract_image_path)
                    .map(startup::registry::expand_environment);
                let company = companies.for_service(&service.name, image_path.as_deref());
                ServiceDto {
                    name: service.name.clone(),
                    display_name: service.display_name.clone(),
                    state: service_state(service.state),
                    start_type: start_type(service.start_type),
                    pid: service.pid,
                    binary_path: service.binary_path.clone(),
                    svchost_group: service.svchost_group.clone(),
                    microsoft: startup::is_microsoft(company.as_deref()),
                    image_path,
                    company,
                    risk: disable_risk(startup::assess_disable(startup::EntryFacts {
                        source: startup::StartupSource::Service,
                        name: &service.name,
                        image_file_name: None,
                        kernel_critical: None,
                    })),
                }
            })
            .collect(),
        unreadable_tasks: inventory.unreadable_tasks,
        impact_measured_at_ms: impact.measured_at_ms(),
    })
}

#[cfg(windows)]
const fn disable_risk(risk: vitals_win::startup::DisableRisk) -> &'static str {
    use vitals_win::startup::DisableRisk as R;
    match risk {
        R::Safe => "safe",
        R::Degrades => "degrades",
        R::SystemCritical => "systemCritical",
        R::Forbidden => "forbidden",
    }
}

/// What the webview may ask a service to do.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceControlDto {
    Start,
    Stop,
    Restart,
}

/// The start types the webview may choose.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartTypeDto {
    Automatic,
    Manual,
    Disabled,
}

/// Runs one startup change off the IPC thread, raising a UAC prompt when
/// the unelevated attempt is denied.
#[cfg(windows)]
async fn run_change(
    change: vitals_win::startup::StartupChange,
    confirmed: Option<bool>,
) -> CommandResult<()> {
    let consent = if confirmed == Some(true) {
        vitals_win::actions::Consent::Confirmed
    } else {
        vitals_win::actions::Consent::Unconfirmed
    };
    tauri::async_runtime::spawn_blocking(move || {
        vitals_win::startup::apply_or_elevate(&change, consent)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the startup change was abandoned: {err}"),
    })??;
    Ok(())
}

/// Enables or disables a startup entry, the way Task Manager does.
///
/// `source` is the translation key the snapshot carried; an unknown one is
/// refused rather than guessed. Desktop-only: a phone must never be able to
/// raise a UAC prompt or change what runs on the machine it watches.
#[tauri::command]
#[cfg(windows)]
pub async fn set_startup_enabled(
    source: String,
    name: String,
    enabled: bool,
    confirmed: Option<bool>,
) -> CommandResult<()> {
    let source = parse_startup_source(&source).ok_or_else(|| CommandError::Refused {
        message: format!("{source} is not a startup source this build knows"),
    })?;
    run_change(
        vitals_win::startup::StartupChange::SetEnabled {
            source,
            name,
            enabled,
        },
        confirmed,
    )
    .await
}

/// Starts, stops or restarts a service.
#[tauri::command]
#[cfg(windows)]
pub async fn control_service(
    name: String,
    action: ServiceControlDto,
    confirmed: Option<bool>,
) -> CommandResult<()> {
    use vitals_win::startup::ServiceControl as C;
    let control = match action {
        ServiceControlDto::Start => C::Start,
        ServiceControlDto::Stop => C::Stop,
        ServiceControlDto::Restart => C::Restart,
    };
    run_change(
        vitals_win::startup::StartupChange::Service { name, control },
        confirmed,
    )
    .await
}

/// Changes when a service starts.
#[tauri::command]
#[cfg(windows)]
pub async fn set_service_start_type(
    name: String,
    start_type: StartTypeDto,
    confirmed: Option<bool>,
) -> CommandResult<()> {
    use vitals_win::startup::SettableStartType as S;
    let start_type = match start_type {
        StartTypeDto::Automatic => S::Automatic,
        StartTypeDto::Manual => S::Manual,
        StartTypeDto::Disabled => S::Disabled,
    };
    run_change(
        vitals_win::startup::StartupChange::StartType { name, start_type },
        confirmed,
    )
    .await
}

/// The inverse of [`startup_source`].
#[cfg(windows)]
fn parse_startup_source(key: &str) -> Option<vitals_win::startup::StartupSource> {
    use vitals_win::startup::StartupSource as S;
    Some(match key {
        "machineRun" => S::MachineRun,
        "machineRun32" => S::MachineRun32,
        "machineRunOnce" => S::MachineRunOnce,
        "machineRunOnce32" => S::MachineRunOnce32,
        "userRun" => S::UserRun,
        "userRunOnce" => S::UserRunOnce,
        "commonStartupFolder" => S::CommonStartupFolder,
        "userStartupFolder" => S::UserStartupFolder,
        "scheduledTask" => S::ScheduledTask,
        "service" => S::Service,
        _ => return None,
    })
}

/// A stable key the UI can translate.
///
/// `ServiceState::Unknown` carries the raw SCM value, so `format!("{:?}")`
/// would emit `Unknown(42)` — a different string for every unrecognised code,
/// which no translation table can key on and which would surface the raw
/// debug formatting to the user. The numeric value is deliberately dropped
/// here: it is diagnostic detail with no UI meaning, and preserving it would
/// trade a translatable label for an untranslatable one.
#[cfg(windows)]
const fn service_state(state: vitals_win::startup::ServiceState) -> &'static str {
    use vitals_win::startup::ServiceState as S;
    match state {
        S::Stopped => "stopped",
        S::StartPending => "startPending",
        S::StopPending => "stopPending",
        S::Running => "running",
        S::ContinuePending => "continuePending",
        S::PausePending => "pausePending",
        S::Paused => "paused",
        S::Unknown(_) => "unknown",
    }
}

/// Stable keys, not `format!("{:?}")`.
///
/// Debug output happens to be `PascalCase` today, and the UI indexes a
/// translation table with whatever arrives. Using it would couple every
/// string in the Startup screen to Rust variant names: renaming `MachineRun`
/// would silently turn a label into a raw key path in front of the user, with
/// nothing failing to compile. An explicit mapping makes that a compile error.
#[cfg(windows)]
const fn startup_source(source: vitals_win::startup::StartupSource) -> &'static str {
    use vitals_win::startup::StartupSource as S;
    match source {
        S::MachineRun => "machineRun",
        S::MachineRun32 => "machineRun32",
        S::MachineRunOnce => "machineRunOnce",
        S::MachineRunOnce32 => "machineRunOnce32",
        S::UserRun => "userRun",
        S::UserRunOnce => "userRunOnce",
        S::CommonStartupFolder => "commonStartupFolder",
        S::UserStartupFolder => "userStartupFolder",
        S::ScheduledTask => "scheduledTask",
        S::Service => "service",
    }
}

#[cfg(windows)]
const fn startup_state(state: vitals_win::startup::StartupState) -> &'static str {
    use vitals_win::startup::StartupState as S;
    match state {
        S::Enabled => "enabled",
        S::Disabled => "disabled",
        // Never collapsed into "enabled". A permission failure reading
        // `StartupApproved` rendered as "this will run" is a fabricated fact,
        // and it is the one the user would act on.
        S::Unknown => "unknown",
    }
}

#[cfg(windows)]
const fn start_type(start: vitals_win::startup::StartType) -> &'static str {
    use vitals_win::startup::StartType as S;
    match start {
        S::Boot => "boot",
        S::System => "system",
        S::Automatic => "automatic",
        S::Manual => "manual",
        S::Disabled => "disabled",
        // Never reported as Manual, which would understate how much runs at
        // boot on an unelevated machine.
        S::Unknown => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Installed applications
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAppDto {
    /// Registry subkey name. Stable across runs, so it works as a UI key.
    pub key_name: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub install_date: Option<String>,
    pub install_location: Option<String>,
    /// Disk footprint as the installer reported it.
    ///
    /// Advisory only — it is whatever the installer chose to write, is often
    /// absent, and Windows never recomputes it. The UI must not present it as
    /// a measurement.
    pub estimated_size: Option<u64>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    /// Managed by Windows Installer. Worth surfacing because removal differs:
    /// an MSI product can be removed silently via `msiexec /x` even with no
    /// published quiet string, whereas a bespoke uninstaller generally cannot.
    pub is_msi: bool,
    pub per_user: bool,
    /// Translation key, not display text. See `app_source`.
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsSnapshot {
    pub apps: Vec<InstalledAppDto>,
    /// Registry subkeys examined, across all four views.
    pub examined: usize,
    /// Entries the "is this a real application?" filter discarded.
    ///
    /// Reported because any filter aggressive enough to be useful can also
    /// hide something the user expected, and without this figure a broken scan
    /// and a well-filtered one look identical — both produce a short list.
    pub rejected: usize,
    /// The same total, broken down by reason.
    ///
    /// The breakdown is what makes the number actionable rather than merely
    /// present: "900 system components" is Windows working as designed, while
    /// "900 with no display name" would mean the scan itself is broken. The
    /// aggregate alone cannot tell those apart.
    pub rejected_by_reason: Vec<RejectionDto>,
    pub duplicates_collapsed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectionDto {
    /// Translation key. See `reject_reason`.
    pub reason: &'static str,
    pub count: usize,
}

/// Enumerates installed applications.
///
/// Never fails as a whole: an absent registry view — `WOW6432Node` does not
/// exist on a 32-bit-only system — is skipped rather than blanking the list.
///
/// Off the main thread for the reason given on [`get_startup`]: ~220 ms of
/// registry walking.
#[tauri::command]
#[cfg(windows)]
pub async fn get_installed_apps() -> CommandResult<AppsSnapshot> {
    tauri::async_runtime::spawn_blocking(collect_installed_apps)
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the installed-apps read was abandoned: {err}"),
        })
}

#[cfg(windows)]
fn collect_installed_apps() -> AppsSnapshot {
    use vitals_win::apps;

    let scan = apps::enumerate_installed_apps();

    AppsSnapshot {
        apps: scan.apps.iter().map(installed_app_dto).collect(),
        examined: scan.examined,
        rejected: scan.rejected_total(),
        rejected_by_reason: scan
            .rejected
            .iter()
            .map(|(reason, count)| RejectionDto {
                reason: reject_reason(*reason),
                count: *count,
            })
            .collect(),
        duplicates_collapsed: scan.duplicates_collapsed,
    }
}

#[cfg(windows)]
const fn reject_reason(reason: vitals_win::apps::RejectReason) -> &'static str {
    use vitals_win::apps::RejectReason as R;
    match reason {
        R::NoDisplayName => "noDisplayName",
        R::SystemComponent => "systemComponent",
        R::UpdateOrHotfix => "updateOrHotfix",
        R::ChildOfAnotherEntry => "childOfAnotherEntry",
        R::OrphanPatch => "orphanPatch",
    }
}

/// Launches an application's own uninstaller.
///
/// # What this deliberately does not do
///
/// It does not delete files, remove registry keys, or "clean up" anything.
/// Vitals launches the command the vendor published in `UninstallString` and
/// stops there. Writing a bespoke uninstaller means guessing which files
/// belong to a product, and a wrong guess is unrecoverable data loss in a
/// tool the user opened to make their computer *better*.
///
/// # Interactive, not silent
///
/// Even where a `QuietUninstallString` exists, the interactive command is
/// preferred. Silent removal of an application the user picked from a list is
/// the wrong default: the vendor's own dialog is the last checkpoint before
/// an irreversible action, it is where "also delete my settings?" is asked,
/// and skipping it turns a misclick into a permanent loss.
///
/// The command is executed through `cmd /c`, because `UninstallString` is a
/// raw command line — `"C:\App\unins.exe" /uninstall` — with quoting and
/// arguments that only a shell parses correctly. Splitting it by hand breaks
/// on the paths most likely to contain spaces.
///
/// # Identity, not a command line
///
/// The webview sends the entry's `key_name` and registry view; the command
/// is re-read from the registry here. It used to take the command itself,
/// which made this the one mutating command that would run any string the
/// webview handed it.
///
/// `raw_arg`, not `args`: `args` applies C-runtime quoting, which turns a
/// leading `"C:\Program Files\X\unins.exe" /S` into `\"C:\…` — a path `cmd`
/// cannot run, so every Inno/NSIS uninstaller with a quoted path failed.
/// `cmd /s /c "<line>"` strips exactly the outer pair and runs the line
/// as the vendor wrote it.
#[tauri::command]
#[cfg(windows)]
// Tauri deserialises command arguments into owned values; it cannot hand us a
// borrow.
#[allow(clippy::needless_pass_by_value)]
pub fn uninstall_app(key_name: String, source: String) -> CommandResult<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    // CREATE_NO_WINDOW: the shell itself must not flash a console. The
    // uninstaller's own UI still appears, which is the point.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Some(source) = parse_app_source(&source) else {
        return Err(CommandError::NotFound {
            message: format!("unknown installation source {source:?}"),
        });
    };
    let command = vitals_win::apps::uninstall_command(&key_name, source)?;

    // The shell by absolute path: a bare "cmd" is resolved through the
    // current directory first, which is a planting risk for anything that
    // spawns processes.
    let system = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let cmd = std::path::Path::new(&system)
        .join("System32")
        .join("cmd.exe");

    Command::new(cmd)
        .raw_arg(format!("/d /s /c \"{command}\""))
        .creation_flags(CREATE_NO_WINDOW)
        // Spawned, never waited on. An uninstaller is interactive and can sit
        // on a confirmation dialog for minutes; blocking the command here
        // would freeze the webview's IPC for exactly that long.
        .spawn()
        .map_err(|err| CommandError::Internal {
            message: format!("could not start the uninstaller: {err}"),
        })?;

    Ok(())
}

/// The inverse of [`app_source`]: the wire name back to the registry view.
#[cfg(windows)]
fn parse_app_source(value: &str) -> Option<vitals_win::apps::AppSource> {
    use vitals_win::apps::AppSource as S;
    Some(match value {
        "machineNative" => S::MachineNative,
        "machineWow64" => S::MachineWow64,
        "userNative" => S::UserNative,
        "userWow64" => S::UserWow64,
        _ => return None,
    })
}

#[cfg(windows)]
fn installed_app_dto(app: &vitals_win::apps::InstalledApp) -> InstalledAppDto {
    InstalledAppDto {
        key_name: app.key_name.clone(),
        name: app.display_name.clone(),
        publisher: app.publisher.clone(),
        version: app.version.clone(),
        // ISO, not the raw `YYYYMMDD` the registry holds: the UI formats dates
        // per locale, and a bare eight-digit string is not parseable by
        // `Intl.DateTimeFormat` without the frontend re-deriving the layout.
        install_date: app.install_date.map(vitals_win::apps::InstallDate::to_iso),
        install_location: app.install_location.clone(),
        estimated_size: app.estimated_size.map(|bytes| bytes.0),
        uninstall_string: app.uninstall_string.clone(),
        quiet_uninstall_string: app.quiet_uninstall_string.clone(),
        is_msi: app.is_msi,
        per_user: app.per_user,
        source: app_source(app.source),
    }
}

/// Stable key, for the same reason as `startup_source`: Debug output is
/// variant names, and indexing a translation table with those makes a Rust
/// rename silently print a key path to the user.
#[cfg(windows)]
const fn app_source(source: vitals_win::apps::AppSource) -> &'static str {
    use vitals_win::apps::AppSource as S;
    match source {
        S::MachineNative => "machineNative",
        S::MachineWow64 => "machineWow64",
        S::UserNative => "userNative",
        S::UserWow64 => "userWow64",
    }
}

// ---------------------------------------------------------------------------
// Devices and sensors
// ---------------------------------------------------------------------------

/// One measured reading, already converted to the unit the UI renders.
///
/// `unit` is a discriminator the frontend switches on to pick a formatter, not
/// a suffix to concatenate: "42 °C" formatted in Romanian is "42 °C" but the
/// decimal separator differs, and building the string here would bypass
/// `Intl` entirely.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorReadingDto {
    /// Stable identity for charting across samples.
    pub key: String,
    /// Human label from the backend. English; a localisable label would need
    /// a key table the sensor set does not have a fixed shape for.
    pub label: String,
    pub value: f32,
    /// Translation key: `temperature`, `power`, `voltage`, `fanSpeed`, `charge`.
    pub unit: &'static str,
    /// Translation key. See `sensor_source`.
    pub source: &'static str,
    /// Translation key: `measured`, `derived`, `nameplate`.
    pub quality: &'static str,
}

/// A capability this build cannot satisfy, and precisely why.
///
/// The most important payload on the Devices screen. An absent temperature
/// with no explanation reads as a broken application; the same absence with
/// "needs a signed kernel driver we do not ship" reads as a considered
/// decision, which is what it is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverGapDto {
    /// Translation key. See `capability_key`.
    pub capability: &'static str,
    /// Translation key. See `unavailable_key`.
    pub reason: &'static str,
    pub label: &'static str,
    /// The concrete mechanism required — an API, register or SDK by name.
    ///
    /// English, and deliberately not translated: it names `RDMSR`,
    /// `IA32_THERM_STATUS` and `NvAPI_GPU_GetThermalSettings`, which are
    /// identifiers rather than prose. A translated register name would be
    /// worse than an untranslated one.
    pub requirement: &'static str,
    /// Whether the user can act on it now (elevate, install a plugin).
    pub actionable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerStateDto {
    /// `ac`, `battery` or `unknown` — never defaulted to `ac`.
    pub line: &'static str,
    /// `bestPowerEfficiency`, `balanced`, `bestPerformance`, `custom`.
    pub mode: &'static str,
    /// Active scheme GUID in canonical printed form, when one was read.
    pub scheme_guid: Option<String>,
    pub has_battery: bool,
    pub power_saver: bool,
    /// Whole-percent charge as the OS reports it.
    pub battery_percent: Option<f32>,
    /// `None` while the OS estimator settles — never rendered as 0 minutes.
    pub seconds_remaining: Option<u32>,
}

/// The OS's summed view across every pack. Absent on a machine with none.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateBatteryDto {
    pub on_ac: bool,
    pub charging: bool,
    pub discharging: bool,
    pub max_capacity: Option<u32>,
    pub remaining_capacity: Option<u32>,
    pub estimated_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryDto {
    pub device_path: String,
    pub chemistry: String,
    pub charge: Option<f32>,
    pub health: Option<f32>,
    pub rate_watts: Option<f32>,
    pub voltage: Option<f32>,
    /// `charging`, `discharging`, `idle`, `unknown`.
    pub state: &'static str,
    pub design_capacity_mwh: Option<u32>,
    pub full_charge_capacity_mwh: Option<u32>,
    pub cycle_count: Option<u32>,
    pub seconds_to_empty: Option<u32>,
    /// When set the mWh figures are unitless gauge counts, not energy, and
    /// must not be rendered with a Wh suffix.
    pub capacity_is_relative: bool,
    /// A UPS, whose runtime means minutes to shutdown rather than hours of use.
    pub is_short_term: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalZoneDto {
    pub instance: String,
    pub celsius: f32,
    pub critical_celsius: Option<f32>,
    pub active_cooling: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorsSnapshot {
    pub power: PowerStateDto,
    pub aggregate_battery: Option<AggregateBatteryDto>,
    pub batteries: Vec<BatteryDto>,
    pub zones: Vec<ThermalZoneDto>,
    /// `available`, `accessDenied`, `noZonesPresent`, `providerMissing`.
    ///
    /// Three of those four produce an empty zone list, and only one is fixed
    /// by elevating. Collapsing them would send the user through a UAC prompt
    /// that changes nothing, or tell a non-admin their board has no sensors.
    pub thermal_availability: &'static str,
    pub readings: Vec<SensorReadingDto>,
    pub gaps: Vec<DriverGapDto>,
    /// How long the backend says to wait before asking again, milliseconds.
    ///
    /// Sent rather than hardcoded in the UI so the cadence stays a single
    /// decision: a WMI round trip costs tens of milliseconds against a 30 ms
    /// frame budget, and a screen polling at 1 Hz would consume it.
    pub cadence_ms: u64,
    /// Wall-clock cost of producing this sample, milliseconds.
    pub elapsed_ms: f64,
}

/// Reads every sensor source once.
///
/// Uncached on purpose: [`vitals_win::sensors::SensorReader`]'s TTL exists to
/// protect the 1 Hz sampler, and this command is only ever called by a screen
/// that already respects `cadence_ms`. Adding a second cache here would make
/// the manual refresh button do nothing for up to five seconds, which reads
/// as a broken button.
///
/// Off the main thread: a WMI round trip is tens of milliseconds (48 ms
/// measured). `read_all` enters and leaves its COM apartment per call, so a
/// pool thread is as good as any.
#[tauri::command]
#[cfg(windows)]
pub async fn get_sensors() -> CommandResult<SensorsSnapshot> {
    tauri::async_runtime::spawn_blocking(read_sensors)
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the sensor read was abandoned: {err}"),
        })
}

#[cfg(windows)]
#[must_use]
fn read_sensors() -> SensorsSnapshot {
    use vitals_win::sensors;

    let sample = sensors::read_all();
    let hint = sensors::SensorReader::new().sample_interval_hint();

    SensorsSnapshot {
        power: power_dto(&sample.power),
        aggregate_battery: sensors::aggregate_battery().map(|agg| AggregateBatteryDto {
            on_ac: agg.on_ac,
            charging: agg.charging,
            discharging: agg.discharging,
            max_capacity: agg.max_capacity,
            remaining_capacity: agg.remaining_capacity,
            estimated_seconds: agg.estimated_seconds,
        }),
        batteries: sample.batteries.iter().map(battery_dto).collect(),
        zones: sample
            .thermal
            .zones
            .iter()
            .map(|zone| ThermalZoneDto {
                instance: zone.instance.clone(),
                celsius: zone.temperature.0,
                critical_celsius: zone.critical.map(|c| c.0),
                active_cooling: zone.active_cooling,
            })
            .collect(),
        thermal_availability: thermal_availability(sample.thermal.availability),
        readings: sample
            .readings
            .iter()
            .map(|reading| SensorReadingDto {
                key: reading.key.clone(),
                label: reading.label.clone(),
                value: reading.value.magnitude(),
                unit: sensor_unit(reading.value),
                source: sensor_source(reading.source),
                quality: sensor_quality(reading.quality),
            })
            .collect(),
        // A gap is listed only while it is one: on an NVIDIA machine the
        // driver's own library now supplies GPU temperature and board power,
        // and listing them as "cannot measure" beside the readings would
        // contradict the table above it.
        gaps: sensors::DRIVER_GAPS
            .iter()
            .filter(|gap| !closed_by(gap, &sample))
            .map(|gap| DriverGapDto {
                capability: capability_key(gap.capability),
                reason: unavailable_key(gap.reason),
                label: gap.label,
                requirement: gap.requirement,
                actionable: gap.reason.is_actionable(),
            })
            .collect(),
        cadence_ms: u64::try_from(hint.as_millis()).unwrap_or(u64::MAX),
        elapsed_ms: sample.elapsed.as_secs_f64() * 1000.0,
    }
}

#[cfg(windows)]
fn closed_by(
    gap: &vitals_win::sensors::DriverGap,
    sample: &vitals_win::sensors::SensorSample,
) -> bool {
    let any = |f: fn(&vitals_win::sensors::NvidiaGpu) -> bool| sample.nvidia.iter().any(f);
    let cpu = sample.cpu.as_ref();
    match gap.label {
        "GPU temperature" => any(|g| g.temperature_celsius.is_some()),
        "GPU board power" => any(|g| g.power_watts.is_some()),
        "CPU core temperature" => cpu
            .and_then(vitals_win::sensors::cpu_service::CpuSensors::temperature)
            .is_some(),
        "CPU package power" => cpu.and_then(|c| c.package_watts).is_some(),
        "Fan speed (RPM)" => cpu.is_some_and(|c| !c.fans.is_empty()),
        "Drive temperature" => sample.drives_measured,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// CPU sensors service (crate vitals-sensors, ADR-0034)
// ---------------------------------------------------------------------------

/// Whether the optional CPU sensors service is there, and what installing it
/// would involve. Drives the Devices screen's install / remove affordance.
// Four independent facts, each rendered separately — PawnIO can be present
// without the service, the service installed but not reading. An enum would
// have to enumerate their product and the UI would unpack it again.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorsServiceDto {
    /// The service answered on its pipe, even if only with an error.
    pub installed: bool,
    /// It answered with a reading.
    pub running: bool,
    /// Why there is no reading: the service's own error, or the pipe's.
    pub error: Option<String>,
    /// The `PawnIO` driver is present; if not, installing downloads it.
    pub pawnio_installed: bool,
    /// This build ships the helper. A dev build without a prior
    /// `cargo build -p vitals-sensors` has none, and the button must say so
    /// rather than fail after a UAC prompt.
    pub helper_available: bool,
}

/// Where the helper can be: the installed app's resources, or the cargo
/// target directory beside a dev build of the app.
#[cfg(windows)]
fn sensors_helper(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;

    let mut dirs = Vec::new();
    // A dev build looks in the source tree FIRST, where
    // `scripts/bundle-sensors.ps1` stages the helper. Its `resource_dir` is
    // `target/debug`, holding the copy tauri-build made on the first build
    // and never refreshes: it came first here once, and a reinstall put a
    // two-hour-old helper without fan support back as the SYSTEM service.
    if cfg!(debug_assertions) {
        dirs.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sensors"));
    }
    if let Ok(resources) = app.path().resource_dir() {
        dirs.push(resources.join("sensors"));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(std::path::Path::to_path_buf))
    {
        dirs.push(exe_dir);
    }
    vitals_win::sensors::cpu_service::find_helper(&dirs)
}

#[tauri::command]
#[cfg(windows)]
pub async fn get_sensors_service(app: tauri::AppHandle) -> CommandResult<SensorsServiceDto> {
    let helper_available = sensors_helper(&app).is_some();
    // Off the IPC thread: the status read waits up to 300 ms for a busy pipe.
    tauri::async_runtime::spawn_blocking(move || {
        let status = vitals_win::sensors::cpu_service::status();
        SensorsServiceDto {
            installed: status.installed,
            running: status.running,
            error: status.error,
            pawnio_installed: status.pawnio_installed,
            helper_available,
        }
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the sensors-service check was abandoned: {err}"),
    })
}

/// Installs (`install: true`) or removes the service, behind one UAC prompt.
///
/// Desktop-only on purpose: installing a SYSTEM service is exactly the kind
/// of act a paired phone must not be able to trigger, so there is no LAN
/// equivalent in `ControlRequest`.
#[tauri::command]
#[cfg(windows)]
pub async fn setup_sensors_service(app: tauri::AppHandle, install: bool) -> CommandResult<()> {
    let helper = sensors_helper(&app).ok_or_else(|| CommandError::Unsupported {
        message: "this build does not include the sensors service helper".to_owned(),
    })?;
    let code = tauri::async_runtime::spawn_blocking(move || {
        vitals_win::sensors::cpu_service::setup(&helper, install)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the sensors-service setup was abandoned: {err}"),
    })??;
    setup_outcome(code)
}

/// Maps the helper's exit code to a result the UI can state.
#[cfg(windows)]
fn setup_outcome(code: i32) -> CommandResult<()> {
    use vitals_sensors::exit;
    match code {
        exit::OK => Ok(()),
        exit::NO_PAWNIO => Err(CommandError::Internal {
            message: "the PawnIO driver could not be installed, so the service was not started"
                .to_owned(),
        }),
        exit::NOT_ELEVATED => Err(CommandError::AccessDenied {
            message: "the helper did not receive administrator rights".to_owned(),
        }),
        other => Err(CommandError::Internal {
            message: format!(
                "the sensors helper failed (exit {other}); details are in \
                 %ProgramData%\\Vitals Sensors\\install.log"
            ),
        }),
    }
}

#[cfg(windows)]
fn power_dto(state: &vitals_win::sensors::PowerState) -> PowerStateDto {
    use vitals_win::sensors::{LineStatus, PowerMode};

    PowerStateDto {
        line: match state.line {
            LineStatus::Ac => "ac",
            LineStatus::Battery => "battery",
            // A VM genuinely declines to answer. Reporting `ac` would be a
            // fabricated fact about the machine's power source.
            LineStatus::Unknown => "unknown",
        },
        mode: match state.mode {
            PowerMode::BestPowerEfficiency => "bestPowerEfficiency",
            PowerMode::Balanced => "balanced",
            PowerMode::BestPerformance => "bestPerformance",
            PowerMode::Custom => "custom",
        },
        scheme_guid: state.scheme_guid.map(format_guid),
        has_battery: state.has_battery,
        power_saver: state.power_saver,
        battery_percent: state.battery_percent.map(vitals_core::units::Percent::get),
        seconds_remaining: state.seconds_remaining,
    }
}

#[cfg(windows)]
fn battery_dto(pack: &vitals_win::sensors::Battery) -> BatteryDto {
    use vitals_win::sensors::ChargeState;

    BatteryDto {
        device_path: pack.device_path.clone(),
        chemistry: pack.chemistry.clone(),
        charge: pack.charge.map(vitals_core::units::Percent::get),
        health: pack.health.map(vitals_core::units::Percent::get),
        rate_watts: pack.rate.map(|w| w.0),
        voltage: pack.voltage.map(|v| v.0),
        state: match pack.state {
            ChargeState::Charging => "charging",
            ChargeState::Discharging => "discharging",
            ChargeState::Idle => "idle",
            ChargeState::Unknown => "unknown",
        },
        design_capacity_mwh: pack.design_capacity_mwh,
        full_charge_capacity_mwh: pack.full_charge_capacity_mwh,
        cycle_count: pack.cycle_count,
        seconds_to_empty: pack.seconds_to_empty,
        capacity_is_relative: pack.capacity_is_relative,
        is_short_term: pack.is_short_term,
    }
}

/// Stable keys, never `format!("{:?}")` — the same reason as `startup_source`.
#[cfg(windows)]
const fn thermal_availability(
    availability: vitals_win::sensors::ThermalAvailability,
) -> &'static str {
    use vitals_win::sensors::ThermalAvailability as A;
    match availability {
        A::Available => "available",
        A::AccessDenied => "accessDenied",
        A::NoZonesPresent => "noZonesPresent",
        A::ProviderMissing => "providerMissing",
    }
}

#[cfg(windows)]
const fn sensor_unit(value: vitals_win::sensors::SensorValue) -> &'static str {
    use vitals_win::sensors::SensorValue as V;
    match value {
        V::Temperature(_) => "temperature",
        V::Power(_) => "power",
        V::Voltage(_) => "voltage",
        V::FanSpeed(_) => "fanSpeed",
        V::Charge(_) => "charge",
        V::Percent(_) => "percent",
    }
}

#[cfg(windows)]
const fn sensor_source(source: vitals_win::sensors::SensorSource) -> &'static str {
    use vitals_win::sensors::SensorSource as S;
    match source {
        S::AcpiThermalZone => "acpiThermalZone",
        S::BatteryMiniport => "batteryMiniport",
        S::SystemPowerStatus => "systemPowerStatus",
        S::VendorLibrary => "vendorLibrary",
        S::KernelDriver => "kernelDriver",
        S::StorageDevice => "storageDevice",
    }
}

#[cfg(windows)]
const fn sensor_quality(quality: vitals_win::sensors::Quality) -> &'static str {
    use vitals_win::sensors::Quality as Q;
    match quality {
        Q::Measured => "measured",
        Q::Derived => "derived",
        Q::Nameplate => "nameplate",
    }
}

/// Every sensor reading, for `/api/v1/sensors`.
///
/// Cached for [`LAN_SENSOR_TTL`]: one read is a WMI round trip plus the
/// sensors pipe (48 ms measured), and a watch or a phone polling on its own
/// schedule must not turn that into a steady cost on the sampled machine.
/// Two clients polling at once share one read.
#[cfg(windows)]
#[must_use]
pub fn lan_sensor_lines() -> Vec<vitals_core::remote::SensorLine> {
    use std::sync::Mutex;
    use std::time::Instant;

    static CACHE: Mutex<Option<(Instant, Vec<vitals_core::remote::SensorLine>)>> = Mutex::new(None);

    let mut guard = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, lines)) = guard.as_ref()
        && at.elapsed() < LAN_SENSOR_TTL
    {
        return lines.clone();
    }
    let lines: Vec<_> = vitals_win::sensors::read_all()
        .readings
        .iter()
        .map(vitals_win::sensors::SensorReading::to_line)
        .collect();
    *guard = Some((Instant::now(), lines.clone()));
    lines
}

#[cfg(not(windows))]
#[must_use]
pub fn lan_sensor_lines() -> Vec<vitals_core::remote::SensorLine> {
    Vec::new()
}

/// How long a LAN sensor read is reused.
#[cfg(windows)]
const LAN_SENSOR_TTL: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(windows)]
const fn capability_key(capability: vitals_core::capability::Capability) -> &'static str {
    use vitals_core::capability::Capability as C;
    match capability {
        C::Thermals => "thermals",
        C::PowerDraw => "powerDraw",
        C::FanControl => "fanControl",
        // The gap list only names the three above today, but `Capability` is
        // `#[non_exhaustive]` and a new gap must not print a raw key path.
        // "other" is a real translation, not a placeholder.
        _ => "other",
    }
}

#[cfg(windows)]
const fn unavailable_key(reason: vitals_core::capability::Unavailable) -> &'static str {
    use vitals_core::capability::Unavailable as U;
    match reason {
        U::NotSupportedOnPlatform => "notSupportedOnPlatform",
        U::NoSuchHardware => "noSuchHardware",
        U::NeedsElevation => "needsElevation",
        U::NeedsHelper => "needsHelper",
        U::NeedsPlugin => "needsPlugin",
        U::DisabledByUser => "disabledByUser",
        U::NotImplemented => "notImplemented",
    }
}

/// Formats a GUID in the canonical printed order.
///
/// `Data1`..`Data3` are little-endian in memory and `Data4` is not, so this
/// cannot be a straight hex dump of the bytes. Getting it wrong produces a
/// plausible-looking GUID that matches nothing, which is how every power
/// scheme once classified as `Custom`.
#[cfg(windows)]
fn format_guid(g: [u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut tail = String::with_capacity(12);
    for byte in &g[10..] {
        let _ = write!(tail, "{byte:02x}");
    }

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{tail}",
        g[3], g[2], g[1], g[0], g[5], g[4], g[7], g[6], g[8], g[9],
    )
}

#[cfg(all(test, windows))]
mod sensor_tests {
    use super::*;

    #[test]
    fn guid_bytes_are_printed_in_canonical_order() {
        // GUID_MIN_POWER_SAVINGS in memory order. If this ever prints as a
        // straight hex dump the byte-order bug is back.
        let bytes = [
            0xDA, 0x7F, 0x5E, 0x8C, 0xBF, 0xE8, 0x96, 0x4A, 0x9A, 0x85, 0xA6, 0xE2, 0x3A, 0x8C,
            0x63, 0x5C,
        ];
        assert_eq!(format_guid(bytes), "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c");
    }

    #[test]
    fn an_empty_thermal_scan_always_carries_a_reason() {
        let snapshot = read_sensors();

        if snapshot.zones.is_empty() {
            assert_ne!(
                snapshot.thermal_availability, "available",
                "no zones yet availability claims success — the UI would render \
                     an empty table with nothing to explain it"
            );
        }
    }

    #[test]
    fn every_gap_is_sent_with_a_reason_and_a_requirement() {
        let snapshot = read_sensors();

        assert!(!snapshot.gaps.is_empty(), "the gap list is the screen");
        for gap in &snapshot.gaps {
            assert!(!gap.reason.is_empty());
            assert!(!gap.requirement.is_empty());
            assert_ne!(gap.capability, "other", "a gap must name its capability");
        }
    }

    #[test]
    fn the_cadence_is_far_slower_than_the_frame_budget() {
        // Polling sensors at the sampler's rate would spend the whole 30 ms
        // budget in WMI. The screen honours this value.
        assert!(read_sensors().cadence_ms >= 1000);
    }

    #[test]
    fn no_reading_is_fabricated() {
        for reading in &read_sensors().readings {
            assert!(!reading.key.is_empty());
            assert!(reading.value.is_finite());
        }
    }
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// One storage operation's cancel token and in-flight flag.
///
/// The scan and the cleanup search each have their own. They used to share a
/// single flag, so "Stop the scan" also stopped a cleanup search, and
/// starting a cleanup search cleared a stop the user had just asked for.
#[cfg(windows)]
struct Operation {
    cancel: std::sync::atomic::AtomicBool,
    busy: std::sync::atomic::AtomicBool,
}

#[cfg(windows)]
impl Operation {
    const fn new() -> Self {
        Self {
            cancel: std::sync::atomic::AtomicBool::new(false),
            busy: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Claims the operation, or refuses if one is already running.
    ///
    /// A second concurrent walk of the same volume would halve the speed of
    /// both, and the UI that asked for it would be showing the wrong one.
    fn begin(&'static self, what: &str) -> CommandResult<OperationGuard> {
        use std::sync::atomic::Ordering;
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(CommandError::Refused {
                message: format!("{what} is already running"),
            });
        }
        // Cleared only once the claim is ours, so a stop meant for a running
        // operation is never erased by a refused second request.
        self.cancel.store(false, Ordering::Release);
        Ok(OperationGuard(self))
    }
}

/// Releases the operation when the command ends, however it ends.
#[cfg(windows)]
struct OperationGuard(&'static Operation);

#[cfg(windows)]
impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.0
            .busy
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(windows)]
static SCAN: Operation = Operation::new();
#[cfg(windows)]
static CLEANUP: Operation = Operation::new();

/// The last scan's tree, kept so it can be navigated without scanning again.
///
/// A full `C:` is about 230 MB of tree, so it is not kept forever: it goes
/// when the next scan starts (before that scan allocates its own, so two
/// never coexist) and after [`KEEP_IDLE`] without a navigation request. The
/// user decided this trade on 2026-09-29: instant navigation while in use,
/// normal memory once the app sits in the tray.
#[cfg(windows)]
struct StoredScan {
    id: u64,
    result: vitals_win::storage::ScanResult,
    last_used: std::time::Instant,
}

#[cfg(windows)]
static STORED: std::sync::Mutex<Option<StoredScan>> = std::sync::Mutex::new(None);
#[cfg(windows)]
static NEXT_SCAN_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
#[cfg(windows)]
const KEEP_IDLE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

#[cfg(windows)]
fn stored() -> std::sync::MutexGuard<'static, Option<StoredScan>> {
    STORED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Runs `f` against the stored scan if `scan_id` is still the one kept.
#[cfg(windows)]
fn with_scan<T>(
    scan_id: u64,
    f: impl FnOnce(&vitals_win::storage::ScanResult) -> CommandResult<T>,
) -> CommandResult<T> {
    let mut guard = stored();
    match guard.as_mut() {
        Some(scan) if scan.id == scan_id => {
            scan.last_used = std::time::Instant::now();
            f(&scan.result)
        }
        // A node id means nothing in another tree, so a stale request is
        // refused rather than answered from whichever scan is current.
        _ => Err(CommandError::NotFound {
            message: "this scan is no longer in memory; scan again to explore it".into(),
        }),
    }
}

/// Keeps `result` and starts the idle timer that will release it.
#[cfg(windows)]
fn keep_scan(result: vitals_win::storage::ScanResult) -> u64 {
    let id = NEXT_SCAN_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    *stored() = Some(StoredScan {
        id,
        result,
        last_used: std::time::Instant::now(),
    });
    // One janitor per kept scan; it exits as soon as its scan is replaced.
    // A minute's resolution is plenty for a fifteen-minute deadline.
    let spawned = std::thread::Builder::new()
        .name("vitals-storage-release".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
                let mut guard = stored();
                match guard.as_ref() {
                    Some(scan) if scan.id == id => {
                        if scan.last_used.elapsed() >= KEEP_IDLE {
                            *guard = None;
                            return;
                        }
                    }
                    _ => return,
                }
            }
        });
    if let Err(err) = spawned {
        // Without the timer the tree stays until the next scan: more memory,
        // never a wrong answer.
        tracing::warn!(%err, "storage release timer did not start");
    }
    id
}

/// Event carrying [`ScanProgressDto`] while a scan runs.
#[cfg(windows)]
pub const SCAN_PROGRESS_EVENT: &str = "vitals://storage/scan-progress";

/// What a running scan has seen so far.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressDto {
    pub root: String,
    pub files_seen: u64,
    pub directories_seen: u64,
    pub bytes_seen: u64,
    pub elapsed_ms: u64,
    /// The folder most recently read, for "now reading …".
    pub current_path: String,
    /// `approval` | `reading` | `building` | `journal` | `walking`.
    pub phase: &'static str,
    /// How far through, 0 to 1, when that is known.
    pub fraction: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDto {
    /// Mount point, e.g. `C:\`. Also the scan root and the React key.
    pub mount: String,
    pub label: Option<String>,
    pub file_system: Option<String>,
    /// Translation key, not display text. See `disk_kind`.
    pub kind: &'static str,
    pub total: u64,
    pub available: u64,
    /// Which scanner applies to this drive letter right now.
    ///
    /// Reported so the UI can tell the user a whole-volume scan will take
    /// minutes before they start one, instead of after.
    pub strategy: &'static str,
    /// A saved index exists, so a rescan re-reads only what changed.
    pub indexed: bool,
}

/// Lists mounted volumes with their capacity.
///
/// Never fails as a whole: a drive that refuses `GetDiskFreeSpaceEx` — an
/// empty card reader, a disconnected network mapping — is omitted by the
/// enumerator rather than blanking the picker.
#[tauri::command]
#[cfg(windows)]
#[must_use]
pub fn get_volumes() -> Vec<VolumeDto> {
    let index_dir = storage_index_dir();
    vitals_win::disk::enumerate_volumes()
        .into_iter()
        .map(|volume| {
            VolumeDto {
                strategy: match vitals_win::storage::strategy_for(volume.file_system.as_deref()) {
                    vitals_win::storage::ScanStrategy::Turbo => "turbo",
                    vitals_win::storage::ScanStrategy::DirectoryWalk => "directoryWalk",
                },
                indexed: vitals_win::storage::index::exists(&index_dir, &volume.mount),
                kind: disk_kind(volume.kind),
                mount: volume.mount,
                label: volume.label,
                file_system: volume.file_system,
                total: volume.total.0,
                available: volume.available.0,
            }
        })
        .collect()
}

/// Stable key, never `{:?}`. A Rust rename must not silently turn a label
/// into a visible key path.
#[cfg(windows)]
const fn disk_kind(kind: vitals_core::metrics::DiskKind) -> &'static str {
    use vitals_core::metrics::DiskKind as K;
    match kind {
        K::Hdd => "hdd",
        K::Ssd => "ssd",
        K::Nvme => "nvme",
        K::Removable => "removable",
        K::Network => "network",
        K::Optical => "optical",
        K::Unknown => "unknown",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntryDto {
    pub path: String,
    /// What the directory occupies on the volume, cluster-rounded. The
    /// primary figure: it is the space that would actually be freed.
    pub allocated: u64,
    /// Sum of file lengths. Carried alongside because it is what Explorer's
    /// "Size:" line reports, and a user comparing the two deserves to see
    /// both rather than conclude one of them is broken.
    pub logical: u64,
    pub files: u64,
    /// Set when this directory's contents could not be read. The row must be
    /// rendered as incomplete rather than as a confident figure.
    pub incomplete: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedPathDto {
    pub path: String,
    /// Translation key. See `skip_reason`.
    pub reason: &'static str,
    /// Whether running elevated would plausibly let this directory be read.
    pub elevation_fixable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSnapshot {
    pub root: String,
    pub largest: Vec<DirectoryEntryDto>,
    pub allocated: u64,
    pub logical: u64,
    /// `None` disables cluster rounding, which under-reports. Surfaced so the
    /// UI can say so rather than presenting an unrounded total as exact.
    pub cluster_bytes: Option<u64>,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub hard_link_duplicates: u64,
    pub hard_link_bytes_saved: u64,
    /// True when the user cancelled. Every figure is then a lower bound.
    pub cancelled: bool,
    /// False when cancelled or when any directory was skipped.
    pub complete: bool,
    pub elapsed_ms: u64,
    /// Directories that contributed nothing, and why.
    ///
    /// Capped, because an unelevated scan of `C:\` skips thousands and the
    /// UI shows a count plus a sample. `skipped_total` keeps the real figure.
    pub skipped: Vec<SkippedPathDto>,
    pub skipped_total: usize,
    /// Links (junctions, symlinks, mount points) recorded rather than
    /// followed. Not gaps: their targets are counted where they live.
    pub links_not_followed: usize,
    /// Identifies the kept tree for [`get_storage_children`] and
    /// [`get_storage_map`]. A node id only means something inside its scan.
    pub scan_id: u64,
    /// The root directory's node, where navigation starts.
    pub root_node: u32,
    /// The largest individual files anywhere under the root, largest first.
    pub largest_files: Vec<LargeFileDto>,
    /// `walk` | `turbo` | `incremental`.
    pub method: &'static str,
    /// For `incremental`: folders copied from the saved index, and folders
    /// read again because they changed.
    pub reused_directories: Option<u64>,
    pub relisted_directories: Option<u64>,
    /// An index was saved, so the next scan of this drive is incremental.
    pub index_saved: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFileDto {
    pub path: String,
    pub allocated: u64,
    pub logical: u64,
    /// The folder it is in, so the UI can open the map there.
    pub dir_node: u32,
}

/// How many skipped paths travel over the wire.
///
/// An unelevated `C:\` scan skips every per-user profile and most of
/// `System32\config`; sending all of them would be a multi-megabyte payload
/// for a list nobody scrolls. The count is exact, the sample is bounded.
#[cfg(windows)]
const SKIPPED_SAMPLE: usize = 50;

/// Walks a directory tree and reports where the space went.
///
/// Runs on the invoke worker thread, which is why it is `async`: Tauri puts
/// async commands on the async runtime instead of blocking the single
/// synchronous command thread, so `cancel_storage_scan` can still be
/// serviced while this is running. Without that the cancel button would
/// deadlock behind the scan it is trying to stop.
///
/// `top_n` bounds the response, not the walk. The whole tree is measured,
/// at every depth; only the largest `top_n` directories are returned,
/// because a table of 400 000 rows is not a UI.
///
/// Progress arrives as [`SCAN_PROGRESS_EVENT`] about ten times a second.
#[tauri::command]
#[cfg(windows)]
pub async fn scan_storage(
    app: tauri::AppHandle,
    path: String,
    top_n: usize,
    mode: Option<String>,
) -> CommandResult<ScanSnapshot> {
    use tauri::Emitter as _;
    use vitals_win::storage::largest_directories;

    if path.trim().is_empty() {
        return Err(CommandError::NotFound {
            message: "no scan root was given".into(),
        });
    }
    // Absent means `auto`, so an older frontend still gets a sensible scan.
    let mode = match mode.as_deref() {
        None | Some("auto") => ScanMode::Auto,
        Some("full") => ScanMode::Full,
        Some("turbo") => ScanMode::Turbo,
        Some(other) => {
            return Err(CommandError::NotFound {
                message: format!("{other} is not a scan mode"),
            });
        }
    };

    let guard = SCAN.begin("a storage scan")?;
    // Released before the walk, not after: otherwise the old tree and the
    // new one would both be in memory at the end of a full-drive scan.
    *stored() = None;

    let root = path.clone();
    // `spawn_blocking`, not inline: a multi-second synchronous walk on an
    // async runtime worker starves every other task on it.
    let handle = tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let progress_root = root.clone();
        let mut on_progress = |p: vitals_win::storage::ScanProgress| {
            // A dropped event is a skipped animation frame, not an error.
            let _ = app.emit(
                SCAN_PROGRESS_EVENT,
                ScanProgressDto {
                    root: progress_root.clone(),
                    files_seen: p.files_seen,
                    directories_seen: p.directories_seen,
                    bytes_seen: p.bytes_seen,
                    elapsed_ms: p.elapsed_ms,
                    current_path: p.current_path,
                    phase: p.phase.key(),
                    fraction: p.fraction,
                },
            );
        };
        let (result, index_saved) = run_scan(&root, mode, &mut on_progress)?;
        let largest = largest_directories(&result, top_n);
        Ok::<_, CommandError>((root, result, largest, index_saved))
    });

    let (root, result, largest, index_saved) =
        handle.await.map_err(|err| CommandError::Internal {
            message: format!("the scan thread did not finish: {err}"),
        })??;

    // Only what leaves bytes out travels as "skipped": a link that was not
    // followed is counted where it points, and listing 95,000 of them as
    // unreadable folders told the user a complete scan was incomplete.
    let skipped: Vec<_> = result
        .tree
        .skipped()
        .iter()
        .filter(|entry| entry.reason.leaves_a_gap())
        .collect();
    let links_not_followed = result.tree.skipped().len() - skipped.len();

    let largest_files = large_file_dtos(&result);
    let root_node = result.tree.root().0;

    let snapshot = ScanSnapshot {
        root,
        largest: directory_dtos(&largest),
        allocated: result.allocated().0,
        logical: result.logical().0,
        cluster_bytes: result.cluster_bytes,
        files_scanned: result.files_scanned,
        directories_scanned: result.directories_scanned,
        hard_link_duplicates: result.hard_link_duplicates,
        hard_link_bytes_saved: result.hard_link_bytes_saved,
        cancelled: result.cancelled,
        complete: result.is_complete(),
        elapsed_ms: result.elapsed_ms,
        skipped: skipped
            .iter()
            .take(SKIPPED_SAMPLE)
            .map(|entry| SkippedPathDto {
                path: entry.path.clone(),
                reason: skip_reason(entry.reason),
                elevation_fixable: entry.reason.is_elevation_fixable(),
            })
            .collect(),
        skipped_total: skipped.len(),
        links_not_followed,
        scan_id: 0,
        root_node,
        largest_files,
        method: match result.method {
            vitals_win::storage::ScanMethod::Walk => "walk",
            vitals_win::storage::ScanMethod::Turbo => "turbo",
            vitals_win::storage::ScanMethod::Incremental => "incremental",
        },
        reused_directories: result.reused_directories,
        relisted_directories: result.relisted_directories,
        index_saved,
    };
    drop(skipped);
    let scan_id = keep_scan(result);
    Ok(ScanSnapshot {
        scan_id,
        ..snapshot
    })
}

/// How the user asked for a scan.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanMode {
    /// Use the saved index when it is valid, else walk.
    Auto,
    /// Walk everything, ignoring the index.
    Full,
    /// Read the file table, elevated.
    Turbo,
}

/// Where saved scan indexes live. One file per drive letter.
#[cfg(windows)]
fn storage_index_dir() -> std::path::PathBuf {
    crate::state::data_dir().join("storage-index")
}

/// Runs the scan the mode asks for, then saves an index when it covered a
/// whole drive. Returns the result and whether an index was saved.
///
/// The journal checkpoint is read **before** the scan starts: a change made
/// while it runs is after the checkpoint, so the next rescan picks it up.
/// Taken after, it would be silently lost.
#[cfg(windows)]
fn run_scan(
    root: &str,
    mode: ScanMode,
    on_progress: &mut dyn FnMut(vitals_win::storage::ScanProgress),
) -> CommandResult<(vitals_win::storage::ScanResult, bool)> {
    use vitals_win::storage::{ScanControl, ScanOptions, index, journal, scan_directory, turbo};

    let dir = storage_index_dir();
    let whole_drive = index::drive_letter(root);
    let checkpoint = whole_drive.and_then(|_| journal::checkpoint(root).ok());
    // Hard links on: `WinSxS` is built almost entirely from links into
    // `System32`, and a naive walk reports it at close to twice its real
    // size. The file ID comes free with the listing.
    let options = ScanOptions::default();

    let result = match (mode, whole_drive) {
        (ScanMode::Turbo, Some(letter)) => {
            turbo::run(letter, Some(&SCAN.cancel), on_progress).map_err(CommandError::from)?
        }
        (ScanMode::Turbo, None) => {
            return Err(CommandError::NotFound {
                message: "a Turbo scan reads a whole drive; pick a drive, not a folder".into(),
            });
        }
        (ScanMode::Auto, Some(_)) if checkpoint.is_some() => {
            let incremental = match (index::load(&dir, root), &checkpoint) {
                (Ok(saved), Some(now)) => {
                    on_progress(vitals_win::storage::ScanProgress {
                        files_seen: 0,
                        directories_seen: 0,
                        bytes_seen: 0,
                        elapsed_ms: 0,
                        current_path: root.to_owned(),
                        phase: vitals_win::storage::ScanPhase::Journal,
                        fraction: None,
                    });
                    let mut control = ScanControl {
                        cancel: Some(&SCAN.cancel),
                        progress: Some(&mut *on_progress),
                    };
                    index::rescan(&saved, now, options, &mut control)
                        .map_err(|why| {
                            tracing::info!(?why, "saved index not used; walking");
                        })
                        .ok()
                }
                (Err(err), _) => {
                    tracing::info!(%err, "no usable saved index; walking");
                    None
                }
                (Ok(_), None) => None,
            };
            if let Some(result) = incremental {
                result
            } else {
                let mut control = ScanControl {
                    cancel: Some(&SCAN.cancel),
                    progress: Some(on_progress),
                };
                scan_directory(std::path::Path::new(root), options, &mut control)
            }
        }
        _ => {
            let mut control = ScanControl {
                cancel: Some(&SCAN.cancel),
                progress: Some(on_progress),
            };
            scan_directory(std::path::Path::new(root), options, &mut control)
        }
    };

    let saved = match (&checkpoint, result.cancelled) {
        (Some(checkpoint), false) => match index::save(&dir, &result, checkpoint) {
            Ok(bytes) => {
                tracing::info!(root, bytes, "saved scan index");
                true
            }
            Err(err) => {
                tracing::warn!(%err, "scan index not saved; the next scan walks");
                false
            }
        },
        _ => false,
    };
    Ok((result, saved))
}

/// The scan's largest files with their full paths rebuilt.
#[cfg(windows)]
fn large_file_dtos(result: &vitals_win::storage::ScanResult) -> Vec<LargeFileDto> {
    result
        .largest_files
        .iter()
        .map(|file| LargeFileDto {
            path: result.path_of_file(file),
            allocated: file.allocated.0,
            logical: file.logical.0,
            dir_node: file.dir.0,
        })
        .collect()
}

#[cfg(windows)]
fn directory_dtos(entries: &[vitals_win::storage::DirectoryEntry]) -> Vec<DirectoryEntryDto> {
    entries
        .iter()
        .map(|entry| DirectoryEntryDto {
            path: entry.path.clone(),
            allocated: entry.allocated.0,
            logical: entry.logical.0,
            files: entry.files,
            incomplete: entry.incomplete.map(skip_reason),
        })
        .collect()
}

/// Stable translation key for a skip reason.
///
/// `OsError(i32)` collapses to one key on purpose: `format!("{reason:?}")`
/// would produce `"OsError(1392)"`, a different string per error code that
/// no translation table can key on.
#[cfg(windows)]
const fn skip_reason(reason: vitals_win::storage::SkipReason) -> &'static str {
    use vitals_win::storage::SkipReason as R;
    match reason {
        R::AccessDenied => "accessDenied",
        R::ReparsePoint => "reparsePoint",
        R::Cancelled => "cancelled",
        R::Vanished => "vanished",
        R::OsError(_) => "osError",
    }
}

/// Stops the running scan.
///
/// Sets the flag and returns at once rather than waiting: every worker checks
/// it between listing batches and the scan returns a partial,
/// honestly-flagged result, which
/// arrives as the resolution of the in-flight `scan_storage` call. Blocking
/// here would make the cancel button appear frozen for exactly as long as
/// the operation the user just asked to abandon.
#[tauri::command]
#[cfg(windows)]
pub fn cancel_storage_scan() {
    SCAN.cancel
        .store(true, std::sync::atomic::Ordering::Release);
}

/// Stops a running cleanup search. Separate from [`cancel_storage_scan`], so
/// stopping one never stops the other.
#[tauri::command]
#[cfg(windows)]
pub fn cancel_cleanup_search() {
    CLEANUP
        .cancel
        .store(true, std::sync::atomic::Ordering::Release);
}

/// One folder as the navigator shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageNodeDto {
    pub node: u32,
    pub name: String,
    pub path: String,
    pub allocated: u64,
    pub logical: u64,
    pub files: u64,
    /// Bytes in files directly inside this folder, not below it.
    pub own_allocated: u64,
    pub own_files: u64,
    pub has_children: bool,
    pub incomplete: Option<&'static str>,
}

/// A folder, the way to it, and what it contains.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageListingDto {
    /// Root first, ending with the folder itself — the breadcrumb.
    pub ancestry: Vec<StorageNodeDto>,
    /// Subfolders, largest first. Every one: a virtual list draws only what
    /// is visible, and a folder with 40,000 subfolders is real
    /// (`WinSxS` has 25,000).
    pub children: Vec<StorageNodeDto>,
}

#[cfg(windows)]
fn node_dto(
    tree: &vitals_win::storage::SizeTree,
    id: vitals_win::storage::NodeId,
) -> Option<StorageNodeDto> {
    let node = tree.node(id)?;
    Some(StorageNodeDto {
        node: id.0,
        name: tree.name_of(id).to_owned(),
        path: tree.path_of(id),
        allocated: node.allocated().0,
        logical: node.logical().0,
        files: node.file_count(),
        own_allocated: node.own_allocated().0,
        own_files: node.own_files(),
        has_children: node.has_children(),
        incomplete: node.skipped().map(skip_reason),
    })
}

#[cfg(windows)]
fn unknown_node() -> CommandError {
    CommandError::NotFound {
        message: "that folder is not part of this scan".into(),
    }
}

/// The node, unless it is not in the tree or was recycled since the scan.
#[cfg(windows)]
fn live_node(
    tree: &vitals_win::storage::SizeTree,
    id: vitals_win::storage::NodeId,
) -> CommandResult<()> {
    if tree.node(id).is_none() || tree.is_detached(id) {
        return Err(unknown_node());
    }
    Ok(())
}

/// A program holding an item open, as "why can't I delete this" names it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderDto {
    pub pid: u32,
    pub name: String,
    pub service: Option<String>,
    /// `window` | `service` | `explorer` | `console` | `critical` | `other`.
    pub kind: &'static str,
}

/// What happened to one basket item.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleItemDto {
    pub path: String,
    /// `recycled` | `refused` | `wouldBePermanent` | `missing` | `locked` |
    /// `accessDenied` | `failed`.
    pub outcome: &'static str,
    /// Only for `refused`: which rule. See `Protection::key`.
    pub protection: Option<&'static str>,
    /// Only for `locked`. `null` when Windows could not say who; empty when
    /// it found nobody.
    pub holders: Option<Vec<HolderDto>>,
    /// Only for `failed`: the HRESULT, for a bug report.
    pub code: Option<i32>,
    /// Bytes taken out of the kept scan for this item, `null` when the scan
    /// did not contain it (or none is kept).
    pub freed: Option<u64>,
}

/// The result of one confirmed basket.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleReportDto {
    pub items: Vec<RecycleItemDto>,
    /// The kept scan's figures after the recycled items were taken out;
    /// `null` when nothing in it changed. The explorer reloads when set.
    pub scan: Option<ScanTotalsDto>,
}

/// The parts of [`ScanSnapshot`] that a recycle changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanTotalsDto {
    pub allocated: u64,
    pub logical: u64,
    pub files_scanned: u64,
    pub largest: Vec<DirectoryEntryDto>,
    pub largest_files: Vec<LargeFileDto>,
}

#[cfg(windows)]
fn holder_dtos(holders: &[vitals_win::storage::Holder]) -> Vec<HolderDto> {
    holders
        .iter()
        .map(|h| HolderDto {
            pid: h.pid,
            name: h.name.clone(),
            service: h.service.clone(),
            kind: h.kind.key(),
        })
        .collect()
}

/// At most this many items per confirmation. A basket is reviewed by a
/// person; a request for more is not one they read.
#[cfg(windows)]
const RECYCLE_MAX: usize = 500;

#[cfg(windows)]
static RECYCLE: Operation = Operation::new();

/// Sends the confirmed basket to the Recycle Bin, item by item.
///
/// # Nothing is deleted outright
///
/// Every path is checked against the protected list here, not only in the
/// UI, and then handed to the shell with `FOFX_RECYCLEONDELETE`; an item the
/// bin cannot take is left where it is and reported as `wouldBePermanent`
/// (see `vitals_win::storage::recycle`). The one confirmation is the UI's
/// dialog: the shell's own prompts are off, because a second dialog that
/// looks different is one people click through.
///
/// Desktop-only, on purpose: a paired phone must not be able to move files.
///
/// When `scan_id` is still the kept scan, each recycled item is taken out of
/// it, so the explorer's sizes go down without a rescan.
#[tauri::command]
#[cfg(windows)]
pub async fn recycle_storage_items(
    paths: Vec<String>,
    scan_id: Option<u64>,
) -> CommandResult<RecycleReportDto> {
    use vitals_win::storage::{Outcome, Rules, largest_directories, recycle};

    if paths.is_empty() {
        return Err(CommandError::Refused {
            message: "the basket is empty".into(),
        });
    }
    if paths.len() > RECYCLE_MAX {
        return Err(CommandError::Refused {
            message: format!("at most {RECYCLE_MAX} items can be recycled at once"),
        });
    }
    let guard = RECYCLE.begin("sending items to the Recycle Bin")?;

    let (paths, outcomes) = tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let outcomes = recycle(&paths, &Rules::for_this_machine());
        (paths, outcomes)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the recycle thread did not finish: {err}"),
    })?;

    let mut kept = stored();
    let mut scan = kept
        .as_mut()
        .filter(|s| scan_id.is_some_and(|id| id == s.id));
    let mut changed = false;

    let items = paths
        .into_iter()
        .zip(outcomes)
        .map(|(path, outcome)| {
            let freed = if outcome == Outcome::Recycled {
                scan.as_mut()
                    .and_then(|s| {
                        s.last_used = std::time::Instant::now();
                        s.result.forget_recycled(&path)
                    })
                    .map(|amount| amount.allocated)
            } else {
                None
            };
            changed |= freed.is_some();
            RecycleItemDto {
                outcome: outcome.key(),
                protection: match &outcome {
                    Outcome::Refused(why) => Some(why.key()),
                    _ => None,
                },
                holders: match &outcome {
                    Outcome::Locked(Some(holders)) => Some(holder_dtos(holders)),
                    _ => None,
                },
                code: match &outcome {
                    Outcome::Failed(code) => Some(*code),
                    _ => None,
                },
                freed,
                path,
            }
        })
        .collect();

    let scan = scan.filter(|_| changed).map(|kept| {
        let result = &kept.result;
        ScanTotalsDto {
            allocated: result.allocated().0,
            logical: result.logical().0,
            files_scanned: result
                .tree
                .node(result.tree.root())
                .map_or(0, vitals_win::storage::Node::file_count),
            largest: directory_dtos(&largest_directories(result, RECYCLE_TOP_N)),
            largest_files: large_file_dtos(result),
        }
    });

    Ok(RecycleReportDto { items, scan })
}

/// Rows of the largest-folders table after a recycle. The same bound the UI
/// asks `scan_storage` for.
#[cfg(windows)]
const RECYCLE_TOP_N: usize = 200;

/// Which programs have a file, or any file in a folder, open.
///
/// Read-only, so it may be asked before recycling as well as after a
/// failure.
#[tauri::command]
#[cfg(windows)]
pub async fn get_file_holders(path: String) -> CommandResult<Vec<HolderDto>> {
    let found = tauri::async_runtime::spawn_blocking(move || {
        vitals_win::storage::holders_of(std::path::Path::new(&path))
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the lookup thread did not finish: {err}"),
    })?;
    found
        .map(|holders| holder_dtos(&holders))
        .map_err(|code| CommandError::Internal {
            message: format!("Restart Manager could not answer (error {code})"),
        })
}

/// Opens one folder of the kept scan.
#[tauri::command]
#[cfg(windows)]
pub fn get_storage_children(scan_id: u64, node: u32) -> CommandResult<StorageListingDto> {
    use vitals_win::storage::NodeId;
    with_scan(scan_id, |result| {
        let tree = &result.tree;
        let id = NodeId(node);
        live_node(tree, id)?;
        Ok(StorageListingDto {
            ancestry: tree
                .ancestry(id)
                .into_iter()
                .filter_map(|a| node_dto(tree, a))
                .collect(),
            children: tree
                .children_by_size(id)
                .into_iter()
                .filter_map(|c| node_dto(tree, c))
                .collect(),
        })
    })
}

/// One rectangle of the map. Coordinates are fractions of the area.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapCellDto {
    /// `directory` | `files` | `smaller`.
    pub kind: &'static str,
    pub node: u32,
    pub depth: u16,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub allocated: u64,
    pub count: u64,
    pub openable: bool,
    /// Only for directories: the tooltip and the list both need it, and a
    /// few thousand short names are cheap next to a second round trip.
    pub name: Option<String>,
    pub incomplete: Option<&'static str>,
}

/// Lays out the map of one folder of the kept scan.
///
/// `shape` is `icicle` or `treemap`; `aspect` is the canvas width over its
/// height, which only the treemap needs. Level of detail is decided here: at
/// most 5,000 cells, anything narrower than about two pixels folded into one.
#[tauri::command]
#[cfg(windows)]
pub fn get_storage_map(
    scan_id: u64,
    node: u32,
    shape: &str,
    aspect: f64,
) -> CommandResult<Vec<MapCellDto>> {
    use vitals_win::storage::{CellKind, Detail, NodeId, icicle, treemap};
    with_scan(scan_id, |result| {
        let tree = &result.tree;
        let id = NodeId(node);
        live_node(tree, id)?;
        let cells = match shape {
            "icicle" => icicle(tree, id, Detail::default()),
            "treemap" => treemap(
                tree,
                id,
                aspect,
                Detail {
                    max_depth: 4,
                    ..Detail::default()
                },
            ),
            other => {
                return Err(CommandError::NotFound {
                    message: format!("unknown map shape {other:?}"),
                });
            }
        };
        // Fractions go out as f32: a 4-byte float resolves a 16,000-pixel
        // canvas to well under a pixel, and it halves the payload.
        #[allow(clippy::cast_possible_truncation)]
        let to_f32 = |v: f64| v as f32;
        Ok(cells
            .into_iter()
            .map(|cell| MapCellDto {
                kind: match cell.kind {
                    CellKind::Directory => "directory",
                    CellKind::Files => "files",
                    CellKind::Smaller => "smaller",
                },
                name: (cell.kind == CellKind::Directory)
                    .then(|| tree.name_of(cell.node).to_owned()),
                node: cell.node.0,
                depth: cell.depth,
                x0: to_f32(cell.x0),
                y0: to_f32(cell.y0),
                x1: to_f32(cell.x1),
                y1: to_f32(cell.y1),
                allocated: cell.allocated,
                count: cell.count,
                openable: cell.openable,
                incomplete: cell.incomplete.map(skip_reason),
            })
            .collect())
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCandidateDto {
    pub path: String,
    /// Translation key. See `cleanup_kind`.
    pub kind: &'static str,
    /// `null` when the location exists but could not be measured.
    ///
    /// Never zero for that case: "we could not size this" and "this is empty"
    /// are different facts, and showing the first as the second is how a
    /// cleanup tool talks a user out of reclaiming 8 GB.
    pub size: Option<u64>,
    /// `safe` | `review` | `risky`.
    pub safety: &'static str,
    /// Whether a UI may offer this behind a single confirmation.
    pub allows_one_click: bool,
    pub needs_elevation: bool,
    /// The Windows tool that frees it — `diskCleanup` | `componentCleanup`
    /// | `hibernateOff` — or `null` when the space is not Windows-managed
    /// and goes through the review basket instead.
    pub tool: Option<&'static str>,
    /// Whether running the tool cannot be undone, so it needs its own
    /// confirmation stating the consequence.
    pub irreversible: bool,
}

/// Finds and sizes reclaimable locations.
///
/// # This never deletes anything
///
/// Sizing a cache is a read; emptying one is a write, and the two do not
/// belong behind the same button. Windows-managed space is freed by
/// [`run_windows_cleanup`], through Windows' own tools.
///
/// Async and `spawn_blocking` for the same reason as [`scan_storage`]: each
/// candidate is sized with a bounded directory walk, and a multi-gigabyte
/// package cache takes seconds.
#[tauri::command]
#[cfg(windows)]
pub async fn find_cleanup_candidates() -> CommandResult<Vec<CleanupCandidateDto>> {
    let guard = CLEANUP.begin("a search for reclaimable space")?;

    let found = tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        vitals_win::storage::find_cleanup_candidates(Some(&CLEANUP.cancel))
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the cleanup scan thread did not finish: {err}"),
    })?;

    Ok(found
        .iter()
        .map(|candidate| CleanupCandidateDto {
            path: candidate.path.display().to_string(),
            kind: cleanup_kind(candidate.kind),
            size: candidate.size.map(|bytes| bytes.0),
            safety: candidate.safety.as_str(),
            allows_one_click: candidate.safety.allows_one_click(),
            needs_elevation: candidate.needs_elevation,
            tool: vitals_win::storage::tool_for(candidate.kind, &candidate.path)
                .map(vitals_win::storage::ManagedTool::family),
            irreversible: vitals_win::storage::tool_for(candidate.kind, &candidate.path)
                .is_some_and(vitals_win::storage::ManagedTool::is_risky),
        })
        .collect())
}

/// Emitted about twice a second while a Windows cleanup tool runs.
#[cfg(windows)]
const CLEANUP_PROGRESS_EVENT: &str = "vitals://storage/cleanup-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsCleanupProgressDto {
    pub path: String,
    /// `measuring` | `approval` | `running` | `remeasuring`.
    pub stage: &'static str,
    pub elapsed_ms: u64,
    /// Free space the drive has gained so far; `null` when unreadable.
    pub drive_freed: Option<i64>,
}

/// What one run of a Windows tool measured.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsCleanupReportDto {
    pub path: String,
    /// `done` | `needsRestart` | `toolFailed`.
    pub outcome: &'static str,
    /// Only for `toolFailed`: the tool's own exit code.
    pub code: Option<u32>,
    /// On-disk bytes at the location, before and after; `null` when it could
    /// not be read (or, for the component store, is not walked on purpose).
    pub location_before: Option<u64>,
    pub location_after: Option<u64>,
    /// How much smaller the location got, measured. Negative if it grew.
    pub location_freed: Option<i64>,
    /// How much free space the drive gained, measured. Includes anything
    /// else writing to the drive meanwhile, which is why both are shown.
    pub drive_freed: Option<i64>,
    pub elapsed_ms: u64,
}

#[cfg(windows)]
static MANAGED: Operation = Operation::new();

/// Frees one Windows-managed location with Windows' own tool.
///
/// # Nothing is deleted by Vitals
///
/// The location is looked up again in the candidate list, so the webview can
/// name a location but never a tool or an arbitrary path; the tool is the one
/// `tool_for` assigns. It runs in a short-lived elevated instance (one UAC
/// prompt, for this action only), and the figures in the report are read
/// before and after it, never estimated. `confirmed` must be true for a tool
/// whose effect cannot be undone; the elevated instance checks it again.
///
/// Desktop-only, like recycling: a paired phone must not free space.
#[tauri::command]
#[cfg(windows)]
pub async fn run_windows_cleanup(
    app: tauri::AppHandle,
    path: String,
    confirmed: bool,
) -> CommandResult<WindowsCleanupReportDto> {
    use tauri::Emitter as _;
    use vitals_win::storage::managed::{self, Consent, Finished};

    let target = std::path::PathBuf::from(&path);
    let tool = managed_tool_for(&target)?;
    let consent = if confirmed {
        Consent::Confirmed
    } else {
        Consent::Unconfirmed
    };
    let guard = MANAGED.begin("a Windows cleanup")?;

    let report = tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let progress_path = path.clone();
        let mut on_progress = |p: managed::Progress| {
            let _ = app.emit(
                CLEANUP_PROGRESS_EVENT,
                WindowsCleanupProgressDto {
                    path: progress_path.clone(),
                    stage: p.stage.key(),
                    elapsed_ms: u64::try_from(p.elapsed.as_millis()).unwrap_or(u64::MAX),
                    drive_freed: p.drive_freed,
                },
            );
        };
        managed::run(tool, &target, consent, &mut on_progress).map(|r| (path, r))
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the cleanup thread did not finish: {err}"),
    })?;
    let (path, report) = report?;

    let (outcome, code) = match report.finished {
        Finished::Done => ("done", None),
        Finished::NeedsRestart => ("needsRestart", None),
        Finished::ToolFailed(code) => ("toolFailed", Some(code)),
    };
    Ok(WindowsCleanupReportDto {
        path,
        outcome,
        code,
        location_before: report.location_before,
        location_after: report.location_after,
        location_freed: report.location_freed(),
        drive_freed: report.drive_freed(),
        elapsed_ms: u64::try_from(report.elapsed.as_millis()).unwrap_or(u64::MAX),
    })
}

/// The tool for a location the candidate list itself produced, else a
/// refusal: the webview names a location, never a tool or an arbitrary path.
#[cfg(windows)]
fn managed_tool_for(target: &std::path::Path) -> CommandResult<vitals_win::storage::ManagedTool> {
    vitals_win::storage::candidate_locations()
        .into_iter()
        .find(|(candidate, _)| {
            candidate
                .as_os_str()
                .eq_ignore_ascii_case(target.as_os_str())
        })
        .and_then(|(candidate, kind)| vitals_win::storage::tool_for(kind, &candidate))
        .ok_or_else(|| CommandError::Refused {
            message: "that location is not one Windows cleans up, so nothing was run".into(),
        })
}

/// Stable translation key. The reason and label live in the UI bundle rather
/// than travelling as English text, because they must be translatable.
#[cfg(windows)]
const fn cleanup_kind(kind: vitals_win::storage::CleanupKind) -> &'static str {
    use vitals_win::storage::CleanupKind as K;
    match kind {
        K::UserTemp => "userTemp",
        K::SystemTemp => "systemTemp",
        K::BrowserCache => "browserCache",
        K::WindowsUpdateCache => "windowsUpdateCache",
        K::RecycleBin => "recycleBin",
        K::CrashDump => "crashDump",
        K::PreviousWindows => "previousWindows",
        K::Hibernation => "hibernation",
        K::PackageManagerCache => "packageManagerCache",
        K::ThumbnailCache => "thumbnailCache",
        K::DeliveryOptimisation => "deliveryOptimisation",
        K::ComponentStore => "componentStore",
    }
}

// `cfg(test)` on its own line: clippy's allow-expect-in-tests recognises a
// module as a test module only by that attribute, not inside `all(...)`.
#[cfg(test)]
#[cfg(windows)]
mod sensors_service_tests {
    use super::*;
    use vitals_win::sensors::cpu_service::CpuSensors;
    use vitals_win::sensors::{
        DRIVER_GAPS, SensorSample, ThermalAvailability, ThermalScan, read_power_state,
    };

    fn sample(cpu: Option<CpuSensors>) -> SensorSample {
        SensorSample {
            thermal: ThermalScan::unavailable(ThermalAvailability::NoZonesPresent),
            batteries: Vec::new(),
            power: read_power_state(),
            nvidia: Vec::new(),
            cpu,
            drives_measured: false,
            readings: Vec::new(),
            elapsed: std::time::Duration::ZERO,
        }
    }

    fn gap(label: &str) -> &'static vitals_win::sensors::DriverGap {
        DRIVER_GAPS
            .iter()
            .find(|g| g.label == label)
            .expect("gap is listed")
    }

    #[test]
    fn the_cpu_gaps_close_only_when_the_service_measured_them() {
        let none = sample(None);
        assert!(!closed_by(gap("CPU core temperature"), &none));
        assert!(!closed_by(gap("CPU package power"), &none));

        // Temperature without power: exactly one gap closes, because a
        // service that read no energy counter has not measured power.
        let temp_only = sample(Some(CpuSensors {
            package_celsius: Some(52.0),
            hottest_core_celsius: None,
            package_watts: None,
            ..CpuSensors::default()
        }));
        assert!(closed_by(gap("CPU core temperature"), &temp_only));
        assert!(!closed_by(gap("CPU package power"), &temp_only));
        assert!(
            !closed_by(gap("Fan speed (RPM)"), &temp_only),
            "a service that read no fan has not measured fans"
        );

        let both = sample(Some(CpuSensors {
            package_celsius: None,
            hottest_core_celsius: Some(70.0),
            package_watts: Some(40.0),
            super_io: Some("IT8689E".to_owned()),
            fans: vec![("Fan 1".to_owned(), 1467)],
        }));
        assert!(closed_by(gap("CPU core temperature"), &both));
        assert!(closed_by(gap("CPU package power"), &both));
        assert!(closed_by(gap("Fan speed (RPM)"), &both));
    }

    #[test]
    fn a_declined_or_failed_install_is_never_reported_as_success() {
        assert!(setup_outcome(vitals_sensors::exit::OK).is_ok());
        assert!(matches!(
            setup_outcome(vitals_sensors::exit::NOT_ELEVATED),
            Err(CommandError::AccessDenied { .. })
        ));
        for code in [
            vitals_sensors::exit::FAILED,
            vitals_sensors::exit::NO_PAWNIO,
            vitals_sensors::exit::USAGE,
            -1,
        ] {
            assert!(setup_outcome(code).is_err(), "exit {code} read as success");
        }
    }
}

#[cfg(test)]
#[cfg(windows)]
mod recycle_tests {
    use super::*;

    #[test]
    fn the_command_refuses_a_system_path_whatever_the_ui_sent() {
        let hosts = r"C:\Windows\System32\drivers\etc\hosts".to_owned();
        let report = tauri::async_runtime::block_on(recycle_storage_items(
            vec![
                hosts.clone(),
                r"c:/windows/system32".to_owned(),
                "C:".to_owned(),
            ],
            None,
        ))
        .expect("a report, not an error");
        let outcomes: Vec<_> = report
            .items
            .iter()
            .map(|i| (i.outcome, i.protection))
            .collect();
        assert_eq!(
            outcomes,
            vec![
                ("refused", Some("systemFolder")),
                ("refused", Some("systemFolder")),
                ("refused", Some("driveRoot")),
            ]
        );
        assert!(std::path::Path::new(&hosts).exists());
        assert!(report.scan.is_none());
    }

    #[test]
    fn an_empty_or_oversized_basket_is_refused_before_anything_runs() {
        let empty = tauri::async_runtime::block_on(recycle_storage_items(Vec::new(), None));
        assert!(matches!(empty, Err(CommandError::Refused { .. })));
        let many = vec![r"C:\nowhere".to_owned(); RECYCLE_MAX + 1];
        let big = tauri::async_runtime::block_on(recycle_storage_items(many, None));
        assert!(matches!(big, Err(CommandError::Refused { .. })));
    }

    #[test]
    fn a_windows_cleanup_runs_only_for_a_location_the_candidate_list_produced() {
        // The webview names a location; a path it made up — or the user's
        // own temp folder, which is the basket's job — never reaches a tool.
        for path in [r"C:\Users", r"C:\Windows\System32", r"D:\anything", ""] {
            assert!(
                matches!(
                    managed_tool_for(std::path::Path::new(path)),
                    Err(CommandError::Refused { .. })
                ),
                "{path:?}"
            );
        }
        if let Ok(temp) = std::env::var("TEMP") {
            assert!(managed_tool_for(std::path::Path::new(&temp)).is_err());
        }
        let root = std::env::var("SystemRoot").expect("SystemRoot");
        assert_eq!(
            managed_tool_for(&std::path::Path::new(&root).join("WinSxS")).ok(),
            Some(vitals_win::storage::ManagedTool::ComponentCleanup)
        );
        let drive = std::env::var("SystemDrive").expect("SystemDrive");
        assert_eq!(
            managed_tool_for(std::path::Path::new(&format!("{drive}\\hiberfil.sys")))
                .map(vitals_win::storage::ManagedTool::is_risky)
                .ok(),
            Some(true)
        );
    }
}
