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

/// Runs store I/O on the blocking pool.
///
/// A sync command runs on the main thread, which is also the window's event
/// loop: a history query against a cold, multi-hundred-megabyte file, or a
/// two-minute flight recording written to a network share, froze every
/// window for as long as the disk took.
async fn off_main<T, F>(what: &'static str, work: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CommandResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the {what} was abandoned: {err}"),
        })?
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
///
/// `command(async)` rather than an `async fn` with `spawn_blocking`: the LAN
/// server calls this directly as a plain function from its own threads, and
/// an `async fn` would hand it a future. The attribute keeps the function
/// synchronous for that caller while Tauri runs the command off the main
/// thread, which is the part that mattered.
#[tauri::command(async)]
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
// Tauri injects the handle by value; it is only read here.
#[allow(clippy::needless_pass_by_value)]
pub async fn get_history_usage(app: tauri::AppHandle) -> CommandResult<HistoryUsage> {
    let flight_frames = flight_frames(&app).len();
    off_main("history usage read", move || {
        read_history_usage().map(|usage| HistoryUsage {
            flight_frames: u64::try_from(flight_frames).unwrap_or(0),
            ..usage
        })
    })
    .await
}

/// The flight recording, from the sampler's in-memory ring.
///
/// Empty before the sampler starts, which is the truth: nothing has been
/// recorded yet.
fn flight_frames(app: &tauri::AppHandle) -> Vec<vitals_store::FlightEntry> {
    use tauri::Manager;
    app.try_state::<crate::sampling::SamplerHandle>()
        .map(|sampler| sampler.flight().frames())
        .unwrap_or_default()
}

fn read_history_usage() -> CommandResult<HistoryUsage> {
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
        flight_frames: 0,
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
///
/// Infallible since the recording moved into memory: there is no file to
/// fail to open.
fn export_flight_recording(app: &tauri::AppHandle) -> FlightRecording {
    let frames = flight_frames(app)
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

    FlightRecording {
        app_version: app.package_info().version.to_string(),
        exported_at: now_secs(),
        host,
        frames,
    }
}

/// Writes the recording to `path`, which the user chose in a save dialog.
///
/// Done here rather than in the webview so the filesystem plugin does not
/// need write access to arbitrary paths, and so a two-minute recording is
/// serialised once instead of crossing the IPC boundary and back.
#[tauri::command]
pub async fn write_flight_recording(app: tauri::AppHandle, path: String) -> CommandResult<()> {
    off_main("flight recording export", move || {
        let recording = export_flight_recording(&app);
        let json = serde_json::to_vec_pretty(&recording).map_err(|e| CommandError::Internal {
            message: format!("serialising the recording: {e}"),
        })?;
        std::fs::write(&path, json).map_err(|e| CommandError::Internal {
            message: format!("writing {path}: {e}"),
        })
    })
    .await
}
