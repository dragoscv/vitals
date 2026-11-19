//! A broker stand-in for Home Assistant, shared by the broker tests.
//!
//! The crate's unit tests build the discovery payloads as values, so a wrong
//! topic or a missing field is caught with no socket at all. What they cannot
//! show is that the bytes reach a broker: an entity exists for Home Assistant
//! only because a *retained* message really landed on
//! `homeassistant/<component>/<node>/<object>/config`, and a reading only
//! because a state message really landed on the topic that document names.
//!
//! # Running a broker test
//!
//! Each test that needs a broker reads `VITALS_MQTT_TEST_BROKER` and **skips
//! loudly** when it is unset, so `cargo test -p vitals-mqtt` still passes with
//! no broker on the machine. To actually run it, start any MQTT 3.1.1 broker
//! and point the variable at it:
//!
//! ```text
//! docker run --rm -p 1883:1883 eclipse-mosquitto:2
//! # or, with no image to pull: npx --yes aedes-cli@1.0.2 --port 1883
//! VITALS_MQTT_TEST_BROKER=mqtt://127.0.0.1:1883 cargo test -p vitals-mqtt -- --nocapture
//! ```
//!
//! `.github/workflows/mqtt.yml` does this on every push that touches the
//! crate: mosquitto as a service container on Linux, aedes started in a step
//! on Windows (Windows runners have no service containers).
//!
//! The harness lives here rather than in the test file so a second broker test
//! does not re-implement the subscription, the `SUBACK` wait or the
//! retained-store replay.

// Each test binary compiles this whole module but uses only part of it.
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde_json::Value;
use tokio::task::JoinHandle;
use vitals_mqtt::BrokerUrl;

pub const DISCOVERY_PREFIX: &str = "homeassistant";
pub const TOPIC_PREFIX: &str = "vitals";

/// One message the broker delivered, in the order it arrived.
#[derive(Debug, Clone)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    /// Whether the broker was holding this for later subscribers. Only a
    /// *replayed* message proves it, so this is meaningful on a connection
    /// that subscribed after the publish — see [`super::broker`].
    pub retain: bool,
}

impl Message {
    /// The payload as JSON, or a panic naming the topic.
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.payload)
            .unwrap_or_else(|error| panic!("{} is not JSON: {error}", self.topic))
    }

    /// The payload as text.
    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.payload).unwrap_or("<not utf-8>")
    }
}

/// The broker a test may use, from `VITALS_MQTT_TEST_BROKER`.
pub fn broker_url() -> Option<String> {
    match std::env::var("VITALS_MQTT_TEST_BROKER") {
        Ok(url) if !url.trim().is_empty() => Some(url),
        _ => None,
    }
}

/// Prints why a broker test is about to do nothing. Return immediately after.
///
/// The first words are a fixed marker: the CI job greps for them and fails if
/// the test ever takes this path, because a silent skip would leave a green
/// job that proved nothing.
pub fn no_broker_skip(test: &str) {
    eprintln!(
        "SKIP the MQTT broker test ({test}): set VITALS_MQTT_TEST_BROKER=mqtt://host:port to \
         run it (CI sets it to a mosquitto service container). No assertion ran."
    );
}

/// The most recent message on `topic`, or a panic naming it.
pub fn last_on<'a>(messages: &'a [Message], topic: &str) -> &'a Message {
    messages
        .iter()
        .rev()
        .find(|message| message.topic == topic)
        .unwrap_or_else(|| panic!("no message arrived on {topic}"))
}

/// The index of the first message on `topic` whose payload is `payload`.
pub fn index_of(messages: &[Message], topic: &str, payload: &str) -> Option<usize> {
    messages
        .iter()
        .position(|message| message.topic == topic && message.text() == payload)
}

/// A second client standing in for Home Assistant: it subscribes to one node's
/// discovery and state topics and records what the broker delivers, in order.
pub struct HomeAssistant {
    messages: Arc<Mutex<Vec<Message>>>,
    subacks: Arc<AtomicUsize>,
    /// Held only so the session outlives the subscription call; the receive
    /// side of it is [`Self::task`].
    #[allow(dead_code)]
    client: AsyncClient,
    /// Drains the event loop and fills the message log.
    pub task: JoinHandle<()>,
}

impl std::fmt::Debug for HomeAssistant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The client and the task have no useful representation and the point
        // of the type is the collected messages, which `snapshot` returns.
        f.debug_struct("HomeAssistant").finish_non_exhaustive()
    }
}

impl HomeAssistant {
    /// Connects and subscribes to this node's discovery and state topics.
    ///
    /// Only this node's topics: a retained message left on the broker by an
    /// earlier run must not be able to satisfy an assertion here.
    ///
    /// # Panics
    /// If the subscriptions cannot be queued — which only happens when the
    /// client is already gone.
    pub async fn connect(broker: &BrokerUrl, node: &str) -> Self {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let subacks = Arc::new(AtomicUsize::new(0));

        let mut options =
            MqttOptions::new(format!("{node}_home_assistant"), &broker.host, broker.port);
        options.set_keep_alive(Duration::from_secs(10));
        let (client, mut eventloop) = AsyncClient::new(options, 64);
        client
            .subscribe(
                format!("{DISCOVERY_PREFIX}/+/{node}/+/config"),
                QoS::AtLeastOnce,
            )
            .await
            .expect("queue the discovery subscription");
        client
            .subscribe(format!("{TOPIC_PREFIX}/{node}/#"), QoS::AtLeastOnce)
            .await
            .expect("queue the state subscription");

        let task = {
            let messages = Arc::clone(&messages);
            let subacks = Arc::clone(&subacks);
            tokio::spawn(async move {
                loop {
                    match eventloop.poll().await {
                        Ok(Event::Incoming(Packet::SubAck(_))) => {
                            subacks.fetch_add(1, Ordering::SeqCst);
                        }
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            messages
                                .lock()
                                .expect("the message log is not poisoned")
                                .push(Message {
                                    topic: publish.topic.clone(),
                                    payload: publish.payload.to_vec(),
                                    retain: publish.retain,
                                });
                        }
                        Ok(_) => {}
                        Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
                    }
                }
            })
        };

        Self {
            messages,
            subacks,
            client,
            task,
        }
    }

    /// Everything received so far, in arrival order.
    #[must_use]
    pub fn snapshot(&self) -> Vec<Message> {
        self.messages
            .lock()
            .expect("the message log is not poisoned")
            .clone()
    }

    /// The most recent message on `topic`, if the broker has delivered one.
    #[must_use]
    pub fn last_on(&self, topic: &str) -> Option<Message> {
        self.messages
            .lock()
            .expect("the message log is not poisoned")
            .iter()
            .rev()
            .find(|message| message.topic == topic)
            .cloned()
    }

    /// Waits until the broker has acknowledged both subscriptions.
    ///
    /// A state reading is not retained, so one published before the
    /// subscription was registered would be lost and the test would flake.
    ///
    /// # Panics
    /// If the broker never acknowledges within five seconds.
    pub async fn wait_subscribed(&self) {
        self.wait_until(5, "the broker to acknowledge the subscriptions", || {
            self.subacks.load(Ordering::SeqCst) < 2
        })
        .await;
    }

    /// Waits until `still_waiting` stops being true, then returns.
    ///
    /// # Panics
    /// If `what` has not happened after `seconds`.
    pub async fn wait_until(&self, seconds: u64, what: &str, still_waiting: impl Fn() -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        while still_waiting() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {what}; topics seen: {:?}",
                self.snapshot()
                    .iter()
                    .map(|message| message.topic.as_str())
                    .collect::<Vec<_>>()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Waits for the retained availability topic to say `offline`.
    ///
    /// That is the last message of a clean shutdown, so waiting for it also
    /// proves every earlier publish was delivered — which is what makes the
    /// "the battery was not published" assertion sound rather than a race.
    ///
    /// # Panics
    /// If the topic never says `offline`.
    pub async fn wait_offline(&self, availability: &str) {
        self.wait_until(15, "the availability topic to say `offline`", || {
            self.last_on(availability)
                .is_none_or(|message| message.text() != "offline")
        })
        .await;
    }
}
