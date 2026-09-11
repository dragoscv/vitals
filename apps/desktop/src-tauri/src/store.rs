//! Commands over the time-series store and flight recorder.
//!
//! Readers open their own connection: SQLite in WAL mode is happy with one
//! writer (the sampler thread) and any number of readers, and a connection is
//! not `Sync`, so sharing the sampler's would mean a lock the sampler waits
//! on. Opening costs about a millisecond and these are user-initiated.

use serde::Serialize;
use vitals_store::{MachineSample, RetentionPolicy, Store};

use crate::commands::CommandError;
use crate::state::{AppState, store_path};

type CommandResult<T> = std::result::Result<T, CommandError>;

fn open_reader() -> CommandResult<Store> {
    Store::open(&store_path(), RetentionPolicy::default()).map_err(|e| CommandError::Internal {
        message: format!("history store: {e}"),
    })
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Applies the "keep data for N days" setting.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_retention_days(state: tauri::State<'_, AppState>, days: u32) -> CommandResult<()> {
    state.set_retention_days(days);
    Ok(())
}

/// Machine-wide samples covering the last `seconds`, at the resolution the
/// retention policy assigns to that span.
///
/// Returns an empty list — not an error — when history is off or the file
/// does not exist yet; the chart shows "no history recorded" either way.
#[tauri::command]
pub fn query_machine_history(seconds: u32) -> CommandResult<Vec<MachineSample>> {
    if !store_path().exists() {
        return Ok(Vec::new());
    }
    let store = open_reader()?;
    let now = now_secs();
    let from = now.saturating_sub(i64::from(seconds));
    store
        .query(from, now, now)
        .map_err(|e| CommandError::Internal {
            message: format!("history query: {e}"),
        })
}

/// How much the store occupies, for the Settings screen.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryUsage {
    pub bytes: u64,
    pub fine_rows: u64,
    pub flight_frames: u64,
}

#[tauri::command]
pub fn get_history_usage() -> CommandResult<HistoryUsage> {
    if !store_path().exists() {
        return Ok(HistoryUsage {
            bytes: 0,
            fine_rows: 0,
            flight_frames: 0,
        });
    }
    let store = open_reader()?;
    let map = |e: vitals_store::StoreError| CommandError::Internal {
        message: format!("history usage: {e}"),
    };
    Ok(HistoryUsage {
        bytes: store.size_bytes().map_err(map)?,
        fine_rows: store.count(vitals_store::Resolution::Second).map_err(map)?,
        flight_frames: u64::try_from(store.flight_frames().map_err(map)?.len()).unwrap_or(0),
    })
}

/// One frame of a flight recording, as the UI received it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlightFrame {
    pub seq: i64,
    pub ts: i64,
    /// The frame JSON, verbatim.
    pub frame: serde_json::Value,
}

/// A bundle suitable for attaching to a bug report.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlightRecording {
    pub app_version: String,
    pub exported_at: i64,
    pub host: Option<vitals_core::provider::HostInfo>,
    pub frames: Vec<FlightFrame>,
}

/// Everything the flight recorder holds, plus what the machine is.
///
/// The frames are the exact bytes the sampler emitted, so a reproduction from
/// this bundle shows what the reporter saw rather than a re-serialisation.
///
/// Not a command: the webview never wants a two-minute recording as a JSON
/// value, only written to a file, so it was registered for weeks and
/// unreachable. `write_flight_recording` is the one entry point.
fn export_flight_recording(app: &tauri::AppHandle) -> CommandResult<FlightRecording> {
    let frames = if store_path().exists() {
        open_reader()?
            .flight_frames()
            .map_err(|e| CommandError::Internal {
                message: format!("flight recorder: {e}"),
            })?
    } else {
        Vec::new()
    };

    let frames = frames
        .into_iter()
        .map(|(seq, ts, bytes)| FlightFrame {
            seq,
            ts,
            frame: serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        })
        .collect();

    #[cfg(windows)]
    let host = Some(vitals_win::hostinfo::read());
    #[cfg(not(windows))]
    let host = None;

    Ok(FlightRecording {
        app_version: app.package_info().version.to_string(),
        exported_at: now_secs(),
        host,
        frames,
    })
}

/// Writes the recording to `path`, which the user chose in a save dialog.
///
/// Done here rather than in the webview so the filesystem plugin does not
/// need write access to arbitrary paths, and so a two-minute recording is
/// serialised once instead of crossing the IPC boundary and back.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn write_flight_recording(app: tauri::AppHandle, path: String) -> CommandResult<()> {
    let recording = export_flight_recording(&app)?;
    let json = serde_json::to_vec_pretty(&recording).map_err(|e| CommandError::Internal {
        message: format!("serialising the recording: {e}"),
    })?;
    std::fs::write(&path, json).map_err(|e| CommandError::Internal {
        message: format!("writing {path}: {e}"),
    })
}
