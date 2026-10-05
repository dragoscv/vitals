//! The system tray icon.
//!
//! The tray is what makes "keep monitoring with the window closed" honest.
//! Before it existed, `startMinimised` produced a process with a hidden
//! window and no way back to it short of Task Manager — the exact failure
//! this app is supposed to diagnose, caused by the app itself.
//!
//! Two costs are guarded carefully here, because the sampler calls
//! [`observe`] once a second for as long as the machine is on:
//!
//! * the icon bitmap is redrawn and handed to the OS only when the **rounded**
//!   CPU percentage changes, and
//! * the tooltip is set only when its text changes.
//!
//! Without those guards the tray would cost a bitmap upload and a shell
//! notification every tick, forever, to display a number that mostly has not
//! moved.

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use parking_lot::Mutex;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_notification::NotificationExt;

use vitals_core::metrics::SystemMetrics;
use vitals_core::sample::SampleRate;

use crate::state::AppState;

/// The tray icon's id, so it can be looked up to update its image.
const TRAY_ID: &str = "main";

/// Icon edge in pixels. Windows scales from this for every DPI it needs.
/// Signed because every pixel loop below does corner arithmetic that goes
/// negative in the intermediate steps.
const ICON_SIZE: i32 = 32;

/// Bytes in one rendered icon: four channels per pixel.
const ICON_BYTES: usize = (ICON_SIZE * ICON_SIZE * 4) as usize;

/// Sentinel for "nothing has been drawn yet". Any real percentage is 0..=100,
/// so this can never collide with one and the first frame always draws.
const NO_PERCENT: u16 = u16::MAX;

/// Localised text for everything the tray shows.
///
/// Pushed from the webview for the same reason alert titles are: the tray
/// renders in Rust because it must work with the window hidden, but i18n
/// lives in the webview. Defaults are English so a tray built before the
/// first push is never blank.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStrings {
    pub show: String,
    pub pause: String,
    /// "Open Windows Task Manager". Lives in the tray because, once Vitals
    /// has replaced Task Manager, the tray is the one place that is always
    /// reachable and is not itself Task Manager.
    pub task_manager: String,
    pub quit: String,
    pub cpu: String,
    pub memory: String,
    pub gpu: String,
    /// Body of the one-time "still running" notification.
    pub still_running: String,
}

impl Default for TrayStrings {
    fn default() -> Self {
        Self {
            show: "Show Vitals".into(),
            pause: "Pause sampling".into(),
            task_manager: "Open Windows Task Manager".into(),
            quit: "Quit".into(),
            cpu: "CPU".into(),
            memory: "Memory".into(),
            gpu: "GPU".into(),
            still_running: "Vitals is still running in the tray.".into(),
        }
    }
}

/// Tray state, managed for the lifetime of the process.
pub struct Tray {
    /// Last rounded CPU percentage handed to the OS as an icon, or
    /// [`NO_PERCENT`].
    last_percent: AtomicU16,
    last_tooltip: Mutex<String>,
    /// Whether the "still running in the tray" notification has been shown.
    /// Once per process: it is an explanation, not a reminder.
    notified_hidden: AtomicBool,
    /// The × button hides rather than closes. On by default because a system
    /// monitor you closed by accident stops monitoring silently.
    hide_on_close: AtomicBool,
    strings: Mutex<TrayStrings>,
    menu: Mutex<Option<MenuHandles>>,
}

/// Menu items kept so their text and checked state can be updated later.
struct MenuHandles {
    show: MenuItem<Wry>,
    pause: CheckMenuItem<Wry>,
    task_manager: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    /// Last value written to the pause checkbox, so the common case (no
    /// change) touches no OS state.
    paused: bool,
}

// Hand-written because Tauri's menu item handles are not `Debug`. The menu is
// summarised rather than omitted so a state dump still says whether it exists.
impl std::fmt::Debug for Tray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray")
            .field("last_percent", &self.last_percent)
            .field("hide_on_close", &self.hide_on_close)
            .field("notified_hidden", &self.notified_hidden)
            .field("menu_installed", &self.menu.lock().is_some())
            .finish_non_exhaustive()
    }
}

impl Default for Tray {
    fn default() -> Self {
        Self::new()
    }
}

impl Tray {
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_percent: AtomicU16::new(NO_PERCENT),
            last_tooltip: Mutex::new(String::new()),
            notified_hidden: AtomicBool::new(false),
            hide_on_close: AtomicBool::new(true),
            strings: Mutex::new(TrayStrings::default()),
            menu: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn close_to_tray(&self) -> bool {
        self.hide_on_close.load(Ordering::Relaxed)
    }

    pub fn set_close_to_tray(&self, enabled: bool) {
        self.hide_on_close.store(enabled, Ordering::Relaxed);
    }

    /// Replaces the localised text and re-labels the menu immediately, so a
    /// language change is visible without reopening the menu.
    pub fn set_strings(&self, strings: &TrayStrings) {
        self.strings.lock().clone_from(strings);
        if let Some(handles) = self.menu.lock().as_ref() {
            let _ = handles.show.set_text(&strings.show);
            let _ = handles.pause.set_text(&strings.pause);
            let _ = handles.task_manager.set_text(&strings.task_manager);
            let _ = handles.quit.set_text(&strings.quit);
        }
        // Force the next observe to rewrite the tooltip in the new language.
        self.last_tooltip.lock().clear();
    }

    /// Shows the "still running" notification the first time the window is
    /// hidden by the × button, and never again this process.
    fn explain_hidden_once(&self, app: &AppHandle) {
        if self.notified_hidden.swap(true, Ordering::Relaxed) {
            return;
        }
        let body = self.strings.lock().still_running.clone();
        if let Err(error) = app
            .notification()
            .builder()
            .title("Vitals")
            .body(body)
            .show()
        {
            tracing::warn!(%error, "tray notification failed");
        }
    }
}

/// Builds the tray icon and installs it.
///
/// # Errors
///
/// Returns an error if the tray cannot be created, which on Windows means the
/// shell refused a notification-area icon.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let tray = Tray::new();
    let strings = TrayStrings::default();

    let show = MenuItem::with_id(app, "show", &strings.show, true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(
        app,
        "pause",
        &strings.pause,
        true,
        is_paused(app),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", &strings.quit, true, None::<&str>)?;
    // Only offered where there is a real Task Manager to open. On other
    // platforms the item would be a button that does nothing.
    let task_manager = MenuItem::with_id(
        app,
        "task_manager",
        &strings.task_manager,
        cfg!(windows),
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&show, &pause, &task_manager, &quit])?;

    *tray.menu.lock() = Some(MenuHandles {
        show,
        pause,
        task_manager,
        quit,
        paused: is_paused(app),
    });

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(0))
        .tooltip("Vitals")
        .menu(&menu)
        // Left click belongs to "show the window"; the menu is the right
        // button. Left-clicking a monitor icon to be shown a menu instead of
        // the monitor is a small daily annoyance.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu_event(app, &event))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(tray);
    Ok(())
}

/// Brings the main window back from hidden, minimised, or merely unfocused.
///
/// All three are done unconditionally: a hidden window that is also minimised
/// needs both, and querying first costs more than the redundant call.
pub fn reveal(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn on_menu_event(app: &AppHandle, event: &MenuEvent) {
    match event.id.as_ref() {
        "show" => reveal(app),
        "pause" => toggle_pause(app),
        "task_manager" => open_real_task_manager(),
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Starts the built-in Task Manager, bypassing our own IFEO hook if it is on.
///
/// Failure is logged, not surfaced: the tray has no UI to show an error in,
/// and the user can see for themselves that nothing opened.
#[cfg(windows)]
fn open_real_task_manager() {
    if let Err(error) = vitals_win::actions::launch_real_task_manager() {
        tracing::warn!(%error, "could not open Windows Task Manager");
    }
}

#[cfg(not(windows))]
fn open_real_task_manager() {}

fn is_paused(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .is_some_and(|state| state.sample_rate() == SampleRate::Paused)
}

/// Pausing stops the sampler entirely; resuming returns to the 1 Hz default
/// rather than whatever rate the window last asked for, because the window
/// may not be visible and will re-assert its own rate when it is.
fn toggle_pause(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let paused = state.sample_rate() == SampleRate::Paused;
    state.set_sample_rate(if paused {
        SampleRate::Normal
    } else {
        SampleRate::Paused
    });
    // Reflect it now: while paused there is no frame to reach `observe`, so
    // waiting for the next tick would leave the checkbox permanently wrong.
    if let Some(tray) = app.try_state::<Tray>() {
        sync_pause(&tray, !paused);
    }
}

fn sync_pause(tray: &Tray, paused: bool) {
    let mut menu = tray.menu.lock();
    if let Some(handles) = menu.as_mut()
        && handles.paused != paused
    {
        let _ = handles.pause.set_checked(paused);
        handles.paused = paused;
    }
}

/// Called by the sampler once per frame.
///
/// Everything here is guarded on a change, so a machine sitting at a steady
/// 3% CPU costs no OS calls at all after the first frame.
pub fn observe(app: &AppHandle, system: &SystemMetrics) {
    let Some(tray) = app.try_state::<Tray>() else {
        return;
    };
    sync_pause(&tray, is_paused(app));

    let cpu = round_percent(system.cpu.total.0);
    if tray.last_percent.swap(u16::from(cpu), Ordering::Relaxed) != u16::from(cpu)
        && let Some(handle) = app.tray_by_id(TRAY_ID)
    {
        let _ = handle.set_icon(Some(icon(cpu)));
    }

    let tooltip = tooltip_text(&tray.strings.lock(), system);
    let mut last = tray.last_tooltip.lock();
    if *last != tooltip
        && let Some(handle) = app.tray_by_id(TRAY_ID)
    {
        let _ = handle.set_tooltip(Some(&tooltip));
        *last = tooltip;
    }
}

/// Called from the window's `CloseRequested` handler.
///
/// Returns `true` when the close was absorbed into a hide, so the caller
/// knows to prevent it.
pub fn hide_instead_of_closing(app: &AppHandle, window: &tauri::Window) -> bool {
    let Some(tray) = app.try_state::<Tray>() else {
        return false;
    };
    if !tray.close_to_tray() {
        return false;
    }
    let _ = window.hide();
    tray.explain_hidden_once(app);
    true
}

/// `"CPU 43 % · Memory 61 % · GPU 16 %"`, with GPU omitted when no adapter
/// reports utilisation — an unmeasured number is not zero.
fn tooltip_text(strings: &TrayStrings, system: &SystemMetrics) -> String {
    let mut parts = vec![
        format!("{} {} %", strings.cpu, round_percent(system.cpu.total.0)),
        format!("{} {} %", strings.memory, memory_percent(system)),
    ];
    if let Some(gpu) = system
        .gpus
        .iter()
        .filter_map(|gpu| gpu.utilization)
        .map(|util| round_percent(util.0))
        .max()
    {
        parts.push(format!("{} {gpu} %", strings.gpu));
    }
    parts.join(" · ")
}

fn memory_percent(system: &SystemMetrics) -> u8 {
    let total = system.memory.total.0;
    if total == 0 {
        return 0;
    }
    // f64 rather than f32: a 64 GB machine in bytes exceeds f32's exact
    // integer range, so the ratio would already be approximate.
    #[allow(clippy::cast_precision_loss)] // Ratio only; the error is far below 1%.
    let ratio = system.memory.used.0 as f64 / total as f64;
    #[allow(clippy::cast_possible_truncation)] // Multiplied into 0..=100 below.
    round_percent((ratio * 100.0) as f32)
}

/// Rounds to a whole percent, clamped to 0..=100.
///
/// Clamping rather than asserting for the same reason [`vitals_core::units::Percent`]
/// does: kernel counters occasionally report slightly over 100 from timer skew.
fn round_percent(value: f32) -> u8 {
    let clamped = value.clamp(0.0, 100.0).round();
    // Provably in 0..=100 by the clamp above, and non-negative.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        clamped as u8
    }
}

/// Draws the tray bitmap: a dark rounded tile with a fill rising from the
/// bottom in proportion to CPU load.
///
/// A fill rather than a number: 32 px is not enough for two legible digits at
/// 100% scaling, and the tooltip carries the exact figures anyway. The fill is
/// the brand's dark-mode fern (`#6cb882`, brand/BRAND.md): it reads 7:1 on the
/// tile, and the dark tile itself carries the icon on a light taskbar — which
/// the system accent colour would not reliably do.
fn render(percent: u8) -> Vec<u8> {
    const BG: [u8; 4] = [0x13, 0x29, 0x1B, 0xFF];
    const FILL: [u8; 4] = [0x6C, 0xB8, 0x82, 0xFF];
    const RADIUS: i32 = 6;

    let filled_rows = i32::from(percent) * ICON_SIZE / 100;
    let fill_top = ICON_SIZE - filled_rows;

    let mut pixels = Vec::with_capacity(ICON_BYTES);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            if outside_rounded_corner(x, y, ICON_SIZE, RADIUS) {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
            } else if y >= fill_top {
                pixels.extend_from_slice(&FILL);
            } else {
                pixels.extend_from_slice(&BG);
            }
        }
    }
    pixels
}

/// Whether a pixel falls outside the rounded rectangle, so the tile does not
/// look like a hard square wedged among the OS's own rounded icons.
fn outside_rounded_corner(x: i32, y: i32, size: i32, radius: i32) -> bool {
    let dx = if x < radius {
        radius - x
    } else if x >= size - radius {
        x - (size - radius - 1)
    } else {
        return false;
    };
    let dy = if y < radius {
        radius - y
    } else if y >= size - radius {
        y - (size - radius - 1)
    } else {
        return false;
    };
    dx * dx + dy * dy > radius * radius
}

fn icon(percent: u8) -> Image<'static> {
    // Non-negative by construction; the constant is signed only for the
    // corner arithmetic in `render`.
    #[allow(clippy::cast_sign_loss)]
    const EDGE: u32 = ICON_SIZE as u32;
    Image::new_owned(render(percent), EDGE, EDGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_is_exactly_one_rgba_pixel_per_position() {
        // `Image::new_owned` trusts the length; a wrong one is a garbled icon
        // or a panic deep in the shell rather than a compile error.
        assert_eq!(render(50).len(), ICON_BYTES);
    }

    #[test]
    fn an_idle_machine_and_a_pinned_one_do_not_look_the_same() {
        assert_ne!(render(0), render(100));
    }

    #[test]
    fn the_same_percent_renders_identically_so_skipping_a_redraw_is_safe() {
        // The whole cost argument for the tray rests on this: if the same
        // input could produce different bytes, skipping `set_icon` when the
        // rounded percent is unchanged would drop a real update.
        assert_eq!(render(43), render(43));
    }

    #[test]
    fn corners_are_transparent_and_the_centre_is_not() {
        let pixels = render(0);
        let at = |x: i32, y: i32| {
            let i = ((y * ICON_SIZE + x) * 4) as usize;
            pixels[i + 3]
        };
        assert_eq!(at(0, 0), 0, "top-left corner");
        assert_eq!(at(ICON_SIZE - 1, ICON_SIZE - 1), 0, "bottom-right corner");
        assert_eq!(at(16, 16), 0xFF, "centre");
    }

    #[test]
    fn an_unmeasurable_percentage_is_clamped_rather_than_wrapping() {
        // A counter reading 100.4 from timer skew must not become 100.4 as u8
        // nonsense, and a negative must not wrap to 255.
        assert_eq!(round_percent(100.4), 100);
        assert_eq!(round_percent(140.0), 100);
        assert_eq!(round_percent(-3.0), 0);
        assert_eq!(round_percent(42.6), 43);
    }

    #[test]
    fn the_tooltip_omits_a_gpu_that_reports_nothing() {
        let strings = TrayStrings::default();
        let mut system = vitals_core::fixtures::system();
        system.gpus.clear();
        let text = tooltip_text(&strings, &system);
        assert!(!text.contains("GPU"), "got {text}");
        assert!(text.starts_with("CPU "), "got {text}");
    }

    #[test]
    fn memory_with_no_reported_total_reads_zero_rather_than_dividing_by_it() {
        let mut system = vitals_core::fixtures::system();
        system.memory.total = vitals_core::units::Bytes(0);
        assert_eq!(memory_percent(&system), 0);
    }
}
