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
#[tauri::command]
#[cfg(windows)]
pub fn get_installed_apps() -> AppsSnapshot {
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
#[tauri::command]
#[cfg(windows)]
// Tauri deserialises command arguments into owned values; it cannot hand us a
// borrow.
#[allow(clippy::needless_pass_by_value)]
pub fn uninstall_app(command: String) -> CommandResult<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    // CREATE_NO_WINDOW: the shell itself must not flash a console. The
    // uninstaller's own UI still appears, which is the point.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    if command.trim().is_empty() {
        return Err(CommandError::NotFound {
            message: "this application published no uninstall command".into(),
        });
    }

    Command::new("cmd")
        .args(["/c", &command])
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
#[tauri::command]
#[cfg(windows)]
#[must_use]
pub fn get_sensors() -> SensorsSnapshot {
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
        gaps: sensors::DRIVER_GAPS
            .iter()
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
    }
}

#[cfg(windows)]
const fn sensor_source(source: vitals_win::sensors::SensorSource) -> &'static str {
    use vitals_win::sensors::SensorSource as S;
    match source {
        S::AcpiThermalZone => "acpiThermalZone",
        S::BatteryMiniport => "batteryMiniport",
        S::SystemPowerStatus => "systemPowerStatus",
        S::KernelDriver => "kernelDriver",
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
        let snapshot = get_sensors();

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
        let snapshot = get_sensors();

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
        assert!(get_sensors().cadence_ms >= 1000);
    }

    #[test]
    fn no_reading_is_fabricated() {
        for reading in &get_sensors().readings {
            assert!(!reading.key.is_empty());
            assert!(reading.value.is_finite());
        }
    }
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Cancellation token shared by the scan and cleanup commands.
///
/// One flag rather than one per request because the UI only ever runs one
/// scan: the screen disables the control while a scan is in flight, and a
/// second concurrent walk of the same volume would halve the throughput of
/// both. A handle map would buy per-request cancellation that nothing asks
/// for, at the cost of leaking an entry every time a webview reloads
/// mid-scan.
///
/// Cleared at the start of every operation, so a cancel left set by an
/// abandoned scan cannot make the next one return instantly-and-empty.
#[cfg(windows)]
static SCAN_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    vitals_win::disk::enumerate_volumes()
        .into_iter()
        .map(|volume| {
            let letter = volume.mount.chars().next().unwrap_or('?');
            VolumeDto {
                strategy: match vitals_win::storage::strategy_for(letter) {
                    vitals_win::storage::ScanStrategy::MftAssisted => "mftAssisted",
                    vitals_win::storage::ScanStrategy::DirectoryWalk => "directoryWalk",
                },
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
/// `top_n` bounds the response, not the walk. The whole tree is measured;
/// only the largest `top_n` directories are returned, because a table of
/// 400 000 rows is not a UI.
#[tauri::command]
#[cfg(windows)]
pub async fn scan_storage(
    path: String,
    max_depth: Option<u32>,
    top_n: usize,
) -> CommandResult<ScanSnapshot> {
    use std::sync::atomic::Ordering;
    use vitals_win::storage::{ScanControl, ScanOptions, largest_directories, scan_directory};

    if path.trim().is_empty() {
        return Err(CommandError::NotFound {
            message: "no scan root was given".into(),
        });
    }

    // Cleared first: a flag left set by a scan the user abandoned would make
    // this one return instantly and empty, which looks like a broken volume.
    SCAN_CANCEL.store(false, Ordering::Relaxed);

    let root = path.clone();
    // `spawn_blocking`, not inline: a multi-minute synchronous walk on an
    // async runtime worker starves every other task on it.
    let handle = tauri::async_runtime::spawn_blocking(move || {
        let mut control = ScanControl {
            cancel: Some(&SCAN_CANCEL),
            progress: None,
        };
        let result = scan_directory(
            std::path::Path::new(&root),
            ScanOptions {
                max_depth,
                // Worth the halved scan rate on a Windows volume: `WinSxS` is
                // built almost entirely from links into `System32`, and a
                // naive walk reports it at close to twice its real size.
                detect_hard_links: true,
                resolve_compressed: true,
                ..Default::default()
            },
            &mut control,
        );
        let largest = largest_directories(&result, top_n);
        (root, result, largest)
    });

    let (root, result, largest) = handle.await.map_err(|err| CommandError::Internal {
        message: format!("the scan thread did not finish: {err}"),
    })?;

    let skipped = result.tree.skipped();

    Ok(ScanSnapshot {
        root,
        largest: largest
            .iter()
            .map(|entry| DirectoryEntryDto {
                path: entry.path.clone(),
                allocated: entry.allocated.0,
                logical: entry.logical.0,
                files: entry.files,
                incomplete: entry.incomplete.map(skip_reason),
            })
            .collect(),
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
    })
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
        R::DepthLimit => "depthLimit",
        R::Cycle => "cycle",
        R::Vanished => "vanished",
        R::OsError(_) => "osError",
    }
}

/// Stops the running scan.
///
/// Sets the flag and returns at once rather than waiting: the scan checks it
/// once per directory and returns a partial, honestly-flagged result, which
/// arrives as the resolution of the in-flight `scan_storage` call. Blocking
/// here would make the cancel button appear frozen for exactly as long as
/// the operation the user just asked to abandon.
#[tauri::command]
#[cfg(windows)]
pub fn cancel_storage_scan() {
    SCAN_CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
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
    /// Whether a UI may offer this behind a single confirmation. Currently
    /// advisory everywhere, because no deletion backend exists.
    pub allows_one_click: bool,
    pub needs_elevation: bool,
}

/// Finds and sizes reclaimable locations.
///
/// # This never deletes anything
///
/// There is no removal command, here or anywhere else in Vitals. Sizing a
/// cache is a read; emptying one is an irreversible write, and the two do not
/// belong behind the same button. The UI renders the action as unavailable
/// rather than pretending.
///
/// Async and `spawn_blocking` for the same reason as [`scan_storage`]: each
/// candidate is sized with a bounded directory walk, and a multi-gigabyte
/// package cache takes seconds.
#[tauri::command]
#[cfg(windows)]
pub async fn find_cleanup_candidates() -> CommandResult<Vec<CleanupCandidateDto>> {
    use std::sync::atomic::Ordering;

    SCAN_CANCEL.store(false, Ordering::Relaxed);

    let found = tauri::async_runtime::spawn_blocking(|| {
        vitals_win::storage::find_cleanup_candidates(Some(&SCAN_CANCEL))
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
        })
        .collect())
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
    }
}
