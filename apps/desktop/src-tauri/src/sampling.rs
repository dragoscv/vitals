//! The sampling thread.
//!
//! Runs off the main thread and pushes frames to the webview as events.
//!
//! Deliberately **not** request/response over `invoke`: the frontend asking
//! for data 1–4 times a second, with thousands of process rows serialised
//! each time, would run the serialisation on the webview's main thread and
//! stutter the UI. Pushing means the webview does nothing until a frame
//! actually arrives.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Event name for a metrics frame. Must match the frontend's listener.
pub const FRAME_EVENT: &str = "vitals://frame";

/// Event name for sampler failures.
pub const ERROR_EVENT: &str = "vitals://sampler-error";

/// Handle to a running sampler thread.
#[derive(Debug)]
pub struct SamplerHandle {
    stop: Arc<AtomicBool>,
}

impl SamplerHandle {
    /// Signals the thread to stop at its next tick.
    ///
    /// Cooperative rather than a kill: a sampler interrupted mid-syscall can
    /// leak the kernel buffer it was filling.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for SamplerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts the sampling thread.
///
/// Returns a handle that stops the thread when dropped, so a closed window
/// cannot leave a thread sampling forever.
pub fn spawn(app: AppHandle) -> SamplerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);

    // Startup, not the hot path: if the OS refuses a thread, aborting with a
    // message is the right outcome — there is nothing to show without it.
    #[allow(clippy::expect_used)]
    thread::Builder::new()
        .name("vitals-sampler".into())
        // 512 KiB: the default 2 MiB is wasteful for a thread whose deepest
        // allocation is a process buffer that lives on the heap anyway.
        .stack_size(512 * 1024)
        .spawn(move || run(&app, &thread_stop))
        .expect("the sampler thread must start; the app is useless without it");

    SamplerHandle { stop }
}

/// The sampling loop.
fn run(app: &AppHandle, stop: &AtomicBool) {
    let mut backend = Backend::new();

    // The store lives in the sampler; the commands need a handle to the same
    // one. Attaching here rather than at construction is what loads the saved
    // history exactly once, on the thread that will be writing it.
    #[cfg(windows)]
    if let Some(state) = app.try_state::<AppState>() {
        state.attach_history(backend.history());
    }

    // Backoff so a persistent failure does not spam the UI with a toast per
    // tick. Resets on the first success.
    let mut consecutive_failures = 0_u32;

    while !stop.load(Ordering::Relaxed) {
        let Some(interval) = current_interval(app) else {
            // Paused. Poll the flag rather than sampling, so resuming is
            // immediate but a paused app costs nothing. Without this branch
            // Paused would silently behave like some default rate, and the
            // setting would appear to do nothing.
            sleep_interruptibly(PAUSED_POLL_INTERVAL, stop);
            continue;
        };
        let started = Instant::now();

        match backend.next_frame() {
            Ok(frame) => {
                consecutive_failures = 0;
                if app.emit(FRAME_EVENT, &frame).is_err() {
                    // The window is gone. Nothing to sample for.
                    break;
                }
            }
            Err(error) => {
                consecutive_failures = consecutive_failures.saturating_add(1);

                // Report the first few, then go quiet. A machine that has
                // genuinely lost a subsystem should not produce an endless
                // stream of identical notifications.
                if consecutive_failures <= 3 {
                    let _ = app.emit(ERROR_EVENT, error.to_string());
                }
            }
        }

        // Subtract the work from the interval so cadence stays honest. A
        // fixed sleep after a 13ms sample makes a "1 Hz" rate actually
        // 0.987 Hz, and every rate computed from it is quietly wrong.
        let elapsed = started.elapsed();
        let remaining = interval.saturating_sub(elapsed);

        if remaining.is_zero() {
            // Sampling took longer than the interval. Yield rather than
            // spinning, so a slow machine degrades to a lower rate instead
            // of pinning a core.
            thread::yield_now();
        } else {
            sleep_interruptibly(remaining, stop);
        }
    }

    // The loop ends when the window closes or the app quits. Writing here is
    // the only place the session's accumulated history is persisted — doing
    // it every tick would mean a disk write per second for data nobody has
    // asked to see.
    #[cfg(windows)]
    if let Some(state) = app.try_state::<AppState>() {
        state.save_history();
    }
}

/// Sleeps in slices so a stop request is honoured promptly.
///
/// A single long sleep would keep the app alive for up to the full interval
/// after the window closes, which reads as a hang.
fn sleep_interruptibly(total: Duration, stop: &AtomicBool) {
    const SLICE: Duration = Duration::from_millis(50);

    let mut remaining = total;
    while !remaining.is_zero() && !stop.load(Ordering::Relaxed) {
        let slice = remaining.min(SLICE);
        thread::sleep(slice);
        remaining = remaining.saturating_sub(slice);
    }
}

/// How often a paused sampler checks whether it has been resumed.
///
/// Short enough that resuming feels instant, long enough that a paused app
/// is genuinely idle.
const PAUSED_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Reads the current interval from shared state, or `None` when paused.
///
/// Read every tick rather than cached so a rate change takes effect
/// immediately — a window losing focus should slow sampling now, not after
/// one more tick at the old rate.
fn current_interval(app: &AppHandle) -> Option<Duration> {
    let rate = app
        .try_state::<AppState>()
        .map_or(vitals_core::sample::SampleRate::Normal, |state| {
            state.sample_rate()
        });

    rate.interval_ms().map(u64::from).map(Duration::from_millis)
}

/// The platform backend: samples, then encodes to a wire frame.
///
/// Wrapping both in one type keeps the loop above free of `cfg` branches, so
/// the scheduling logic is written once and identically on every platform.
#[cfg(windows)]
struct Backend {
    sampler: vitals_win::SystemSampler,
    frames: vitals_win::FrameBuilder,
}

#[cfg(windows)]
impl Backend {
    fn new() -> Self {
        Self {
            sampler: vitals_win::SystemSampler::new(),
            frames: vitals_win::FrameBuilder::new(),
        }
    }

    fn history(&self) -> vitals_win::history::SharedHistory {
        self.sampler.history()
    }

    fn next_frame(&mut self) -> vitals_core::error::Result<vitals_core::sample::Frame> {
        let sample = self.sampler.sample()?;
        Ok(self.frames.build(sample))
    }
}

/// Placeholder until the other platform backends land.
///
/// Fails rather than returning fabricated data: an empty dashboard is
/// obviously unfinished, whereas plausible zeroes look like a working app
/// reporting a suspiciously idle machine.
#[cfg(not(windows))]
struct Backend;

#[cfg(not(windows))]
impl Backend {
    const fn new() -> Self {
        Self
    }

    fn next_frame(&mut self) -> vitals_core::error::Result<vitals_core::sample::Frame> {
        Err(vitals_core::Error::Unsupported(
            "no sampling backend for this platform yet".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_is_observable_by_the_loop() {
        let stop = AtomicBool::new(false);
        assert!(!stop.load(Ordering::Relaxed));
        stop.store(true, Ordering::Relaxed);
        assert!(stop.load(Ordering::Relaxed));
    }

    #[test]
    fn an_interruptible_sleep_returns_early() {
        // A full-interval sleep after the window closes reads as a hang.
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(60));
            flag.store(true, Ordering::Relaxed);
        });

        let started = Instant::now();
        sleep_interruptibly(Duration::from_secs(5), &stop);
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(500),
            "sleep ignored the stop flag for {elapsed:?}"
        );
    }

    #[test]
    fn a_completed_sleep_takes_roughly_the_requested_time() {
        let stop = AtomicBool::new(false);
        let started = Instant::now();
        sleep_interruptibly(Duration::from_millis(120), &stop);
        let elapsed = started.elapsed();

        assert!(
            elapsed >= Duration::from_millis(100),
            "returned early: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_millis(400),
            "overslept: {elapsed:?}"
        );
    }

    #[test]
    fn a_zero_sleep_returns_immediately() {
        let stop = AtomicBool::new(false);
        let started = Instant::now();
        sleep_interruptibly(Duration::ZERO, &stop);
        assert!(started.elapsed() < Duration::from_millis(20));
    }

    #[test]
    fn the_handle_stops_the_thread_when_dropped() {
        // A closed window must not leave a thread sampling forever.
        let stop = Arc::new(AtomicBool::new(false));
        {
            let _handle = SamplerHandle {
                stop: Arc::clone(&stop),
            };
        }
        assert!(
            stop.load(Ordering::Relaxed),
            "dropping the handle should signal stop"
        );
    }
}
