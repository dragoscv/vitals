//! Background updates: check shortly after launch and every twelve hours
//! after, download quietly, install when the user quits.
//!
//! Startup is never delayed — the check runs after the window is up and the
//! sampler is ticking. The signed installer is verified as it downloads
//! (minisign, `plugins.updater.pubkey`) and held in memory; on `Exit` it is
//! handed to NSIS in passive mode, so the next launch is the new version.
//! Nothing is installed while the user is looking at the app, and an update
//! that fails verification is discarded without a word to anyone.
//!
//! On Windows the install on quit must also be one nobody has to approve
//! (DEV-3, ADR 0038): a UAC prompt raised as the app vanishes is answered by
//! no one, and the update silently never happens. `vitals_win::install`
//! decides per copy — a per-user copy updates as the invoker, a copy someone
//! chose to put in Program Files waits for "Restart to update", where the
//! prompt follows a click.

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

/// How long between checks after the first.
///
/// A system monitor runs for weeks from the tray; checking only at launch
/// meant a machine that is never rebooted never learned of a fix. Twice a
/// day is one small HTTPS request each time — nothing a user could notice —
/// and a security fix still reaches everyone within a day.
const RECHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(12 * 60 * 60);

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
    // A plain thread rather than a task: it sleeps for twelve hours at a
    // time, which would pin a worker of the blocking pool for as long. A
    // parked thread costs a stack and nothing else.
    let _ = std::thread::Builder::new()
        .name("vitals-updates".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                // With one already downloaded there is nothing newer to
                // want until it is installed on quit. Skipped rather than
                // ending the loop: turning the setting off drops the
                // download, and turning it back on must find it again.
                if !update_ready(&app)
                    && let Err(error) = tauri::async_runtime::block_on(fetch(&app))
                {
                    tracing::info!(%error, "background update check did not complete");
                }
                std::thread::sleep(RECHECK_EVERY);
            }
        });
}

fn update_ready(app: &tauri::AppHandle) -> bool {
    app.try_state::<PendingInstall>()
        .is_some_and(|p| p.ready.lock().is_some())
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
        crate::tray::set_update_ready(app, true);
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
pub fn set_auto_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, PendingInstall>,
    enabled: bool,
) {
    pending.enabled.store(enabled, Ordering::Relaxed);
    if !enabled {
        pending.ready.lock().take();
        crate::tray::set_update_ready(&app, false);
    }
}

/// The scope switch (`/CurrentUser` or `/AllUsers`) every setup this
/// process starts carries, so the updater never guesses which half of an
/// `installMode: both` install to update. Also what the plugin builder in
/// `lib.rs` is given, which covers the Settings panel's own download.
#[must_use]
pub fn scope_args() -> Vec<&'static str> {
    #[cfg(windows)]
    {
        vitals_win::install::installer_args(
            vitals_win::install::current_scope(),
            vitals_win::actions::is_elevated(),
        )
        .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// What quitting would do with the update that is waiting, for the
/// Settings panel and the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuitInstall {
    /// Installs on quit, with no prompt.
    Silent,
    /// Needs administrator approval: offered as "Restart to update".
    NeedsApproval,
    /// This copy is not one the updater owns (portable, Scoop, a build).
    NotOurs,
}

fn quit_install() -> QuitInstall {
    #[cfg(windows)]
    {
        use vitals_win::install::{UpdatePlan, current_scope, plan};
        match plan(current_scope(), vitals_win::actions::is_elevated()) {
            UpdatePlan::Install { .. } => QuitInstall::Silent,
            UpdatePlan::NeedsApproval => QuitInstall::NeedsApproval,
            UpdatePlan::NotOurs => QuitInstall::NotOurs,
        }
    }
    #[cfg(not(windows))]
    {
        QuitInstall::Silent
    }
}

/// How the waiting update will be installed.
#[tauri::command]
pub async fn get_update_install_mode() -> QuitInstall {
    quit_install()
}

/// Called by the Settings panel right before it asks the plugin to install
/// what it downloaded: the same no-prompt preparation as the install on
/// quit, and the same outcome marker. A copy that needs approval gets no
/// preparation — its prompt now follows the click, which is the point.
#[tauri::command]
pub async fn prepare_update_install(version: String) {
    prepare(&version);
}

fn prepare(version: &str) {
    #[cfg(windows)]
    {
        use vitals_win::install::{UpdatePlan, current_scope, plan};
        if let UpdatePlan::Install {
            as_invoker: true, ..
        } = plan(current_scope(), vitals_win::actions::is_elevated())
            && let Err(error) = vitals_win::install::run_children_as_invoker()
        {
            tracing::warn!(%error, "the setup may ask for administrator approval");
        }
    }
    outcome::expect(version);
}

/// Installs the waiting update now, raising the UAC prompt in front of the
/// person who clicked — the path for a copy that cannot update on quit.
/// The app exits on success; the setup restarts it.
#[tauri::command]
pub async fn install_update_now(
    app: tauri::AppHandle,
) -> Result<(), crate::commands::CommandError> {
    // Blocking: the UAC prompt is on screen until it is answered.
    tauri::async_runtime::spawn_blocking(move || install_now(&app))
        .await
        .map_err(|e| crate::commands::CommandError::Internal {
            message: e.to_string(),
        })?
        .map_err(|message| crate::commands::CommandError::Refused { message })
}

/// "Restart to update", from the Settings panel or the tray. Returns only on
/// failure: on success the plugin exits the process and the setup restarts
/// it. A declined prompt keeps the download, so the offer stays.
///
/// # Errors
///
/// A sentence for the user when nothing is downloaded or the setup did not
/// start.
pub fn install_now(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(pending) = app.try_state::<PendingInstall>() else {
        return Err("no update is downloaded".into());
    };
    let Some((update, bytes)) = pending.ready.lock().take() else {
        return Err("no update is downloaded".into());
    };
    prepare(&update.version);
    stop_for_install(app);
    let Err(error) = update.install(&bytes) else {
        return Ok(());
    };
    let message = error.to_string();
    tracing::warn!(%message, "restart to update did not start the setup");
    outcome::failed(&update.version, &message);
    *pending.ready.lock() = Some((update, bytes));
    Err(format!("the update was not installed: {message}"))
}

/// The sampler and servers stop before the setup replaces the binary, as on
/// quit.
fn stop_for_install(app: &tauri::AppHandle) {
    if let Some(sampler) = app.try_state::<crate::sampling::SamplerHandle>() {
        sampler.stop_and_wait(std::time::Duration::from_secs(2));
    }
    crate::server::stop_local_api(app);
    crate::ipc::stop(app);
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
    if pending.ready.lock().is_none() {
        return;
    }
    #[cfg(windows)]
    {
        use vitals_win::install::{UpdatePlan, current_scope, plan};
        match plan(current_scope(), vitals_win::actions::is_elevated()) {
            UpdatePlan::Install { as_invoker, .. } => {
                if as_invoker && let Err(error) = vitals_win::install::run_children_as_invoker() {
                    // The setup would raise a prompt nobody sees: keep the
                    // download for next time rather than lose it to a 1223.
                    tracing::warn!(%error, "not installing on quit: cannot avoid a UAC prompt");
                    return;
                }
            }
            UpdatePlan::NeedsApproval => {
                tracing::info!(
                    "update waiting; this copy needs approval, offered as Restart to update"
                );
                return;
            }
            UpdatePlan::NotOurs => {
                tracing::info!("update waiting; this copy is not an installation the updater owns");
                return;
            }
        }
    }
    let Some((update, bytes)) = pending.ready.lock().take() else {
        return;
    };
    let version = update.version.clone();
    outcome::expect(&version);
    if let Err(error) = update.install(bytes) {
        tracing::warn!(%error, "the downloaded update could not be installed");
        outcome::failed(&version, &error.to_string());
    }
}

/// What became of the last install attempt, read once at the next launch.
///
/// A failed install on quit used to leave one line in a log nobody reads;
/// the next launch was simply the old version. A marker written before the
/// setup starts, and compared with the running version afterwards, turns
/// both "it failed" and "it never ran" into something the app can say.
pub mod outcome {
    use std::path::PathBuf;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Marker {
        /// The version the install was meant to produce.
        pub version: String,
        /// Why the setup did not start, when it did not. Empty when it
        /// started and the outcome is decided by the version that runs next.
        #[serde(default)]
        pub error: String,
    }

    /// What the next launch reports.
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
    #[serde(rename_all = "camelCase", tag = "kind")]
    pub enum Outcome {
        Installed { version: String },
        Failed { version: String, error: String },
    }

    /// Pure: the marker against the version now running.
    #[must_use]
    pub fn judge(marker: &Marker, running: &str) -> Outcome {
        if marker.error.is_empty() && marker.version == running {
            Outcome::Installed {
                version: marker.version.clone(),
            }
        } else {
            Outcome::Failed {
                version: marker.version.clone(),
                error: marker.error.clone(),
            }
        }
    }

    fn path() -> PathBuf {
        crate::state::data_dir().join("update-attempt.json")
    }

    fn write(marker: &Marker) {
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(marker) {
            let _ = std::fs::write(path, text);
        }
    }

    pub(super) fn expect(version: &str) {
        write(&Marker {
            version: version.to_owned(),
            error: String::new(),
        });
    }

    pub(super) fn failed(version: &str, error: &str) {
        write(&Marker {
            version: version.to_owned(),
            error: error.to_owned(),
        });
    }

    /// Reads and removes the marker: each outcome is reported once.
    #[must_use]
    pub fn take(running: &str) -> Option<Outcome> {
        let path = path();
        let text = std::fs::read_to_string(&path).ok()?;
        let _ = std::fs::remove_file(&path);
        let marker = serde_json::from_str::<Marker>(&text).ok()?;
        Some(judge(&marker, running))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn marker(version: &str, error: &str) -> Marker {
            Marker {
                version: version.into(),
                error: error.into(),
            }
        }

        #[test]
        fn running_the_expected_version_is_reported_as_installed() {
            assert_eq!(
                judge(&marker("1.0.4", ""), "1.0.4"),
                Outcome::Installed {
                    version: "1.0.4".into()
                }
            );
        }

        #[test]
        fn a_setup_that_never_replaced_the_binary_is_a_failure_even_without_an_error() {
            // The 1223 case before DEV-3: the setup started, the prompt went
            // unanswered, the old version came back with nothing logged here.
            assert_eq!(
                judge(&marker("1.0.4", ""), "1.0.3"),
                Outcome::Failed {
                    version: "1.0.4".into(),
                    error: String::new()
                }
            );
        }

        #[test]
        fn a_setup_that_did_not_start_is_a_failure_with_its_reason() {
            assert_eq!(
                judge(&marker("1.0.4", "os error 1223"), "1.0.3"),
                Outcome::Failed {
                    version: "1.0.4".into(),
                    error: "os error 1223".into()
                }
            );
        }
    }
}

#[cfg(test)]
mod config_guard {
    //! The shape DEV-3's fix depends on, pinned. Each of these was a
    //! working setting that someone could reasonably "tidy"; each change
    //! brings back an update that silently never installs.

    fn conf() -> serde_json::Value {
        let text = include_str!("../tauri.conf.json");
        serde_json::from_str(text).unwrap_or(serde_json::Value::Null)
    }

    #[test]
    fn the_installer_offers_both_scopes_so_a_per_user_copy_exists_to_update_unprompted() {
        // perMachine forces every update through UAC (os error 1223 when
        // nobody answers); currentUser would orphan every 1.0.0-1.0.3 copy.
        assert_eq!(
            conf()["bundle"]["windows"]["nsis"]["installMode"],
            "both",
            "ADR 0038"
        );
    }

    #[test]
    fn the_updater_never_adds_its_own_scope_switch_on_top_of_ours() {
        // installerArgs in the config would be appended to every setup next
        // to the /CurrentUser or /AllUsers chosen at run time, and NSIS
        // takes the last one it parses.
        let windows = &conf()["plugins"]["updater"]["windows"];
        assert!(windows.get("installerArgs").is_none());
        assert_eq!(windows["installMode"], "passive");
    }

    #[test]
    fn the_hooks_still_give_a_migrated_copy_its_shortcuts() {
        let hooks = include_str!("../windows/hooks.nsh");
        assert!(hooks.contains(r#"$MultiUser.InstallMode == "CurrentUser""#));
        assert!(hooks.contains(r#"CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk""#));
    }
}

/// The result of the last install attempt, once, at launch.
#[tauri::command]
pub async fn take_update_outcome(app: tauri::AppHandle) -> Option<outcome::Outcome> {
    outcome::take(&app.package_info().version.to_string())
}

/// A Program Files copy left behind after this one moved to a per-user
/// install, if any — the Settings panel offers to remove it.
#[tauri::command]
pub async fn get_leftover_install() -> Option<String> {
    #[cfg(windows)]
    {
        vitals_win::install::leftover_machine_install().map(|p| p.display().to_string())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Removes that copy through one elevated child (one UAC prompt, after a
/// click). The child reads the folder from `HKLM` itself.
#[tauri::command]
pub async fn remove_leftover_install() -> Result<(), crate::commands::CommandError> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let code = vitals_win::actions::taskmgr::run_elevated(
                vitals_win::install::REMOVE_MACHINE_INSTALL_ARG,
                "the old copy was left where it is",
            )?;
            if code == 0 {
                Ok(())
            } else {
                Err(crate::commands::CommandError::Internal {
                    message: format!("the old copy could not be removed (code {code})"),
                })
            }
        })
        .await
        .map_err(|e| crate::commands::CommandError::Internal {
            message: e.to_string(),
        })?
    }
    #[cfg(not(windows))]
    {
        Err(crate::commands::CommandError::Unsupported {
            message: "only Windows has a per-machine install to remove".into(),
        })
    }
}
