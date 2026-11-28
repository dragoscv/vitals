//! The desktop widget windows: one chromeless overlay per widget instance.
//!
//! A widget is a second webview on the same machine, created on demand rather
//! than at startup — the same argument the HUD makes, taken further: a user
//! may want six of them, and six webview processes nobody asked for is not a
//! monitoring app's opening move. The main window owns the list and pushes it
//! here with [`sync_widgets`]; this module's job is to make the set of live
//! windows equal that list and to tell everyone when it changes.
//!
//! Two rules shape the whole file:
//!
//! 1. **A widget window is positioned once, at creation.** Every other change
//!    (opacity, style, flags, a re-sync from the settings screen) leaves the
//!    position alone. Without this, an unrelated sync drags a widget back to
//!    where the store last saw it while the user is still moving it, and the
//!    window fights the mouse. The cost is that the position is owned by the
//!    window, not the store: a drag is written back through
//!    [`handle_window_event`], and that write-back is the only thing that
//!    changes a stored coordinate after creation.
//! 2. **The list is the single source of truth, and it lives in Rust.** The
//!    frontend has its own copy for rendering, but anything that could get out
//!    of step (a window closed by the OS, Alt+F4, a drag) is reconciled here,
//!    because this is the side that can observe it.
//!
//! `WidgetSpec` has no `ts_rs` derive: only `vitals-core`'s types are
//! generated into `@vitals/protocol`, and a type in this crate cannot be. The
//! field names are instead pinned from both ends — the frontend's
//! `features/widgets/model.ts` holds the TypeScript interface, and the test at
//! the bottom of this file asserts the serialised key set, so a rename on
//! either side fails a test rather than silently sending a field the other
//! end ignores.
//!
//! Two things can ask for a widget, and both end in the same reconciliation:
//! the settings screen calls [`sync_widgets`] from the webview, and
//! `vitals widget` on the CLI asks over the attach pipe (see
//! [`PipeWidgetHost`]). Neither builds a window by hand, because two ways to
//! create one would drift and only one of them would be tested.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{Emitter as _, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use vitals_ipc::attach::{
    WidgetAction, WidgetHost, WidgetInfo, WidgetKind, WidgetLayout, WidgetSize, WidgetsReply,
};

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// Window label prefix. `scripts/check-drift.ps1` reads labels literally, so
/// this constant and `WIDGET_LABEL_PREFIX` in the frontend's `model.ts` are
/// the pair that has to agree; the label round-trip test below pins this end.
pub const LABEL_PREFIX: &str = "widget-";

/// Emitted to one widget window whenever its spec changes, with the whole
/// spec as the payload. Per-window rather than global: a widget re-renders
/// only for its own edits.
pub const CONFIG_EVENT: &str = "vitals://widget-config";

/// Emitted globally when a widget is dragged, so the settings screen shows
/// where it actually is rather than where it was created.
pub const MOVED_EVENT: &str = "vitals://widget-moved";

/// Emitted globally when a widget window goes away.
pub const CLOSED_EVENT: &str = "vitals://widget-closed";

/// Emitted globally when a widget changes its own flags from its toolbar.
/// Deliberately separate from [`CONFIG_EVENT`]: the settings screen listens to
/// this one to follow a change it did not make, and nothing here listens to
/// it, so a change originating in the screen cannot echo back.
pub const CHANGED_EVENT: &str = "vitals://widget-changed";

/// Emitted globally when a widget asks for its settings screen.
pub const EDIT_EVENT: &str = "vitals://widget-edit";

/// Mirrors `MAX_WIDGETS` in the frontend's `model.ts`. Enforced on both sides
/// on purpose: the frontend's copy is what the settings screen honours, and
/// this one is what stops a command — reachable from any window — from asking
/// for two hundred webviews.
const MAX_WIDGETS: usize = 12;

/// Anything smaller than this between two coordinates is the same coordinate.
/// A window manager hands back the position it was given, through a physical
/// pixel and a scale factor, so an exact comparison would report a change on
/// every drag tick and push a pointless event to the screen.
const EPSILON: f64 = 0.5;

/// One widget, as both sides describe it.
///
/// Width and height are derived from `size` on the frontend and sent along
/// rather than recomputed here: the layout that decides them is a frontend
/// concern, and having one place that decides keeps the two from disagreeing
/// by a few pixels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSpec {
    pub id: String,
    pub kind: String,
    pub size: String,
    pub style: String,
    pub layout: String,
    pub opacity: f64,
    pub always_on_top: bool,
    pub click_through: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The other machine whose readings this widget shows, or `None` for this
    /// one.
    ///
    /// A machine's identity, not its address: the address and its token live
    /// in the remote registry, so re-pairing a machine on a new port does not
    /// rewrite every widget watching it. The host never dereferences it — the
    /// frontend decides whose readings to render — but it does carry it, so
    /// the widget window learns which machine to watch from its own spec and
    /// from [`CONFIG_EVENT`]. Serialised as `null` rather than omitted: the
    /// frontend's `WidgetInstance` always declares the key, and a key that
    /// sometimes vanishes is a key that reads as "unknown" on the other side.
    ///
    /// An absent key and an explicit `null` both decode to `None` ("this
    /// machine"), so a hand-written spec need not name it.
    pub remote_connection_id: Option<String>,
}

impl WidgetSpec {
    /// A spec that can become a window. A zero-sized one cannot, and an empty
    /// id cannot produce a label.
    fn is_valid(&self) -> bool {
        !self.id.is_empty() && self.width > 0.0 && self.height > 0.0
    }
}

/// The live widget list. Managed state, written by the commands below and by
/// window events, read by everything that needs to know what is open.
#[derive(Debug, Default)]
pub struct Widgets(Mutex<Vec<WidgetSpec>>);

/// The window label for a widget id.
pub fn label_for(id: &str) -> String {
    format!("{LABEL_PREFIX}{id}")
}

/// The widget id inside a window label, or `None` for any other window.
fn id_from_label(label: &str) -> Option<&str> {
    label.strip_prefix(LABEL_PREFIX).filter(|id| !id.is_empty())
}

/// Payload of [`MOVED_EVENT`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetMoved {
    id: String,
    x: f64,
    y: f64,
}

/// Payload of [`CLOSED_EVENT`] and [`EDIT_EVENT`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetRef {
    id: String,
}

fn os_error(context: &str, error: &tauri::Error) -> CommandError {
    CommandError::from(vitals_core::Error::Os {
        context: format!("{context}: {error}"),
        code: 0,
    })
}

/// Two specs describe the same window. Compared field by field with an
/// epsilon on the doubles rather than by `PartialEq`: a position that came
/// back from the window manager differs in the last bits often enough that a
/// derived comparison would report a change on almost every event.
fn same(a: &WidgetSpec, b: &WidgetSpec) -> bool {
    a.id == b.id
        && a.kind == b.kind
        && a.size == b.size
        && a.style == b.style
        && a.layout == b.layout
        && a.remote_connection_id == b.remote_connection_id
        && a.always_on_top == b.always_on_top
        && a.click_through == b.click_through
        && (a.opacity - b.opacity).abs() < EPSILON
        && (a.x - b.x).abs() < EPSILON
        && (a.y - b.y).abs() < EPSILON
        && (a.width - b.width).abs() < EPSILON
        && (a.height - b.height).abs() < EPSILON
}

/// Reads or rewrites the live list.
///
/// `None` when the state is missing, which means the app was built without
/// the call in `setup` — an internal fault rather than anything the user did,
/// so it is reported as one instead of panicking inside a command.
fn with_specs<R>(
    app: &tauri::AppHandle,
    edit: impl FnOnce(&mut Vec<WidgetSpec>) -> R,
) -> Option<R> {
    let state = app.try_state::<Widgets>()?;
    let mut specs = state.0.lock();
    Some(edit(&mut specs))
}

fn unmanaged() -> CommandError {
    CommandError::Internal {
        message: "the widget list is not initialised".to_owned(),
    }
}

fn stored(app: &tauri::AppHandle, id: &str) -> Option<WidgetSpec> {
    with_specs(app, |specs| {
        specs.iter().find(|spec| spec.id == id).cloned()
    })
    .flatten()
}

/// Replaces the stored list with what the frontend says, keeping anything the
/// frontend does not know about.
///
/// The position is the one exception to "the frontend owns the list": it is
/// read back from the window rather than taken from the incoming spec, so a
/// drag that happened while the settings screen was open is not undone by the
/// next edit made there.
fn store(app: &tauri::AppHandle, incoming: &[WidgetSpec]) {
    let _ = with_specs(app, |specs| {
        for spec in incoming {
            match specs.iter_mut().find(|stored| stored.id == spec.id) {
                Some(existing) => {
                    let (x, y) = (existing.x, existing.y);
                    *existing = spec.clone();
                    existing.x = x;
                    existing.y = y;
                }
                None => specs.push(spec.clone()),
            }
        }
    });
}

fn forget(app: &tauri::AppHandle, id: &str) -> bool {
    with_specs(app, |specs| {
        let before = specs.len();
        specs.retain(|spec| spec.id != id);
        specs.len() != before
    })
    .unwrap_or(false)
}

/// Closes a widget's window and announces it.
///
/// `destroy` rather than `close`: closing asks the window whether it wants to
/// go, and the answer has to travel back through the event loop before the
/// window is gone. Bookkeeping here is done first and the window is torn down
/// outright, so the list and the screen are never briefly wrong.
fn shut(app: &tauri::AppHandle, id: &str) {
    if forget(app, id) {
        let _ = app.emit(CLOSED_EVENT, WidgetRef { id: id.to_owned() });
    }
    if let Some(window) = app.get_webview_window(&label_for(id))
        && let Err(error) = window.destroy()
    {
        tracing::warn!(%error, widget = id, "could not destroy a widget window");
    }
}

/// Makes the live window match the spec it is given.
///
/// Position is not touched here; see the module docs. Size is, because a size
/// change is a settings change and the user expects it while they are looking
/// at the control, not at the next launch.
///
/// `previous` is the spec the window already had, and an unchanged one short-
/// circuits the whole thing. That is the common case while the user drags a
/// widget: each tick re-syncs a list whose only difference is a position the
/// window itself reported a moment ago, and re-applying it would push a
/// configuration event per tick into a webview that has nothing to do.
fn apply(
    window: &tauri::WebviewWindow,
    spec: &WidgetSpec,
    previous: Option<&WidgetSpec>,
) -> CommandResult<()> {
    if previous.is_some_and(|before| same(before, spec)) {
        return Ok(());
    }
    let want = LogicalSize::new(spec.width, spec.height);
    let have = window
        .inner_size()
        .map_err(|e| os_error("read widget size", &e))?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let same_size = (f64::from(have.width) - spec.width * scale).abs() < EPSILON
        && (f64::from(have.height) - spec.height * scale).abs() < EPSILON;
    if !same_size {
        window
            .set_size(want)
            .map_err(|e| os_error("resize widget", &e))?;
    }
    if window.is_always_on_top().unwrap_or(false) != spec.always_on_top {
        window
            .set_always_on_top(spec.always_on_top)
            .map_err(|e| os_error("set widget always-on-top", &e))?;
    }
    window
        .set_ignore_cursor_events(spec.click_through)
        .map_err(|e| os_error("set widget click-through", &e))?;
    // Best effort: a lost event leaves the widget rendering its previous
    // configuration, which the next edit corrects.
    let _ = window.emit_to(window.label(), CONFIG_EVENT, spec);
    Ok(())
}

/// Builds a widget window.
///
/// The builder chain mirrors `hud.rs` deliberately — same transparency gate,
/// same absence from the taskbar, same no-shadow decision — because a
/// chromeless overlay has the same three platform problems there.
fn create(app: &tauri::AppHandle, spec: &WidgetSpec) -> CommandResult<tauri::WebviewWindow> {
    let label = label_for(&spec.id);
    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("widget.html".into()));
    // On Windows a transparent webview needs the window itself undecorated; on
    // macOS the method is behind `macos-private-api`, which bars an App Store
    // listing, so the widget is opaque there. Without this gate the macOS
    // build does not compile.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(true);
    let window = builder
        .decorations(false)
        .always_on_top(spec.always_on_top)
        // Absent from Alt+Tab and the taskbar: six widgets in the taskbar and
        // in every window cycle would make the switcher useless.
        .skip_taskbar(true)
        // Not resizable: the size is a setting with three named values, and a
        // drag handle on a window whose whole surface is a drag region fights
        // the dragging.
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        // Never steal focus. A readout that takes the caret from whatever the
        // user is typing in is worse than no readout.
        .focused(false)
        // A drop shadow on a transparent window is drawn around the window
        // rectangle, not around the rounded panel inside it.
        .shadow(false)
        .inner_size(spec.width, spec.height)
        .title("Vitals widget")
        .build()
        .map_err(|e| os_error("create widget", &e))?;

    window
        .set_ignore_cursor_events(spec.click_through)
        .map_err(|e| os_error("set widget click-through", &e))?;
    // The only place a stored position is applied. `set_position` takes a
    // logical position, which is what the user's coordinates are; the window
    // manager converts using the monitor's scale factor.
    window
        .set_position(tauri::LogicalPosition::new(spec.x, spec.y))
        .map_err(|e| os_error("place widget", &e))?;
    Ok(window)
}

fn ensure(app: &tauri::AppHandle, spec: &WidgetSpec) -> CommandResult<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window(&label_for(&spec.id)) {
        return Ok(window);
    }
    create(app, spec)
}

/// Brings the live widget windows into line with `specs`.
///
/// This is the one call the frontend makes after any change to the list, and
/// it is idempotent: unchanged widgets keep their windows, new ones get one,
/// and windows whose widget has left the list are closed. Returns the ids that
/// are actually open, so the caller renders what exists rather than what it
/// asked for — a particular window can fail to build (no permission, a
/// monitor that has been unplugged) while the rest succeed.
///
/// `async` with nothing to await, on purpose. Tauri runs a sync command on the
/// main thread and `WebviewWindowBuilder::build` dispatches to that thread and
/// waits, which Tauri documents as a deadlock on Windows; an async command
/// runs on the async runtime instead.
#[tauri::command]
// Tauri's command macro injects the handle by value and cannot hand us a
// borrow.
#[allow(clippy::needless_pass_by_value)]
// See above: the `async` is what moves the command off the main thread.
#[allow(clippy::unused_async)]
pub async fn sync_widgets(
    app: tauri::AppHandle,
    specs: Vec<WidgetSpec>,
) -> CommandResult<Vec<String>> {
    reconcile(&app, specs)
}

/// The body of [`sync_widgets`], callable from any thread that is not the main
/// one.
///
/// Split out so the attach pipe's widget host runs exactly this reconciliation
/// rather than a second implementation that creates windows by hand. The pipe
/// answers on its own client thread, which is why calling this synchronously
/// there is safe: `WebviewWindowBuilder::build` dispatches to the main thread
/// and waits, and the deadlock Tauri documents is only for a caller that *is*
/// the main thread.
fn reconcile(app: &tauri::AppHandle, specs: Vec<WidgetSpec>) -> CommandResult<Vec<String>> {
    let wanted: Vec<WidgetSpec> = specs
        .into_iter()
        .filter(WidgetSpec::is_valid)
        .take(MAX_WIDGETS)
        .collect();

    let closed = with_specs(app, |live| {
        live.retain(|spec| {
            let keep = wanted.iter().any(|want| want.id == spec.id);
            if !keep {
                let label = label_for(&spec.id);
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.destroy();
                }
            }
            keep
        });
    });
    if closed.is_none() {
        return Err(unmanaged());
    }

    let mut live = Vec::with_capacity(wanted.len());
    for spec in &wanted {
        // Read before the list is rewritten at the end of this function, so
        // this is what the window had, not what it is about to have.
        let previous = stored(app, &spec.id);
        match ensure(app, spec) {
            Ok(window) => {
                if let Err(error) = apply(&window, spec, previous.as_ref()) {
                    tracing::warn!(widget = %spec.id, %error, "a widget window could not be updated");
                }
                live.push(spec.id.clone());
            }
            // One widget that cannot be built must not take the others down
            // with it; the caller renders the ones that exist.
            Err(error) => {
                tracing::warn!(widget = %spec.id, %error, "a widget window could not be opened");
            }
        }
    }
    store(app, &wanted);
    Ok(live)
}

/// One widget's spec, for its own window to render itself from.
///
/// A window that is somehow open with no stored spec answers `None` rather
/// than an error: the overlay renders a "closed" state, and the next sync
/// either gives it a spec or closes it.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn get_widget(app: tauri::AppHandle, id: String) -> CommandResult<Option<WidgetSpec>> {
    Ok(stored(&app, &id))
}

/// Closes one widget. Safe to call for an id that is already gone.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn close_widget(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    shut(&app, &id);
    Ok(())
}

/// Changes the two flags a widget exposes on its own toolbar.
///
/// Separate from [`sync_widgets`] because the frontend cannot be the only path
/// for these two: turning click-through off from the settings screen has to
/// work on a window that is currently ignoring the cursor, so the widget's own
/// window asks for it too, and this is what lets the settings screen hear
/// about it afterwards.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn set_widget_flags(
    app: tauri::AppHandle,
    id: String,
    always_on_top: bool,
    click_through: bool,
) -> CommandResult<()> {
    let Some(window) = app.get_webview_window(&label_for(&id)) else {
        return Err(CommandError::NotFound {
            message: format!("no widget window is open with id '{id}'"),
        });
    };
    window
        .set_always_on_top(always_on_top)
        .map_err(|e| os_error("set widget always-on-top", &e))?;
    window
        .set_ignore_cursor_events(click_through)
        .map_err(|e| os_error("set widget click-through", &e))?;

    let updated = with_specs(&app, |specs| {
        specs.iter_mut().find(|spec| spec.id == id).map(|spec| {
            spec.always_on_top = always_on_top;
            spec.click_through = click_through;
            spec.clone()
        })
    })
    .ok_or_else(unmanaged)?;

    if let Some(spec) = updated {
        let _ = app.emit_to(window.label(), CONFIG_EVENT, &spec);
        let _ = app.emit(CHANGED_EVENT, &spec);
    }
    Ok(())
}

/// Brings the main window forward on the settings screen, for the widget's own
/// "settings" button.
///
/// A widget cannot host the settings screen: it is 190 px wide with no
/// decoration, and a form inside it would be a second implementation of a
/// screen that already exists.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn focus_widget_settings(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    if let Some(main) = app.get_webview_window("main")
        && let Err(error) = main.show().and_then(|()| main.set_focus())
    {
        tracing::warn!(%error, "could not bring the main window forward for widget settings");
    }
    let _ = app.emit(EDIT_EVENT, WidgetRef { id });
    Ok(())
}

/// Handles a window event for a widget window.
///
/// Returns whether the window was a widget's, so the shell's handler knows not
/// to treat it as the main window. That matters for two events: a widget at
/// Alt+F4 must close rather than hide (a hidden widget is unreachable, since
/// the settings screen shows the ones in the list, not the ones on disk), and
/// its position must be written back.
///
/// `Moved` arrives on every drag tick, which is why the write-back compares
/// before it emits: the screen redraws a coordinate, not a window.
pub fn handle_window_event(
    app: &tauri::AppHandle,
    window: &tauri::Window,
    event: &tauri::WindowEvent,
) -> bool {
    let Some(id) = id_from_label(window.label()).map(str::to_owned) else {
        return false;
    };

    match event {
        tauri::WindowEvent::Moved(position) => {
            // Physical to logical, because the stored position is what
            // `set_position` will be given at the next launch and that call
            // takes logical coordinates. A monitor that reports no scale
            // factor is a 1:1 monitor.
            let scale = window.scale_factor().unwrap_or(1.0);
            let moved = with_specs(app, |specs| {
                let spec = specs.iter_mut().find(|spec| spec.id == id)?;
                let x = f64::from(position.x) / scale;
                let y = f64::from(position.y) / scale;
                if (spec.x - x).abs() < EPSILON && (spec.y - y).abs() < EPSILON {
                    return None;
                }
                spec.x = x;
                spec.y = y;
                Some(WidgetMoved {
                    id: id.clone(),
                    x,
                    y,
                })
            })
            .flatten();
            if let Some(payload) = moved {
                let _ = app.emit(MOVED_EVENT, payload);
            }
        }
        // The window is being taken away by something other than our own
        // `destroy` — Alt+F4, the window manager, a shutdown. The list must
        // lose it, or the settings screen offers a widget that is not there.
        tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            if forget(app, &id) =>
        {
            let _ = app.emit(CLOSED_EVENT, WidgetRef { id });
        }
        _ => {}
    }
    true
}

// ── The attach pipe's widget host ────────────────────────────────────────
//
// `vitals widget --list | --kind … | --close …` on the CLI asks the *running*
// app, over the attach pipe (`vitals_ipc::attach`). The pipe carries the
// request but owns no windows, so it forwards to the handler registered here.
//
// ## Who may ask, and in what order
//
// The attach pipe has no application-level authorisation, and this host adds
// none. Its admission control is the pipe's own DACL (`PIPE_SDDL`: SYSTEM,
// Administrators and the creating user, nothing for Everyone or Anonymous), so
// a caller that is not this user is refused by the kernel before a single byte
// reaches `serve_connection`. That has two consequences, and both are why the
// code below looks the way it does:
//
// 1. There is no place where a malformed request could be told apart from an
//    unauthorised one — an unauthorised caller gets no reply at all. The
//    ordering hazard the LAN server guards against (parsing a body before
//    checking the token turns a 422 into a schema probe for a 403 caller) does
//    not exist on this transport, so nothing is rearranged to fake it.
// 2. Opening a window is not a new privilege. The bytes are parsed by hand in
//    `vitals_ipc::attach::Decoder` — the "take the raw bytes and parse
//    ourselves" shape — and a same-user process can already create a widget
//    window through `sync_widgets` from any webview, or simply launch the app.
//    This host therefore refuses nothing that the same transport's other
//    actions allow, and grants nothing extra: it is the same-user boundary,
//    applied by the kernel, once.
//
// What it *does* validate is everything the user could get wrong: an unknown
// kind, size or layout never arrives (the wire enums in `vitals-ipc` reject
// it), a widget the app cannot describe is refused by name rather than
// dropped, and asking for one widget past `MAX_WIDGETS` is an error rather
// than a silently discarded window.

/// The logical window size for a size step.
///
/// Mirrors `GEOMETRY` in the frontend's `model.ts`. The main window resolves
/// this itself (`toWindowSpec`) and sends pixels in the spec, but a widget
/// opened over the attach pipe arrives as the word `small`, `medium` or
/// `large` and nothing else — the request carries no geometry — so the table
/// has to exist here too. It is a duplicate, and
/// `the_pipe_sizes_a_new_widget_the_way_the_frontend_would` and
/// `the_pipe_places_a_new_widget_where_the_frontend_would` read the numbers
/// back out of `model.ts` so the two cannot drift apart unnoticed.
fn geometry_for(size: WidgetSize) -> (f64, f64) {
    match size {
        WidgetSize::Small => (190.0, 120.0),
        WidgetSize::Medium => (280.0, 170.0),
        WidgetSize::Large => (400.0, 260.0),
    }
}

/// Cascade step, wrap and origin for a widget that was given no position.
/// Mirrors `CASCADE_STEP` / `CASCADE_WRAP` / `CASCADE_ORIGIN` in `model.ts`.
const CASCADE_STEP: f64 = 28.0;
const CASCADE_WRAP: usize = 8;
const CASCADE_ORIGIN: (f64, f64) = (80.0, 80.0);

/// Where a new widget goes when the request named no position.
///
/// Mirrors `defaultPosition` in `model.ts`: a cascade rather than dead centre,
/// because several widgets stacked on one spot look like one widget, and it
/// wraps so the tenth is still on screen. `index` is the number of widgets
/// already placed, exactly as the frontend passes `registry.length`.
fn cascade_position(index: usize) -> (f64, f64) {
    let step = (index % CASCADE_WRAP) as f64;
    (
        CASCADE_ORIGIN.0 + step * CASCADE_STEP,
        CASCADE_ORIGIN.1 + step * CASCADE_STEP,
    )
}

/// Whether a kind can draw a layout. Mirrors `layoutsFor` in `model.ts`: a ring
/// gauge needs one number that reads 0–100, so only CPU and memory offer it.
///
/// The *values* are not duplicated — they come from the wire enums — but which
/// arrangement suits which reading is a frontend rule, and a request may name
/// any layout for any kind.
fn layout_fits(kind: WidgetKind, layout: WidgetLayout) -> bool {
    match layout {
        WidgetLayout::Ring => matches!(kind, WidgetKind::Cpu | WidgetKind::Memory),
        WidgetLayout::Stack | WidgetLayout::Split => true,
    }
}

/// The layout a kind gets when the requested one cannot be drawn. Mirrors
/// `defaultLayoutFor` in `model.ts` exactly — `ring` for CPU and `stack` for
/// every other kind — and is what the host stores, so the reply reports the
/// layout the app is actually drawing rather than the one asked for.
///
/// Memory is allowed a ring (`layout_fits` above says so, as `layoutsFor`
/// does), so this fallback is only ever reached by the kinds that have no ring
/// to draw. It mirrors the frontend's answer anyway rather than answering "a
/// layout that works": two functions with the same name and the same job
/// should not disagree, or the next reader has to work out which one is right.
fn default_layout(kind: WidgetKind) -> WidgetLayout {
    if matches!(kind, WidgetKind::Cpu) {
        WidgetLayout::Ring
    } else {
        WidgetLayout::Stack
    }
}

/// What one widget action asks for, decided without touching a window.
#[derive(Debug, PartialEq)]
struct WidgetPlan {
    /// The full list the app should end up with, or `None` for a read-only
    /// action that changes nothing.
    apply: Option<Vec<WidgetSpec>>,
    /// The widget an `open` created or a `close` removed. `None` for a list,
    /// and for a close of an id that was not open.
    id: Option<String>,
}

/// Decides what an action does to `current`, or refuses it.
///
/// Pure on purpose: this is the part worth testing, and a test for it needs no
/// window, no Tauri app and no pipe. `new_id` is called at most once, when the
/// action opens a widget, so a caller that counts ids only spends one on a
/// request that really creates something.
///
/// # Errors
/// The list already holds `MAX_WIDGETS` and the action would add another.
fn plan(
    action: &WidgetAction,
    current: &[WidgetSpec],
    new_id: &dyn Fn() -> String,
) -> Result<WidgetPlan, String> {
    match action {
        WidgetAction::List => Ok(WidgetPlan {
            apply: None,
            id: None,
        }),
        WidgetAction::Open {
            kind,
            size,
            layout,
            x,
            y,
        } => {
            if current.len() >= MAX_WIDGETS {
                return Err(format!(
                    "this machine already has {MAX_WIDGETS} widgets open; close one before opening another"
                ));
            }
            // A half-named position is treated as no position: honouring one
            // axis and cascading the other would put the widget somewhere
            // nobody asked for.
            let (x, y) = match (x, y) {
                (Some(x), Some(y)) => (*x, *y),
                _ => cascade_position(current.len()),
            };
            let (width, height) = geometry_for(*size);
            let resolved = if layout_fits(*kind, *layout) {
                *layout
            } else {
                default_layout(*kind)
            };
            let id = new_id();
            let mut wanted = current.to_vec();
            wanted.push(WidgetSpec {
                id: id.clone(),
                kind: kind.as_str().to_owned(),
                size: size.as_str().to_owned(),
                // The rest of the defaults are the ones `addWidget` in
                // `model.ts` gives a widget the settings screen creates: an
                // opaque plain panel, pinned, and not click-through, so a
                // widget opened from a terminal behaves like one opened from
                // the screen.
                style: "plain".to_owned(),
                layout: resolved.as_str().to_owned(),
                opacity: 1.0,
                always_on_top: true,
                click_through: false,
                x,
                y,
                width,
                height,
                // A widget asked for from the CLI watches this machine. The
                // wire action carries no machine, so there is nothing else it
                // could honestly mean.
                remote_connection_id: None,
            });
            Ok(WidgetPlan {
                apply: Some(wanted),
                id: Some(id),
            })
        }
        WidgetAction::Close { id } => {
            if !current.iter().any(|spec| &spec.id == id) {
                // Not an error: closing a widget that is already gone is the
                // state the caller asked for. Nothing is applied, so no window
                // is touched and the reply simply carries no id.
                return Ok(WidgetPlan {
                    apply: None,
                    id: None,
                });
            }
            let wanted = current
                .iter()
                .filter(|spec| &spec.id != id)
                .cloned()
                .collect();
            Ok(WidgetPlan {
                apply: Some(wanted),
                id: Some(id.clone()),
            })
        }
    }
}

/// The wire description of one stored spec.
///
/// # Errors
/// The stored kind, size or layout is not one this protocol describes — a
/// hand-edited settings file, or a widget written by a build that knew more
/// than this one. Refused by name rather than skipped: a `--list` that quietly
/// leaves a window out is worse than one that says why it cannot answer.
fn widget_info(spec: &WidgetSpec) -> Result<WidgetInfo, String> {
    Ok(WidgetInfo {
        id: spec.id.clone(),
        kind: parse_wire(&spec.kind).ok_or_else(|| unknown_field(spec, "kind", &spec.kind))?,
        size: parse_wire(&spec.size).ok_or_else(|| unknown_field(spec, "size", &spec.size))?,
        layout: parse_wire(&spec.layout)
            .ok_or_else(|| unknown_field(spec, "layout", &spec.layout))?,
    })
}

fn unknown_field(spec: &WidgetSpec, field: &str, value: &str) -> String {
    format!(
        "the widget '{}' has {field} '{value}', which this protocol does not describe",
        spec.id
    )
}

/// Parses a stored spelling through the same serde enum that crosses the wire,
/// so the accepted values cannot drift from the ones the CLI can send.
fn parse_wire<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).ok()
}

/// Ids for widgets opened over the pipe.
///
/// Mirrors `newWidgetId` in `model.ts` — the letter `w`, the millisecond clock
/// in base 36, the counter in base 36 — and skips any id a stored widget
/// already uses, so an id made here cannot collide with one the settings
/// screen made.
#[derive(Debug, Default)]
struct IdSource {
    counter: AtomicU64,
}

impl IdSource {
    /// An id no stored widget is using.
    fn next(&self, taken: &[WidgetSpec], now_ms: u64) -> String {
        loop {
            let count = self
                .counter
                .fetch_add(1, Ordering::SeqCst)
                .saturating_add(1);
            let id = format!("w{}{}", base36(now_ms), base36(count));
            if !taken.iter().any(|spec| spec.id == id) {
                return id;
            }
        }
    }
}

/// Lowercase base 36, the alphabet `Number.prototype.toString(36)` uses in the
/// frontend.
fn base36(value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_owned();
    }
    let mut reversed = String::new();
    let mut left = value;
    while left > 0 {
        let digit = (left % 36) as usize;
        // `push` rather than `push_str(&format!(…))`: the pedantic
        // `format_push_string` lint is denied and this is the honest form.
        reversed.push(char::from(DIGITS[digit]));
        left /= 36;
    }
    reversed.chars().rev().collect()
}

/// Milliseconds since the Unix epoch, or `0` on a clock set before it.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// What the pipe host needs from the app, so its own logic can be tested
/// without a window.
///
/// The production implementation is a two-line adapter over the same
/// [`reconcile`] the `sync_widgets` command calls. That is the point of the
/// seam: a second implementation that built windows itself would drift from
/// the command and only one of the two would be covered by a test.
trait WidgetWindows: Send + Sync + 'static {
    /// Every widget the app holds, whether or not its window is on screen.
    ///
    /// # Errors
    /// The widget state is not installed — an internal fault, not the user's.
    fn specs(&self) -> Result<Vec<WidgetSpec>, String>;

    /// Makes the live windows match `wanted`.
    ///
    /// # Errors
    /// The widget state is not installed.
    fn apply(&self, wanted: Vec<WidgetSpec>) -> Result<(), String>;

    /// Announces that a widget is gone, so the settings screen drops it.
    fn announce_closed(&self, id: &str);
}

/// The real writer: the app handle, and [`reconcile`].
#[derive(Debug)]
struct DesktopWindows(tauri::AppHandle);

impl WidgetWindows for DesktopWindows {
    fn specs(&self) -> Result<Vec<WidgetSpec>, String> {
        with_specs(&self.0, |specs| specs.clone()).ok_or_else(|| unmanaged().to_string())
    }

    fn apply(&self, wanted: Vec<WidgetSpec>) -> Result<(), String> {
        reconcile(&self.0, wanted)
            .map(|_live| ())
            .map_err(|error| error.to_string())
    }

    fn announce_closed(&self, id: &str) {
        // The same event `shut` emits: without it a widget closed from a
        // terminal would stay in the settings screen, and the next edit there
        // would resurrect the window.
        let _ = self.0.emit(CLOSED_EVENT, WidgetRef { id: id.to_owned() });
    }
}

/// The attach pipe's widget handler.
struct PipeWidgetHost<W: WidgetWindows> {
    windows: W,
    ids: IdSource,
}

impl<W: WidgetWindows> PipeWidgetHost<W> {
    fn new(windows: W) -> Self {
        Self {
            windows,
            ids: IdSource::default(),
        }
    }
}

impl<W: WidgetWindows> WidgetHost for PipeWidgetHost<W> {
    fn widgets(&self, action: &WidgetAction) -> Result<WidgetsReply, String> {
        let current = self.windows.specs()?;
        let new_id = || self.ids.next(&current, now_ms());
        let decided = plan(action, &current, &new_id)?;

        if let Some(wanted) = decided.apply {
            self.windows.apply(wanted)?;
            // A close is the one action whose effect the settings screen cannot
            // infer from a reply it never sees.
            if let Some(id) = decided.id.as_deref()
                && matches!(action, WidgetAction::Close { .. })
            {
                self.windows.announce_closed(id);
            }
        }

        // Read the list back rather than reporting the plan: the stored list is
        // what the next request will see, and reconciling it is what the app
        // actually did. It is deliberately not pruned to the windows that
        // opened — a window the compositor refused and one the user closed are
        // indistinguishable from here, and dropping either would delete the
        // user's configuration.
        let after = self.windows.specs()?;
        let widgets = after.iter().map(widget_info).collect::<Result<_, _>>()?;
        Ok(WidgetsReply::ok(widgets, decided.id))
    }
}

/// The pipe's widget host, for the app to install into the attach pipe.
///
/// Installed once from `setup`. A request that arrives before it is registered
/// is refused with "this build of Vitals does not offer widgets over the attach
/// pipe", which the CLI reports as its own failure rather than as "no app".
pub fn pipe_host(app: tauri::AppHandle) -> Arc<dyn WidgetHost> {
    Arc::new(PipeWidgetHost::new(DesktopWindows(app)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::atomic::AtomicUsize;
    use vitals_ipc::attach::{AttachClient, AttachServer, ProtocolError};

    fn spec() -> WidgetSpec {
        WidgetSpec {
            id: "w1".to_owned(),
            kind: "cpu".to_owned(),
            size: "small".to_owned(),
            style: "glass".to_owned(),
            layout: "ring".to_owned(),
            opacity: 0.6,
            always_on_top: true,
            click_through: false,
            x: 100.0,
            y: 40.0,
            width: 190.0,
            height: 120.0,
            remote_connection_id: None,
        }
    }

    /// The guarantee: a spec crosses the boundary with exactly the field names
    /// the frontend's `WidgetInstance` declares, camelCase, and with no field
    /// the interface does not have.
    ///
    /// The names are parsed out of `model.ts` rather than listed here, so a
    /// rename on the frontend side is a failing test on this side. Nothing else
    /// can notice one: `WidgetSpec` carries no `ts_rs` derive (only
    /// `vitals-core`'s types are generated into `@vitals/protocol`), so no
    /// compiler ever sees both declarations.
    #[test]
    fn widget_spec_matches_the_frontend() {
        let source = frontend_model();
        let value = serde_json::to_value(spec()).expect("a spec serialises");
        let object = value.as_object().expect("a spec is a JSON object");
        let mut keys: Vec<String> = object.keys().cloned().collect();
        keys.sort_unstable();

        // `WidgetWindowSpec` extends the interface with the two pixels the
        // window is actually built at, so those two are this side's own
        // addition and are expected here explicitly rather than smuggled in by
        // the parsed interface.
        let mut expected = frontend_widget_fields(&source);
        expected.push("height".to_owned());
        expected.push("width".to_owned());
        expected.sort_unstable();
        assert_eq!(keys, expected);

        // And back: a payload the frontend builds from its own interface
        // deserialises into the same struct with nothing lost. A key it spells
        // differently is simply ignored by serde, so this is the assertion that
        // catches a typo rather than a rename.
        let from_frontend: WidgetSpec = serde_json::from_value(json!({
            "id": "w1",
            "kind": "cpu",
            "size": "small",
            "style": "glass",
            "layout": "ring",
            "opacity": 0.6,
            "alwaysOnTop": true,
            "clickThrough": false,
            "x": 100.0,
            "y": 40.0,
            "width": 190.0,
            "height": 120.0,
            "remoteConnectionId": null
        }))
        .expect("the frontend's own JSON deserialises");
        assert_eq!(from_frontend, spec());
    }

    /// The guarantee: a window label carries the widget id in a form the
    /// frontend can recover, and nothing else looks like a widget.
    ///
    /// Seen to fail: changing `label_for` to the bare id — `id_from_label`
    /// then answers `None` for it and the round-trip assertion fails.
    #[test]
    fn a_widget_label_round_trips_its_id() {
        assert_eq!(label_for("w1"), "widget-w1");
        assert_eq!(id_from_label("widget-w1"), Some("w1"));
        // The main window, the overlay, and a label that is only the prefix.
        assert_eq!(id_from_label("main"), None);
        assert_eq!(id_from_label("hud"), None);
        assert_eq!(id_from_label("widget-"), None);
    }

    /// The guarantee: `same` answers "unchanged" for a position that came back
    /// from the window manager a fraction of a pixel away, and "changed" for a
    /// real move. This is what keeps a drag from flooding the screen with
    /// events and, more importantly, keeps a rounding difference from being
    /// read as the user having moved the window.
    ///
    /// Seen to fail: making the comparison exact (`a.x == b.x`) — the
    /// sub-pixel case then answers "changed".
    #[test]
    fn a_sub_pixel_difference_is_not_a_move() {
        let base = spec();
        let mut nudged = base.clone();
        nudged.x += 0.2;
        assert!(same(&base, &nudged));

        let mut moved = base.clone();
        moved.x += 8.0;
        assert!(!same(&base, &moved));

        let mut restyled = base.clone();
        restyled.style = "plain".to_owned();
        assert!(!same(&base, &restyled));
    }

    /// The guarantee: a spec that cannot become a window — no id, no size —
    /// is refused rather than turned into a zero-sized window or a label that
    /// clashes with the main one.
    ///
    /// Seen to fail: dropping the `width > 0.0` term — an empty id still
    /// passes, so the assertion on the zero-width spec fails.
    #[test]
    fn an_unbuildable_spec_is_refused() {
        assert!(spec().is_valid());

        let mut unnamed = spec();
        unnamed.id = String::new();
        assert!(!unnamed.is_valid());

        let mut flat = spec();
        flat.width = 0.0;
        assert!(!flat.is_valid());

        let mut squashed = spec();
        squashed.height = 0.0;
        assert!(!squashed.is_valid());
    }

    /// The guarantee: a position that came back from the window manager
    /// differs in the last bits, so a difference too small to be a move is the
    /// same window - and a difference the user could see is not.
    ///
    /// Seen to fail: setting `EPSILON` to `0.0`, or comparing the coordinates
    /// with `==`, makes the sub-threshold case a different window and the
    /// first assertion fails.
    #[test]
    fn same_treats_an_unmeasurable_difference_as_the_same_window() {
        let base = spec();
        let mut wobbled = spec();
        wobbled.x += EPSILON / 4.0;
        wobbled.y -= EPSILON / 4.0;
        assert!(same(&base, &wobbled));

        let mut moved = spec();
        moved.x += EPSILON * 2.0;
        assert!(!same(&base, &moved));

        // A field that is not a coordinate has no epsilon: a different style
        // is a different window however small the change looks.
        let mut restyled = spec();
        restyled.style = "plain".to_owned();
        assert!(!same(&base, &restyled));

        // A JSON `Value` is compared here only to prove the helper is wired
        // to serde_json rather than to a hand-rolled encoder.
        let encoded: Value = serde_json::to_value(WidgetRef { id: "w1".into() }).expect("encodes");
        assert_eq!(encoded, json!({ "id": "w1" }));
    }

    /// The guarantee: a widget carries the machine it watches across the wire
    /// in both directions — a machine survives the round trip, and `null`
    /// means "this machine" rather than "unknown".
    #[test]
    fn a_widget_carries_the_machine_it_watches_through_the_spec_round_trip() {
        let mut watching = spec();
        watching.remote_connection_id = Some("home".to_owned());
        let value = serde_json::to_value(&watching).expect("a spec serialises");
        assert_eq!(value["remoteConnectionId"], json!("home"));
        let back: WidgetSpec =
            serde_json::from_value(value).expect("what it wrote, it can read back");
        assert_eq!(back, watching, "the machine survives the round trip");

        // This machine is spelled `null`, not left out: the frontend's
        // interface always declares the key, and a key that sometimes vanishes
        // is one that reads as "unknown" on the other side.
        let mine = serde_json::to_value(spec()).expect("a spec serialises");
        assert_eq!(mine["remoteConnectionId"], Value::Null);

        // An absent key is still read as this machine, so a hand-written spec
        // need not name it — that is a merge of an older registry, not an
        // error.
        let mut payload = serde_json::to_value(spec()).expect("a spec serialises");
        let removed = payload
            .as_object_mut()
            .expect("a spec is a JSON object")
            .remove("remoteConnectionId");
        assert_eq!(removed, Some(Value::Null));
        let older: WidgetSpec =
            serde_json::from_value(payload).expect("a spec without the key still reads");
        assert_eq!(older.remote_connection_id, None);
    }

    /// The guarantee: two widgets that differ only in which machine they watch
    /// are different windows, so pointing one at another machine rebuilds it
    /// rather than leaving the old machine's readings on screen.
    #[test]
    fn a_widget_that_switches_machine_is_a_different_window() {
        let base = spec();
        let mut elsewhere = spec();
        elsewhere.remote_connection_id = Some("home".to_owned());
        assert!(
            !same(&base, &elsewhere),
            "another machine is another window"
        );

        let mut also_elsewhere = spec();
        also_elsewhere.remote_connection_id = Some("home".to_owned());
        assert!(same(&elsewhere, &also_elsewhere));
    }

    // ── The numbers and rules this crate reads out of `model.ts` ────────────
    //
    // `WidgetSpec` has no `ts_rs` derive, so the two sides are pinned by
    // reading the frontend's source from here. A second copy that cannot drift
    // unnoticed is a copy, not a duplicate.

    /// The frontend's widget model, as the shipped file spells it.
    ///
    /// Read, never skipped when it is missing: a silently skipped pin is a
    /// rename that lands unnoticed, which is the failure these tests exist to
    /// prevent.
    fn frontend_model() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/features/widgets/model.ts");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
    }

    /// The camelCase names `WidgetInstance` declares, in declaration order.
    fn frontend_widget_fields(source: &str) -> Vec<String> {
        let head = "export interface WidgetInstance {";
        let start = source
            .find(head)
            .unwrap_or_else(|| panic!("{head} is no longer declared"));
        let body = &source[start + head.len()..];
        let end = body
            .find("\n}")
            .unwrap_or_else(|| panic!("WidgetInstance's body is never closed"));
        body[..end]
            .lines()
            .filter_map(|line| {
                // `readonly remoteConnectionId: string | null;` — the name runs
                // up to the `:` (or the `?` of an optional member). Doc-comment
                // lines start with `*`, so they are skipped here.
                let rest = line.trim().strip_prefix("readonly ")?;
                let name = rest.split([':', '?']).next()?.trim();
                (!name.is_empty()).then(|| name.to_owned())
            })
            .collect()
    }

    /// The body of `export function <name>(…)`, whitespace collapsed, so a rule
    /// written as an expression can be pinned as text instead of being
    /// re-implemented as a second TypeScript parser.
    fn frontend_function_body(source: &str, name: &str) -> String {
        let head = format!("export function {name}(");
        let start = source
            .find(&head)
            .unwrap_or_else(|| panic!("{name} is no longer declared"));
        let body = &source[start..];
        let open = body
            .find('{')
            .unwrap_or_else(|| panic!("{name} has no body"));
        let close = body[open..]
            .find("\n}")
            .unwrap_or_else(|| panic!("{name}'s body is never closed"));
        body[open + 1..open + close]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The integer that follows `anchor` in the frontend's source.
    fn frontend_integer(source: &str, anchor: &str) -> i64 {
        let at = source
            .find(anchor)
            .unwrap_or_else(|| panic!("{anchor} is no longer declared"));
        source[at + anchor.len()..]
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or_else(|error| panic!("{anchor} is not followed by a number: {error}"))
    }

    /// The `{ width, height }` pair the frontend's `GEOMETRY` gives a size step.
    fn frontend_size(source: &str, size: &str) -> (i64, i64) {
        let head = format!("{size}: {{ width: ");
        let at = source
            .find(&head)
            .unwrap_or_else(|| panic!("GEOMETRY no longer declares {size}"));
        let rest = &source[at + head.len()..];
        let width: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let height: String = rest[width.len()..]
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        let number = |digits: &str, which: &str| {
            digits
                .parse()
                .unwrap_or_else(|error| panic!("{size} {which}: {error}"))
        };
        (number(&width, "width"), number(&height, "height"))
    }

    /// The kind names a rule written as `kind === 'a' || kind === 'b' ? …`
    /// mentions, in the order the frontend writes them.
    fn kinds_named_by(rule: &str) -> Vec<String> {
        rule.split("kind === '")
            .skip(1)
            .map(|chunk| chunk.split('\'').next().unwrap_or_default().to_owned())
            .collect()
    }

    /// The guarantee: the size table this crate uses to size a widget the pipe
    /// opened is the frontend's own `GEOMETRY`, read out of `model.ts`, and the
    /// cap it enforces is the frontend's `MAX_WIDGETS` — so a widget opened from
    /// a terminal is the size the settings screen would have made it, and the
    /// list stops at the same number.
    #[test]
    fn the_pipe_sizes_a_new_widget_the_way_the_frontend_would() {
        let source = frontend_model();

        for (name, size) in [
            ("small", WidgetSize::Small),
            ("medium", WidgetSize::Medium),
            ("large", WidgetSize::Large),
        ] {
            let (width, height) = frontend_size(&source, name);
            assert_eq!(
                geometry_for(size),
                (width as f64, height as f64),
                "the size the pipe gives `{name}`"
            );
        }

        assert_eq!(
            i64::try_from(MAX_WIDGETS).expect("the cap fits an i64"),
            frontend_integer(&source, "MAX_WIDGETS = "),
            "the cap the pipe enforces"
        );
    }

    /// The guarantee: a widget the pipe opened without a position lands on the
    /// frontend's own cascade — its step, its wrap and its origin, read out of
    /// `model.ts` — so the CLI's widget is offset from the widgets already there
    /// rather than stacked exactly on top of them, and the tenth is still on
    /// screen.
    #[test]
    fn the_pipe_places_a_new_widget_where_the_frontend_would() {
        let source = frontend_model();

        assert_eq!(
            frontend_integer(&source, "CASCADE_STEP = "),
            CASCADE_STEP as i64,
            "the cascade step"
        );
        assert_eq!(
            frontend_integer(&source, "CASCADE_WRAP = "),
            i64::try_from(CASCADE_WRAP).expect("the wrap fits an i64"),
            "the cascade wrap"
        );
        let anchor = "CASCADE_ORIGIN = { x: ";
        let declared = &source[source.find(anchor).expect("CASCADE_ORIGIN is declared")..];
        assert_eq!(
            CASCADE_ORIGIN,
            (
                frontend_integer(declared, anchor) as f64,
                frontend_integer(declared, "y: ") as f64
            ),
            "the cascade origin"
        );

        // The placement itself: the first widget at the origin, a later one a
        // step along, and the ninth back at the origin rather than off screen.
        assert_eq!(cascade_position(0), CASCADE_ORIGIN);
        assert_eq!(
            cascade_position(2),
            (
                CASCADE_ORIGIN.0 + 2.0 * CASCADE_STEP,
                CASCADE_ORIGIN.1 + 2.0 * CASCADE_STEP
            )
        );
        assert_eq!(
            cascade_position(CASCADE_WRAP),
            CASCADE_ORIGIN,
            "the tenth widget starts the cascade again"
        );
    }

    /// The guarantee: which layouts a kind may name is the frontend's rule —
    /// read out of `model.ts` and then cross-checked against this crate for
    /// every kind the wire can carry — so the pipe cannot accept a layout the
    /// settings screen would refuse, nor refuse one it would offer.
    #[test]
    fn the_pipe_offers_the_layouts_the_frontend_offers() {
        let source = frontend_model();

        // Both rules are expressions rather than data, so they are pinned as
        // text; what they *mean* for each kind is asserted against this crate's
        // answer below, which is what makes the pin worth having.
        let default = frontend_function_body(&source, "defaultLayoutFor");
        let offered = frontend_function_body(&source, "layoutsFor");
        assert_eq!(
            kinds_named_by(&default),
            vec!["cpu"],
            "the kind `defaultLayoutFor` opens as a ring: {default}"
        );
        assert_eq!(
            kinds_named_by(&offered),
            vec!["cpu", "memory"],
            "the kinds `layoutsFor` offers a ring to: {offered}"
        );
        assert!(
            offered.contains("['ring', 'stack', 'split']")
                && offered.contains("['stack', 'split']"),
            "the layouts each kind is offered changed: {offered}"
        );

        for kind in [
            WidgetKind::Cpu,
            WidgetKind::Memory,
            WidgetKind::Disk,
            WidgetKind::Network,
            WidgetKind::Thermal,
            WidgetKind::Overview,
        ] {
            let name = kind.as_str();
            let ring_offered = kinds_named_by(&offered)
                .iter()
                .any(|known| known.as_str() == name);
            assert_eq!(
                layout_fits(kind, WidgetLayout::Ring),
                ring_offered,
                "`{name}` and the ring"
            );
            assert!(
                layout_fits(kind, WidgetLayout::Stack),
                "`{name}` and the stack"
            );
            assert!(
                layout_fits(kind, WidgetLayout::Split),
                "`{name}` and the split"
            );
            let default_is_ring = kinds_named_by(&default)
                .iter()
                .any(|known| known.as_str() == name);
            assert_eq!(
                default_layout(kind) == WidgetLayout::Ring,
                default_is_ring,
                "`{name}`'s default layout"
            );
        }
    }

    // ── The attach pipe's widget host ───────────────────────────────────────
    //
    // `FakeWindows` stands in for the app with the same contract `reconcile`
    // offers — "make the stored list this list" — and counters where the
    // windows were. What is under test is the host's own behaviour: which list
    // it hands to the app, and what it answers.

    fn spec_named(id: &str) -> WidgetSpec {
        WidgetSpec {
            id: id.to_owned(),
            ..spec()
        }
    }

    fn open_cpu() -> WidgetAction {
        WidgetAction::Open {
            kind: WidgetKind::Cpu,
            size: WidgetSize::Small,
            layout: WidgetLayout::Stack,
            x: None,
            y: None,
        }
    }

    /// A stand-in for the running app's widgets.
    #[derive(Default)]
    struct FakeWindows {
        specs: Mutex<Vec<WidgetSpec>>,
        /// How often the app was asked to make its windows match a list.
        applied: AtomicUsize,
        /// Every list it was asked to match, so a test can see what it was told.
        applied_lists: Mutex<Vec<Vec<WidgetSpec>>>,
        closed: Mutex<Vec<String>>,
    }

    impl FakeWindows {
        /// An app already showing `ids`.
        fn showing(ids: &[&str]) -> Self {
            let fake = Self::default();
            fake.apply(ids.iter().map(|id| spec_named(id)).collect())
                .expect("the fake accepts a list");
            fake.forget_counts();
            fake
        }

        /// Back to "nothing has happened yet", after seeding.
        fn forget_counts(&self) {
            self.applied.store(0, Ordering::SeqCst);
            self.applied_lists.lock().clear();
        }

        fn held(&self) -> Vec<String> {
            self.specs
                .lock()
                .iter()
                .map(|spec| spec.id.clone())
                .collect()
        }
    }

    impl WidgetWindows for FakeWindows {
        fn specs(&self) -> Result<Vec<WidgetSpec>, String> {
            Ok(self.specs.lock().clone())
        }

        fn apply(&self, wanted: Vec<WidgetSpec>) -> Result<(), String> {
            // The shape `reconcile` has: an unbuildable spec is dropped and
            // what survives becomes the stored list.
            let wanted: Vec<WidgetSpec> = wanted.into_iter().filter(WidgetSpec::is_valid).collect();
            self.applied.fetch_add(1, Ordering::SeqCst);
            self.applied_lists.lock().push(wanted.clone());
            *self.specs.lock() = wanted;
            Ok(())
        }

        fn announce_closed(&self, id: &str) {
            self.closed.lock().push(id.to_owned());
        }
    }

    /// The pipe owns its host, so the fake is shared with the test through an
    /// `Arc`. `WidgetWindows` belongs to this crate, so the delegation lives
    /// here instead of a blanket impl the production code has no use for.
    impl WidgetWindows for Arc<FakeWindows> {
        fn specs(&self) -> Result<Vec<WidgetSpec>, String> {
            (**self).specs()
        }

        fn apply(&self, wanted: Vec<WidgetSpec>) -> Result<(), String> {
            (**self).apply(wanted)
        }

        fn announce_closed(&self, id: &str) {
            (**self).announce_closed(id);
        }
    }

    /// A bound pipe whose widget host is `fake`, and the name the CLI would
    /// connect to. The server is returned because dropping it closes the pipe.
    fn pipe_with(tag: &str, fake: Arc<FakeWindows>) -> (AttachServer, String) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "vitals-widget-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let server = AttachServer::start(&name, "test".to_owned()).expect("the pipe binds");
        server.set_widget_host(Arc::new(PipeWidgetHost::new(fake)));
        (server, name)
    }

    /// One request, as `vitals widget` sends it: connect, ask, disconnect.
    fn ask(name: &str, action: WidgetAction) -> std::io::Result<WidgetsReply> {
        let (client, _) = AttachClient::connect(name).expect("the app answers a hello");
        client.widget(action)
    }

    /// The guarantee: `open` over the pipe creates exactly one window, through
    /// the same reconciliation the settings screen uses, and the reply is the
    /// full list of what is open afterwards.
    #[test]
    fn opening_a_widget_through_the_pipe_creates_exactly_one_window() {
        let fake = Arc::new(FakeWindows::default());
        let (_server, name) = pipe_with("open", Arc::clone(&fake));

        let reply = ask(
            &name,
            WidgetAction::Open {
                kind: WidgetKind::Memory,
                size: WidgetSize::Medium,
                layout: WidgetLayout::Ring,
                x: Some(100.0),
                y: Some(40.0),
            },
        )
        .expect("the app answers a well-formed open");

        assert_eq!(reply.status, "ok");
        assert_eq!(reply.widgets.len(), 1, "one widget: {:?}", reply.widgets);
        let opened = &reply.widgets[0];
        assert_eq!(opened.kind, WidgetKind::Memory);
        assert_eq!(opened.size, WidgetSize::Medium);
        assert_eq!(
            opened.layout,
            WidgetLayout::Ring,
            "memory offers a ring, so the one asked for is kept"
        );
        let id = reply
            .id
            .clone()
            .expect("the reply names the widget it created");
        assert_eq!(opened.id, id);

        assert_eq!(
            fake.applied.load(Ordering::SeqCst),
            1,
            "one reconciliation, not one per field"
        );
        let held = fake.specs.lock().clone();
        assert_eq!(held.len(), 1, "exactly one window: {:?}", fake.held());
        assert_eq!(held[0].id, id);
        assert_eq!(
            (held[0].x, held[0].y),
            (100.0, 40.0),
            "the position asked for"
        );
        assert_eq!((held[0].width, held[0].height), (280.0, 170.0), "medium");
        assert_eq!(held[0].kind, "memory");
        assert_eq!(held[0].size, "medium");
        assert_eq!(held[0].style, "plain", "the default `addWidget` uses");
        assert_eq!(held[0].opacity, 1.0);
        assert!(held[0].always_on_top);
        assert!(!held[0].click_through);
        assert_eq!(
            held[0].remote_connection_id, None,
            "a widget asked for from a terminal watches this machine"
        );
        assert!(
            fake.closed.lock().is_empty(),
            "nothing was closed, so nothing is announced closed"
        );
    }

    /// The guarantee: listing widgets on an app that has none is a successful
    /// answer with no widgets and no id, and it touches nothing.
    #[test]
    fn listing_widgets_on_an_empty_registry_answers_with_no_widgets_and_no_id() {
        let fake = Arc::new(FakeWindows::default());
        let (_server, name) = pipe_with("empty", Arc::clone(&fake));

        let reply = ask(&name, WidgetAction::List).expect("the app answers a list");

        assert_eq!(reply.status, "ok");
        assert!(reply.widgets.is_empty(), "no widgets: {:?}", reply.widgets);
        assert_eq!(reply.id, None, "a list names no widget");
        assert_eq!(
            fake.applied.load(Ordering::SeqCst),
            0,
            "a read-only action touches no window"
        );
    }

    /// The guarantee: a list reports every widget the app holds, in the order
    /// it holds them, described with the kinds the wire names.
    #[test]
    fn listing_widgets_reports_the_widgets_the_app_holds() {
        let fake = Arc::new(FakeWindows::showing(&["w1", "w2"]));
        let (_server, name) = pipe_with("list", Arc::clone(&fake));

        let reply = ask(&name, WidgetAction::List).expect("the app answers a list");

        assert_eq!(reply.status, "ok");
        let reported: Vec<(&str, WidgetKind)> = reply
            .widgets
            .iter()
            .map(|widget| (widget.id.as_str(), widget.kind))
            .collect();
        assert_eq!(
            reported,
            vec![("w1", WidgetKind::Cpu), ("w2", WidgetKind::Cpu)],
            "the list is what the app holds, in its order"
        );
        assert_eq!(reply.id, None);
        assert_eq!(fake.applied.load(Ordering::SeqCst), 0);
    }

    /// The guarantee: closing an id that was never open is answered as the state
    /// the caller asked for — success, the list unchanged, no id — and no window
    /// is touched or announced.
    #[test]
    fn closing_an_id_that_is_not_open_is_a_defined_no_op_reply() {
        let fake = Arc::new(FakeWindows::showing(&["w1"]));
        let (_server, name) = pipe_with("close-unknown", Arc::clone(&fake));

        let reply = ask(
            &name,
            WidgetAction::Close {
                id: "w9".to_owned(),
            },
        )
        .expect("an already-gone widget is not an error");

        assert_eq!(reply.status, "ok");
        assert_eq!(reply.id, None, "no widget was removed, so none is named");
        assert_eq!(fake.held(), vec!["w1"], "the list is untouched");
        assert_eq!(fake.applied.load(Ordering::SeqCst), 0);
        assert!(fake.closed.lock().is_empty());
    }

    /// The guarantee: closing a widget removes exactly that one, keeps the
    /// others, names the one it removed, and tells the settings screen — without
    /// that, a widget closed from a terminal would reappear on the next edit
    /// there.
    #[test]
    fn closing_a_widget_through_the_pipe_removes_exactly_that_window() {
        let fake = Arc::new(FakeWindows::showing(&["w1", "w2", "w3"]));
        let (_server, name) = pipe_with("close", Arc::clone(&fake));

        let reply = ask(
            &name,
            WidgetAction::Close {
                id: "w2".to_owned(),
            },
        )
        .expect("the app answers a close");

        assert_eq!(reply.id.as_deref(), Some("w2"));
        let reported: Vec<&str> = reply
            .widgets
            .iter()
            .map(|widget| widget.id.as_str())
            .collect();
        assert_eq!(reported, vec!["w1", "w3"]);
        assert_eq!(fake.held(), vec!["w1", "w3"], "and that is what is stored");
        assert_eq!(
            fake.closed.lock().clone(),
            vec!["w2".to_owned()],
            "announced once, for the widget that went away"
        );
        assert_eq!(fake.applied.load(Ordering::SeqCst), 1);
    }

    /// The guarantee: the twelfth widget is the last one this app opens. One
    /// more is refused with a message naming the limit, and nothing is created —
    /// a silently discarded window is a widget the user can see is missing.
    #[test]
    fn opening_more_widgets_than_the_cap_is_refused_rather_than_creating_a_thirteenth() {
        let ids: Vec<String> = (0..MAX_WIDGETS).map(|n| format!("w{n}")).collect();
        let borrowed: Vec<&str> = ids.iter().map(String::as_str).collect();
        let fake = Arc::new(FakeWindows::showing(&borrowed));
        assert_eq!(fake.held().len(), MAX_WIDGETS);
        let (_server, name) = pipe_with("cap", Arc::clone(&fake));

        let error = ask(&name, open_cpu()).expect_err("the cap refuses a thirteenth");

        let refused = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<ProtocolError>())
            .expect("the app's refusal arrives as a protocol error");
        let limit = format!("{MAX_WIDGETS} widgets");
        assert!(
            matches!(refused, ProtocolError::Server(message) if message.contains(&limit)),
            "the refusal names the limit ({limit}): {refused}"
        );
        assert_eq!(
            fake.applied.load(Ordering::SeqCst),
            0,
            "the refused request reconciled nothing"
        );
        assert_eq!(fake.held().len(), MAX_WIDGETS);
    }

    /// The guarantee: a request naming a layout the kind cannot draw is answered
    /// with the layout the kind gets by default, and that is what is stored — so
    /// the reply reports what the app is drawing rather than what was asked for.
    #[test]
    fn a_layout_the_kind_cannot_draw_is_answered_with_the_kinds_own_layout() {
        let fake = Arc::new(FakeWindows::default());
        let (_server, name) = pipe_with("ring-on-disk", Arc::clone(&fake));

        let reply = ask(
            &name,
            WidgetAction::Open {
                kind: WidgetKind::Disk,
                size: WidgetSize::Small,
                layout: WidgetLayout::Ring,
                x: Some(300.0),
                y: Some(120.0),
            },
        )
        .expect("the app answers");

        assert_eq!(reply.widgets.len(), 1, "one widget: {:?}", reply.widgets);
        assert_eq!(
            reply.widgets[0].layout,
            WidgetLayout::Stack,
            "a byte counter has no ring to draw"
        );
        let held = fake.specs.lock().clone();
        assert_eq!(held[0].layout, "stack", "and that is what the app stores");
    }

    /// The guarantee: a position with only one coordinate is no position. The
    /// widget cascades instead of being placed at one coordinate the caller
    /// never named.
    #[test]
    fn a_half_named_position_cascades_rather_than_placing_the_widget() {
        let fake = Arc::new(FakeWindows::default());
        let (_server, name) = pipe_with("half-position", Arc::clone(&fake));

        let _answer = ask(
            &name,
            WidgetAction::Open {
                kind: WidgetKind::Cpu,
                size: WidgetSize::Small,
                layout: WidgetLayout::Stack,
                x: Some(300.0),
                y: None,
            },
        )
        .expect("the app answers");

        let held = fake.specs.lock().clone();
        assert_eq!(
            (held[0].x, held[0].y),
            cascade_position(0),
            "the position nobody asked for"
        );
    }

    /// The guarantee: a second widget opened without a position cascades a step
    /// further, so two widgets asked for from a terminal are not stacked on one
    /// spot where they look like one.
    #[test]
    fn a_second_widget_without_a_position_cascades_one_step_further() {
        let fake = Arc::new(FakeWindows::showing(&["w1"]));
        let (_server, name) = pipe_with("cascade", Arc::clone(&fake));

        let _answer = ask(&name, open_cpu()).expect("the app answers");

        let held = fake.specs.lock().clone();
        assert_eq!(held.len(), 2, "one more window: {:?}", fake.held());
        assert_eq!(
            (held[1].x, held[1].y),
            cascade_position(1),
            "one step along the cascade, not on top of the first widget"
        );
    }

    /// The guarantee: a widget the app cannot describe is refused by name.
    /// Leaving it out of the list would be a `--list` that quietly denies a
    /// window the user can see with their own eyes.
    #[test]
    fn a_stored_widget_this_build_cannot_describe_is_named_in_the_refusal() {
        let fake = Arc::new(FakeWindows::default());
        let mut strange = spec_named("w1");
        strange.kind = "banana".to_owned();
        fake.specs.lock().push(strange);
        let (_server, name) = pipe_with("unknown-stored", Arc::clone(&fake));

        let error = ask(&name, WidgetAction::List).expect_err("the app refuses to answer");

        let refused = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<ProtocolError>())
            .expect("the refusal arrives as a protocol error");
        assert!(
            matches!(refused, ProtocolError::Server(message)
                if message.contains("banana") && message.contains("w1")),
            "the refusal names both the widget and what it cannot read: {refused}"
        );
        assert_eq!(
            fake.applied.load(Ordering::SeqCst),
            0,
            "a list reconciles nothing, even when it refuses"
        );
    }

    /// The guarantee: a request this build does not understand is answered with
    /// an error and creates nothing — never a window, never a panic.
    ///
    /// The CLI cannot send this (`WidgetAction` has no other verbs), so the line
    /// is written by hand, which is the only way to reach the decoder with a
    /// verb it does not know. Windows-gated: the pipe is reached through
    /// `\\.\pipe`, and the same refusal by way of `AttachClient` is covered on
    /// every platform in `vitals-ipc`'s own tests.
    #[cfg(windows)]
    #[test]
    fn an_unknown_verb_is_answered_with_an_error_and_creates_no_widget() {
        use std::io::{BufRead as _, BufReader, Write as _};

        let fake = Arc::new(FakeWindows::default());
        let (_server, name) = pipe_with("unknown-verb", Arc::clone(&fake));

        let pipe = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!(r"\\.\pipe\{name}"))
            .expect("the pipe opens for this user");
        let mut pipe = BufReader::new(pipe);
        pipe.get_mut()
            .write_all(br#"{"op":"widget","action":{"verb":"destroy-everything"}}"#)
            .expect("the request is written");
        pipe.get_mut().write_all(b"\n").expect("the request ends");
        let mut line = String::new();
        pipe.read_line(&mut line).expect("the app answers");
        assert!(
            line.contains(r#""status":"error""#),
            "an error reply rather than a reply: {line:?}"
        );
        assert_eq!(
            fake.applied.load(Ordering::SeqCst),
            0,
            "an unreadable request creates no window"
        );
        assert!(fake.specs.lock().is_empty());
    }
}
