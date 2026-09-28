//! The always-on-top overlay window.
//!
//! A second webview loading `hud.html`, created on demand rather than at
//! startup: an overlay nobody has asked for should not cost a webview process,
//! and the setting that re-opens it on launch calls the same command the
//! toggle does, so there is one code path rather than two that can disagree.
//!
//! Its capability (`capabilities/hud.json`) is scoped to this window's label
//! and grants dragging, cursor pass-through, always-on-top, hide and close —
//! nothing else. A chromeless window that floats above everything is the
//! easiest surface in the product to mistake for something it is not, so it
//! gets the least it can work with.

use tauri::{Emitter as _, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// The window label. Must match `"windows"` in `capabilities/hud.json`, or the
/// webview loads with no permissions at all and every `invoke` from it is
/// denied at runtime with nothing failing to compile.
pub const LABEL: &str = "hud";

/// Small enough to sit in a corner, large enough for three rows and a trace.
/// Not resizable: there is nothing to reflow, and a drag handle on a window
/// whose whole surface is a drag region fights the dragging.
const WIDTH: f64 = 220.0;
const HEIGHT: f64 = 96.0;

/// Emitted to the overlay after it has been shown by the shortcut or the
/// settings switch, so its toolbar state matches the window's real state.
/// The frontend listens in `hud/lib/live.ts`.
pub const SHOWN_EVENT: &str = "vitals://hud-shown";

// Click-through is deliberately NOT a command here. The overlay toggles its
// own cursor pass-through through `core:window:allow-set-ignore-cursor-events`,
// which its capability already grants, and a command would be a second way to
// do the same thing that the main window has no reason to reach for. The
// grant is narrower than it looks: the capability is scoped to this label, so
// nothing else in the app gains it.

fn os_error(context: &str, error: &tauri::Error) -> CommandError {
    CommandError::from(vitals_core::Error::Os {
        context: format!("{context}: {error}"),
        code: 0,
    })
}

/// Shows the overlay and makes it clickable again.
///
/// Click-through is the one HUD state a user cannot undo from the HUD: a
/// window that ignores the cursor cannot receive the click that would turn
/// that off. The hint beside the toggle says "Press Ctrl+Shift+H to bring
/// it back", and before this the shortcut only hid and re-showed a window
/// that was still ignoring the cursor — the only way out was quitting.
/// Every show path therefore restores cursor events. Hiding leaves the
/// flag alone; there is nothing to click while hidden.
fn show_clickable(window: &tauri::WebviewWindow) -> CommandResult<()> {
    window
        .set_ignore_cursor_events(false)
        .map_err(|e| os_error("restore overlay clicks", &e))?;
    window.show().map_err(|e| os_error("show overlay", &e))?;
    // Best effort: the toolbar re-renders its toggle; a lost event leaves a
    // stale icon, not a stuck window.
    let _ = window.emit_to(LABEL, SHOWN_EVENT, ());
    Ok(())
}

/// Shows the overlay if it is hidden, hides it if it is visible.
///
/// Returns whether the overlay is visible afterwards, so the caller's switch
/// reflects what actually happened rather than what it asked for.
///
/// Hide rather than close on the way out: recreating the webview costs a
/// process start and loses the sparkline history, and this is bound to a
/// global shortcut people press repeatedly.
#[tauri::command]
pub fn toggle_hud(app: tauri::AppHandle) -> CommandResult<bool> {
    if let Some(window) = app.get_webview_window(LABEL) {
        let visible = window.is_visible().unwrap_or(false);
        if visible {
            window.hide().map_err(|e| os_error("hide overlay", &e))?;
        } else {
            show_clickable(&window)?;
        }
        return Ok(!visible);
    }

    set_hud_visible(app, true)
}

/// Drives the overlay to a known state, creating it if needed.
///
/// This is what the settings switch and the "re-open on start" path call. A
/// toggle cannot serve them: restoring a stored `true` at launch must not
/// close an overlay a race left open.
#[tauri::command]
// Tauri's command macro injects the handle by value; it cannot hand us a
// borrow, and the early-return branches do not consume it.
#[allow(clippy::needless_pass_by_value)]
pub fn set_hud_visible(app: tauri::AppHandle, visible: bool) -> CommandResult<bool> {
    if let Some(window) = app.get_webview_window(LABEL) {
        if visible {
            show_clickable(&window)?;
        } else {
            window.hide().map_err(|e| os_error("hide overlay", &e))?;
        }
        return Ok(visible);
    }

    if !visible {
        // Nothing to hide. Building a window in order to hide it would start a
        // webview process for a feature the user has just turned off.
        return Ok(false);
    }

    let builder = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("hud.html".into()));
    // On Windows a transparent webview needs the window itself undecorated; a
    // decorated frame paints an opaque client area behind the page whatever
    // the page's own background says. On macOS the method exists only behind
    // tauri's `macos-private-api` feature, which also bars an App Store
    // listing — not worth it for a port that is not yet published (ADR 0007),
    // so the overlay is opaque there. Without this gate the macOS build does
    // not compile at all.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(true);
    builder
        .decorations(false)
        .always_on_top(true)
        // Absent from Alt+Tab and the taskbar: an overlay is a readout, not a
        // window anyone wants to cycle to, and one that steals a tab stop
        // from the app underneath is worse than no overlay.
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(false)
        // A drop shadow on a transparent window is drawn around the window
        // rectangle, not around the rounded panel inside it, so it renders as
        // a grey box floating on the wallpaper.
        .shadow(false)
        .inner_size(WIDTH, HEIGHT)
        .title("Vitals overlay")
        .build()
        .map_err(|e| os_error("create overlay", &e))?;

    Ok(true)
}
