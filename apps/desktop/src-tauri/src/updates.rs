//! Background updates: check shortly after launch, download quietly, install
//! when the user quits.
//!
//! Startup is never delayed — the check runs after the window is up and the
//! sampler is ticking. The signed installer is verified as it downloads
//! (minisign, `plugins.updater.pubkey`) and held in memory; on `Exit` it is
//! handed to NSIS in passive mode, so the next launch is the new version.
//! Nothing is installed while the user is looking at the app, and an update
//! that fails verification is discarded without a word to anyone.

use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use tauri::Manager;

/// A downloaded, verified update waiting for the app to quit.
///
/// `enabled` mirrors the "Install updates automatically" setting. It starts
/// `true` because the webview pushes the stored value long before the first
/// check fires ([`FIRST_CHECK_DELAY`]); a user who turned it off is never
/// checked for.
pub struct PendingInstall {
    ready: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
    enabled: AtomicBool,
}

impl Default for PendingInstall {
    fn default() -> Self {
        Self {
            ready: Mutex::new(None),
            enabled: AtomicBool::new(true),
        }
    }
}

impl std::fmt::Debug for PendingInstall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingInstall")
            .field("ready", &self.ready.lock().is_some())
            .field("enabled", &self.enabled.load(Ordering::Relaxed))
            .finish()
    }
}

/// How long after launch to wait before the first check, so the network
/// request never competes with the first paint.
const FIRST_CHECK_DELAY: std::time::Duration = std::time::Duration::from_secs(20);

/// Starts the background check. No-op in debug builds, which have no
/// installer to replace.
pub fn spawn(app: &tauri::AppHandle) {
    // The Store build is updated by the Store; replacing our own files there
    // is forbidden and would fail against the read-only package directory.
    if cfg!(debug_assertions)
        || crate::distribution::packaged()
        || std::env::var_os("VITALS_NO_AUTO_UPDATE").is_some()
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ =
            tauri::async_runtime::spawn_blocking(|| std::thread::sleep(FIRST_CHECK_DELAY)).await;
        if let Err(error) = fetch(&app).await {
            tracing::info!(%error, "background update check did not complete");
        }
    });
}

async fn fetch(app: &tauri::AppHandle) -> Result<(), tauri_plugin_updater::Error> {
    use tauri_plugin_updater::UpdaterExt;
    let enabled = || {
        app.try_state::<PendingInstall>()
            .is_some_and(|p| p.enabled.load(Ordering::Relaxed))
    };
    if !enabled() {
        tracing::info!("automatic updates are off; not checking");
        return Ok(());
    }
    let Some(update) = app.updater()?.check().await? else {
        tracing::info!("up to date");
        return Ok(());
    };
    tracing::info!(version = %update.version, "update found; downloading in the background");
    let bytes = update.download(|_, _| {}, || {}).await?;
    if let Some(pending) = app.try_state::<PendingInstall>() {
        *pending.ready.lock() = Some((update, bytes));
        tracing::info!("update verified; it installs when Vitals quits");
    }
    Ok(())
}

/// The version downloaded and waiting for the app to quit, if any.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_pending_update(pending: tauri::State<'_, PendingInstall>) -> Option<String> {
    pending
        .ready
        .lock()
        .as_ref()
        .map(|(u, _)| u.version.clone())
}

/// Pushed from the webview on hydration and whenever the setting changes.
/// Turning it off also drops an update already downloaded: "off" must mean
/// nothing gets installed, not "nothing more gets downloaded".
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_auto_update(pending: tauri::State<'_, PendingInstall>, enabled: bool) {
    pending.enabled.store(enabled, Ordering::Relaxed);
    if !enabled {
        pending.ready.lock().take();
    }
}

/// Called on `Exit`. Installs the verified update, if one is waiting.
pub fn install_on_exit(app: &tauri::AppHandle) {
    if crate::distribution::packaged() {
        return;
    }
    let Some(pending) = app.try_state::<PendingInstall>() else {
        return;
    };
    if !pending.enabled.load(Ordering::Relaxed) {
        return;
    }
    let Some((update, bytes)) = pending.ready.lock().take() else {
        return;
    };
    if let Err(error) = update.install(bytes) {
        tracing::warn!(%error, "the downloaded update could not be installed");
    }
}
