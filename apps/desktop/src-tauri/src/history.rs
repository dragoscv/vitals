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
#[tauri::command]
#[cfg(windows)]
#[allow(clippy::needless_pass_by_value)]
pub fn clear_app_history(state: tauri::State<'_, AppState>) -> CommandResult<()> {
    state.clear_history();
    Ok(())
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
