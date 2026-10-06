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
    /// Taken by [`Self::stop_and_wait`]. A `Mutex` because the handle lives
    /// in Tauri's managed state, which hands out `&self` only.
    thread: parking_lot::Mutex<Option<thread::JoinHandle<()>>>,
}

impl SamplerHandle {
    /// Signals the thread to stop at its next tick.
    ///
    /// Cooperative rather than a kill: a sampler interrupted mid-syscall can
    /// leak the kernel buffer it was filling.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Stops the sampler and waits for its final flush, up to `limit`.
    ///
    /// Called on app exit. Tauri never drops managed state, so the `Drop`
    /// below never ran and the loop's closing `save_history` / `rec.flush()`
    /// was dead code: every Quit lost up to a minute of app history. The
    /// wait is bounded — a sampler stuck in a syscall must not turn Quit
    /// into a hang — and the loop polls `stop` every 50 ms, so it exits
    /// well inside the limit in practice.
    pub fn stop_and_wait(&self, limit: Duration) {
        self.stop();
        let Some(handle) = self.thread.lock().take() else {
            return;
        };
        let deadline = Instant::now() + limit;
        while !handle.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if handle.is_finished() {
            let _ = handle.join();
        } else {
            tracing::warn!(
                "sampler did not stop within {limit:?}; exiting without its final flush"
            );
        }
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
    let thread = thread::Builder::new()
        .name("vitals-sampler".into())
        // 512 KiB: the default 2 MiB is wasteful for a thread whose deepest
        // allocation is a process buffer that lives on the heap anyway.
        .stack_size(512 * 1024)
        .spawn(move || run(&app, &thread_stop))
        .expect("the sampler thread must start; the app is useless without it");

    SamplerHandle {
        stop,
        thread: parking_lot::Mutex::new(Some(thread)),
    }
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

    // The time-series store and flight recorder. Opened here so the SQLite
    // connection lives on the one thread that writes it. A failure to open
    // (read-only profile, corrupt file) disables recording and says so once;
    // it must never stop sampling.
    let mut applied = crate::state::RecordingSettings::default();
    let mut recorder = match vitals_store::Recorder::open(&crate::state::store_path()) {
        Ok(mut r) => {
            // `reconcile_recorder` only pushes a *change*, and `applied`
            // starts equal to the defaults — so the default "keep 7 days"
            // was never applied and the store kept its built-in 30 days of
            // 5-minute and a year of hourly rows. Pushed once here instead.
            r.set_retention_days(applied.retention_days);
            Some(r)
        }
        Err(error) => {
            tracing::warn!(%error, "history store unavailable; recording disabled");
            None
        }
    };

    // Backoff so a persistent failure does not spam the UI with a toast per
    // tick. Resets on the first success.
    let mut consecutive_failures = 0_u32;

    // History used to be written only when this loop exited. This is a task
    // manager: its user kills processes for a living, and a killed Vitals
    // lost the whole session's tally. A flush a minute bounds the loss to
    // sixty seconds at the cost of one small write nobody will notice.
    #[cfg(windows)]
    let mut last_flush = Instant::now();

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

        if keyframe_wanted(app) {
            backend.request_keyframe();
        }

        #[cfg(windows)]
        arm_startup_impact(app, &mut backend);

        match backend.next_frame() {
            Ok(mut frame) => {
                consecutive_failures = 0;
                #[cfg(windows)]
                feed_startup_impact(app, &mut backend, &frame);
                #[cfg(windows)]
                if let Some(state) = app.try_state::<AppState>() {
                    state.publish_processes(backend.take_rollup_sample());
                    // Which counter the Disk column is showing. Only knowable
                    // after a real enumeration, because the enumerator learns
                    // it from what the kernel agreed to serve.
                    state.publish_disk_counter_source(backend.disk_counter_source());
                }
                if let Some(rec) = recorder.as_mut() {
                    reconcile_recorder(app, rec, &mut applied);
                    // Serialised once here and once by `emit`. The recorder
                    // serialises its own copy because it leaves out owners
                    // and MAC addresses, which the UI does receive.
                    if let Err(error) = rec.observe(&mut frame) {
                        tracing::warn!(%error, "recorder write failed");
                    }
                }
                // The LAN server, only while it is running: a clone of a
                // 600-process keyframe every tick for a server that is off
                // would be the sampler paying for a feature nobody enabled.
                if let Some(server) = app.try_state::<crate::server::LanServer>()
                    && server.is_running()
                {
                    server.publish(std::sync::Arc::new(frame.clone()));
                }
                // The attach pipe, on the same terms: the clone and the fold
                // only happen while a CLI is actually connected.
                if let Some(pipe) = app.try_state::<crate::ipc::AttachPipe>()
                    && pipe.has_clients()
                {
                    pipe.publish(&std::sync::Arc::new(frame.clone()));
                }
                // Alerts run here, on the sampler thread, so they keep
                // working with the window hidden and cost one evaluation per
                // tick regardless of how many consumers read them.
                if let Some(alerts) = app.try_state::<crate::alerts::Alerts>() {
                    alerts.observe(app, frame.system());
                }
                // The tray, for the same reason: it must keep showing load
                // while the window is hidden. Both the icon and the tooltip
                // are guarded on a change, so a steady machine costs nothing.
                crate::tray::observe(app, frame.system());
                if let Some(taskbar) = app.try_state::<crate::taskbar::TaskbarLoad>() {
                    taskbar.observe(frame.system().cpu.total.0);
                }
                if app.emit(FRAME_EVENT, &frame).is_err() {
                    // The window is gone. Nothing to sample for.
                    break;
                }
                #[cfg(windows)]
                if last_flush.elapsed() >= HISTORY_FLUSH_INTERVAL {
                    if let Some(state) = app.try_state::<AppState>() {
                        state.save_history();
                    }
                    last_flush = Instant::now();
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

    // The loop ends when the window closes or the app quits. Final flush of
    // whatever accrued since the last periodic one.
    #[cfg(windows)]
    if let Some(state) = app.try_state::<AppState>() {
        state.save_history();
    }
    if let Some(rec) = recorder.as_mut() {
        let _ = rec.flush();
    }
}

/// Decides, before sampling, whether this tick still falls inside the boot
/// window, so the backend only builds per-process observations (one
/// allocation per process) while somebody will consume them.
#[cfg(windows)]
fn arm_startup_impact(app: &AppHandle, backend: &mut Backend) {
    let wants = app
        .try_state::<crate::startup_impact::StartupImpactStore>()
        .is_some_and(|store| store.is_measuring(backend.last_uptime_secs().unwrap_or(0)));
    backend.set_wants_impact(wants);
}

/// Folds the tick into the startup-impact accumulator.
///
/// Every tick, not only measured ones: the call that lands outside the
/// window is the one that finalises and persists the measurement.
#[cfg(windows)]
fn feed_startup_impact(app: &AppHandle, backend: &mut Backend, frame: &vitals_core::sample::Frame) {
    if let Some(store) = app.try_state::<crate::startup_impact::StartupImpactStore>() {
        store.observe(
            frame.system().cpu.uptime_secs,
            frame.elapsed_ms,
            &backend.take_impact_observations(),
        );
    }
}

/// Applies any settings change to the recorder. Cheap when nothing changed.
fn reconcile_recorder(
    app: &AppHandle,
    rec: &mut vitals_store::Recorder,
    applied: &mut crate::state::RecordingSettings,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let wanted = state.recording();
    if wanted == *applied {
        return;
    }
    if wanted.history_enabled != applied.history_enabled {
        rec.set_history_enabled(wanted.history_enabled);
    }
    if wanted.retention_days != applied.retention_days {
        rec.set_retention_days(wanted.retention_days);
    }
    if wanted.clear_generation != applied.clear_generation
        && let Err(error) = rec.clear()
    {
        tracing::warn!(%error, "failed to clear the history store");
    }
    *applied = wanted;
}

/// How often the accumulated history is written while running.
#[cfg(windows)]
const HISTORY_FLUSH_INTERVAL: Duration = Duration::from_secs(60);

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
    let watched = app
        .try_state::<crate::server::LanServer>()
        .is_some_and(|server| server.has_viewers());

    effective_interval(rate, watched)
        .map(u64::from)
        .map(Duration::from_millis)
}

/// The interval to sample at, given the window's rate and whether a remote
/// client is watching.
///
/// A hidden window drops to one frame every 20 s, which is right for the
/// desktop and wrong for a phone streaming the machine: its numbers froze
/// for twenty seconds at a time. A watcher raises the rate to at least 1 Hz.
/// A pause is still a pause — it is the user's explicit instruction, and a
/// phone must not be able to override it (ADR-0033).
fn effective_interval(rate: vitals_core::sample::SampleRate, watched: bool) -> Option<u32> {
    let own = rate.interval_ms()?;
    let live = vitals_core::sample::SampleRate::Normal
        .interval_ms()
        .unwrap_or(own);
    Some(if watched { own.min(live) } else { own })
}

/// The platform backend: samples, then encodes to a wire frame.
///
/// Wrapping both in one type keeps the loop above free of `cfg` branches, so
/// the scheduling logic is written once and identically on every platform.
#[cfg(windows)]
struct Backend {
    sampler: vitals_win::SystemSampler,
    frames: vitals_win::FrameBuilder,
    /// The per-session reduction of the last sample, taken before the sample
    /// is consumed by the frame builder.
    rollup: Vec<vitals_win::users::ProcessSample>,
    /// Per-process observations for the startup-impact accumulator, built
    /// only while `wants_impact` is set — the boot window is two minutes and
    /// the sampler runs for hours, so this is empty almost always.
    impact: Vec<vitals_win::startup::Observation>,
    wants_impact: bool,
    /// Uptime from the last sample, so the loop can ask whether the window
    /// is still open before the next sample is taken.
    last_uptime_secs: Option<u64>,
}

#[cfg(windows)]
impl Backend {
    fn new() -> Self {
        Self {
            sampler: vitals_win::SystemSampler::new(),
            frames: vitals_win::FrameBuilder::new(),
            rollup: Vec::new(),
            impact: Vec::new(),
            // True until the first sample says otherwise: the first tick is
            // the one most likely to be inside the window, and skipping it
            // because uptime was not yet known would lose a second of boot.
            wants_impact: true,
            last_uptime_secs: None,
        }
    }

    fn history(&self) -> vitals_win::history::SharedHistory {
        self.sampler.history()
    }

    fn next_frame(&mut self) -> vitals_core::error::Result<vitals_core::sample::Frame> {
        let sample = self.sampler.sample()?;
        self.rollup = sample
            .processes
            .iter()
            .map(|p| vitals_win::users::ProcessSample {
                pid: p.raw.key.pid,
                session_id: p.raw.session_id,
                cpu_percent: f64::from(p.cpu.0),
                private_bytes: p.raw.private_bytes,
            })
            .collect();
        self.last_uptime_secs = Some(sample.system.cpu.uptime_secs);
        if self.wants_impact {
            self.impact = crate::startup_impact::observations(&sample.processes);
        }
        Ok(self.frames.build(sample))
    }

    fn take_rollup_sample(&mut self) -> Vec<vitals_win::users::ProcessSample> {
        std::mem::take(&mut self.rollup)
    }

    fn set_wants_impact(&mut self, wants: bool) {
        self.wants_impact = wants;
    }

    fn last_uptime_secs(&self) -> Option<u64> {
        self.last_uptime_secs
    }

    fn take_impact_observations(&mut self) -> Vec<vitals_win::startup::Observation> {
        std::mem::take(&mut self.impact)
    }

    fn request_keyframe(&mut self) {
        self.frames.request_keyframe();
    }

    fn disk_counter_source(&self) -> vitals_core::process::DiskCounterSource {
        self.sampler.disk_counter_source()
    }
}

/// Whether a consumer has no baseline and needs the next frame whole.
///
/// A CLI that has just attached has no keyframe to fold deltas onto (frames
/// are not forwarded while nobody is attached, so the pipe's view is empty).
/// The webview asks once, when it starts listening — after launch or a
/// reload it would otherwise fold deltas onto nothing until the periodic
/// keyframe. Both are checked every tick so neither request is swallowed by
/// the other (`take_keyframe_request` clears its flag).
fn keyframe_wanted(app: &AppHandle) -> bool {
    let pipe = app
        .try_state::<crate::ipc::AttachPipe>()
        .is_some_and(|pipe| pipe.wants_keyframe());
    let webview = app
        .try_state::<AppState>()
        .is_some_and(|state| state.take_keyframe_request());
    pipe || webview
}

/// Placeholder until the other platform backends land.
///
/// Fails rather than returning fabricated data: an empty dashboard is
/// obviously unfinished, whereas plausible zeroes look like a working app
/// reporting a suspiciously idle machine.
#[cfg(not(windows))]
struct Backend;

#[cfg(not(windows))]
// Same method set as the Windows backend so the sampling loop is written
// once; this one has no state to read, which clippy calls an unused `self`.
#[allow(clippy::unused_self)]
impl Backend {
    const fn new() -> Self {
        Self
    }

    fn next_frame(&mut self) -> vitals_core::error::Result<vitals_core::sample::Frame> {
        // No sampler on this platform yet (ADR 0007). `Os` with a context
        // string rather than `Unsupported(<capability>)`, which would name
        // one specific feature as the thing missing when it is all of them.
        Err(vitals_core::Error::Os {
            context: "no sampling backend for this platform yet".into(),
            code: 0,
        })
    }

    const fn request_keyframe(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watching_phone_lifts_a_hidden_window_to_live_rate() {
        use vitals_core::sample::SampleRate;
        assert_eq!(
            effective_interval(SampleRate::Background, false),
            Some(20_000)
        );
        assert_eq!(
            effective_interval(SampleRate::Background, true),
            Some(1_000)
        );
        // Never slows a faster rate the window asked for.
        assert_eq!(effective_interval(SampleRate::High, true), Some(500));
    }

    #[test]
    fn a_watching_phone_cannot_undo_a_pause() {
        use vitals_core::sample::SampleRate;
        assert_eq!(effective_interval(SampleRate::Paused, true), None);
    }

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
                thread: parking_lot::Mutex::new(None),
            };
        }
        assert!(
            stop.load(Ordering::Relaxed),
            "dropping the handle should signal stop"
        );
    }

    #[test]
    fn stop_and_wait_returns_only_after_the_thread_has_run_its_final_work() {
        // Quit must wait for the sampler's closing flush, not race it.
        let stop = Arc::new(AtomicBool::new(false));
        let flushed = Arc::new(AtomicBool::new(false));
        let (thread_stop, thread_flushed) = (Arc::clone(&stop), Arc::clone(&flushed));
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(1));
            }
            thread::sleep(Duration::from_millis(30));
            thread_flushed.store(true, Ordering::Relaxed);
        });
        let handle = SamplerHandle {
            stop,
            thread: parking_lot::Mutex::new(Some(thread)),
        };
        handle.stop_and_wait(Duration::from_secs(2));
        assert!(
            flushed.load(Ordering::Relaxed),
            "returned before the final work ran"
        );
    }

    #[test]
    fn stop_and_wait_gives_up_at_the_limit_rather_than_hanging_quit() {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = thread::spawn(|| thread::sleep(Duration::from_secs(3)));
        let handle = SamplerHandle {
            stop,
            thread: parking_lot::Mutex::new(Some(thread)),
        };
        let started = Instant::now();
        handle.stop_and_wait(Duration::from_millis(50));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
