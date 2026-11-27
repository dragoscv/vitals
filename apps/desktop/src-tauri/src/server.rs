//! The LAN server, as the desktop app hosts it.
//!
//! `vitals-server` knows nothing about Tauri; this module is the glue. It owns
//! the on/off state, persists the pairing tokens, feeds frames from the
//! sampler, and implements [`Controller`] on top of the same `vitals_win`
//! actions the context menu uses — so "end task from the phone" and "end task
//! from the right-click menu" are the same function with the same risk
//! checks.
//!
//! **The LAN server is off by default.** Nothing binds a network-facing
//! socket until `start_lan_server` is called, and it is called only when the
//! user flips the switch.
//!
//! The **local API** is different: it listens on `127.0.0.1` from startup so
//! the `vitals` CLI can attach to the running app without any setup. It is
//! unreachable from the network by construction — the bind address is the
//! mechanism, not a filter — and grants a tokenless loopback caller `read`
//! only (see `ApiState::loopback_scope`). It used to grant `control`, on the
//! reasoning that a local process already owns the app; but loopback TCP is
//! shared by every account on the machine, so another user (fast user
//! switching, RDP, a service) could end this user's processes. The CLI acts
//! on its own machine through the OS instead, where Windows checks the
//! caller's own rights.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use vitals_server::state::ServerLock;
use vitals_server::{
    Advertisement, ApiState, ControlError, ControlRequest, Controller, FrameSource, Interface,
    IssuedCode, PairingDesk, PairingStatus, Scope, ServeHandle, Token, TokenSet,
};

use crate::commands::CommandError;
use crate::state::data_dir;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// The default port. Unassigned by IANA, unlikely to collide, and easy to
/// type by hand if the QR scan fails.
pub const DEFAULT_PORT: u16 = 7331;

/// The loopback listener's preferred port. One below the LAN default so the
/// two are easy to tell apart in `netstat`. If it is taken the OS picks
/// another and the discovery file says which.
pub const LOCAL_PORT: u16 = 7330;

/// What a tokenless caller on the local API may do. `Read`, never `Control`:
/// loopback TCP is shared by every account on the machine (module docs).
pub const LOCAL_SCOPE: Scope = Scope::Read;

/// What the CLI reads to find the running app.
///
/// The `pid` lets a reader detect a stale file left by a crash: if that
/// process is gone, so is the listener, however plausible the port looks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalApiDiscovery {
    pub port: u16,
    pub pid: u32,
    pub version: String,
}

/// Managed by Tauri: everything about the server that outlives a request.
pub struct LanServer {
    frames: FrameSource,
    tokens: Arc<ServerLock<TokenSet>>,
    running: Mutex<Option<ServeHandle>>,
    /// The loopback-only listener. Held apart from `running` so the LAN
    /// status and port keep meaning "what a phone can reach"; the local API
    /// is never shown in Settings and is never advertised.
    local: Mutex<Option<ServeHandle>>,
    /// The `mDNS` record, held beside the listener rather than inside it so
    /// the two can only ever be started and stopped together.
    advertisement: Mutex<Option<Advertisement>>,
    controller: Arc<dyn Controller>,
    /// The TV pairing code. One desk shared by both listeners, so a code
    /// shown in Settings is the only one either of them will accept.
    pairing: Arc<PairingDesk>,
}

impl std::fmt::Debug for LanServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanServer")
            .field("running", &self.running.lock().is_some())
            .field("local", &self.local.lock().is_some())
            .finish_non_exhaustive()
    }
}

impl Default for LanServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LanServer {
    #[must_use]
    pub fn new() -> Self {
        let tokens = load_tokens();
        Self {
            frames: FrameSource::new(),
            tokens: Arc::new(ServerLock::new(tokens)),
            running: Mutex::new(None),
            local: Mutex::new(None),
            advertisement: Mutex::new(None),
            controller: Arc::new(DesktopController),
            // Redeemed tokens go through the same `save_tokens` as a QR
            // pairing, so a paired TV survives a restart like a phone does.
            pairing: Arc::new(PairingDesk::new(Arc::new(save_tokens))),
        }
    }

    /// Called by the sampler every tick. Cheap when nothing is listening.
    pub fn publish(&self, frame: Arc<vitals_core::sample::Frame>) {
        self.frames.publish(frame);
    }

    /// True while any listener — LAN or loopback — wants frames. The sampler
    /// gates its per-tick clone on this, so it must cover both or the CLI
    /// would see a server that answers `204` forever.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.lock().is_some() || self.local.lock().is_some()
    }

    /// Whether a client is streaming or has polled recently. The sampler
    /// keeps at least 1 Hz while this holds, so a phone does not watch
    /// numbers frozen by a hidden window.
    #[must_use]
    pub fn has_viewers(&self) -> bool {
        self.is_running() && self.frames.has_viewers()
    }

    /// The LAN listener's port only. The local API is deliberately excluded:
    /// Settings shows this as "what your phone connects to".
    fn port(&self) -> Option<u16> {
        self.running.lock().as_ref().map(|h| h.addr.port())
    }

    /// The adapter the LAN listener is bound to, or `None` when it listens
    /// on every interface (or is not running).
    fn bound_adapter(&self) -> Option<std::net::Ipv4Addr> {
        self.running
            .lock()
            .as_ref()
            .and_then(|h| lan_bound_adapter(h.addr))
    }

    /// The loopback listener's port, if it is up.
    #[must_use]
    pub fn local_port(&self) -> Option<u16> {
        self.local.lock().as_ref().map(|h| h.addr.port())
    }

    /// Builds the state both listeners serve. One function so the loopback
    /// and LAN servers can never disagree about frames, tokens or controller;
    /// the two differ only in `assets` and `loopback_scope`.
    ///
    /// `pub(crate)` so the MQTT publisher (`crate::mqtt`) can read the same
    /// materialised frame and alert list the LAN clients do, instead of
    /// opening a second sampling path.
    pub(crate) fn api_state(
        &self,
        app: &tauri::AppHandle,
        assets: Option<vitals_server::StaticAssets>,
        loopback_scope: Option<Scope>,
    ) -> ApiState {
        ApiState {
            frames: self.frames.clone(),
            tokens: Arc::clone(&self.tokens),
            controller: Arc::clone(&self.controller),
            assets,
            host: Arc::new(host_info),
            alerts: {
                let app = app.clone();
                Arc::new(move || {
                    tauri::Manager::try_state::<crate::alerts::Alerts>(&app)
                        .map(|a| a.active())
                        .unwrap_or_default()
                })
            },
            history: Arc::new(|seconds| {
                crate::store::query_machine_history(seconds).unwrap_or_else(|error| {
                    tracing::warn!(?error, "history for a LAN client failed");
                    Vec::new()
                })
            }),
            sensors: Arc::new(crate::inventory::lan_sensor_lines),
            connectivity: Arc::new(crate::commands::probe_connectivity),
            version: app.package_info().version.to_string(),
            loopback_scope,
            pairing: Some(Arc::clone(&self.pairing)),
            agents: Some(Arc::new(crate::agents::LanAgents::new(app.clone()))),
        }
    }
}

// ── The local API ──────────────────────────────────────────────────────

fn discovery_path() -> PathBuf {
    data_dir().join("local-api.json")
}

/// Starts the loopback listener and writes the discovery file.
///
/// Called from `setup`, before the sampler's first tick. Tries
/// [`LOCAL_PORT`] first so the CLI has a well-known default, and falls back
/// to an OS-chosen port rather than failing: a second Vitals build running
/// side by side, or an unrelated program on 7330, must not cost the user
/// their monitor. Failure here is logged, never fatal — the desktop works
/// without it and only the CLI is affected.
pub fn start_local_api(app: &tauri::AppHandle) {
    let Some(server) = tauri::Manager::try_state::<LanServer>(app) else {
        tracing::warn!("local API not started: server state is not managed");
        return;
    };
    if server.local.lock().is_some() {
        return;
    }

    let state = server.api_state(app, None, Some(LOCAL_SCOPE));
    let version = app.package_info().version.to_string();
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        let loopback = std::net::Ipv4Addr::LOCALHOST;
        let preferred = std::net::SocketAddr::from((loopback, LOCAL_PORT));
        let handle = match vitals_server::serve_on(state.clone(), preferred).await {
            Ok(handle) => handle,
            Err(error) => {
                tracing::info!(%error, port = LOCAL_PORT, "preferred local API port is taken; asking the OS for one");
                match vitals_server::serve_on(state, std::net::SocketAddr::from((loopback, 0)))
                    .await
                {
                    Ok(handle) => handle,
                    Err(error) => {
                        tracing::warn!(%error, "could not start the local API; the CLI will not find this app");
                        return;
                    }
                }
            }
        };

        let port = handle.addr.port();
        let Some(server) = tauri::Manager::try_state::<LanServer>(&app) else {
            return;
        };
        *server.local.lock() = Some(handle);
        tracing::info!(port, "local API listening on 127.0.0.1");

        write_discovery(&LocalApiDiscovery {
            port,
            pid: std::process::id(),
            version,
        });
    });
}

/// Stops the loopback listener and removes the discovery file.
///
/// Best effort, called on the quit path. The `pid` in the file lets a CLI
/// cope with the file surviving a crash, so a failure to delete here is a
/// nuisance rather than a wrong answer.
pub fn stop_local_api(app: &tauri::AppHandle) {
    if let Some(server) = tauri::Manager::try_state::<LanServer>(app)
        && let Some(mut handle) = server.local.lock().take()
    {
        handle.stop();
        tracing::info!("local API stopped");
    }
    remove_discovery();
}

fn write_discovery(discovery: &LocalApiDiscovery) {
    let path = discovery_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec(discovery) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&path, bytes) {
                tracing::warn!(%error, ?path, "could not write the local API discovery file");
            }
        }
        Err(error) => tracing::warn!(%error, "could not serialise the local API discovery file"),
    }
}

fn remove_discovery() {
    let path = discovery_path();
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        // Already gone is the desired end state, not a failure.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, ?path, "could not remove the local API discovery file");
        }
    }
}

// ── Tokens on disk ─────────────────────────────────────────────────────

fn tokens_path() -> PathBuf {
    data_dir().join("lan-tokens.json")
}

/// Where an unreadable token file is moved aside so it is not silently lost.
fn backup_path(path: &std::path::Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".bak");
    path.with_file_name(name)
}

/// The temp file a save writes through before renaming it over the live file.
///
/// A sibling of the target, in the same directory, so the rename that follows
/// is atomic — `rename` cannot cross a filesystem, and a temp in `/tmp` would
/// silently become a copy. The PID keeps two writers from colliding on the
/// name; the rename is what serialises them.
fn temp_sibling(path: &std::path::Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

/// Writes `bytes` to `path` without ever exposing a half-written file.
///
/// A plain `std::fs::write` truncates the live file and then streams into it,
/// so a crash, a full disk or a killed process mid-write leaves a truncated
/// JSON document — and the token set is the only record of every paired
/// device. Writing a sibling temp first and renaming it over the target is
/// atomic on both NTFS and POSIX, so a reader sees the old file or the new
/// one and never a fragment.
fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_sibling(path);
    std::fs::write(&tmp, bytes)?;
    if let Err(error) = std::fs::rename(&tmp, path) {
        // Do not leave the temp behind when the rename failed: its whole
        // purpose is the atomic replace, and a stranded temp is litter.
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

/// Reads the token set. A file from before tokens were hashed is rewritten
/// hashed immediately, so no secret outlives the first launch of this build.
fn load_tokens() -> TokenSet {
    load_tokens_from(&tokens_path())
}

/// Reads the token set at `path`, preserving an unreadable file rather than
/// discarding it.
///
/// `unwrap_or_default()` here used to turn any parse error into "no tokens"
/// and the next save overwrote the file: one bad byte silently unpaired every
/// phone. A parse error now keeps the bytes as a `.bak` beside the live file
/// — the prefixes a user needs to recognise their pairings are still
/// recoverable — and only then starts from an empty set.
fn load_tokens_from(path: &std::path::Path) -> TokenSet {
    let Ok(bytes) = std::fs::read(path) else {
        return TokenSet::default();
    };
    match serde_json::from_slice::<TokenSet>(&bytes) {
        Ok(tokens) => {
            if bytes.windows(9).any(|w| w == b"\"secret\"") {
                save_tokens_to(path, &tokens);
            }
            tokens
        }
        Err(error) => {
            let bak = backup_path(path);
            tracing::warn!(%error, ?path, ?bak, "LAN token file is unreadable; keeping a .bak copy and starting fresh");
            if let Err(rename) = std::fs::rename(path, &bak) {
                tracing::warn!(%rename, ?path, "could not keep a .bak copy of the unreadable token file");
            }
            TokenSet::default()
        }
    }
}

/// Writes the token set. Failure is logged, not surfaced: the tokens still
/// work for this session, and the user will simply have to pair again after a
/// restart — a nuisance, not a reason to refuse the pairing.
fn save_tokens(tokens: &TokenSet) {
    save_tokens_to(&tokens_path(), tokens);
}

fn save_tokens_to(path: &std::path::Path, tokens: &TokenSet) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec_pretty(tokens) {
        Ok(bytes) => {
            if let Err(error) = write_atomically(path, &bytes) {
                tracing::warn!(%error, "could not persist LAN tokens");
            }
        }
        Err(error) => tracing::warn!(%error, "could not serialise LAN tokens"),
    }
}

// ── The controller ─────────────────────────────────────────────────────

/// Routes control requests through the same actions the UI uses.
#[derive(Debug, Clone, Copy)]
struct DesktopController;

impl Controller for DesktopController {
    #[cfg(windows)]
    fn apply(&self, request: ControlRequest) -> Result<(), ControlError> {
        use vitals_win::actions;

        let outcome = match request {
            // Never confirmed: nobody saw the desktop's risk dialog. A
            // critical or protected target is refused by the live check.
            ControlRequest::Terminate { key } => {
                actions::terminate(key, 1, actions::Consent::Unconfirmed)
            }
            ControlRequest::Suspend { key } => actions::suspend(key, actions::Consent::Unconfirmed),
            ControlRequest::Resume { key } => actions::resume(key),
            ControlRequest::SetPriority { key, priority } => {
                let Some(priority) = parse_priority(&priority) else {
                    return Err(ControlError::Unsupported {
                        message: format!("unknown priority {priority:?}"),
                    });
                };
                // Realtime starves input handling and makes the machine look
                // frozen; the desktop menu labels it as such. A phone gets no
                // such warning, so it does not get the option.
                if matches!(priority, vitals_win::Priority::Realtime) {
                    return Err(ControlError::AccessDenied);
                }
                actions::set_priority(key, priority)
            }
            ControlRequest::SetEfficiencyMode { key, enabled } => {
                actions::set_efficiency_mode(key, enabled)
            }
        };

        outcome.map_err(|error| match error {
            // Refusing to end a critical process is not an error; it is the
            // risk model working. Reported as denied, like a real access
            // failure, so the phone shows the same "Windows protects this"
            // the desktop does.
            vitals_core::Error::AccessDenied { .. } | vitals_core::Error::Refused(_) => {
                ControlError::AccessDenied
            }
            vitals_core::Error::NotFound(_) => ControlError::NotFound,
            vitals_core::Error::Unsupported(capability) => ControlError::Unsupported {
                message: format!("{capability:?} is not available"),
            },
            other => ControlError::Internal {
                message: other.to_string(),
            },
        })
    }

    #[cfg(target_os = "macos")]
    fn apply(&self, request: ControlRequest) -> Result<(), ControlError> {
        use vitals_core::provider::Priority;

        let outcome = match request {
            ControlRequest::Terminate { key } => vitals_macos::terminate(key),
            ControlRequest::Suspend { key } => vitals_macos::suspend(key),
            ControlRequest::Resume { key } => vitals_macos::resume(key),
            ControlRequest::SetPriority { key, priority } => {
                let priority = match priority.as_str() {
                    "idle" => Priority::Idle,
                    "below-normal" => Priority::BelowNormal,
                    "normal" => Priority::Normal,
                    "above-normal" => Priority::AboveNormal,
                    "high" => Priority::High,
                    // Same reasoning as Windows: a phone gets no warning
                    // dialog, so it does not get the option.
                    "realtime" => return Err(ControlError::AccessDenied),
                    other => {
                        return Err(ControlError::Unsupported {
                            message: format!("unknown priority {other:?}"),
                        });
                    }
                };
                vitals_macos::set_priority(key, priority)
            }
            ControlRequest::SetEfficiencyMode { .. } => {
                return Err(ControlError::Unsupported {
                    message: "efficiency mode is a Windows feature".into(),
                });
            }
        };
        outcome.map_err(|error| match error {
            vitals_core::Error::AccessDenied { .. } | vitals_core::Error::Refused(_) => {
                ControlError::AccessDenied
            }
            vitals_core::Error::NotFound(_) => ControlError::NotFound,
            vitals_core::Error::Unsupported(capability) => ControlError::Unsupported {
                message: format!("{capability:?} is not available"),
            },
            other => ControlError::Internal {
                message: other.to_string(),
            },
        })
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn apply(&self, _request: ControlRequest) -> Result<(), ControlError> {
        Err(ControlError::Unsupported {
            message: "no process backend on this platform".into(),
        })
    }
}

#[cfg(windows)]
fn parse_priority(value: &str) -> Option<vitals_win::Priority> {
    use vitals_win::Priority as P;
    Some(match value {
        "idle" => P::Idle,
        "below-normal" => P::BelowNormal,
        "normal" => P::Normal,
        "above-normal" => P::AboveNormal,
        "high" => P::High,
        "realtime" => P::Realtime,
        _ => return None,
    })
}

// ── Commands ───────────────────────────────────────────────────────────

/// What the Settings panel shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanStatus {
    pub running: bool,
    pub port: Option<u16>,
    /// The address the listener actually bound, so the UI can show what a
    /// phone must reach. `None` when it listens on every interface (or is
    /// off) — the same fact `bound_adapter` reports.
    pub bound_address: Option<std::net::Ipv4Addr>,
    pub interfaces: Vec<Interface>,
    pub tokens: Vec<TokenSummary>,
}

// How much of a secret the UI shows: enough to tell two pairings apart in a
// list, far too little to use. `revoke_pairing` requires all of it.
use vitals_server::auth::PREFIX_CHARS;

/// A token as shown in the UI: everything except the secret.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSummary {
    /// Enough to identify it for revocation, not enough to use it.
    pub prefix: String,
    pub scope: Scope,
    pub label: String,
    pub created: i64,
}

impl From<&Token> for TokenSummary {
    fn from(t: &Token) -> Self {
        Self {
            prefix: t.prefix.clone(),
            scope: t.scope,
            label: t.label.clone(),
            created: t.created,
        }
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_lan_status(server: tauri::State<'_, LanServer>) -> LanStatus {
    LanStatus {
        // The LAN listener only. `is_running` also counts the always-on
        // loopback API, so this said "on" from the moment the app started:
        // the Settings switch showed Remote access enabled, clicking it
        // "stopped" a LAN server that was never running, and the user could
        // never actually turn it on. `is_running` stays the sampler's gate.
        running: server.port().is_some(),
        port: server.port(),
        bound_address: server.bound_adapter(),
        interfaces: vitals_server::interfaces(),
        tokens: server
            .tokens
            .read()
            .tokens
            .iter()
            .map(TokenSummary::from)
            .collect(),
    }
}

/// Starts listening. Idempotent: a second call while running returns the
/// existing port rather than binding a second socket.
///
/// `address` is the adapter the user picked in Settings. When given, the
/// socket is bound to **that address only**: a phone on the home Wi-Fi is
/// the audience, and listening on `0.0.0.0` also answered on the VPN, the
/// Hyper-V switch, a mobile hotspot and any other network the PC happened to
/// join. With no choice the best guess is bound instead — the same adapter
/// `create_pairing` puts in the QR — never `0.0.0.0`. Only a machine with no
/// LAN address at all falls back to an every-interface bind, the one case
/// where it is the only way to be reachable.
#[tauri::command]
pub async fn start_lan_server(
    app: tauri::AppHandle,
    server: tauri::State<'_, LanServer>,
    port: Option<u16>,
    address: Option<std::net::Ipv4Addr>,
) -> CommandResult<u16> {
    if let Some(existing) = server.port() {
        return Ok(existing);
    }

    // `loopback_scope: None` on purpose: this listener faces the network, and
    // a local caller who wants tokenless access has the loopback listener.
    let state = server.api_state(&app, Some(embedded_assets(app.clone())), None);

    let port = port.unwrap_or(DEFAULT_PORT);
    // Never `0.0.0.0` unless the machine genuinely has no LAN address: an
    // every-interface bind also answers on the VPN and the Hyper-V switch.
    let bind = bind_address_for(address, default_route_address());
    let bound = match bind {
        Some(adapter) => {
            vitals_server::serve_on(state, std::net::SocketAddr::from((adapter, port))).await
        }
        None => vitals_server::serve(state, port).await,
    };
    let handle = bound.map_err(|e| CommandError::Internal {
        message: format!("could not start the LAN server: {e}"),
    })?;

    let bound = handle.addr.port();
    *server.running.lock() = Some(handle);
    tracing::info!(port = bound, "LAN server listening");

    // Only now, with the socket actually bound: an advertisement for a server
    // that failed to start would point every phone at a closed port.
    start_advertising(
        &server,
        bind,
        bound,
        &app.package_info().version.to_string(),
    );

    Ok(bound)
}

/// The address the LAN listener binds.
///
/// `chosen` is what the user picked in Settings and always wins. With no
/// choice the best guess is the highest-scored interface — the same address
/// `create_pairing` puts in the QR — deliberately **not** `0.0.0.0`, because
/// binding every interface also exposes the server on the VPN, the Hyper-V
/// switch and any hotspot the machine has joined. `None` only when there is
/// no LAN address at all, where every-interface is the only way to be reached.
fn bind_address_for(
    chosen: Option<std::net::Ipv4Addr>,
    best: Option<std::net::Ipv4Addr>,
) -> Option<std::net::Ipv4Addr> {
    chosen.or(best)
}

/// The highest-scored adapter's address, or `None` when there is no candidate.
fn default_route_address() -> Option<std::net::Ipv4Addr> {
    vitals_server::interfaces().first().map(|i| i.address)
}

/// The IPv4 address a listener is bound to, or `None` when it listens on
/// every interface. Split out so the rule is testable without a socket.
fn lan_bound_adapter(addr: std::net::SocketAddr) -> Option<std::net::Ipv4Addr> {
    match addr.ip() {
        std::net::IpAddr::V4(v4) if !v4.is_unspecified() => Some(v4),
        _ => None,
    }
}

/// Announces the running server over `mDNS`, or explains why it could not.
///
/// Discovery is a convenience: a failure here leaves the server perfectly
/// usable through the QR code, so it is logged and never propagated.
fn start_advertising(
    server: &tauri::State<'_, LanServer>,
    address: Option<std::net::Ipv4Addr>,
    port: u16,
    app_version: &str,
) {
    let chosen = address.or_else(|| vitals_server::interfaces().first().map(|i| i.address));
    let Some(chosen) = chosen else {
        tracing::warn!("no LAN address to advertise on; discovery is unavailable");
        return;
    };

    match vitals_server::advertise(chosen, port, app_version) {
        Ok(advertisement) => {
            tracing::info!(name = advertisement.fullname(), %chosen, "advertising over mDNS");
            *server.advertisement.lock() = Some(advertisement);
        }
        // Multicast is commonly blocked on locked-down networks. The server
        // still works; only automatic discovery is lost.
        Err(error) => tracing::warn!(%error, "could not advertise over mDNS"),
    }
}

/// Stops listening and withdraws the `mDNS` record.
///
/// The advertisement is torn down **first**: a phone that resolves the record
/// during the gap would otherwise connect to a socket that is already closing.
/// Nothing is broadcast once this returns — that is the whole point of the
/// feature being off by default.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn stop_lan_server(server: tauri::State<'_, LanServer>) {
    if let Some(mut advertisement) = server.advertisement.lock().take() {
        advertisement.shutdown();
    }
    if let Some(mut handle) = server.running.lock().take() {
        handle.stop();
        tracing::info!("LAN server stopped");
    }
    // A code outliving the switch would still be redeemable over the
    // loopback listener, and would be waiting when sharing is turned back on.
    server.pairing.cancel();
}

/// A freshly minted pairing: the QR the phone scans and the URL behind it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    pub url: String,
    pub qr_svg: String,
    pub token: TokenSummary,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub label: String,
    pub scope: Scope,
    /// The adapter address the user chose. `None` picks the best guess.
    pub address: Option<std::net::Ipv4Addr>,
}

/// Creates a token and renders its QR code.
///
/// The secret is embedded in the SVG and the URL and returned once. The
/// desktop never shows it again: the summary that goes into the token list
/// carries only a prefix, so a screenshot of Settings does not pair a device.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn create_pairing(
    server: tauri::State<'_, LanServer>,
    request: PairingRequest,
) -> CommandResult<Pairing> {
    let Some(port) = server.port() else {
        return Err(CommandError::Unsupported {
            message: "start the LAN server before pairing".into(),
        });
    };

    // A listener bound to one adapter answers on that address and no other,
    // so a QR naming any other address is a pairing that cannot connect.
    let bound = server.bound_adapter();
    if let (Some(bound), Some(chosen)) = (bound, request.address)
        && bound != chosen
    {
        return Err(CommandError::Unsupported {
            message: format!(
                "remote access is listening on {bound} only; stop and start it on {chosen} \
                 to pair through that address"
            ),
        });
    }
    let address = match request.address.or(bound) {
        Some(chosen) => chosen,
        None => vitals_server::interfaces()
            .first()
            .map(|i| i.address)
            .ok_or_else(|| CommandError::Unsupported {
                message: "this machine has no LAN address a phone could reach".into(),
            })?,
    };

    let secret = vitals_server::auth::generate_secret();
    let token = Token::new(&secret, request.scope, request.label, now_secs());
    let url = vitals_server::pairing_url(address, port, &secret);
    let qr_svg = vitals_server::pairing_qr_svg(&url).map_err(|e| CommandError::Internal {
        message: format!("QR: {e}"),
    })?;

    let summary = TokenSummary::from(&token);
    {
        let mut tokens = server.tokens.write();
        tokens.tokens.push(token);
        save_tokens(&tokens);
    }

    Ok(Pairing {
        url,
        qr_svg,
        token: summary,
    })
}

/// Shows a six-digit code a TV can exchange for a token of `scope` at
/// `POST /api/v1/pair`, replacing any code already shown.
///
/// Refused while the LAN server is off, like [`create_pairing`]: a code for
/// a server nobody on the network can reach is a promise that cannot be kept.
/// The code is returned to the webview and never logged.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn create_pairing_code(
    server: tauri::State<'_, LanServer>,
    scope: Scope,
) -> CommandResult<IssuedCode> {
    if server.port().is_none() {
        return Err(CommandError::Unsupported {
            message: "start the LAN server before pairing".into(),
        });
    }
    Ok(server.pairing.create(scope))
}

/// Withdraws the code on screen. Harmless when there is none.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn cancel_pairing_code(server: tauri::State<'_, LanServer>) {
    server.pairing.cancel();
}

/// Whether the code on screen can still be used. Settings polls this: a
/// code that turns inactive before its expiry was redeemed by the TV.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn pairing_code_status(server: tauri::State<'_, LanServer>) -> PairingStatus {
    server.pairing.status()
}

/// Revokes one token by its prefix, as shown in the list.
///
/// Exactly one: `retain(!starts_with(prefix))` with an empty or short prefix
/// revoked every pairing, and the UI has a separate, deliberately labelled
/// "revoke all" for that. A prefix shorter than the eight characters the
/// list shows, or one matching several tokens, revokes nothing.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn revoke_pairing(server: tauri::State<'_, LanServer>, prefix: String) {
    let mut tokens = server.tokens.write();
    if revoke_by_prefix(&mut tokens, &prefix) {
        save_tokens(&tokens);
    } else {
        tracing::warn!("revoke_pairing refused: prefix does not identify exactly one token");
    }
}

/// Removes the one token `prefix` identifies. Returns whether it did.
fn revoke_by_prefix(tokens: &mut TokenSet, prefix: &str) -> bool {
    if prefix.chars().count() < PREFIX_CHARS {
        return false;
    }
    let matching = tokens.tokens.iter().filter(|t| t.prefix == prefix).count();
    if matching != 1 {
        return false;
    }
    tokens.tokens.retain(|t| t.prefix != prefix);
    true
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn revoke_all_pairings(server: tauri::State<'_, LanServer>) {
    let mut tokens = server.tokens.write();
    tokens.revoke_all();
    save_tokens(&tokens);
}

// ── Helpers ────────────────────────────────────────────────────────────

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

// `Option` because the API state's signature is platform-neutral and other
// platforms genuinely have nothing to return.
#[allow(clippy::unnecessary_wraps)]
fn host_info() -> Option<vitals_core::provider::HostInfo> {
    #[cfg(windows)]
    {
        Some(vitals_win::hostinfo::read())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Serves the phone the same files the desktop webview loads.
///
/// Tauri embeds `frontendDist` in the binary and exposes it through the asset
/// resolver, so nothing is shipped twice and the mobile page can never be a
/// different build from the desktop it talks to.
fn embedded_assets(app: tauri::AppHandle) -> vitals_server::StaticAssets {
    Arc::new(move |path: &str| {
        let resolver = app.asset_resolver();
        let key = asset_key(path);
        resolver.get(key.clone()).map(|asset| {
            let mime: &'static str = vitals_server::router::mime_for(&key);
            (asset.bytes().to_vec(), mime)
        })
    })
}

/// The embedded key to serve for a request path.
///
/// A path that names a file is that file. Anything else is a route the
/// single-page app owns, and the page a phone gets is `mobile.html`.
///
/// Not `path` itself, because Tauri answers a key it does not hold with its
/// single-page fallback, and that fallback is the **desktop's** `index.html`:
/// a phone deep-linking a route received the desktop application, a different
/// bundle from the one it had loaded, over a link meant for a phone. The
/// server's router already knows how to tell a route from a file — this asks
/// it, so the two cannot drift.
fn asset_key(path: &str) -> String {
    let path = path.trim_start_matches('/');
    if vitals_server::router::is_file_path(path) {
        format!("/{path}")
    } else {
        "/mobile.html".to_owned()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn token(secret: &str) -> Token {
        Token::new(secret, Scope::Read, secret, 0)
    }

    #[test]
    fn the_local_api_never_grants_control_to_a_tokenless_caller() {
        // Another Windows account on the same PC reaches 127.0.0.1 too; a
        // Control grant here let it end this user's processes.
        assert_eq!(LOCAL_SCOPE, Scope::Read);
    }

    #[test]
    fn a_route_gets_the_phone_page_and_a_file_gets_itself() {
        // The defect this pins: Tauri's fallback for an unknown key is the
        // desktop's index.html, so a deep link from a phone used to render the
        // desktop bundle — which is why the resolver must ask for mobile.html
        // by name rather than let the fallback decide.
        for route in ["some/deep/link", "devices", "settings", ""] {
            assert_eq!(asset_key(route), "/mobile.html", "route {route:?}");
        }
        assert_eq!(asset_key("mobile.html"), "/mobile.html");
        assert_eq!(asset_key("index.html"), "/index.html");
        for file in [
            "assets/index-abc.js",
            "icon.png",
            "styles.css",
            "app.webmanifest",
        ] {
            assert_eq!(asset_key(file), format!("/{file}"), "file {file:?}");
        }
    }

    #[test]
    fn revoking_by_an_empty_or_short_prefix_removes_nothing() {
        // `retain(!starts_with(""))` is `retain(false)`: an empty prefix from
        // a UI glitch revoked every pairing at once.
        let mut set = TokenSet {
            tokens: vec![token("aaaaaaaa-one"), token("bbbbbbbb-two")],
        };
        assert!(!revoke_by_prefix(&mut set, ""));
        assert!(!revoke_by_prefix(&mut set, "aaa"));
        assert_eq!(set.tokens.len(), 2);
    }

    #[test]
    fn revoking_by_the_shown_prefix_removes_exactly_that_token() {
        let mut set = TokenSet {
            tokens: vec![token("aaaaaaaa-one"), token("bbbbbbbb-two")],
        };
        assert!(revoke_by_prefix(&mut set, "aaaaaaaa"));
        assert_eq!(set.tokens.len(), 1);
        assert_eq!(set.tokens[0].prefix, "bbbbbbbb");
    }

    #[test]
    fn an_ambiguous_prefix_revokes_nothing_rather_than_both() {
        let mut set = TokenSet {
            tokens: vec![token("aaaaaaaa-one"), token("aaaaaaaa-two")],
        };
        assert!(!revoke_by_prefix(&mut set, "aaaaaaaa"));
        assert_eq!(set.tokens.len(), 2);
    }

    #[test]
    fn the_discovery_file_has_the_exact_shape_the_cli_parses() {
        // `{"port":N,"pid":P,"version":"x.y.z"}` is the contract the CLI is
        // built against; a serde rename here would break `vitals ps` with
        // nothing failing to compile.
        let text = serde_json::to_string(&LocalApiDiscovery {
            port: 7330,
            pid: 4242,
            version: "1.2.3".into(),
        })
        .unwrap();
        assert_eq!(text, r#"{"port":7330,"pid":4242,"version":"1.2.3"}"#);

        let back: LocalApiDiscovery = serde_json::from_str(&text).unwrap();
        assert_eq!(back.port, 7330);
        assert_eq!(back.pid, 4242);
    }

    #[test]
    fn the_local_listener_counts_as_running_but_is_not_the_lan_port() {
        // The sampler publishes only while `is_running()`; if the loopback
        // listener did not count, the CLI would see 204 forever. Yet the
        // Settings panel's port must keep meaning the LAN listener only.
        let server = LanServer::new();
        assert!(!server.is_running());
        assert_eq!(server.port(), None);

        let handle = tauri::async_runtime::block_on(async {
            let state = ApiState {
                frames: server.frames.clone(),
                tokens: Arc::clone(&server.tokens),
                controller: Arc::clone(&server.controller),
                assets: None,
                host: Arc::new(|| None),
                alerts: Arc::new(Vec::new),
                history: Arc::new(|_| Vec::new()),
                sensors: Arc::new(Vec::new),
                connectivity: Arc::new(crate::commands::probe_connectivity),
                version: "test".into(),
                loopback_scope: Some(Scope::Control),
                pairing: None,
                agents: None,
            };
            vitals_server::serve_on(state, "127.0.0.1:0".parse().unwrap())
                .await
                .unwrap()
        });
        let port = handle.addr.port();
        *server.local.lock() = Some(handle);

        assert!(server.is_running());
        assert_eq!(server.port(), None, "the LAN port must not report loopback");
        // What `get_lan_status.running` is derived from: the Settings switch
        // must read "off" here, or remote access can never be turned on.
        assert!(
            server.port().is_none(),
            "remote access must not look enabled because loopback is up"
        );
        assert_eq!(server.local_port(), Some(port));

        if let Some(mut h) = server.local.lock().take() {
            h.stop();
        }
    }

    fn token_set(secrets: &[&str]) -> TokenSet {
        TokenSet {
            tokens: secrets.iter().map(|s| token(s)).collect(),
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vitals-tokens-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_token_save_is_written_through_a_temp_file_and_renamed_into_place() {
        // A plain write truncates the live file and streams into it, so a
        // crash mid-write leaves a truncated document — and the token set is
        // the only record of every paired device. The save must go through a
        // sibling temp and rename it over the target.
        let dir = temp_dir("atomic");
        let live = dir.join("lan-tokens.json");
        save_tokens_to(&live, &token_set(&["aaaaaaaa-one"]));
        let good = std::fs::read(&live).unwrap();

        // Plant the half-written file a crash would leave, where it would
        // land: in the temp. The live file must be untouched and still parse.
        let stray = temp_sibling(&live);
        std::fs::write(&stray, b"{\"tokens\": [{\"prefix\": \"aa").unwrap();
        assert_eq!(
            std::fs::read(&live).unwrap(),
            good,
            "a stray temp must not touch the live file"
        );
        assert!(serde_json::from_slice::<TokenSet>(&std::fs::read(&live).unwrap()).is_ok());

        // A normal save writes through that same temp path and renames it over
        // the live file, so the temp is gone afterwards — something a save
        // that wrote the live file directly could never achieve.
        save_tokens_to(&live, &token_set(&["bbbbbbbb-two"]));
        assert!(
            !stray.exists(),
            "the temp must have been renamed into place, not left behind"
        );
        let after: TokenSet = serde_json::from_slice(&std::fs::read(&live).unwrap()).unwrap();
        assert_eq!(after.tokens.len(), 1);
        assert_eq!(after.tokens[0].prefix, "bbbbbbbb");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_save_that_cannot_rename_leaves_the_previous_file_and_no_temp() {
        // The rename is the only step that touches the live file. When it
        // fails — here the destination is a directory — nothing may be
        // stranded and the previous file must survive intact.
        let dir = temp_dir("rename-fail");
        let blocked = dir.join("blocked.json");
        std::fs::create_dir(&blocked).unwrap();
        assert!(write_atomically(&blocked, b"{\"tokens\":[]}").is_err());
        assert!(
            std::fs::read_dir(&dir).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")),
            "a failed save must not leave a temp file behind"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unreadable_token_file_is_kept_as_a_bak_copy_rather_than_discarded() {
        // `unwrap_or_default` dropped the only record of every pairing and the
        // next save overwrote it: one bad byte silently unpaired every phone.
        let dir = temp_dir("corrupt");
        let live = dir.join("lan-tokens.json");
        std::fs::write(&live, b"this is not token json {[}").unwrap();

        let tokens = load_tokens_from(&live);
        assert!(tokens.tokens.is_empty());

        let bak = backup_path(&live);
        assert_eq!(
            std::fs::read(&bak).unwrap(),
            b"this is not token json {[}",
            "the unreadable file must be preserved for recovery"
        );
        assert!(
            !live.exists(),
            "it was moved aside so it is not re-read and lost again"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_readable_token_file_loads_its_tokens_and_is_not_backed_up() {
        // The guard above must not swallow a good file.
        let dir = temp_dir("good");
        let live = dir.join("lan-tokens.json");
        save_tokens_to(&live, &token_set(&["aaaaaaaa-one", "bbbbbbbb-two"]));
        let tokens = load_tokens_from(&live);
        assert_eq!(tokens.tokens.len(), 2);
        assert_eq!(tokens.tokens[0].prefix, "aaaaaaaa");
        assert!(
            !backup_path(&live).exists(),
            "a readable file is not backed up"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn with_no_choice_the_lan_server_binds_the_best_adapter_not_every_interface() {
        let best = std::net::Ipv4Addr::new(192, 168, 1, 20);
        assert_eq!(bind_address_for(None, Some(best)), Some(best));
        assert_ne!(
            bind_address_for(None, Some(best)),
            Some(std::net::Ipv4Addr::UNSPECIFIED),
            "the default bind must never be 0.0.0.0 when an adapter is known"
        );
        // The user's pick always wins over the guess.
        let chosen = std::net::Ipv4Addr::new(10, 0, 0, 5);
        assert_eq!(bind_address_for(Some(chosen), Some(best)), Some(chosen));
        // Only a machine with no LAN address at all falls back to every
        // interface, which `serve` then binds as 0.0.0.0.
        assert_eq!(bind_address_for(None, None), None);
    }

    #[test]
    fn the_bound_address_accessor_reports_the_adapter_and_never_every_interface() {
        assert_eq!(
            lan_bound_adapter("192.168.1.20:7331".parse().unwrap()),
            Some(std::net::Ipv4Addr::new(192, 168, 1, 20))
        );
        assert_eq!(lan_bound_adapter("0.0.0.0:7331".parse().unwrap()), None);
    }

    #[test]
    fn the_status_carries_the_address_the_listener_is_bound_to() {
        let status = LanStatus {
            running: true,
            port: Some(7331),
            bound_address: Some(std::net::Ipv4Addr::new(192, 168, 1, 20)),
            interfaces: Vec::new(),
            tokens: Vec::new(),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["boundAddress"], "192.168.1.20");

        // Every-interface: null, and the UI falls back to the port alone.
        let unbound = LanStatus {
            running: true,
            port: Some(7331),
            bound_address: None,
            interfaces: Vec::new(),
            tokens: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(&unbound).unwrap()["boundAddress"],
            serde_json::Value::Null
        );
    }
}
