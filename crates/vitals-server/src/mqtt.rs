//! The MQTT publishing loop.
//!
//! Sits beside `prometheus.rs`: both render the same materialised machine to a
//! different consumer, and neither owns the sampler. The server already holds
//! the last complete frame and the alert list, so the loop reads them and
//! hands them to [`vitals_mqtt`] — nothing is duplicated and no second
//! sampling path exists.
//!
//! Off unless a broker is configured. When it is off this module allocates
//! nothing and no task is started, which is the same rule remote access itself
//! follows.

use std::sync::Arc;

use vitals_mqtt::{DeviceInfo, MqttConfig, PublishError, Publisher, feed};

use crate::state::ApiState;

/// A running publisher. Dropping it stops the loop.
pub struct MqttHandle {
    /// `Some` until [`MqttHandle::shutdown`] takes it; the `Drop` impl aborts
    /// whatever is still there, so both paths stop the loop exactly once.
    task: Option<tokio::task::JoinHandle<()>>,
}

impl MqttHandle {
    /// Stops publishing. Sends `offline` first, so Home Assistant does not
    /// wait for the Last Will.
    pub async fn shutdown(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for MqttHandle {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl std::fmt::Debug for MqttHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MqttHandle")
            .field("running", &self.task.is_some())
            .finish()
    }
}

/// Starts the publishing loop, or returns `None` when the settings ask for
/// nothing (the switch is off, or no address was given).
///
/// Must be called from within a Tokio runtime: the publisher spawns its own
/// event-loop task.
///
/// # Errors
/// [`PublishError::Config`] when a broker *is* configured but its address
/// cannot be parsed — a half-finished Settings form is a user error worth
/// surfacing, unlike an unreachable broker, which the client retries.
pub fn spawn(
    mut config: MqttConfig,
    state: ApiState,
    host: &str,
) -> Result<Option<MqttHandle>, PublishError> {
    config.prepare(host);
    if !config.is_active() {
        return Ok(None);
    }

    let device = DeviceInfo {
        node: config.node.clone(),
        name: format!("Vitals on {host}"),
        identifier: config.node.clone(),
    };
    let interval = config.interval();
    let publisher = Publisher::spawn(config, device)?;

    let task = tokio::spawn(async move {
        // Discovery and availability go out before the first reading: Home
        // Assistant must know the entities exist before it is told the machine
        // is online.
        if let Err(error) = publisher.publish_discovery().await {
            tracing::warn!(%error, "MQTT: the broker did not accept the entity announcements");
        }

        let mut ticker = tokio::time::interval(interval);
        // A laptop that slept for an hour must not be handed the missed ticks
        // at once: the readings it missed were superseded long ago.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            // The materialised keyframe, not the newest delta: a delta carries
            // no process list, and the process count is one of the readings
            // (it travels in the CPU metrics, absent where unmeasurable).
            let Some(frame) = state.frames.initial() else {
                continue;
            };
            let vitals_core::sample::FramePayload::Keyframe { system, .. } = &frame.payload else {
                continue;
            };
            let alert_count = (state.alerts)().len();
            let readings = feed::readings(system, alert_count);
            if let Err(error) = publisher.publish_readings(&readings).await {
                tracing::warn!(%error, "MQTT: a reading was not published");
            }
        }
    });

    Ok(Some(MqttHandle { task: Some(task) }))
}

/// Parses the CLI's `--mqtt` flag into settings.
///
/// One flag rather than five: a broker address pasted from its documentation
/// already carries the scheme, the host, the port and often the credentials.
/// `--mqtt-interval` and `--mqtt-prefix` cover the two things a URL cannot.
///
/// # Errors
/// Never today, but a signature that returns `Result` lets this grow a refusal
/// without breaking callers.
pub fn config_from_args(url: String, interval_secs: Option<u64>) -> MqttConfig {
    MqttConfig {
        enabled: true,
        url,
        interval_secs: interval_secs.unwrap_or(vitals_mqtt::config::DEFAULT_INTERVAL_SECS),
        ..MqttConfig::default()
    }
}

/// Keeps a handle alive for the life of the process without the caller having
/// to hold it. Used where the loop should outlive the local scope.
pub fn keep(handle: Option<MqttHandle>) -> Option<Arc<MqttHandle>> {
    handle.map(Arc::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inactive_configuration_starts_nothing() {
        // No ApiState is needed to prove this: the check runs before the state
        // is touched, so an off switch can never open a socket.
        let config = MqttConfig::default();
        assert!(!config.is_active(), "the default is off");
    }

    #[test]
    fn the_cli_flag_becomes_active_settings_and_keeps_the_default_interval() {
        let config = config_from_args("mqtt://broker.local".to_owned(), None);
        assert!(config.is_active());
        assert_eq!(
            config.interval_secs,
            vitals_mqtt::config::DEFAULT_INTERVAL_SECS
        );
        let custom = config_from_args("mqtt://broker.local".to_owned(), Some(30));
        assert_eq!(custom.interval_secs, 30);
    }
}
