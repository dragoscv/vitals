//! Background updates: check shortly after launch and every twelve hours
//! after, download quietly, install when the user quits — or while nobody is
//! at the machine and Vitals is only in the tray.
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
//!
//! Why not on quit alone (2026-10-18): a tray monitor almost never quits.
//! On the owner's machine the only "quits" in two weeks were Windows
//! restarts; each started the setup three seconds after the restart began,
//! the session ended under it, and 1.0.0, 1.0.1 and 1.0.2 were downloaded,
//! verified and lost while 0.9.0 kept running — with the watchdog fixes those
//! versions carried. So the setup now runs once the user has been away for
//! [`IDLE_BEFORE_INSTALL`] with no Vitals window open, relaunches the app
//! into the tray, and never starts while Windows is ending the session.

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
    /// Labels of the windows the pre-install hook hid, so a setup that did
    /// not start can put back exactly what the user was looking at.
    hidden: Mutex<Vec<String>>,
}

impl Default for PendingInstall {
    fn default() -> Self {
        Self {
            ready: Mutex::new(None),
            enabled: AtomicBool::new(true),
            hidden: Mutex::new(Vec::new()),
        }
    }
}

impl std::fmt::Debug for PendingInstall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingInstall")
            .field("ready", &self.ready.lock().is_some())
            .field("enabled", &self.enabled.load(Ordering::Relaxed))
            .field("hidden", &*self.hidden.lock())
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

/// How often the waiting update looks for a quiet moment to install.
const IDLE_POLL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// How long nobody may have touched keyboard or mouse before the setup runs.
/// Long enough that a person reading something is not interrupted; short
/// enough that a lunch break is plenty.
const IDLE_BEFORE_INSTALL: u64 = 10 * 60;

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
    // A plain thread rather than a task: it sleeps between polls, which would
    // pin a worker of the blocking pool for as long. A parked thread costs a
    // stack and nothing else.
    let _ = std::thread::Builder::new()
        .name("vitals-updates".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            let mut last_check: Option<std::time::Instant> = None;
            loop {
                // With one already downloaded there is nothing newer to
                // want until it is installed. Skipped rather than
                // ending the loop: turning the setting off drops the
                // download, and turning it back on must find it again.
                let due = last_check.is_none_or(|t| t.elapsed() >= RECHECK_EVERY);
                if !update_ready(&app) && due {
                    last_check = Some(std::time::Instant::now());
                    if let Err(error) = tauri::async_runtime::block_on(fetch(&app)) {
                        tracing::info!(%error, "background update check did not complete");
                    }
                }
                if update_ready(&app) {
                    install_if_idle(&app);
                }
                std::thread::sleep(IDLE_POLL);
            }
        });
}

fn update_ready(app: &tauri::AppHandle) -> bool {
    app.try_state::<PendingInstall>()
        .is_some_and(|p| p.ready.lock().is_some())
}

/// What the idle install looks at, gathered in one place so the rule is pure.
#[derive(Debug, Clone, Copy)]
struct Moment {
    /// Seconds since the last input; `None` when Windows cannot say.
    idle_secs: Option<u64>,
    /// Any Vitals window on screen.
    window_visible: bool,
    /// Windows is logging off, restarting or shutting down.
    session_ending: bool,
    /// "Install updates automatically" is on and the install needs no
    /// approval (`QuitInstall::Silent`): the two settings-side conditions,
    /// one field because neither changes between polls on its own terms.
    permitted: bool,
}

/// Whether now is a moment to replace the app under nobody's nose.
///
/// An unknown idle time is not an absence, and a visible window means
/// someone may be reading it even without touching anything.
fn idle_install_due(m: Moment) -> bool {
    m.permitted
        && !m.window_visible
        && !m.session_ending
        && m.idle_secs.is_some_and(|s| s >= IDLE_BEFORE_INSTALL)
}

fn moment(app: &tauri::AppHandle) -> Moment {
    #[cfg(windows)]
    let (idle_secs, session_ending) = (
        vitals_win::input::idle_secs(),
        vitals_win::input::session_ending(),
    );
    #[cfg(not(windows))]
    let (idle_secs, session_ending) = (None, false);
    Moment {
        idle_secs,
        window_visible: app
            .webview_windows()
            .values()
            .any(|w| w.is_visible().unwrap_or(false)),
        session_ending,
        permitted: app
            .try_state::<PendingInstall>()
            .is_some_and(|p| p.enabled.load(Ordering::Relaxed))
            && quit_install() == QuitInstall::Silent,
    }
}

/// Installs the waiting update when nobody is there, relaunching into the
/// tray. On failure everything is put back by [`install_now`] and the next
/// poll tries again.
fn install_if_idle(app: &tauri::AppHandle) {
    let m = moment(app);
    if !idle_install_due(m) {
        return;
    }
    tracing::info!(idle_secs = ?m.idle_secs, "installing the waiting update while the PC is idle");
    quiet_relaunch::arm();
    if let Err(message) = install_now(app) {
        quiet_relaunch::disarm();
        tracing::warn!(%message, "idle install did not start; trying again later");
    }
}

/// A launch started by the setup after an idle install stays in the tray.
///
/// The setup relaunches with the arguments this process had, so a copy the
/// user opened by hand would come back with its window up — on a screen
/// nobody asked to change. A marker file left just before the setup starts
/// tells the next launch to behave as if it had been autostarted; it is read
/// once and removed, and ignored when older than the install could take.
pub mod quiet_relaunch {
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::time::{Duration, SystemTime};

    /// Long enough for a passive NSIS setup on a slow disk; short enough that
    /// a marker left by a setup that never ran cannot hide a launch tomorrow.
    const VALID_FOR: Duration = Duration::from_secs(15 * 60);

    fn path() -> PathBuf {
        crate::state::data_dir().join("relaunch-quietly")
    }

    pub(super) fn arm() {
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, b"");
    }

    pub(super) fn disarm() {
        let _ = std::fs::remove_file(path());
    }

    /// Pure: whether a marker written at `written` still applies at `now`.
    #[must_use]
    pub fn fresh(written: SystemTime, now: SystemTime) -> bool {
        now.duration_since(written)
            .is_ok_and(|age| age <= VALID_FOR)
    }

    /// Whether this launch is the setup's relaunch after an idle install.
    /// Decided once per process, because the marker is consumed.
    #[must_use]
    pub fn requested() -> bool {
        static ANSWER: OnceLock<bool> = OnceLock::new();
        *ANSWER.get_or_init(|| {
            let path = path();
            let written = std::fs::metadata(&path).and_then(|m| m.modified());
            let _ = std::fs::remove_file(&path);
            written.is_ok_and(|w| fresh(w, SystemTime::now()))
        })
    }
}

/// The updater every download in this app goes through.
///
/// With our own pre-exit hook rather than the plugin's default, which runs
/// Tauri's `cleanup_before_exit`: that removes the tray icon for good and
/// hides every window, and it runs *before* the setup is started — so when
/// the start then failed (a declined UAC prompt, os error 1223) the app
/// went on running invisibly, with no tray and nothing sampling (ENG2-1).
/// Ours hides instead, and [`resume_after_failed_install`] undoes it.
fn updater(
    app: &tauri::AppHandle,
) -> Result<tauri_plugin_updater::Updater, tauri_plugin_updater::Error> {
    use tauri_plugin_updater::UpdaterExt;
    let hook_app = app.clone();
    app.updater_builder()
        .on_before_exit(move || hide_for_install(&hook_app))
        .build()
}

/// Hides the tray icon and every visible window, remembering which, right
/// before the setup starts. Reversible on purpose; see [`updater`].
fn hide_for_install(app: &tauri::AppHandle) {
    let mut hidden = Vec::new();
    for (label, window) in app.webview_windows() {
        if window.is_visible().unwrap_or(false) && window.hide().is_ok() {
            hidden.push(label);
        }
    }
    if let Some(pending) = app.try_state::<PendingInstall>() {
        *pending.hidden.lock() = hidden;
    }
    crate::tray::set_visible(app, false);
}

async fn fetch(app: &tauri::AppHandle) -> Result<(), tauri_plugin_updater::Error> {
    let enabled = || {
        app.try_state::<PendingInstall>()
            .is_some_and(|p| p.enabled.load(Ordering::Relaxed))
    };
    if !enabled() {
        tracing::info!("automatic updates are off; not checking");
        return Ok(());
    }
    let Some(update) = updater(app)?.check().await? else {
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

/// Progress of [`download_update`], in the shape the panel already reads
/// from the plugin's own download.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", content = "data", rename_all_fields = "camelCase")]
pub enum DownloadEvent {
    Started { content_length: Option<u64> },
    Progress { chunk_length: usize },
    Finished,
}

/// The Settings panel's "Download update": downloads into the same slot the
/// background check fills, so installing it is [`install_now`] with its
/// teardown and its restart on failure — never the plugin's own `install`,
/// which stopped nothing and restarted nothing (ENG2-5, DEV-6).
///
/// Returns the version downloaded. Refuses when the server now offers a
/// different version from the one the user was shown.
#[tauri::command]
pub async fn download_update(
    app: tauri::AppHandle,
    version: String,
    on_event: tauri::ipc::Channel<DownloadEvent>,
) -> Result<String, crate::commands::CommandError> {
    let internal = |e: tauri_plugin_updater::Error| crate::commands::CommandError::Internal {
        message: e.to_string(),
    };
    let update = updater(&app)
        .map_err(internal)?
        .check()
        .await
        .map_err(internal)?
        .ok_or_else(|| crate::commands::CommandError::NotFound {
            message: "there is no update to download any more".into(),
        })?;
    if update.version != version {
        return Err(crate::commands::CommandError::Refused {
            message: format!(
                "the update on offer changed from {version} to {}; check again",
                update.version
            ),
        });
    }
    let mut first = true;
    let bytes = update
        .download(
            |chunk_length, content_length| {
                if first {
                    first = false;
                    let _ = on_event.send(DownloadEvent::Started { content_length });
                }
                let _ = on_event.send(DownloadEvent::Progress { chunk_length });
            },
            || {
                let _ = on_event.send(DownloadEvent::Finished);
            },
        )
        .await
        .map_err(internal)?;
    let downloaded = update.version.clone();
    if let Some(pending) = app.try_state::<PendingInstall>() {
        *pending.ready.lock() = Some((update, bytes));
    }
    crate::tray::set_update_ready(&app, true);
    Ok(downloaded)
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

/// The no-prompt preparation every setup start gets, plus the outcome
/// marker. A copy that needs approval gets no `RunAsInvoker` — its prompt
/// follows the click, which is the point.
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

/// Undoes [`prepare`]'s environment change. Every path on which the setup
/// did not start must call it (ENG2-4): left set, `__COMPAT_LAYER` is
/// inherited by every elevated child the app later starts for the user — the
/// Task Manager switch, ending a protected process — and `RunAsInvoker`
/// silently strips their elevation.
fn unprepare() {
    #[cfg(windows)]
    vitals_win::install::clear_inherited_compat_layer();
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
/// it. A declined prompt keeps the download, so the offer stays — and puts
/// back everything stopped for the install, so the app the user is still
/// looking at goes on working (ENG2-1).
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
    unprepare();
    resume_after_failed_install(app);
    outcome::failed(&update.version, &message);
    *pending.ready.lock() = Some((update, bytes));
    Err(format!("the update was not installed: {message}"))
}

/// The sampler and servers stop before the setup replaces the binary, as on
/// quit. Each step has its inverse in [`resume_after_failed_install`].
fn stop_for_install(app: &tauri::AppHandle) {
    if let Some(sampler) = app.try_state::<crate::sampling::SamplerHandle>() {
        sampler.stop_and_wait(std::time::Duration::from_secs(2));
    }
    crate::server::stop_local_api(app);
    crate::ipc::stop(app);
}

/// Puts back what [`stop_for_install`] and the pre-install hook took down,
/// when the setup did not start.
fn resume_after_failed_install(app: &tauri::AppHandle) {
    crate::ipc::restart(app);
    crate::server::start_local_api(app);
    if let Some(sampler) = app.try_state::<crate::sampling::SamplerHandle>() {
        sampler.restart(app.clone());
    }
    let hidden = app
        .try_state::<PendingInstall>()
        .map(|p| std::mem::take(&mut *p.hidden.lock()))
        .unwrap_or_default();
    for label in hidden {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.show();
        }
    }
    crate::tray::set_visible(app, true);
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
    // A restart or logoff ends the session seconds after this runs, and the
    // setup with it: three updates were lost exactly that way. The download
    // is only in memory, so it goes too, but the next launch finds it again
    // within [`FIRST_CHECK_DELAY`] and installs it while idle.
    #[cfg(windows)]
    if vitals_win::input::session_ending() {
        tracing::info!("update waiting; Windows is ending the session, so not starting the setup");
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
                    unprepare();
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
        // The process is exiting anyway; cleared for symmetry, so no path
        // that failed to start the setup leaves the layer set.
        unprepare();
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
mod idle_install {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn away() -> Moment {
        Moment {
            idle_secs: Some(IDLE_BEFORE_INSTALL),
            window_visible: false,
            session_ending: false,
            permitted: true,
        }
    }

    #[test]
    fn installs_once_the_user_has_been_away_with_vitals_only_in_the_tray() {
        assert!(idle_install_due(away()));
    }

    #[test]
    fn never_starts_the_setup_while_windows_ends_the_session() {
        // The 2026-10-14/15 losses: the setup started three seconds into a
        // restart and died with the session.
        assert!(!idle_install_due(Moment {
            session_ending: true,
            ..away()
        }));
    }

    #[test]
    fn waits_while_a_vitals_window_is_on_screen_even_without_input() {
        assert!(!idle_install_due(Moment {
            window_visible: true,
            ..away()
        }));
    }

    #[test]
    fn a_minute_short_of_the_threshold_is_not_away() {
        assert!(!idle_install_due(Moment {
            idle_secs: Some(IDLE_BEFORE_INSTALL - 60),
            ..away()
        }));
    }

    #[test]
    fn an_unknown_idle_time_is_not_an_absence() {
        assert!(!idle_install_due(Moment {
            idle_secs: None,
            ..away()
        }));
    }

    #[test]
    fn updates_off_or_a_copy_needing_approval_never_installs_unattended() {
        // Covers both: automatic updates off, and a Program Files copy whose
        // UAC prompt nobody would see.
        assert!(!idle_install_due(Moment {
            permitted: false,
            ..away()
        }));
    }

    #[test]
    fn a_relaunch_marker_hides_only_the_launch_that_follows_the_setup() {
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let at = |secs: u64| written + Duration::from_secs(secs);
        assert!(quiet_relaunch::fresh(written, at(90)));
        // Left by a setup that never ran: must not hide tomorrow's launch.
        assert!(!quiet_relaunch::fresh(written, at(16 * 60)));
        // A clock that went backwards is not a fresh marker.
        let before = written - Duration::from_secs(1);
        assert!(!quiet_relaunch::fresh(written, before));
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

    #[test]
    fn a_per_user_uninstall_asks_for_administrator_rights_before_leaving_the_sensors_service() {
        // S21-06: the helper exits NOT_ELEVATED (5) in a per-user
        // uninstaller; that code, and only that code, must trigger the
        // elevated retry, and a refusal must tell the user what is left.
        let hooks = include_str!("../windows/hooks.nsh");
        #[cfg(windows)]
        assert_eq!(vitals_sensors::exit::NOT_ELEVATED, 5);
        assert!(hooks.contains("${If} $0 == 5"));
        assert!(hooks.contains(r#"ExecShellWait "runas" "$1" "uninstall""#));
        assert!(hooks.contains(r#"MessageBox MB_ICONINFORMATION|MB_OK "$5" /SD IDOK"#));
        // Someone who never turned sensors on is never prompted.
        assert!(hooks.contains(r"SYSTEM\CurrentControlSet\Services\vitals-sensors"));
        // An update never removes it, consent or not.
        let removal = hooks
            .find("Removing the Vitals sensors service")
            .unwrap_or(0);
        let guard = hooks[..removal].rfind("${If} $UpdateMode <> 1");
        assert!(
            guard.is_some(),
            "the removal must sit inside the not-updating branch"
        );
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
