//! CPU load on the taskbar button, as the button's progress fill.
//!
//! Windows draws a progress bar across a taskbar button
//! (`ITaskbarList3::SetProgressValue`, which Tauri exposes as
//! `set_progress_bar`). Vitals uses it as a live CPU gauge while its window
//! is open: the user sees the machine's load on the taskbar without
//! switching to the app.
//!
//! # Smooth, and cheap when nothing moves
//!
//! The sampler reports once a second, so setting the value per frame would
//! step. An animator thread eases the drawn value toward the latest reading
//! (critically damped, ~250 ms to settle) at up to 30 updates a second, and
//! sleeps on a condition variable once it has arrived — a steady machine
//! costs nothing between samples, and a minimised or hidden window none at
//! all.
//!
//! # Colour
//!
//! Normal below 75 % (Windows draws it in the system accent colour — blue
//! by default on Windows 11, not green), Paused (amber) to 90 %, Error (red)
//! above: the taskbar's only three colours. Measured on the reference
//! machine 2026-10-06: the fill tracked Vitals' own CPU figure (~50 %
//! drawn at ~50 %), in the accent blue.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Manager};

/// Frame interval while animating.
const FRAME: Duration = Duration::from_millis(33);
/// Time constant of the easing: 63 % of the way in this long.
const TAU_SECS: f32 = 0.09;

#[derive(Debug, Default)]
struct Shared {
    /// Latest CPU reading, 0–100; `None` hides the bar.
    target: Option<f32>,
    enabled: bool,
}

/// Drives the taskbar progress; managed as Tauri state.
#[derive(Debug, Clone)]
pub struct TaskbarLoad {
    shared: Arc<(Mutex<Shared>, Condvar)>,
}

impl TaskbarLoad {
    /// Starts the animator thread for `app`'s main window.
    pub fn spawn(app: &AppHandle) -> Self {
        let this = Self {
            shared: Arc::new((
                Mutex::new(Shared {
                    target: None,
                    enabled: true,
                }),
                Condvar::new(),
            )),
        };
        let shared = Arc::clone(&this.shared);
        let app = app.clone();
        let _ = std::thread::Builder::new()
            .name("vitals-taskbar".into())
            .spawn(move || animate(&app, &shared));
        this
    }

    /// The latest CPU reading. Wakes the animator only if it changed.
    pub fn observe(&self, cpu_percent: f32) {
        let (lock, wake) = &*self.shared;
        let mut state = lock.lock();
        if state.target.is_none_or(|t| (t - cpu_percent).abs() >= 0.5) {
            state.target = Some(cpu_percent.clamp(0.0, 100.0));
            wake.notify_one();
        }
    }

    /// The user's setting. Off clears the bar at once.
    pub fn set_enabled(&self, enabled: bool) {
        let (lock, wake) = &*self.shared;
        lock.lock().enabled = enabled;
        wake.notify_one();
    }
}

/// The taskbar's colour for a load.
fn status_for(percent: f32) -> ProgressBarStatus {
    if percent >= 90.0 {
        ProgressBarStatus::Error
    } else if percent >= 75.0 {
        ProgressBarStatus::Paused
    } else {
        ProgressBarStatus::Normal
    }
}

/// One easing step from `current` toward `target` over `dt`.
fn ease(current: f32, target: f32, dt: Duration) -> f32 {
    let alpha = 1.0 - (-dt.as_secs_f32() / TAU_SECS).exp();
    current + (target - current) * alpha
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=100 after clamp
fn to_progress(percent: f32) -> u64 {
    // At least 1: progress 0 with status Normal draws nothing, and an idle
    // machine should still show that the gauge is alive.
    percent.round().clamp(1.0, 100.0) as u64
}

fn animate(app: &AppHandle, shared: &(Mutex<Shared>, Condvar)) {
    let (lock, wake) = shared;
    let mut drawn: Option<f32> = None;
    let mut last_sent: Option<(u64, u8)> = None;
    let mut last_tick = Instant::now();

    loop {
        let Some(window) = app.get_webview_window("main") else {
            return; // The app is shutting down.
        };
        let (target, enabled) = {
            let state = lock.lock();
            (state.target, state.enabled)
        };
        let show = enabled
            && window.is_visible().unwrap_or(false)
            && !window.is_minimized().unwrap_or(false);

        let now = Instant::now();
        let dt = now
            .duration_since(last_tick)
            .min(Duration::from_millis(250));
        last_tick = now;

        let settled = if let (true, Some(target)) = (show, target) {
            let next = drawn.map_or(target, |d| ease(d, target, dt));
            drawn = Some(next);
            let progress = to_progress(next);
            let status = status_for(next);
            let key = (progress, status as u8);
            if last_sent != Some(key) {
                let _ = window.set_progress_bar(ProgressBarState {
                    status: Some(status),
                    progress: Some(progress),
                });
                last_sent = Some(key);
            }
            (target - next).abs() < 0.25
        } else {
            if last_sent.take().is_some() {
                let _ = window.set_progress_bar(ProgressBarState {
                    status: Some(ProgressBarStatus::None),
                    progress: None,
                });
            }
            drawn = None;
            true
        };

        if settled {
            // Nothing to animate: sleep until a new reading or a setting
            // change, with a one-second cap so a window that is shown or
            // restored picks the bar back up within a frame of data.
            let mut state = lock.lock();
            wake.wait_for(&mut state, Duration::from_secs(1));
        } else {
            std::thread::sleep(FRAME);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_eases_toward_the_reading_rather_than_jumping() {
        let one_frame = ease(0.0, 100.0, FRAME);
        assert!(one_frame > 10.0 && one_frame < 50.0, "{one_frame}");
        // Within a quarter of a second it is essentially there.
        let mut v = 0.0;
        for _ in 0..8 {
            v = ease(v, 100.0, FRAME);
        }
        assert!(v > 94.0, "{v}");
    }

    #[test]
    fn colour_bands_match_busy_and_pegged() {
        assert!(matches!(status_for(40.0), ProgressBarStatus::Normal));
        assert!(matches!(status_for(80.0), ProgressBarStatus::Paused));
        assert!(matches!(status_for(95.0), ProgressBarStatus::Error));
    }

    #[test]
    fn an_idle_machine_still_shows_a_sliver() {
        assert_eq!(to_progress(0.0), 1);
        assert_eq!(to_progress(42.4), 42);
        assert_eq!(to_progress(120.0), 100);
    }
}
