//! Runs the alert engine on the sampler thread and fans the result out.
//!
//! Three consumers, one evaluation:
//!
//! - the webview, via [`ALERTS_EVENT`] carrying the full active list each
//!   time it changes (not each tick — a list that did not change is not an
//!   event);
//! - Windows toast notifications, for transitions the user asked to hear
//!   about, gated by the settings the Notifications panel has shown for a
//!   long time without anything reading them;
//! - the tray and the LAN API, which read [`AppState::alerts`] on demand.
//!
//! Notification preferences are pushed from the frontend (`set_alert_prefs`)
//! because the settings store is a webview-side plugin; the defaults here are
//! the panel's defaults, so a fresh install behaves the same before the first
//! push arrives.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use vitals_core::alerts::{Alert, AlertKind, AlertTransitions, Engine};
use vitals_core::metrics::SystemMetrics;

/// Event carrying `Vec<Alert>` — the current active list — whenever it
/// changes. The frontend replaces its list wholesale; there is no delta.
pub const ALERTS_EVENT: &str = "vitals://alerts";

/// What the Notifications panel offers. Mirrors `settings/schema.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// Four independent switches the user flips one at a time. An enum or
// bitflags would be more compact and would not match the panel or the
// settings schema it is deserialised from.
#[allow(clippy::struct_excessive_bools)]
pub struct AlertPrefs {
    pub notifications_enabled: bool,
    pub notify_high_cpu: bool,
    pub notify_high_memory: bool,
    pub notify_thermal: bool,
}

impl Default for AlertPrefs {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            notify_high_cpu: true,
            notify_high_memory: true,
            notify_thermal: true,
        }
    }
}

impl AlertPrefs {
    /// Whether this alert should become a toast.
    ///
    /// The three switches map onto families rather than individual kinds so
    /// the panel stays three switches. Kinds outside any family (disk
    /// health, battery, network) always notify when notifications are on:
    /// SMART failing is the one message nobody should be able to miss.
    #[must_use]
    pub fn wants(self, kind: AlertKind) -> bool {
        if !self.notifications_enabled {
            return false;
        }
        match kind {
            AlertKind::CpuSustained | AlertKind::CpuThrottled => self.notify_high_cpu,
            AlertKind::MemoryPressure | AlertKind::MemoryCommit => self.notify_high_memory,
            AlertKind::ThermalCpu | AlertKind::GpuThrottled => self.notify_thermal,
            AlertKind::DiskSaturated
            | AlertKind::DiskLatency
            | AlertKind::DiskSpace
            | AlertKind::DiskHealth
            | AlertKind::NetworkErrors
            | AlertKind::BatteryLow
            | AlertKind::BatteryHealth => true,
        }
    }
}

/// The engine plus everything needed to act on its output.
#[derive(Debug)]
pub struct Alerts {
    engine: Mutex<Engine>,
    active: Mutex<Vec<Alert>>,
    prefs: Mutex<AlertPrefs>,
    /// Notification bodies are rendered here, not in the webview, because a
    /// toast must fire while the window is hidden. The frontend pushes its
    /// current locale's strings for every kind once at start and again on a
    /// language change.
    strings: Mutex<Option<NotificationStrings>>,
}

/// Localised toast text, keyed by the serialised `AlertKind`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationStrings {
    pub titles: std::collections::HashMap<String, String>,
    pub cleared_suffix: String,
}

impl Default for Alerts {
    fn default() -> Self {
        Self::new()
    }
}

impl Alerts {
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: Mutex::new(Engine::new()),
            active: Mutex::new(Vec::new()),
            prefs: Mutex::new(AlertPrefs::default()),
            strings: Mutex::new(None),
        }
    }

    /// Current list, most serious first.
    #[must_use]
    pub fn active(&self) -> Vec<Alert> {
        self.active.lock().clone()
    }

    pub fn set_prefs(&self, prefs: AlertPrefs) {
        *self.prefs.lock() = prefs;
    }

    pub fn set_strings(&self, strings: NotificationStrings) {
        *self.strings.lock() = Some(strings);
    }

    /// Called by the sampler once per frame.
    pub fn observe(&self, app: &AppHandle, system: &SystemMetrics) {
        let transitions = self.engine.lock().poll(system);
        if transitions.is_empty() {
            return;
        }

        let active = self.engine.lock().active();
        self.active.lock().clone_from(&active);
        // Best effort: a webview that is gone has nothing to show.
        let _ = app.emit(ALERTS_EVENT, &active);

        self.notify(app, &transitions);
    }

    fn notify(&self, app: &AppHandle, transitions: &AlertTransitions) {
        let prefs = *self.prefs.lock();
        let strings = self.strings.lock().clone();

        for alert in &transitions.raised {
            if !prefs.wants(alert.kind) {
                continue;
            }
            let kind = serde_json::to_string(&alert.kind)
                .map(|s| s.trim_matches('"').to_owned())
                .unwrap_or_default();
            // Without pushed strings there is nothing honest to say, so say
            // nothing rather than an English fallback in a Romanian UI.
            let Some(title) = strings.as_ref().and_then(|s| s.titles.get(&kind).cloned()) else {
                continue;
            };
            let body = if alert.subject.is_empty() {
                title.clone()
            } else {
                format!("{title} — {}", alert.subject)
            };
            if let Err(error) = app
                .notification()
                .builder()
                .title("Vitals")
                .body(body)
                .show()
            {
                tracing::warn!(%error, "notification failed");
            }
        }
        // Clears are not toasted. A "CPU is fine again" toast is noise; the
        // list emptying is the signal.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_switches_cover_their_families_and_nothing_silences_smart() {
        let off = AlertPrefs {
            notifications_enabled: true,
            notify_high_cpu: false,
            notify_high_memory: false,
            notify_thermal: false,
        };
        assert!(!off.wants(AlertKind::CpuSustained));
        assert!(!off.wants(AlertKind::MemoryPressure));
        assert!(!off.wants(AlertKind::ThermalCpu));
        assert!(
            off.wants(AlertKind::DiskHealth),
            "SMART failing always notifies"
        );
    }

    #[test]
    fn the_master_switch_silences_everything() {
        let prefs = AlertPrefs {
            notifications_enabled: false,
            ..AlertPrefs::default()
        };
        assert!(!prefs.wants(AlertKind::DiskHealth));
    }
}
