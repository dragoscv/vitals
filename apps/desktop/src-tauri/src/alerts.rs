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
use tauri::{AppHandle, Emitter, Manager as _};
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
            | AlertKind::BatteryHealth
            // The user wrote the rule and chose its severity, so there is
            // nothing to second-guess: if notifications are on, it notifies.
            // A service that was running and is not is exactly the kind of
            // thing nobody notices until something else breaks.
            | AlertKind::Custom
            | AlertKind::ServiceStopped => true,
        }
    }
}

/// The engine's view of the machine's services.
///
/// A translation rather than a shared type: `vitals-win` is the platform
/// crate and must not depend on the alert engine's vocabulary, and the engine
/// deliberately carries only the four things its one alert needs. Enumerating
/// here rather than through `startup::collect` because that also reads the
/// registry, the startup folders and every scheduled task — work the alert
/// has no use for and would pay for on every poll.
///
/// An unreadable list yields an empty one, which raises nothing: a machine
/// whose services could not be read is not a machine with every service
/// stopped.
#[cfg(windows)]
#[must_use]
pub fn services_for_engine() -> Vec<vitals_core::alerts::ServiceState> {
    use vitals_core::alerts::{ServiceStartType, ServiceState};
    use vitals_win::startup::services::enumerate_services;
    use vitals_win::startup::{ServiceState as WinState, StartType};

    enumerate_services(true)
        .unwrap_or_default()
        .into_iter()
        .map(|info| {
            // The alert's sentence has to name the service, so an absent
            // display name falls back to the key name. The Services tab keeps
            // the two apart, because there the difference *is* the
            // information; here a blank subject would say nothing at all.
            let display_name = info
                .display_name
                .clone()
                .unwrap_or_else(|| info.name.clone());
            ServiceState {
                name: info.name,
                display_name,
                start_type: match info.start_type {
                    StartType::Boot => ServiceStartType::Boot,
                    StartType::System => ServiceStartType::System,
                    StartType::Automatic => ServiceStartType::Auto,
                    StartType::Manual => ServiceStartType::Manual,
                    StartType::Disabled => ServiceStartType::Disabled,
                    StartType::Unknown => ServiceStartType::Unknown,
                },
                // Every state but `Stopped` counts as running. `StopPending` is a
                // service on its way down and the next poll confirms it; treating
                // the transition as a stop would alert during a normal restart,
                // and `Unknown` means the SCM said something we do not recognise,
                // where silence is the honest answer.
                running: !matches!(info.state, WinState::Stopped),
                triggered: info.triggered,
            }
        })
        .collect()
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

    /// Replaces the user's rules and re-evaluates immediately.
    ///
    /// `sample_ms` is the sampler's current interval, because the windows the
    /// user writes are in seconds while the engine counts samples, and the
    /// rate changes with the window state.
    pub fn set_rules(&self, rules: Vec<vitals_core::alerts::Rule>, sample_ms: u32) {
        let mut engine = self.engine.lock();
        engine.set_rules(rules, sample_ms);
        // A rule that was deleted must disappear from the list at once, not
        // at the next transition — which for a steady condition never comes.
        let active = engine.active();
        drop(engine);
        self.active.lock().clone_from(&active);
    }

    pub fn set_strings(&self, strings: NotificationStrings) {
        *self.strings.lock() = Some(strings);
    }

    /// Replaces the service list the engine judges.
    ///
    /// Called from the sampler's own slow poll, not once per frame:
    /// enumerating services on Windows is hundreds of SCM and registry round
    /// trips, and the alert it feeds is about a transition that takes seconds
    /// to matter. The engine keeps the "was running" memory across calls, so a
    /// service that stopped between two polls is still caught.
    pub fn set_services(&self, services: Vec<vitals_core::alerts::ServiceState>) {
        self.engine.lock().set_services(services);
    }

    /// Records that the user stopped this service from Vitals itself.
    ///
    /// Stopping something on purpose is not the failure the "it stopped"
    /// alert is for. The engine forgets the note as soon as the service runs
    /// again, so a deliberate stop silences one episode and not the service
    /// forever.
    pub fn note_stopped_on_purpose(&self, service: &str) {
        self.engine.lock().note_stopped_on_purpose(service);
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
                .body(body.clone())
                .show()
            {
                tracing::warn!(%error, "notification failed");
            }

            // Off the machine, to whatever the user configured. Best effort:
            // a channel that is down must not hold the sampler, and it must
            // never be the reason a toast did not appear.
            if let Some(channels) = app.try_state::<std::sync::Arc<crate::channels::Channels>>() {
                let routed = self.engine.lock().routed_channels(alert);
                let notification = vitals_core::channels::Notification {
                    title: title.clone(),
                    body,
                    severity: alert.severity,
                    kind,
                    subject: alert.subject.clone(),
                    host: channels.host(),
                };
                let sent = channels.dispatch(&notification, alert.severity, &routed);
                if sent > 0 {
                    tracing::debug!(sent, "alert dispatched to channels");
                }
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
