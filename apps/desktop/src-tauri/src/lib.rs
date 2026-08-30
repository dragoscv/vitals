//! The Vitals desktop shell.
//!
//! Deliberately thin. The webview renders; it does not compute. Every
//! measurement happens in Rust on a dedicated sampler thread and arrives as
//! prepared frames, because a task manager whose own UI thread is busy
//! aggregating numbers will misreport the machine it is measuring.

pub mod commands;
pub mod sampling;
pub mod state;

use tauri::Manager;

/// Builds and runs the application.
///
/// # Panics
///
/// Panics if the Tauri runtime cannot start, which is unrecoverable.
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
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state::AppState::new())
        // The sampler starts with the app and stops when the handle drops at
        // shutdown. Managed so it stays alive for the process lifetime —
        // dropping the handle would silently stop all sampling.
        .setup(|app| {
            let handle = sampling::spawn(app.handle().clone());
            app.manage(handle);

            // Safety net for a window that is created hidden.
            //
            // The frontend calls `show_main_window` after first paint. If it
            // never gets that far — a JavaScript error, a failed asset, a
            // webview that will not start — nothing else would ever show the
            // window, and the app would run invisibly with no way to reach
            // it. A blank window the user can close beats a process they can
            // only find in Task Manager.
            let reveal = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));

                if let Some(window) = reveal.get_webview_window("main") {
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_host_info,
            commands::get_capabilities,
            commands::set_sample_rate,
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
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Vitals application");
}
