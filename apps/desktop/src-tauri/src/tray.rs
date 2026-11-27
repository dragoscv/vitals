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
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem};
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

/// Sentinel for "the plain mark is drawn". Distinct from [`NO_PERCENT`] so the
/// first frame with the icon switched **off** still replaces the placeholder
/// bitmap, and so a switch back to off redraws exactly once.
const PLAIN_MARK: u16 = u16::MAX - 1;

/// Which reading the tray icon carries.
///
/// `Off` is the default and costs nothing: no value is computed and no bitmap
/// is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayMetric {
    #[default]
    Off,
    Cpu,
    CpuTemperature,
    Memory,
    Network,
    Battery,
}

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
    /// "Restart to update". Optional so a webview from before it existed
    /// (none ships, but a mismatched dev build can) still deserialises.
    #[serde(default = "default_restart_to_update")]
    pub restart_to_update: String,
    pub cpu: String,
    pub memory: String,
    pub gpu: String,
    /// Body of the one-time "still running" notification.
    pub still_running: String,
    /// Title of the once-a-day notification while an update waits for
    /// approval. `{version}` is substituted in Rust, because the toast is
    /// built where the version is known.
    #[serde(default = "default_update_ready_title")]
    pub update_ready_title: String,
    /// Body of that notification: what the user has to do about it.
    #[serde(default = "default_update_ready_body")]
    pub update_ready_body: String,
    /// Appended to the tooltip while an update is waiting, so the tray says
    /// what the badge means rather than leaving a colour to be guessed at.
    #[serde(default = "default_update_waiting")]
    pub update_waiting: String,
}

impl Default for TrayStrings {
    fn default() -> Self {
        Self {
            show: "Show Vitals".into(),
            pause: "Pause sampling".into(),
            task_manager: "Open Windows Task Manager".into(),
            quit: "Quit".into(),
            restart_to_update: default_restart_to_update(),
            cpu: "CPU".into(),
            memory: "Memory".into(),
            gpu: "GPU".into(),
            still_running: "Vitals is still running in the tray.".into(),
            update_ready_title: default_update_ready_title(),
            update_ready_body: default_update_ready_body(),
            update_waiting: default_update_waiting(),
        }
    }
}

fn default_restart_to_update() -> String {
    "Restart to update".into()
}

fn default_update_ready_title() -> String {
    "Vitals {version} is ready".into()
}

fn default_update_ready_body() -> String {
    "Restart to update, and Windows will ask for administrator.".into()
}

fn default_update_waiting() -> String {
    "update {version} ready".into()
}

/// Tray state, managed for the lifetime of the process.
pub struct Tray {
    /// Last rounded CPU percentage handed to the OS as an icon, or
    /// [`NO_PERCENT`].
    last_percent: AtomicU16,
    /// The current [`TrayMetric`], as a `u8` so it can be swapped without a
    /// lock from the sampler thread.
    metric: std::sync::atomic::AtomicU8,
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
    /// `None` off Windows: the entry is left out of the menu entirely rather
    /// than shown disabled, because there is no real Task Manager to open and
    /// a permanently greyed item names a feature this build does not have.
    task_manager: Option<MenuItem<Wry>>,
    /// Shown only while an update is downloaded (DEV-3): the one-click way
    /// to install it, with any prompt following the click.
    update: MenuItem<Wry>,
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
            .field("metric", &self.metric())
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
            metric: std::sync::atomic::AtomicU8::new(TrayMetric::Off as u8),
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

    /// Which reading the icon carries.
    #[must_use]
    pub fn metric(&self) -> TrayMetric {
        match self.metric.load(Ordering::Relaxed) {
            1 => TrayMetric::Cpu,
            2 => TrayMetric::CpuTemperature,
            3 => TrayMetric::Memory,
            4 => TrayMetric::Network,
            5 => TrayMetric::Battery,
            _ => TrayMetric::Off,
        }
    }

    /// Changes the reading. Forces the next [`observe`] to redraw by clearing
    /// the remembered value, so switching the option takes effect immediately
    /// rather than at the next change of the number.
    pub fn set_metric(&self, metric: TrayMetric) {
        self.metric.store(metric as u8, Ordering::Relaxed);
        self.last_percent.store(NO_PERCENT, Ordering::Relaxed);
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
            if let Some(task_manager) = handles.task_manager.as_ref() {
                let _ = task_manager.set_text(&strings.task_manager);
            }
            let _ = handles.update.set_text(&strings.restart_to_update);
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

    /// The title and body of the "an update is waiting" notice, for a version.
    #[must_use]
    pub fn update_prompt(&self, version: &str) -> (String, String) {
        let strings = self.strings.lock();
        (
            strings.update_ready_title.replace("{version}", version),
            strings.update_ready_body.clone(),
        )
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
    let update = MenuItem::with_id(
        app,
        "update",
        &strings.restart_to_update,
        false,
        None::<&str>,
    )?;
    // Only offered where there is a real Task Manager to open. On other
    // platforms the item is left out rather than disabled: a greyed entry
    // names a feature this build does not have, and every other macOS app's
    // menu simply does not contain it.
    #[cfg(windows)]
    let task_manager = Some(MenuItem::with_id(
        app,
        "task_manager",
        &strings.task_manager,
        true,
        None::<&str>,
    )?);
    #[cfg(not(windows))]
    let task_manager: Option<MenuItem<Wry>> = None;

    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&show, &pause];
    if let Some(task_manager) = task_manager.as_ref() {
        items.push(task_manager);
    }
    items.push(&update);
    items.push(&quit);
    let menu = Menu::with_items(app, &items)?;

    *tray.menu.lock() = Some(MenuHandles {
        show,
        pause,
        task_manager,
        update,
        quit,
        paused: is_paused(app),
    });

    TrayIconBuilder::with_id(TRAY_ID)
        // The plain mark until the first frame says otherwise; with the option
        // off (the default) this is also its final state.
        .icon(icon(None))
        .tooltip("Vitals")
        .menu(&menu)
        // macOS renders a template as a mask, so the mark takes the menu
        // bar's own light/dark appearance instead of sitting in it as a
        // coloured bitmap beside every native item.
        .icon_as_template(cfg!(target_os = "macos"))
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
        "update" => {
            // Off the event loop: the UAC prompt can sit for minutes.
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(error) = crate::updates::install_now(&app) {
                    tracing::warn!(%error, "restart to update from the tray failed");
                }
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Enables "Restart to update" once an update is downloaded.
pub fn set_update_ready(app: &AppHandle, ready: bool) {
    if let Some(tray) = app.try_state::<Tray>()
        && let Some(handles) = tray.menu.lock().as_ref()
    {
        let _ = handles.update.set_enabled(ready);
    }
}

/// Hides or shows the notification-area icon without destroying it.
///
/// The update path hides it right before the setup starts, so no dead icon
/// lingers after the process exits, and shows it again when the setup did
/// not start — removing it instead, as Tauri's own pre-exit cleanup does,
/// left a running app with no tray after a declined UAC prompt (ENG2-1).
pub fn set_visible(app: &AppHandle, visible: bool) {
    if let Some(handle) = app.tray_by_id(TRAY_ID) {
        let _ = handle.set_visible(visible);
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

    // Nothing is computed when the option is off: the match below short-
    // circuits before touching a single reading.
    let value = value_for(tray.metric(), system);
    let encoded = value.map_or(PLAIN_MARK, u16::from);
    if tray.last_percent.swap(encoded, Ordering::Relaxed) != encoded
        && let Some(handle) = app.tray_by_id(TRAY_ID)
    {
        let _ = handle.set_icon(Some(icon(value)));
    }

    let waiting = crate::updates::waiting_version(app);
    let tooltip = tooltip_text(&tray.strings.lock(), system, waiting.as_deref());
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
///
/// `waiting` is the version of an update that is downloaded and not yet
/// installed. It goes in the tooltip rather than only on the tray icon so the
/// badge is explained in words: a mark with no legend is a decoration.
fn tooltip_text(strings: &TrayStrings, system: &SystemMetrics, waiting: Option<&str>) -> String {
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
    if let Some(version) = waiting {
        parts.push(strings.update_waiting.replace("{version}", version));
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

/// The value the icon should carry, or `None` for the plain mark.
///
/// Three of these are percentages already — CPU, memory and battery charge —
/// and two are not, so each states its scale here rather than leaving the
/// reader to guess what a full icon means.
#[must_use]
pub fn value_for(metric: TrayMetric, system: &SystemMetrics) -> Option<u8> {
    match metric {
        TrayMetric::Off => None,
        TrayMetric::Cpu => Some(round_percent(system.cpu.total.0)),
        // A CPU temperature is not a percentage. 0–100 °C maps onto the whole
        // icon, which is where the range of every consumer CPU lives; anything
        // hotter clamps to full rather than wrapping.
        TrayMetric::CpuTemperature => system.cpu.temperature.map(|t| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let value = t.0.clamp(0.0, 100.0).round() as u8;
            value
        }),
        TrayMetric::Memory => Some(memory_percent(system)),
        TrayMetric::Network => Some(network_percent(system)),
        // A charge is a percentage already, and it is the reading this fill was
        // shaped for. A machine with no battery — a desktop — has no value
        // here, so it draws the plain mark rather than a full icon; that is the
        // same rule as every other reading the OS cannot report.
        TrayMetric::Battery => system.battery.as_ref().map(|b| round_percent(b.charge.0)),
    }
}

/// Traffic on the busiest connected adapter, as a share of a 1 Gbit/s link.
///
/// A fixed ceiling rather than the adapter's negotiated speed: link speed is
/// reported in units that differ between drivers, and a "full" icon that means
/// something different on every machine is worse than a stated constant.
fn network_percent(system: &SystemMetrics) -> u8 {
    #[allow(clippy::cast_precision_loss)] // Throughput only; bytes/s, not money.
    const CEILING_BYTES_PER_SEC: f64 = 125.0 * 1024.0 * 1024.0;
    let busiest = system
        .networks
        .iter()
        .filter(|adapter| adapter.connected)
        .map(|adapter| (adapter.rx.0 + adapter.tx.0) as f64)
        .fold(0.0_f64, f64::max);
    #[allow(clippy::cast_possible_truncation)]
    round_percent((busiest / CEILING_BYTES_PER_SEC * 100.0) as f32)
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
/// bottom in proportion to the chosen reading.
///
/// A fill rather than a number: 32 px is not enough for two legible digits at
/// 100% scaling, and the tooltip carries the exact figures anyway. `None`
/// draws the plain mark, which is what the icon looks like when the option is
/// off. The fill is
/// the brand's dark-mode fern (`#6cb882`, brand/BRAND.md): it reads 7:1 on the
/// tile, and the dark tile itself carries the icon on a light taskbar — which
/// the system accent colour would not reliably do.
fn render(value: Option<u8>) -> Vec<u8> {
    const BG: [u8; 4] = [0x13, 0x29, 0x1B, 0xFF];
    const FILL: [u8; 4] = [0x6C, 0xB8, 0x82, 0xFF];
    const RADIUS: i32 = 6;

    let percent = value.unwrap_or(0);
    // At least one row whenever a reading is being drawn: without it, a
    // genuine 0 % would render byte-for-byte like the icon being switched
    // off, and the two states would be indistinguishable on the taskbar.
    let filled_rows = if value.is_some() {
        (i32::from(percent) * ICON_SIZE / 100).max(1)
    } else {
        0
    };
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

fn icon(value: Option<u8>) -> Image<'static> {
    // Non-negative by construction; the constant is signed only for the
    // corner arithmetic in `render`.
    #[allow(clippy::cast_sign_loss)]
    const EDGE: u32 = ICON_SIZE as u32;
    Image::new_owned(render(value), EDGE, EDGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_is_exactly_one_rgba_pixel_per_position() {
        // `Image::new_owned` trusts the length; a wrong one is a garbled icon
        // or a panic deep in the shell rather than a compile error.
        assert_eq!(render(Some(50)).len(), ICON_BYTES);
        assert_eq!(render(None).len(), ICON_BYTES);
    }

    #[test]
    fn an_idle_machine_and_a_pinned_one_do_not_look_the_same() {
        assert_ne!(render(Some(0)), render(Some(100)));
    }

    #[test]
    fn the_same_percent_renders_identically_so_skipping_a_redraw_is_safe() {
        // The whole cost argument for the tray rests on this: if the same
        // input could produce different bytes, skipping `set_icon` when the
        // rounded percent is unchanged would drop a real update.
        assert_eq!(render(Some(43)), render(Some(43)));
    }

    #[test]
    fn corners_are_transparent_and_the_centre_is_not() {
        let pixels = render(Some(0));
        let at = |x: i32, y: i32| {
            let i = ((y * ICON_SIZE + x) * 4) as usize;
            pixels[i + 3]
        };
        assert_eq!(at(0, 0), 0, "top-left corner");
        assert_eq!(at(ICON_SIZE - 1, ICON_SIZE - 1), 0, "bottom-right corner");
        assert_eq!(at(16, 16), 0xFF, "centre");
    }

    #[test]
    fn switching_the_icon_off_draws_the_plain_mark_not_a_zero_reading() {
        // The trap this guards: "off" must not be indistinguishable from "a
        // genuine 0 %", or the two states could not be told apart on screen.
        assert_ne!(render(None), render(Some(0)));
        assert_eq!(
            value_for(TrayMetric::Off, &vitals_core::fixtures::system()),
            None
        );
    }

    #[test]
    fn the_off_metric_is_the_default_so_nothing_is_drawn_unless_asked() {
        assert_eq!(TrayMetric::default(), TrayMetric::Off);
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
        let text = tooltip_text(&strings, &system, None);
        assert!(!text.contains("GPU"), "got {text}");
        assert!(text.starts_with("CPU "), "got {text}");
    }

    #[test]
    fn memory_with_no_reported_total_reads_zero_rather_than_dividing_by_it() {
        let mut system = vitals_core::fixtures::system();
        system.memory.total = vitals_core::units::Bytes(0);
        assert_eq!(memory_percent(&system), 0);
    }

    #[test]
    fn the_battery_metric_draws_the_charge_it_has_and_plain_mark_where_there_is_no_battery() {
        let mut system = vitals_core::fixtures::system();
        assert_eq!(
            value_for(TrayMetric::Battery, &system),
            None,
            "a desktop with no battery must draw the plain mark, not a full icon"
        );
        system.battery = Some(vitals_core::metrics::BatteryMetrics {
            charge: vitals_core::units::Percent(37.4),
            charging: false,
            time_remaining_secs: None,
            power: None,
            health: None,
            cycle_count: None,
            temperature: None,
        });
        assert_eq!(value_for(TrayMetric::Battery, &system), Some(37));
    }

    #[test]
    fn every_metric_the_webview_offers_is_a_name_the_deserialiser_knows() {
        // The option list lives in the webview and the variants live here. A
        // name this enum does not know is rejected at runtime, the webview
        // swallows that error by design, and the icon silently keeps its
        // previous reading — so the two lists have to be checked against each
        // other somewhere, and this is that place.
        assert_eq!(
            serde_json::from_str::<TrayMetric>("\"off\"").expect("off must deserialise"),
            TrayMetric::Off
        );
        for name in ["cpu", "cpuTemperature", "memory", "network", "battery"] {
            let parsed: TrayMetric = serde_json::from_str(&format!("\"{name}\""))
                .unwrap_or_else(|error| panic!("{name} must deserialise: {error}"));
            assert_ne!(parsed, TrayMetric::Off, "{name} fell back to the default");
        }
    }
}
