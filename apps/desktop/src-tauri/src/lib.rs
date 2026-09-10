//! The Vitals desktop shell.
//!
//! Deliberately thin. The webview renders; it does not compute. Every
//! measurement happens in Rust on a dedicated sampler thread and arrives as
//! prepared frames, because a task manager whose own UI thread is busy
//! aggregating numbers will misreport the machine it is measuring.

pub mod benchmarks;
pub mod commands;
pub mod history;
pub mod inventory;
pub mod sampling;
pub mod server;
pub mod state;
pub mod store;
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
        // The sampler starts with the app and stops when the handle drops at
        // shutdown. Managed so it stays alive for the process lifetime —
        // dropping the handle would silently stop all sampling.
        .setup(|app| {
            let handle = sampling::spawn(app.handle().clone());
            app.manage(handle);
            arm_reveal_safety_net(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            store::export_flight_recording,
            store::write_flight_recording,
            server::get_lan_status,
            server::start_lan_server,
            server::stop_lan_server,
            server::create_pairing,
            server::revoke_pairing,
            server::revoke_all_pairings,
            users::get_users,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Vitals application");
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
/// and the tray is how the user reaches it.
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
