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
}

impl AppState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rate: RwLock::new(SampleRate::Normal),
            capabilities: RwLock::new(platform_capabilities()),
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
        vitals_win::WindowsHost::new().capabilities()
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
