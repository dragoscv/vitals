//! Startup impact: feeding the boot-window accumulator and keeping its result.
//!
//! The accumulator itself is pure and lives in `vitals_win::startup::impact`.
//! This module owns the two things it cannot: the sampler tick that feeds it,
//! and the file that carries the last measurement across restarts so the
//! Startup screen can show *last* boot's figures when this run started too
//! late to measure anything.
//!
//! The file sits beside `history.sqlite` and `lan-tokens.json`, written the
//! same way: best effort, logged on failure, never fatal. A boot whose
//! measurement did not persist shows an em dash next time, which is true.

use std::path::PathBuf;

use parking_lot::Mutex;
use vitals_core::startup::StartupImpact;
use vitals_win::startup::MeasuredWindow;
#[cfg(windows)]
use vitals_win::startup::{ImpactAccumulator, Observation};

/// Managed state: the live accumulator and the most recent complete window.
#[derive(Debug)]
pub struct StartupImpactStore {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    #[cfg(windows)]
    accumulator: ImpactAccumulator,
    /// The window to answer queries from. Loaded from disk at start; replaced
    /// by this run's window once it closes, if this run measured one.
    window: Option<MeasuredWindow>,
    /// Whether this run's window has been finalised and written.
    #[cfg(windows)]
    finished: bool,
}

impl Default for StartupImpactStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupImpactStore {
    /// Loads the previous measurement, if any, so the screen has something
    /// true to show before this run's window closes.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                #[cfg(windows)]
                accumulator: ImpactAccumulator::new(),
                window: load(&impact_path()),
                #[cfg(windows)]
                finished: false,
            }),
        }
    }

    /// The impact of one executable, from whichever window is current.
    ///
    /// `None` when there is no window or the executable was never observed
    /// in it. Every field of a returned value is `Some`, because a tally that
    /// exists was measured.
    #[must_use]
    pub fn lookup(&self, image_path: &str) -> Option<StartupImpact> {
        let inner = self.inner.lock();
        let window = inner.window.as_ref()?;
        let tally = window.lookup(image_path)?;
        Some(StartupImpact {
            cpu_ms: Some(tally.cpu_ms),
            disk_bytes: Some(tally.disk_bytes),
            measured_at_ms: Some(window.measured_at_ms),
        })
    }

    /// When the current window was measured, for the screen's caption.
    #[must_use]
    pub fn measured_at_ms(&self) -> Option<u64> {
        self.inner.lock().window.as_ref().map(|w| w.measured_at_ms)
    }

    /// Whether the boot window is still open, so the caller can decide
    /// whether to pay for building observations at all.
    #[cfg(windows)]
    #[must_use]
    pub fn is_measuring(&self, uptime_secs: u64) -> bool {
        let inner = self.inner.lock();
        !inner.finished && inner.accumulator.is_measuring(uptime_secs)
    }

    /// Folds one tick in. Called by the sampler thread every frame.
    ///
    /// Once the window closes this finalises the measurement exactly once —
    /// replacing the loaded window with this boot's and writing it to disk —
    /// and every later call costs a lock and a boolean.
    #[cfg(windows)]
    pub fn observe(&self, uptime_secs: u64, elapsed_ms: u32, observations: &[Observation]) {
        let mut inner = self.inner.lock();
        if inner.finished {
            return;
        }

        // Resolving a full image path needs a handle per process, which is
        // the per-tick cost the enumerator exists to avoid. It is paid here
        // only inside the two-minute window, and only once per process
        // because the accumulator remembers the answer.
        let counted = inner
            .accumulator
            .observe(uptime_secs, elapsed_ms, observations, |key| {
                vitals_win::actions::executable_path(key).ok().flatten()
            });
        if counted {
            return;
        }

        inner.finished = true;
        if let Some(window) = inner.accumulator.finish(now_ms()) {
            save(&impact_path(), &window);
            inner.window = Some(window);
        }
        // Nothing measured: keep whatever the previous boot left. Overwriting
        // it with "nothing" would throw away a true figure for no reason.
    }
}

/// Builds the per-process observations for one tick from the sampler's
/// processes. Separate from `observe` so the sampler can skip the allocation
/// entirely once the window has closed.
#[cfg(windows)]
#[must_use]
pub fn observations(processes: &[vitals_win::sampler::SampledProcess]) -> Vec<Observation> {
    processes
        .iter()
        .map(|p| Observation {
            key: p.raw.key,
            name: p.raw.name.clone(),
            cpu_percent: p.cpu.0,
            disk_bytes_per_sec: p.disk_read.0.saturating_add(p.disk_write.0),
        })
        .collect()
}

fn impact_path() -> PathBuf {
    crate::state::data_dir().join("startup-impact.json")
}

fn load(path: &std::path::Path) -> Option<MeasuredWindow> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<MeasuredWindow>(&bytes) {
        Ok(window) => Some(window),
        Err(error) => {
            // A file from an older build with a different shape is not worth
            // a dialog; the screen shows em dashes until the next boot writes
            // a fresh one.
            tracing::warn!(%error, ?path, "startup impact file unreadable; ignoring it");
            None
        }
    }
}

fn save(path: &std::path::Path, window: &MeasuredWindow) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_vec_pretty(window) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(path, bytes) {
                tracing::warn!(%error, ?path, "could not persist the startup impact window");
            }
        }
        Err(error) => tracing::warn!(%error, "could not serialise the startup impact window"),
    }
}

#[cfg(windows)]
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_window_is_read_back_by_the_next_run() {
        let dir = std::env::temp_dir().join(format!("vitals-impact-{}", std::process::id()));
        let path = dir.join("startup-impact.json");
        let mut totals = std::collections::BTreeMap::new();
        totals.insert(
            "c:\\a\\b.exe".to_owned(),
            vitals_win::startup::impact::Tally {
                cpu_ms: 1_234,
                disk_bytes: 56_789,
            },
        );
        let window = MeasuredWindow {
            measured_at_ms: 1_700_000_000_000,
            window_secs: 120,
            totals,
        };

        save(&path, &window);
        let loaded = load(&path);
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(loaded, Some(window));
    }

    #[test]
    fn an_unreadable_file_yields_no_window_rather_than_a_default_one() {
        let dir = std::env::temp_dir().join(format!("vitals-impact-bad-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("startup-impact.json");
        std::fs::write(&path, b"{ not json").expect("write fixture");

        let loaded = load(&path);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(loaded.is_none());
    }
}
