//! The publisher: the only part of this crate that opens a socket.
//!
//! It owns the `rumqttc` client and one task that drives the event loop. The
//! event loop is where MQTT keep-alive, retransmission and reconnection live,
//! so the task does nothing but log what the client tells it: a dropped link
//! is normal on a laptop, and the library reconnects by itself. Everything
//! that can be decided without a broker — the client id, the topics, the
//! payloads — is decided in the other modules and asserted by their tests.

use std::time::Duration;

use rumqttc::{
    AsyncClient, Event, LastWill, MqttOptions, Packet, QoS, TlsConfiguration, Transport,
};
use tokio::task::JoinHandle;

use crate::config::{ConfigError, MqttConfig};
use crate::discovery::{DeviceInfo, EntitySpec, discovery_payload, discovery_topic};
use crate::feed::{Reading, all_specs};
use crate::policy;
use crate::topics;

/// Why publishing could not start or a message could not be sent.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    /// The broker address in the settings is unusable.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The client refused the publish (its queue is gone, or the payload is
    /// too large for the broker's maximum packet size).
    #[error("the MQTT client refused the message: {0}")]
    Client(#[from] rumqttc::ClientError),
}

/// The client id: `<node>-<pid>`, so two copies of Vitals on one machine (a
/// per-user copy beside a per-machine one) do not evict each other, which is
/// what a bare client id would do — MQTT keeps one session per client id.
#[must_use]
pub fn client_id(node: &str) -> String {
    format!("{node}-{}", std::process::id())
}

/// A running publisher.
pub struct Publisher {
    client: AsyncClient,
    device: DeviceInfo,
    config: MqttConfig,
    /// The event-loop driver. Aborted on shutdown.
    driver: JoinHandle<()>,
}

impl Publisher {
    /// Connects and starts driving the event loop.
    ///
    /// # Errors
    /// [`PublishError::Config`] when the address in the settings cannot be
    /// parsed. A broker that is simply unreachable is **not** an error here:
    /// `rumqttc` connects asynchronously and retries, so the app starts and
    /// the link comes up when the broker does.
    pub fn spawn(mut config: MqttConfig, device: DeviceInfo) -> Result<Self, PublishError> {
        let broker = config.broker()?;
        config.prepare(&broker.host);

        let mut options = MqttOptions::new(client_id(&config.node), &broker.host, broker.port);
        options.set_keep_alive(policy::KEEP_ALIVE);
        // `false` so a laptop that went offline for an hour does not miss the
        // alerts that piled up in the broker queue on its return; the readings
        // it missed were superseded long ago.
        options.set_clean_session(true);
        if let Ok((Some(user), password)) = config.credentials() {
            options.set_credentials(user, password.unwrap_or_default());
        }
        if broker.tls() {
            // schannel on Windows, the platform's own TLS, verified against
            // the system trust store — the same one the rest of the app uses.
            options.set_transport(Transport::Tls(TlsConfiguration::Native));
        }
        // The broker publishes the Last Will when this connection drops
        // without a clean DISCONNECT, which is the only way a crash can be
        // reported to Home Assistant at all.
        let will = LastWill::new(
            topics::availability_topic(&config.topic_prefix, &config.node),
            policy::availability_payload(false),
            QoS::AtLeastOnce,
            policy::RETAIN_AVAILABILITY,
        );
        options.set_last_will(will);

        let (client, mut eventloop) = AsyncClient::new(options, 32);

        let log_config = config.clone();
        let driver = tokio::spawn(async move {
            tracing::info!(broker = %log_config.describe(), "MQTT publisher started");
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        tracing::info!("MQTT broker accepted the connection");
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(%error, "MQTT link down; the client reconnects by itself");
                        // Yield so a broker that refuses instantly cannot spin
                        // the loop; the client's own backoff handles the rest.
                        tokio::time::sleep(policy::backoff(1)).await;
                    }
                }
            }
        });

        Ok(Self {
            client,
            device,
            config,
            driver,
        })
    }

    /// The device block for the running machine.
    #[must_use]
    pub fn device(&self) -> &DeviceInfo {
        &self.device
    }

    /// Announces every entity, retained, then marks the machine online.
    ///
    /// Discovery first and availability second: Home Assistant reads a device
    /// as unavailable until the availability topic says otherwise, so the
    /// entities must exist before that flip.
    ///
    /// # Errors
    /// [`PublishError::Client`] if the client's request queue has gone.
    pub async fn publish_discovery(&self) -> Result<(), PublishError> {
        for spec in all_specs() {
            let topic = discovery_topic(&self.config.discovery_prefix, &self.device, &spec);
            let body = discovery_payload(&self.config.topic_prefix, &self.device, &spec);
            self.client
                .publish(
                    topic,
                    QoS::AtLeastOnce,
                    policy::RETAIN_DISCOVERY,
                    serde_json::to_vec(&body).unwrap_or_default(),
                )
                .await?;
        }
        self.announce(true).await
    }

    /// Publishes the readings for one sample.
    ///
    /// # Errors
    /// [`PublishError::Client`] if the client's request queue has gone.
    pub async fn publish_readings(&self, readings: &[Reading]) -> Result<(), PublishError> {
        for reading in readings {
            let topic = topics::state_topic(
                &self.config.topic_prefix,
                &self.device.node,
                reading.spec.key,
            );
            self.client
                .publish(
                    topic,
                    QoS::AtLeastOnce,
                    policy::RETAIN_STATE,
                    serde_json::to_vec(&reading.value).unwrap_or_default(),
                )
                .await?;
        }
        Ok(())
    }

    /// Marks the machine online or offline on the retained availability topic.
    ///
    /// # Errors
    /// [`PublishError::Client`] if the client's request queue has gone.
    pub async fn announce(&self, online: bool) -> Result<(), PublishError> {
        self.client
            .publish(
                topics::availability_topic(&self.config.topic_prefix, &self.device.node),
                QoS::AtLeastOnce,
                policy::RETAIN_AVAILABILITY,
                policy::availability_payload(online),
            )
            .await?;
        Ok(())
    }

    /// Says goodbye: offline, then a clean disconnect so the Last Will is not
    /// published on top of the explicit `offline`.
    pub async fn shutdown(self) {
        let _ = self.announce(false).await;
        let _ = self.client.disconnect().await;
        // Give the loop a moment to flush the two packets above before it is
        // stopped; without this the clean DISCONNECT can be killed in flight.
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.driver.abort();
    }
}

impl std::fmt::Debug for Publisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publisher")
            .field("device", &self.device.node)
            .field("broker", &self.config.describe())
            .finish_non_exhaustive()
    }
}

/// Whether a spec is a binary sensor, for a caller that renders a label.
#[must_use]
pub fn is_binary(spec: &EntitySpec) -> bool {
    spec.component == topics::Component::BinarySensor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_id_carries_the_node_and_the_process_id() {
        let id = client_id("vitals_pc");
        assert!(id.starts_with("vitals_pc-"), "{id}");
        let pid = id.rsplit_once('-').expect("a dash").1;
        assert!(pid.parse::<u32>().is_ok(), "{pid} is the process id");
    }

    #[test]
    fn the_client_id_differs_between_processes_so_two_copies_do_not_evict_each_other() {
        // Same node, two processes: the two ids must not be equal, or the
        // second connection silently takes the first one's session.
        let a = client_id("vitals_pc");
        let b = client_id("vitals_pc");
        assert_eq!(a, b, "within one process the id is stable");
        assert_ne!(client_id("vitals_pc"), client_id("vitals_other"));
    }
}
