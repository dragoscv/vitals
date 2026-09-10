//! The LAN server, as the desktop app hosts it.
//!
//! `vitals-server` knows nothing about Tauri; this module is the glue. It owns
//! the on/off state, persists the pairing tokens, feeds frames from the
//! sampler, and implements [`Controller`] on top of the same `vitals_win`
//! actions the context menu uses — so "end task from the phone" and "end task
//! from the right-click menu" are the same function with the same risk
//! checks.
//!
//! **Off by default.** Nothing here binds a socket until `start_lan_server`
//! is called, and it is called only when the user flips the switch.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use vitals_server::state::ServerLock;
use vitals_server::{
    Advertisement, ApiState, ControlError, ControlRequest, Controller, FrameSource, Interface,
    Scope, ServeHandle, Token, TokenSet,
};

use crate::commands::CommandError;
use crate::state::data_dir;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// The default port. Unassigned by IANA, unlikely to collide, and easy to
/// type by hand if the QR scan fails.
pub const DEFAULT_PORT: u16 = 7331;

/// Managed by Tauri: everything about the server that outlives a request.
pub struct LanServer {
    frames: FrameSource,
    tokens: Arc<ServerLock<TokenSet>>,
    running: Mutex<Option<ServeHandle>>,
    /// The `mDNS` record, held beside the listener rather than inside it so
    /// the two can only ever be started and stopped together.
    advertisement: Mutex<Option<Advertisement>>,
    controller: Arc<dyn Controller>,
}

impl std::fmt::Debug for LanServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanServer")
            .field("running", &self.running.lock().is_some())
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
            advertisement: Mutex::new(None),
            controller: Arc::new(DesktopController),
        }
    }

    /// Called by the sampler every tick. Cheap when nothing is listening.
    pub fn publish(&self, frame: Arc<vitals_core::sample::Frame>) {
        self.frames.publish(frame);
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.lock().is_some()
    }

    fn port(&self) -> Option<u16> {
        self.running.lock().as_ref().map(|h| h.addr.port())
    }
}

// ── Tokens on disk ─────────────────────────────────────────────────────

fn tokens_path() -> PathBuf {
    data_dir().join("lan-tokens.json")
}

fn load_tokens() -> TokenSet {
    std::fs::read(tokens_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes the token set. Failure is logged, not surfaced: the tokens still
/// work for this session, and the user will simply have to pair again after a
/// restart — a nuisance, not a reason to refuse the pairing.
fn save_tokens(tokens: &TokenSet) {
    let path = tokens_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec_pretty(tokens) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&path, bytes) {
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
            ControlRequest::Terminate { key } => actions::terminate(key, 1),
            ControlRequest::Suspend { key } => actions::suspend(key),
            ControlRequest::Resume { key } => actions::resume(key),
            ControlRequest::SetPriority { key, priority } => {
                let Some(priority) = parse_priority(&priority) else {
                    return Err(ControlError::Unsupported {
                        message: format!("unknown priority {priority:?}"),
                    });
                };
                actions::set_priority(key, priority)
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

    #[cfg(not(windows))]
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
    pub interfaces: Vec<Interface>,
    pub tokens: Vec<TokenSummary>,
}

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
            prefix: t.secret.chars().take(8).collect(),
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
        running: server.is_running(),
        port: server.port(),
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
/// `address` is the adapter the user picked in Settings; `None` takes the
/// best guess, the same one `create_pairing` puts in the QR code, so the
/// advertised address and the paired address cannot disagree.
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

    let state = ApiState {
        frames: server.frames.clone(),
        tokens: Arc::clone(&server.tokens),
        controller: Arc::clone(&server.controller),
        assets: Some(embedded_assets(app.clone())),
        host: Arc::new(host_info),
        alerts: {
            let app = app.clone();
            Arc::new(move || {
                tauri::Manager::try_state::<crate::alerts::Alerts>(&app)
                    .map(|a| a.active())
                    .unwrap_or_default()
            })
        },
        version: app.package_info().version.to_string(),
    };

    let handle = vitals_server::serve(state, port.unwrap_or(DEFAULT_PORT))
        .await
        .map_err(|e| CommandError::Internal {
            message: format!("could not start the LAN server: {e}"),
        })?;

    let bound = handle.addr.port();
    *server.running.lock() = Some(handle);
    tracing::info!(port = bound, "LAN server listening");

    // Only now, with the socket actually bound: an advertisement for a server
    // that failed to start would point every phone at a closed port.
    start_advertising(
        &server,
        address,
        bound,
        &app.package_info().version.to_string(),
    );

    Ok(bound)
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

    let address = match request.address {
        Some(chosen) => chosen,
        None => vitals_server::interfaces()
            .first()
            .map(|i| i.address)
            .ok_or_else(|| CommandError::Unsupported {
                message: "this machine has no LAN address a phone could reach".into(),
            })?,
    };

    let token = Token {
        secret: vitals_server::auth::generate_secret(),
        scope: request.scope,
        label: request.label,
        created: now_secs(),
    };
    let url = vitals_server::pairing_url(address, port, &token.secret);
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

/// Revokes one token by its prefix, as shown in the list.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn revoke_pairing(server: tauri::State<'_, LanServer>, prefix: String) {
    let mut tokens = server.tokens.write();
    tokens.tokens.retain(|t| !t.secret.starts_with(&prefix));
    save_tokens(&tokens);
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
        let key = format!("/{path}");
        resolver.get(key.clone()).map(|asset| {
            let mime: &'static str = vitals_server::router::mime_for(&key);
            (asset.bytes().to_vec(), mime)
        })
    })
}
