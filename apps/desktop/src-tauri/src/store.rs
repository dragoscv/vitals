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

/// Writes the recording to a file the user picks in a native save dialog.
///
/// The dialog is shown **here, in Rust**, and the path never crosses the IPC
/// boundary: a compromised webview can ask for an export but cannot name the
/// destination, so it cannot be made to write anywhere it likes — the old
/// signature took a `path: String` straight from the webview. Doing the write
/// on this side also means a two-minute recording is serialised once, not
/// crossing the IPC boundary and back.
///
/// Returns the chosen path, or `None` when the user cancelled — a normal
/// outcome, not an error.
#[tauri::command]
pub async fn write_flight_recording(app: tauri::AppHandle) -> CommandResult<Option<String>> {
    off_main("flight recording export", move || {
        let Some(path) = pick_recording_path(&app)? else {
            return Ok(None);
        };
        let recording = export_flight_recording(&app);
        write_recording(&path, &recording)?;
        Ok(Some(path.display().to_string()))
    })
    .await
}

/// Shows the save dialog and returns the file the user chose.
///
/// The default name is built here, not passed in, so the webview cannot steer
/// the destination. `None` means the dialog was cancelled. The blocking API
/// is for exactly this context — it must not run on the main thread, which is
/// why it is called from `off_main`'s blocking worker.
fn pick_recording_path(app: &tauri::AppHandle) -> CommandResult<Option<std::path::PathBuf>> {
    use tauri_plugin_dialog::DialogExt as _;
    let picked = app
        .dialog()
        .file()
        .set_file_name(format!("vitals-recording-{}.json", now_secs()))
        .add_filter("JSON", &["json"])
        .blocking_save_file();
    match picked {
        None => Ok(None),
        Some(file) => file
            .into_path()
            .map(Some)
            .map_err(|e| CommandError::Internal {
                message: format!("the save dialog returned an unusable path: {e}"),
            }),
    }
}

/// Serialises the recording and writes it to `path`, which Rust chose.
fn write_recording(path: &std::path::Path, recording: &FlightRecording) -> CommandResult<()> {
    let json = serde_json::to_vec_pretty(recording).map_err(|e| CommandError::Internal {
        message: format!("serialising the recording: {e}"),
    })?;
    std::fs::write(path, json).map_err(|e| CommandError::Internal {
        message: format!("writing {}: {e}", path.display()),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_flight_recording_command_takes_no_destination_from_the_webview() {
        // Pins the security property at compile time: the destination comes
        // from the native save dialog in Rust, so the command's only input is
        // the app handle. Reintroducing the `path: String` parameter this
        // closes stops this compiling.
        fn only_app_handle<F, Fut>(_: F)
        where
            F: FnOnce(tauri::AppHandle) -> Fut,
        {
        }
        only_app_handle(write_flight_recording);
    }

    #[test]
    fn a_recording_written_to_a_rust_chosen_path_is_readable_json() {
        let dir = std::env::temp_dir().join(format!("vitals-flight-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("recording.json");
        let recording = FlightRecording {
            app_version: "9.9.9".into(),
            exported_at: 1_700_000_000,
            host: None,
            frames: vec![FlightFrame {
                seq: 1,
                ts: 2,
                frame: serde_json::json!({ "cpu": 12.5 }),
            }],
        };
        write_recording(&path, &recording).unwrap();
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(back["appVersion"], "9.9.9");
        assert_eq!(back["frames"][0]["frame"]["cpu"], 12.5);
        std::fs::remove_dir_all(&dir).ok();
    }
}
