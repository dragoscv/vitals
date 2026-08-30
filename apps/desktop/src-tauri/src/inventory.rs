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

use crate::commands::CommandError;

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
#[tauri::command]
#[cfg(windows)]
pub fn get_connections() -> ConnectionsSnapshot {
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
}

/// Collects startup entries and services.
///
/// `with_service_config` opens each service to read its start type and binary
/// path, which costs an SCM round trip per service — on a machine with ~370
/// services that is the difference between a fast list and a slow one. The
/// screen asks for it only when the user opens the Services tab.
#[tauri::command]
#[cfg(windows)]
pub fn get_startup(with_service_config: bool) -> CommandResult<StartupSnapshot> {
    use vitals_win::startup;

    let inventory = startup::collect(with_service_config)?;

    Ok(StartupSnapshot {
        entries: inventory
            .entries
            .iter()
            .map(|entry| StartupEntryDto {
                name: entry.name.clone(),
                display_name: entry.display_name.clone(),
                command: entry.command.clone(),
                image_path: entry
                    .image_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                publisher: entry.publisher.clone(),
                source: startup_source(entry.source),
                state: startup_state(entry.state),
                pid: entry.pid,
            })
            .collect(),
        services: inventory
            .services
            .iter()
            .map(|service| ServiceDto {
                name: service.name.clone(),
                display_name: service.display_name.clone(),
                state: service_state(service.state),
                start_type: start_type(service.start_type),
                pid: service.pid,
                binary_path: service.binary_path.clone(),
                svchost_group: service.svchost_group.clone(),
            })
            .collect(),
        unreadable_tasks: inventory.unreadable_tasks,
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
    pub source: String,
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
    pub duplicates_collapsed: usize,
}

/// Enumerates installed applications.
///
/// Never fails as a whole: an absent registry view — `WOW6432Node` does not
/// exist on a 32-bit-only system — is skipped rather than blanking the list.
#[tauri::command]
#[cfg(windows)]
pub fn get_installed_apps() -> AppsSnapshot {
    use vitals_win::apps;

    let scan = apps::enumerate_installed_apps();

    AppsSnapshot {
        apps: scan.apps.iter().map(installed_app_dto).collect(),
        examined: scan.examined,
        rejected: scan.rejected_total(),
        duplicates_collapsed: scan.duplicates_collapsed,
    }
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
        source: format!("{:?}", app.source),
    }
}
