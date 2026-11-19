//! MQTT publishing, as the desktop app hosts it.
//!
//! The Settings screen owns the switch and the broker address. This module
//! owns the two things it cannot: the publisher's lifetime, and the password.
//! The password lives in the OS secret store beside the Health page's token
//! (Credential Manager on Windows, the Keychain on macOS) rather than in the
//! settings file, because a broker password in `settings.json` would be
//! readable by anything that can read the profile and would end up in a
//! backup.
//!
//! **Off until the user turns it on.** `start_mqtt` opens the connection and
//! starts the publish loop; until it is called, no socket is opened and no
//! timer runs. That is the same rule the LAN server follows.
//!
//! The publish loop itself lives in `vitals_server::mqtt` and the crate that
//! renders a sample into readings is `vitals_mqtt`; nothing is re-implemented
//! here, so the desktop and `vitals serve --mqtt` publish identical material.

use parking_lot::Mutex;
use serde::Serialize;
use vitals_mqtt::MqttConfig;
use vitals_server::mqtt::MqttHandle;

use crate::commands::CommandError;
use crate::server::LanServer;

type CommandResult<T> = std::result::Result<T, CommandError>;

// ── The password, in the OS secret store ───────────────────────────────
//
// Kept beside `health.rs`'s token stores on purpose: one uninstall step
// (`windows/hooks.nsh`) removes the same targets.

#[cfg(windows)]
const PASSWORD_TARGET: &str = "vitals/mqtt-password";
#[cfg(windows)]
const CREDENTIAL_USER: &str = "mqtt";

#[cfg(windows)]
fn password_write(secret: &str) -> Result<(), String> {
    vitals_win::credentials::write(PASSWORD_TARGET, CREDENTIAL_USER, secret)
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn password_read() -> Result<Option<String>, String> {
    vitals_win::credentials::read(PASSWORD_TARGET).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn password_delete() -> Result<(), String> {
    vitals_win::credentials::delete(PASSWORD_TARGET).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "app.vitals.mqtt";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "password";
/// `errSecItemNotFound`: "nothing stored" is an answer, not a failure.
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
fn password_write(secret: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        KEYCHAIN_ACCOUNT,
        secret.as_bytes(),
    )
    .map_err(|e| format!("keychain: {}", e.code()))
}

#[cfg(target_os = "macos")]
fn password_read() -> Result<Option<String>, String> {
    match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
        Err(e) => Err(format!("keychain: {}", e.code())),
    }
}

#[cfg(target_os = "macos")]
fn password_delete() -> Result<(), String> {
    match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
    {
        Err(e) if e.code() != ERR_SEC_ITEM_NOT_FOUND => Err(format!("keychain: {}", e.code())),
        _ => Ok(()),
    }
}

/// No secret store this app trusts on other systems yet. The user is told
/// rather than silently losing the password — and a password in a plain file
/// would be readable by anything that can read the home directory. A broker
/// that needs no password still works everywhere.
#[cfg(not(any(windows, target_os = "macos")))]
const NO_STORE: &str =
    "Vitals has no secret store on this system yet, so the broker password cannot be saved here";

#[cfg(not(any(windows, target_os = "macos")))]
fn password_write(_secret: &str) -> Result<(), String> {
    Err(NO_STORE.into())
}

// `Result` for the same signature as the real stores; there is nothing to read.
#[cfg(not(any(windows, target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
fn password_read() -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(not(any(windows, target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
fn password_delete() -> Result<(), String> {
    Ok(())
}

/// Whether a password is stored. An unreadable store reads as "none": the
/// switch that matters is the publisher, and a store the OS refuses to open
/// is not something the Settings screen can fix.
fn password_is_set() -> bool {
    matches!(password_read(), Ok(Some(_)))
}

// ── State ──────────────────────────────────────────────────────────────

/// The running publisher, and why the last start attempt was refused.
///
/// Managed by Tauri. The handle is dropped — and the loop with it — when the
/// app exits, so a forgotten `stop_mqtt` cannot leave a task publishing.
#[derive(Debug, Default)]
pub struct Mqtt {
    handle: Mutex<Option<MqttHandle>>,
    /// The last refusal, kept so `get_mqtt_status` can explain a start that
    /// failed before the panel opened.
    error: Mutex<Option<String>>,
}

impl Mqtt {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn running(&self) -> bool {
        self.handle.lock().is_some()
    }
}

/// What the Settings panel shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MqttStatus {
    /// Whether the publish loop is up. Not whether the broker is *reachable*:
    /// `rumqttc` connects asynchronously and retries, so the loop is running
    /// from the moment it is spawned.
    pub running: bool,
    /// The last refused start, if any. `None` when nothing has failed.
    pub error: Option<String>,
    /// Whether a password is stored for the broker.
    pub password_set: bool,
}

/// Reads the current status, off the IPC thread.
///
/// The password flag costs a Credential Manager / Keychain round trip, which
/// is why it is not read inline: a blocking OS call on the async runtime is
/// the same defect as one on the main thread, just harder to see.
async fn status(mqtt: &Mqtt) -> MqttStatus {
    let running = mqtt.running();
    let error = mqtt.error.lock().clone();
    let password_set = tauri::async_runtime::spawn_blocking(password_is_set)
        .await
        .unwrap_or(false);
    MqttStatus {
        running,
        error,
        password_set,
    }
}

// ── Commands ───────────────────────────────────────────────────────────

/// Reports whether the publisher is running and whether a password is stored.
#[tauri::command]
pub async fn get_mqtt_status(state: tauri::State<'_, Mqtt>) -> CommandResult<MqttStatus> {
    Ok(status(&state).await)
}

/// Starts publishing to `url`, or restarts on a new address or password.
///
/// Called by the Settings switch and once at launch when the switch was left
/// on. Any existing loop is stopped first: "start" is also how a changed
/// password takes effect, and a second client on the same client id would
/// fight the first for the broker session.
///
/// # Errors
/// [`CommandError::Refused`] when the address cannot be parsed — the message
/// is the crate's, so a bad scheme or port reads the same here as it does on
/// the command line. [`CommandError::Internal`] when the secret store cannot
/// be read.
#[tauri::command]
pub async fn start_mqtt(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mqtt>,
    server: tauri::State<'_, LanServer>,
    url: String,
    username: Option<String>,
) -> CommandResult<MqttStatus> {
    // The guard must not live across the `await`: a `parking_lot` guard is not
    // `Send`, so holding it over a suspension point does not compile.
    let existing = state.handle.lock().take();
    if let Some(handle) = existing {
        handle.shutdown().await;
    }

    // The webview never sends the password; it is read here so a secret cannot
    // reach the settings file by construction rather than by convention.
    let password = tauri::async_runtime::spawn_blocking(password_read)
        .await
        .map_err(|error| CommandError::Internal {
            message: error.to_string(),
        })?
        .map_err(|message| CommandError::Internal { message })?;

    let host = crate::host_name();
    let mut config = MqttConfig {
        enabled: true,
        url,
        // An empty field is "no username", not a username that is the empty
        // string — the crate would otherwise send it as one.
        username: username.filter(|name| !name.is_empty()),
        password,
        ..MqttConfig::default()
    };
    // Filled here as well as inside `spawn` so the one log line names the
    // node; `prepare` is idempotent, which is what lets it be called twice.
    config.prepare(&host);
    let summary = config.describe();

    let api = server.api_state(&app, None, None);
    match vitals_server::mqtt::spawn(config, api, &host) {
        Ok(Some(handle)) => {
            *state.handle.lock() = Some(handle);
            *state.error.lock() = None;
            tracing::info!(broker = %summary, "MQTT publishing started");
        }
        Ok(None) => {
            // Unreachable while `enabled` is true and an address was given,
            // but reporting a start that did not happen would be a lie.
            return Err(refuse(&state, "no broker address was given".to_owned()));
        }
        Err(error) => return Err(refuse(&state, error.to_string())),
    }
    Ok(status(&state).await)
}

/// Stops publishing. Idempotent.
#[tauri::command]
pub async fn stop_mqtt(state: tauri::State<'_, Mqtt>) -> CommandResult<MqttStatus> {
    let existing = state.handle.lock().take();
    if let Some(handle) = existing {
        handle.shutdown().await;
    }
    *state.error.lock() = None;
    Ok(status(&state).await)
}

/// Stores — or with `None`/an empty string, removes — the broker password.
///
/// Returns whether a password is now stored, so the panel can update its hint
/// without a second round trip.
///
/// # Errors
/// [`CommandError::Internal`] when the OS secret store refuses the write.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn set_mqtt_password(password: Option<String>) -> CommandResult<bool> {
    tauri::async_runtime::spawn_blocking(move || match password {
        Some(secret) if !secret.is_empty() => password_write(&secret),
        _ => password_delete(),
    })
    .await
    .map_err(|error| CommandError::Internal {
        message: error.to_string(),
    })?
    .map_err(|message| CommandError::Internal { message })?;
    Ok(tauri::async_runtime::spawn_blocking(password_is_set)
        .await
        .unwrap_or(false))
}

/// Records a refusal so the panel can explain it later, and hands it back.
fn refuse(state: &tauri::State<'_, Mqtt>, message: String) -> CommandError {
    *state.error.lock() = Some(message.clone());
    // `Refused`, not `Internal`: a bad address is the user's to fix, and the
    // UI must not offer "retry as administrator" for it.
    CommandError::Refused { message }
}
