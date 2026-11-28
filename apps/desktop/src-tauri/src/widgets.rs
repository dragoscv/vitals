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

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{Emitter as _, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

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
    let wanted: Vec<WidgetSpec> = specs
        .into_iter()
        .filter(WidgetSpec::is_valid)
        .take(MAX_WIDGETS)
        .collect();

    let closed = with_specs(&app, |live| {
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
        let previous = stored(&app, &spec.id);
        match ensure(&app, spec) {
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
    store(&app, &wanted);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

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
        }
    }

    /// The guarantee: a spec crosses the boundary with exactly the field names
    /// the frontend's `WidgetInstance` declares, camelCase and no extras.
    ///
    /// Seen to fail: renaming `click_through`'s serde output to snake case —
    /// the key set differs and the test reports `click_through` missing and
    /// `clickThrough` present.
    #[test]
    fn widget_spec_matches_the_frontend() {
        let value = serde_json::to_value(spec()).expect("a spec serialises");
        let object = value.as_object().expect("a spec is a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "alwaysOnTop",
                "clickThrough",
                "height",
                "id",
                "kind",
                "layout",
                "opacity",
                "size",
                "style",
                "width",
                "x",
                "y",
            ]
        );
        // And back: a payload the frontend builds from its own interface
        // deserialises into the same struct. A key it spells differently is
        // simply ignored by serde, so this is the assertion that catches it.
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
            "height": 120.0
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
}
