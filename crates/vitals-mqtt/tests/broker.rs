//! End-to-end against a real broker: the bytes Home Assistant consumes.
//!
//! The unit tests beside each module build the payloads as values, so a wrong
//! topic or a missing field is caught without a socket. What they cannot show
//! is that the publisher puts those bytes on the wire — an entity exists for
//! Home Assistant only because a *retained* message really arrived on
//! `homeassistant/<component>/<node>/<object>/config`, and a reading only
//! because a *state* message really arrived on the topic that document names.
//!
//! This test drives the real `Publisher` against a broker and reads back what
//! a Home Assistant instance would consume. That proof existed once, by hand,
//! against mosquitto (recorded in `docs/tracker.csv`); this is the copy that
//! keeps it.
//!
//! It is gated on `VITALS_MQTT_TEST_BROKER` and **skips loudly** when that is
//! unset, so a developer gets a green `cargo test -p vitals-mqtt` with no
//! broker. The harness, the skip and the recipe for starting a broker live in
//! `tests/common`.
//!
//! Two details of MQTT decide how the assertions below are written:
//!
//! - **Retention is only visible to a later subscriber.** A broker must clear
//!   the retain flag when it forwards a message live to a subscription that
//!   already exists (MQTT 3.1.1, section 3.3.1.3), so the first client cannot
//!   tell a retained discovery document from an unretained one. A second
//!   client therefore connects *after* the publisher has shut down, and the
//!   broker replays its retained store to it — which is exactly what Home
//!   Assistant does after a restart, and the only way to prove the entities
//!   survive one.
//! - For the same reason "a reading is not retained" is proved by that late
//!   client receiving none of them, not by a flag on a live message.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::time::Duration;

use serde_json::{Value, json};
use vitals_core::fixtures;
use vitals_core::units::Celsius;
use vitals_mqtt::discovery::{DeviceInfo, discovery_topic};
use vitals_mqtt::feed::all_specs;
use vitals_mqtt::{MqttConfig, Publisher, parse_broker_url, readings};

use common::{
    DISCOVERY_PREFIX, HomeAssistant, Message, TOPIC_PREFIX, broker_url, index_of, last_on,
    no_broker_skip,
};

/// How many entities a Home Assistant instance must be offered.
///
/// Stated here rather than read from `all_specs` on purpose: dropping a spec
/// from `all_specs` must shrink what the broker receives and fail an assertion
/// here, rather than shrinking the expectation along with it and staying green.
const ENTITY_COUNT: usize = 7;

/// Every `<prefix>/<node>/state/…` topic for one node.
fn state_prefix(node: &str) -> String {
    format!("{TOPIC_PREFIX}/{node}/state/")
}

/// The entity announcements the broker delivered.
fn announcements(messages: &[Message]) -> Vec<&Message> {
    messages
        .iter()
        .filter(|message| {
            message.topic.starts_with("homeassistant/") && message.topic.ends_with("/config")
        })
        .collect()
}

/// Asserts the discovery half: one entity per spec, each naming the state
/// topic, the `unique_id` and the device block Home Assistant reads.
fn assert_discovery(messages: &[Message], device: &DeviceInfo, node: &str, availability: &str) {
    let announced = announcements(messages);
    assert_eq!(
        announced.len(),
        ENTITY_COUNT,
        "expected {ENTITY_COUNT} discovery entities, the broker received {:?}",
        announced
            .iter()
            .map(|message| message.topic.as_str())
            .collect::<Vec<_>>()
    );

    for spec in all_specs() {
        let topic = discovery_topic(DISCOVERY_PREFIX, device, &spec);
        let body = last_on(messages, &topic).json();
        assert_eq!(
            body["unique_id"],
            format!("{node}_{}", spec.object),
            "unique_id on {topic}"
        );
        assert_eq!(
            body["state_topic"],
            format!("{TOPIC_PREFIX}/{node}/state/{}", spec.key),
            "state_topic on {topic}"
        );
        assert_eq!(
            body["availability_topic"], availability,
            "availability_topic on {topic}"
        );
        let block = &body["device"];
        assert_eq!(
            block["identifiers"][0], node,
            "device identifiers on {topic}"
        );
        assert_eq!(block["name"], device.name, "device name on {topic}");
        assert_eq!(
            block["manufacturer"], "Vitals",
            "device manufacturer on {topic}"
        );
        assert!(
            block["sw_version"].is_string(),
            "device sw_version on {topic}"
        );
        assert_eq!(
            block["availability_topic"], availability,
            "device availability on {topic}"
        );
    }
}

/// Asserts the state half: exactly the readings the machine can measure, and
/// no fabricated value for the one it cannot.
fn assert_states(messages: &[Message], node: &str) {
    let expected = [
        ("summary/cpu", json!(33.5)),
        ("summary/cpu_temperature", json!(48.0)),
        ("summary/memory", json!(56.3)),
        ("summary/processes", json!(287)),
        ("alerts/active", json!(2)),
        ("alerts/problem", Value::from("ON")),
    ];
    for (key, value) in expected {
        let topic = format!("{TOPIC_PREFIX}/{node}/state/{key}");
        assert_eq!(last_on(messages, &topic).json(), value, "state on {topic}");
    }

    // The fixture has no battery, so the battery entity is announced (it is in
    // `all_specs`) and never published: an absent reading is absent, never a
    // fabricated zero.
    let battery = format!("{TOPIC_PREFIX}/{node}/state/summary/battery");
    assert!(
        !messages.iter().any(|message| message.topic == battery),
        "a battery reading was published for a machine that has no battery"
    );
}

/// Asserts the lifecycle a live Home Assistant watches: the machine says it is
/// online before the first reading, and ends `offline` after a clean shutdown.
fn assert_lifecycle(messages: &[Message], node: &str, availability: &str) {
    assert_eq!(
        last_on(messages, availability).text(),
        "offline",
        "the availability topic must end `offline` after a clean shutdown"
    );
    let online = index_of(messages, availability, "online")
        .expect("the machine never announced itself as online");
    let first_reading = messages
        .iter()
        .position(|message| message.topic.starts_with(&state_prefix(node)))
        .expect("no reading arrived at all");
    assert!(
        online < first_reading,
        "`online` must be published before the first reading, or Home Assistant \
         shows the device as unavailable when it first appears"
    );
}

/// Asserts what a Home Assistant that starts *later* finds already on the
/// broker: the entity announcements and the availability, and no readings.
fn assert_retained_replay(messages: &[Message], node: &str, availability: &str) {
    let announced = announcements(messages);
    assert_eq!(
        announced.len(),
        ENTITY_COUNT,
        "a late subscriber was replayed {} entity announcements, not {ENTITY_COUNT}",
        announced.len()
    );
    for message in &announced {
        assert!(
            message.retain,
            "{} was replayed without the retain flag, so a Home Assistant that \
             restarts would not know the entity exists",
            message.topic
        );
    }

    let availability_message = last_on(messages, availability);
    assert_eq!(
        availability_message.text(),
        "offline",
        "the retained availability must say the machine is not publishing"
    );
    assert!(
        availability_message.retain,
        "the availability topic must be retained, or Home Assistant cannot tell a \
         stopped machine from an idle one"
    );

    // The readings must NOT be in the broker's store: a CPU percentage replayed
    // from an hour ago is a lie about the machine.
    let replayed: Vec<&str> = messages
        .iter()
        .filter(|message| message.topic.starts_with(&state_prefix(node)))
        .map(|message| message.topic.as_str())
        .collect();
    assert!(
        replayed.is_empty(),
        "these readings were retained and replayed to a new subscriber: {replayed:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_broker_receives_every_discovery_entity_and_only_the_readings_the_machine_can_measure() {
    let Some(url) = broker_url() else {
        no_broker_skip("tests/broker.rs");
        return;
    };
    let broker = parse_broker_url(&url)
        .expect("VITALS_MQTT_TEST_BROKER is mqtt://host:port")
        .0;
    // The node carries the process id so a repeat run against a reused broker
    // does not inherit a retained discovery message from the previous one.
    let node = format!("vitals_probe_{}", std::process::id());
    let availability = format!("{TOPIC_PREFIX}/{node}/availability");

    // The client that is present the whole time, as a running Home Assistant is.
    let live = HomeAssistant::connect(&broker, &node).await;
    live.wait_subscribed().await;

    let device = DeviceInfo {
        node: node.clone(),
        name: format!("Vitals on {node}"),
        identifier: node.clone(),
    };
    let config = MqttConfig {
        enabled: true,
        url: url.clone(),
        node: node.clone(),
        ..MqttConfig::default()
    };
    let publisher = Publisher::spawn(config, device.clone()).expect("start the publisher");
    // `rumqttc` connects on the first poll of its own event loop; let that
    // happen before the first publish so the two clients are not racing.
    tokio::time::sleep(Duration::from_millis(150)).await;

    publisher
        .publish_discovery()
        .await
        .expect("publish the discovery entities");

    // The CPU temperature is present and the battery is not: this is the value
    // gate, and the fixture supplies exactly one of the two optional readings.
    let mut machine = fixtures::system();
    machine.cpu.temperature = Some(Celsius(48.0));
    assert!(machine.battery.is_none(), "the fixture has no battery");
    publisher
        .publish_readings(&readings(&machine, 2))
        .await
        .expect("publish the readings");

    publisher.shutdown().await;
    live.wait_offline(&availability).await;
    live.task.abort();

    let messages = live.snapshot();
    assert_discovery(&messages, &device, &node, &availability);
    assert_states(&messages, &node);
    assert_lifecycle(&messages, &node, &availability);

    // A Home Assistant that starts after the machine stopped: everything it
    // can learn comes from the retained store.
    let late = HomeAssistant::connect(&broker, &node).await;
    late.wait_subscribed().await;
    late.wait_until(10, "the retained discovery set", || {
        let received = late.snapshot();
        announcements(&received).len() < ENTITY_COUNT
    })
    .await;
    // A `SUBACK` does not promise the retained store has been drained, so give
    // the broker a moment before asserting what is *not* there.
    tokio::time::sleep(Duration::from_millis(300)).await;
    late.task.abort();

    assert_retained_replay(&late.snapshot(), &node, &availability);
}
