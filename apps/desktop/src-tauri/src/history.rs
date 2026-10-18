//! App history commands.
//!
//! Unlike the high-frequency sampler metrics that are pushed, history is polled
//! — it changes slowly and is only wanted while the History screen is open.
//! The frontend calls `get_app_history` on mount and after a refresh, and
//! `clear_app_history` when the user asks to reset it.

use serde::Serialize;

use crate::commands::CommandError;
#[cfg(windows)]
use crate::state::AppState;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// Per-executable resource usage DTO, serialised for the webview.
///
/// `SystemTime` cannot cross the bridge, so timestamps are flattened to
/// milliseconds since the Unix epoch. Milliseconds rather than a formatted
/// string because formatting is a display concern and the frontend already
/// knows the user's locale — sending ISO text would bake an English-shaped
/// decision into the wire format.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppHistoryRecordDto {
    pub executable: String,
    pub name: String,
    pub cpu_seconds: f64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub peak_private_bytes: u64,
    /// Milliseconds since the Unix epoch.
    pub first_seen: u64,
    /// Milliseconds since the Unix epoch.
    pub last_seen: u64,
    pub sessions: u32,
}

/// Snapshot of all app history.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppHistorySnapshot {
    pub records: Vec<AppHistoryRecordDto>,
}

/// Milliseconds since the Unix epoch, saturating at 0 for times before it.
///
/// A pre-epoch timestamp can only mean a corrupt history file or a clock that
/// was wound back; clamping is honest enough for a "first seen" column and
/// avoids propagating a signed time nobody downstream is prepared for.
#[cfg(windows)]
fn to_millis(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Reads the accumulated app history.
///
/// Cheap — a few hundred records copied out of shared state — but still not
/// on the sampler tick, because it is only interesting while the App history
/// screen is open.
#[tauri::command]
#[cfg(windows)]
// `State` is a handle, and Tauri's command macro requires it by value.
#[allow(clippy::needless_pass_by_value)]
pub fn get_app_history(state: tauri::State<'_, AppState>) -> CommandResult<AppHistorySnapshot> {
    Ok(AppHistorySnapshot {
        records: state
            .history_snapshot()
            .into_iter()
            .map(|record| AppHistoryRecordDto {
                executable: record.executable.display().to_string(),
                name: record.name,
                cpu_seconds: record.cpu_seconds,
                disk_read_bytes: record.disk_read_bytes,
                disk_write_bytes: record.disk_write_bytes,
                peak_private_bytes: record.peak_private_bytes,
                first_seen: to_millis(record.first_seen),
                last_seen: to_millis(record.last_seen),
                sessions: record.sessions,
            })
            .collect(),
    })
}

/// Clears all accumulated app history.
///
/// Permanent, and written through to disk immediately — the frontend confirms
/// with the user before calling this.
///
/// Async and on the blocking pool (ENG2-14): it writes the history file and
/// waits on the history lock the sampler holds during a flush, neither of
/// which belongs on the UI thread.
#[tauri::command]
#[cfg(windows)]
pub async fn clear_app_history(app: tauri::AppHandle) -> CommandResult<()> {
    off_main(app, AppState::clear_history).await
}

/// Turns accumulation on or off, mirroring the "Record history" setting.
///
/// The frontend sends this on hydration and on every change. Until the first
/// call arrives the store records nothing: a user who has never opened
/// Settings has the default (off), and a tally that started before their
/// preference was known would be recording without consent.
///
/// Async for the same reason as [`clear_app_history`]: turning it off saves
/// the file.
#[tauri::command]
#[cfg(windows)]
pub async fn set_history_enabled(app: tauri::AppHandle, enabled: bool) -> CommandResult<()> {
    off_main(app, move |state| state.set_history_enabled(enabled)).await
}

/// Runs `work` against the managed [`AppState`] on the blocking pool.
#[cfg(windows)]
async fn off_main(
    app: tauri::AppHandle,
    work: impl FnOnce(&AppState) + Send + 'static,
) -> CommandResult<()> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(state) = app.try_state::<AppState>() {
            work(&state);
        }
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the history change was abandoned: {err}"),
    })
}

/// Placeholder stubs for non-Windows platforms.
///
/// The app is Windows-only, but the frontend might be previewed in a browser
/// or on another OS during development. These stubs let it compile and return
/// empty data rather than failing at the bridge.
#[tauri::command]
#[cfg(not(windows))]
pub fn get_app_history() -> CommandResult<AppHistorySnapshot> {
    Ok(AppHistorySnapshot {
        records: Vec::new(),
    })
}

#[tauri::command]
#[cfg(not(windows))]
pub fn clear_app_history() -> CommandResult<()> {
    Ok(())
}

#[tauri::command]
#[cfg(not(windows))]
pub fn set_history_enabled(_enabled: bool) -> CommandResult<()> {
    Ok(())
}
