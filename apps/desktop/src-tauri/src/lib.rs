//! The Vitals desktop shell.
//!
//! Deliberately thin. The webview renders; it does not compute. Every
//! measurement happens in Rust on a dedicated sampler thread and arrives as
//! prepared frames, because a task manager whose own UI thread is busy
//! aggregating numbers will misreport the machine it is measuring.

pub mod alerts;
pub mod benchmarks;
pub mod commands;
pub mod history;
pub mod hud;
pub mod inventory;
pub mod ipc;
pub mod sampling;
pub mod server;
pub mod state;
pub mod store;
pub mod tray;
pub mod users;

use tauri::Manager;

/// Builds and runs the application.
///
/// # Panics
///
/// Panics if the Tauri runtime cannot start, which is unrecoverable.
#[allow(clippy::expect_used)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("VITALS_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        // Single instance must be registered first so a second launch is
        // routed to the running window instead of starting a second sampler.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                // `show` as well as unminimise: an instance started minimised
                // to the tray has a hidden window, and a second launch is the
                // user asking for it.
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        // `--minimized` is what the Run-key entry passes back to us; the
        // frontend reads it via `get_launch_options` and skips the reveal.
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--minimized"])
                .app_name("Vitals")
                .build(),
        )
        .manage(state::AppState::new())
        .manage(server::LanServer::new())
        .manage(alerts::Alerts::new())
        // The sampler starts with the app and stops when the handle drops at
        // shutdown. Managed so it stays alive for the process lifetime —
        // dropping the handle would silently stop all sampling.
        .setup(setup)
        .on_window_event(on_window_event)
        .invoke_handler(handler())
        .build(tauri::generate_context!())
        .expect("failed to start the Vitals application")
        .run(on_run_event);
}

/// Every command the webview may call.
///
/// Its own function because the list is long enough to push `run()` past
/// clippy's line budget, and because `scripts/check-drift.ps1` reads this
/// exact `generate_handler![...]` block to compare against the frontend's
/// `invoke` calls — keep the macro call shape intact.
fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        commands::get_host_info,
        commands::get_capabilities,
        commands::set_sample_rate,
        commands::get_launch_options,
        commands::show_main_window,
        #[cfg(windows)]
        commands::plan_terminate_process,
        #[cfg(windows)]
        commands::plan_suspend_process,
        #[cfg(windows)]
        commands::terminate_process,
        #[cfg(windows)]
        commands::suspend_process,
        #[cfg(windows)]
        commands::resume_process,
        #[cfg(windows)]
        commands::set_process_priority,
        #[cfg(windows)]
        commands::set_process_affinity,
        // Per-process detail the sampler deliberately does not carry:
        // efficiency mode needs a handle per process, and handles and
        // modules cost megabytes. All four are asked for one row at a time,
        // when the user opens that row.
        commands::get_efficiency_mode,
        commands::set_efficiency_mode,
        commands::get_process_handles,
        commands::get_process_modules,
        commands::get_executable_path,
        // Shell integrations. No `cfg`: the non-Windows builds answer
        // `Unsupported`, which the UI can render, rather than Tauri
        // reporting a command that does not exist.
        commands::open_file_location,
        commands::show_file_properties,
        // On-demand inventories. Request/response rather than pushed:
        // slow to gather, rarely changing, and only wanted while their
        // own screen is open. See `inventory` for the full argument.
        #[cfg(windows)]
        inventory::get_connections,
        #[cfg(windows)]
        inventory::get_startup,
        #[cfg(windows)]
        inventory::get_installed_apps,
        #[cfg(windows)]
        inventory::uninstall_app,
        #[cfg(windows)]
        inventory::get_sensors,
        // Storage. `scan_storage` and `find_cleanup_candidates` are
        // async so the synchronous command thread stays free — otherwise
        // `cancel_storage_scan` would queue behind the very scan it is
        // meant to stop.
        #[cfg(windows)]
        inventory::get_volumes,
        #[cfg(windows)]
        inventory::scan_storage,
        #[cfg(windows)]
        inventory::cancel_storage_scan,
        #[cfg(windows)]
        inventory::find_cleanup_candidates,
        // Benchmarks. The extreme case of the on-demand argument above:
        // a suite occupies every core for seconds, so it is only ever
        // started by the user from its own screen.
        #[cfg(windows)]
        benchmarks::list_benchmarks,
        #[cfg(windows)]
        benchmarks::run_benchmarks,
        // App history and logon sessions. Both have non-Windows
        // fallbacks, so unlike the inventories above they need no `cfg`
        // here — the fallback answers on every other platform rather
        // than the command simply not existing.
        history::get_app_history,
        history::clear_app_history,
        history::set_history_enabled,
        store::set_retention_days,
        store::query_machine_history,
        store::get_history_usage,
        store::write_flight_recording,
        server::get_lan_status,
        server::start_lan_server,
        server::stop_lan_server,
        server::create_pairing,
        server::revoke_pairing,
        server::revoke_all_pairings,
        users::get_users,
        commands::get_alerts,
        commands::set_alert_prefs,
        commands::set_alert_strings,
        commands::diagnose,
        commands::set_close_to_tray,
        commands::set_tray_strings,
        commands::quit_app,
        // The overlay. `toggle_hud` is what the Ctrl+Shift+H shortcut
        // calls; `set_hud_visible` is what the settings switch and the
        // restore-on-launch path call, because a toggle would close an
        // overlay that a race had already opened.
        hud::toggle_hud,
        hud::set_hud_visible,
    ]
}

/// Starts the sampler and installs the tray.
///
/// The tray goes up before the reveal safety net is armed: both that net and
/// the × button assume there is already a way back to a hidden window.
fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Before the sampler, so the first tick can already see whether a CLI
    // is waiting (it is managed state the sampler looks up per tick).
    ipc::start(app.handle());
    let handle = sampling::spawn(app.handle().clone());
    app.manage(handle);
    tray::install(app.handle())?;
    server::start_local_api(app.handle());
    arm_reveal_safety_net(app.handle().clone());
    Ok(())
}

/// Teardown that must happen however the app ends.
///
/// `Exit` is the one event both quit paths — the tray menu and the
/// `quit_app` command, which each call `app.exit(0)` — funnel through, so the
/// local API's discovery file is removed here rather than in each of them.
// Tauri's `run` callback signature takes the event by value; there is no
// borrowing variant to satisfy `needless_pass_by_value` with.
#[allow(clippy::needless_pass_by_value)]
fn on_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    if matches!(event, tauri::RunEvent::Exit) {
        server::stop_local_api(app);
        ipc::stop(app);
    }
}

/// The × button hides rather than closes while `closeToTray` is on.
///
/// A system monitor that stops monitoring because a window was closed by
/// reflex is the failure this prevents; Quit lives in the tray menu.
fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event
        && tray::hide_instead_of_closing(window.app_handle(), window)
    {
        api.prevent_close();
    }
}

/// Safety net for a window that is created hidden.
///
/// The frontend calls `show_main_window` after first paint. If it never gets
/// that far — a JavaScript error, a failed asset, a webview that will not
/// start — nothing else would ever show the window, and the app would run
/// invisibly with no way to reach it. A blank window the user can close beats
/// a process they can only find in Task Manager.
///
/// Not armed for an autostarted launch: hidden is then the intended state,
/// and the tray — installed in `setup`, before this is armed — is how the
/// user reaches it.
fn arm_reveal_safety_net(app: tauri::AppHandle) {
    if commands::launched_minimised() {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));

        if let Some(window) = app.get_webview_window("main") {
            // Already shown by the frontend: nothing to do.
            if window.is_visible().unwrap_or(false) {
                return;
            }

            tracing::warn!(
                "frontend did not signal readiness within 5s; showing the window anyway"
            );
            let _ = window.show();
        }
    });
}
