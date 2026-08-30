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
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rate: RwLock::new(SampleRate::Normal),
            capabilities: RwLock::new(platform_capabilities()),
            #[cfg(windows)]
            history: RwLock::new(None),
        }
    }

    pub fn sample_rate(&self) -> SampleRate {
        *self.rate.read()
    }

    pub fn set_sample_rate(&self, rate: SampleRate) {
        *self.rate.write() = rate;
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
        *self.history.write() = Some(history);
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
    #[cfg(windows)]
    pub fn clear_history(&self) {
        if let Some(history) = self.history.read().as_ref() {
            history.clear_and_save(&history_path());
        }
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

/// Where the accumulated history is kept.
///
/// Local app data rather than roaming: the history describes what ran on this
/// machine, so following the user to another one would be actively wrong.
#[cfg(windows)]
fn history_path() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA").map_or_else(
        || std::path::PathBuf::from("vitals-history.json"),
        |base| {
            std::path::Path::new(&base)
                .join("Vitals")
                .join("app-history.json")
        },
    )
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
