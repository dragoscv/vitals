//! Shared application state.

use parking_lot::RwLock;
use vitals_core::capability::Capabilities;
use vitals_core::sample::SampleRate;

/// State shared across Tauri commands.
///
/// `RwLock` rather than `Mutex`: capabilities and the current rate are read
/// on every frame and written almost never, so readers must not contend.
#[derive(Debug)]
pub struct AppState {
    rate: RwLock<SampleRate>,
    capabilities: RwLock<Capabilities>,
    /// Accumulated per-application usage.
    ///
    /// A handle to the store the sampler thread writes on every tick, not a
    /// second copy. Installed by the sampler once it starts, so before that
    /// it is `None` and the history screen reports empty rather than
    /// inventing records.
    #[cfg(windows)]
    history: RwLock<Option<vitals_win::history::SharedHistory>>,
    /// The user's "record history" preference, kept here so it survives the
    /// window between the frontend sending it and the sampler attaching.
    #[cfg(windows)]
    history_enabled: std::sync::atomic::AtomicBool,
    /// The most recent per-process sample, reduced to what session rollups
    /// need.
    ///
    /// Published by the sampler every tick so request/response commands
    /// (Users, and later the LAN API) can answer from the same instant the
    /// dashboard is showing, instead of taking a second, unsynchronised
    /// snapshot of their own. Empty until the first tick lands.
    #[cfg(windows)]
    latest_processes: RwLock<std::sync::Arc<Vec<vitals_win::users::ProcessSample>>>,
    /// Which kernel counter the per-process disk figures came from.
    ///
    /// `None` until the first tick, and that is the honest answer: the
    /// enumerator only learns which class the kernel will serve by asking
    /// for it. The UI renders the unknown state rather than guessing, for
    /// the same reason every other unmeasured value here is an `Option`.
    disk_counter_source: RwLock<Option<vitals_core::process::DiskCounterSource>>,
    /// What the sampler's recorder should be doing. The sampler reads this
    /// every tick and reconciles; commands only ever write it. The SQLite
    /// connection itself never crosses a thread — readers open their own.
    recording: RwLock<RecordingSettings>,
}

/// Recorder configuration as the user has set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingSettings {
    pub history_enabled: bool,
    pub retention_days: u32,
    /// Bumped by `clear`; the sampler clears the store when it sees a change.
    pub clear_generation: u64,
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            history_enabled: false,
            retention_days: 7,
            clear_generation: 0,
        }
    }
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rate: RwLock::new(SampleRate::Normal),
            capabilities: RwLock::new(platform_capabilities()),
            #[cfg(windows)]
            history: RwLock::new(None),
            #[cfg(windows)]
            history_enabled: std::sync::atomic::AtomicBool::new(false),
            #[cfg(windows)]
            latest_processes: RwLock::new(std::sync::Arc::new(Vec::new())),
            disk_counter_source: RwLock::new(None),
            recording: RwLock::new(RecordingSettings::default()),
        }
    }

    pub fn recording(&self) -> RecordingSettings {
        *self.recording.read()
    }

    pub fn set_retention_days(&self, days: u32) {
        self.recording.write().retention_days = days;
    }

    /// Asks the sampler to wipe the time-series store on its next tick.
    pub fn request_store_clear(&self) {
        self.recording.write().clear_generation += 1;
    }

    pub fn sample_rate(&self) -> SampleRate {
        *self.rate.read()
    }

    pub fn set_sample_rate(&self, rate: SampleRate) {
        *self.rate.write() = rate;
    }

    /// Replaces the published process sample. Called by the sampler thread.
    #[cfg(windows)]
    pub fn publish_processes(&self, processes: Vec<vitals_win::users::ProcessSample>) {
        *self.latest_processes.write() = std::sync::Arc::new(processes);
    }

    /// The most recent process sample, shared rather than copied.
    #[cfg(windows)]
    pub fn latest_processes(&self) -> std::sync::Arc<Vec<vitals_win::users::ProcessSample>> {
        std::sync::Arc::clone(&self.latest_processes.read())
    }

    /// Records which disk counter the enumerator settled on. Sampler thread.
    ///
    /// Written every tick rather than once: the value is sticky within a run,
    /// but writing it unconditionally means there is no ordering question
    /// about which tick was the first.
    pub fn publish_disk_counter_source(&self, source: vitals_core::process::DiskCounterSource) {
        *self.disk_counter_source.write() = Some(source);
    }

    /// Which disk counter the Disk column is currently showing.
    pub fn disk_counter_source(&self) -> Option<vitals_core::process::DiskCounterSource> {
        *self.disk_counter_source.read()
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities.read().clone()
    }

    /// Recomputes capabilities after a privilege change, so features unlock
    /// without requiring the user to restart the app.
    pub fn refresh_capabilities(&self) {
        *self.capabilities.write() = platform_capabilities();
    }

    /// Adopts the sampler's history store and loads any saved history into it.
    #[cfg(windows)]
    pub fn attach_history(&self, history: vitals_win::history::SharedHistory) {
        history.load_from(&history_path());
        // Off until the frontend says otherwise. The store's own default is
        // on, which is right for a library and wrong for a user who has not
        // consented.
        history.set_enabled(
            self.history_enabled
                .load(std::sync::atomic::Ordering::Relaxed),
        );
        *self.history.write() = Some(history);
    }

    /// Applies the "record history" setting to both stores: the per-app
    /// tally and the machine-wide time series.
    pub fn set_history_enabled(&self, enabled: bool) {
        self.recording.write().history_enabled = enabled;
        #[cfg(windows)]
        {
            self.history_enabled
                .store(enabled, std::sync::atomic::Ordering::Relaxed);
            if let Some(history) = self.history.read().as_ref() {
                history.set_enabled(enabled);
                if !enabled {
                    // Turning it off is a good moment to make what exists durable.
                    let _ = history.save(&history_path());
                }
            }
        }
    }

    /// Reads the accumulated history.
    ///
    /// Empty before the sampler has started, which is the truthful answer:
    /// nothing has been observed yet.
    #[cfg(windows)]
    pub fn history_snapshot(&self) -> Vec<vitals_win::history::AppHistoryRecord> {
        self.history
            .read()
            .as_ref()
            .map(vitals_win::history::SharedHistory::snapshot)
            .unwrap_or_default()
    }

    /// Discards the accumulated history and the file backing it.
    ///
    /// Written through immediately: the user asked for the history to be gone,
    /// and leaving the old file to be reloaded at next start would quietly
    /// undo that.
    pub fn clear_history(&self) {
        #[cfg(windows)]
        if let Some(history) = self.history.read().as_ref() {
            history.clear_and_save(&history_path());
        }
        self.request_store_clear();
    }

    /// Persists the history, best effort.
    ///
    /// Called on shutdown. A failure here loses at most the current session's
    /// tally, which is not worth blocking exit over.
    #[cfg(windows)]
    pub fn save_history(&self) {
        if let Some(history) = self.history.read().as_ref() {
            let _ = history.save(&history_path());
        }
    }
}

/// The directory every local data file lives in.
///
/// Local app data rather than roaming: the history describes what ran on this
/// machine, so following the user to another one would be actively wrong.
#[must_use]
pub fn data_dir() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map_or_else(
            || std::path::PathBuf::from("."),
            |base| std::path::Path::new(&base).join("Vitals"),
        )
}

/// Where the accumulated per-app history is kept.
#[cfg(windows)]
fn history_path() -> std::path::PathBuf {
    data_dir().join("app-history.json")
}

/// Where the machine-wide time series and flight recorder live.
#[must_use]
pub fn store_path() -> std::path::PathBuf {
    data_dir().join("history.sqlite")
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Queries the platform backend for its capability set.
fn platform_capabilities() -> Capabilities {
    #[cfg(windows)]
    {
        // `detect()`, not `new()`: the latter is a zero-value constructor for
        // the pure capability tests, and calling it here meant an elevated
        // Vitals reported the same capabilities as an unelevated one.
        vitals_win::WindowsHost::detect().capabilities()
    }

    #[cfg(not(windows))]
    {
        // Other platforms report nothing until their backend lands, which is
        // honest: the UI will grey everything out rather than offering
        // buttons that cannot work.
        Capabilities::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_the_normal_sample_rate() {
        assert_eq!(AppState::new().sample_rate(), SampleRate::Normal);
    }

    #[test]
    fn sample_rate_is_updatable() {
        let state = AppState::new();
        state.set_sample_rate(SampleRate::Background);
        assert_eq!(state.sample_rate(), SampleRate::Background);
    }

    #[test]
    fn capabilities_are_populated_at_construction() {
        let caps = AppState::new().capabilities();
        // On Windows this is non-empty; elsewhere it is deliberately empty.
        // Either way the call must not panic and must be internally coherent.
        assert!(
            caps.available.iter().all(|c| caps.reason(*c).is_none()),
            "a capability cannot be both available and have an unavailability reason"
        );
    }
}
