//! Start-with-Windows, by whichever route works for this installation.
//!
//! The direct install writes `HKCU\…\Run` through the autostart plugin. The
//! Store package cannot — its registry writes are virtualised and never read
//! at logon — so it toggles the `windows.startupTask` its manifest declares.
//! One pair of commands hides the difference from the webview.

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

#[cfg(windows)]
fn startup_task(on: Option<bool>) -> CommandResult<bool> {
    use vitals_win::startup_task::{APP_TASK, is_enabled, set};
    match on {
        None => is_enabled(APP_TASK).ok_or_else(|| CommandError::Unsupported {
            message: "the startup task is missing from the package manifest".into(),
        }),
        Some(on) => set(APP_TASK, on).map_err(|e| CommandError::Internal {
            message: format!("Windows did not change the startup task: {e}"),
        }),
    }
}

#[cfg(not(windows))]
fn startup_task(_on: Option<bool>) -> CommandResult<bool> {
    Err(CommandError::Unsupported {
        message: "startup tasks exist only on Windows".into(),
    })
}

fn plugin(app: &tauri::AppHandle, on: Option<bool>) -> CommandResult<bool> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let fail = |e: tauri_plugin_autostart::Error| CommandError::Internal {
        message: e.to_string(),
    };
    match on {
        None => launcher.is_enabled().map_err(fail),
        Some(true) => launcher
            .enable()
            .and_then(|()| launcher.is_enabled())
            .map_err(fail),
        Some(false) => launcher
            .disable()
            .and_then(|()| launcher.is_enabled())
            .map_err(fail),
    }
}

fn route(app: &tauri::AppHandle, on: Option<bool>) -> CommandResult<bool> {
    if crate::distribution::packaged() {
        startup_task(on)
    } else {
        plugin(app, on)
    }
}

/// Whether Vitals starts at logon.
#[tauri::command]
pub async fn get_autostart(app: tauri::AppHandle) -> CommandResult<bool> {
    // Off the IPC thread: the startup-task read is an async WinRT call joined here.
    tauri::async_runtime::spawn_blocking(move || route(&app, None))
        .await
        .map_err(|e| CommandError::Internal {
            message: e.to_string(),
        })?
}

/// Turns start-at-logon on or off and returns what Windows settled on, which
/// can differ: the user may have disabled the entry in Task Manager.
#[tauri::command]
pub async fn set_autostart(app: tauri::AppHandle, enabled: bool) -> CommandResult<bool> {
    tauri::async_runtime::spawn_blocking(move || route(&app, Some(enabled)))
        .await
        .map_err(|e| CommandError::Internal {
            message: e.to_string(),
        })?
}
