//! Home Assistant itself, as the only consumer that decides whether a
//! discovery document is usable.
//!
//! `tests/broker.rs` proves the bytes reach a broker and that Home Assistant's
//! *pattern* of reading them finds what it needs. It cannot prove Home
//! Assistant *accepts* them: the broker is not the validator. This file is.
//!
//! Why it exists - the failure it was written for:
//!
//! ```text
//! Error 'not a valid option at 'device.availability_topic'' when processing
//! MQTT discovery message topic: 'homeassistant/sensor/vitals_x/vitals_x_cpu/config'
//! ```
//!
//! `availability_topic`, `payload_available` and `payload_not_available` were
//! nested inside the discovery document's `device` block. Home Assistant
//! validates that block against its device schema, found three keys that are
//! not part of it, and discarded the WHOLE document - so no entity was ever
//! created and the integration silently did nothing, in every 1.3.0 desktop
//! build. It was fixed in `e2a7f9b` by moving availability to the entity level.
//!
//! Every Vitals test passed while it was broken, and CI stayed green, because
//! none of them was Home Assistant. The unit tests build the payload as a
//! value and cannot know what the consumer accepts; the broker test proves
//! delivery, not acceptance; and Home Assistant reports the rejection only in
//! `/config/home-assistant.log` *inside its container*, which `docker logs`
//! never shows. The two tests here close that gap:
//!
//! - [`every_key_in_a_discovery_document_is_one_home_assistant_accepts`] pins
//!   the document against Home Assistant's accepted vocabulary, at the entity
//!   level and inside the `device` block, and needs no broker. This is the test
//!   that would have caught the defect, and it runs everywhere.
//! - [`home_assistant_creates_an_entity_from_the_discovery_document`] boots
//!   against a live Home Assistant and asserts an entity really appeared with a
//!   reading. It is gated on `VITALS_HA_URL` and `VITALS_HA_TOKEN` and **skips
//!   loudly** when either is unset; CI sets both
//!   (`.github/workflows/mqtt.yml`, job `mqtt-home-assistant`) and fails the
//!   job if the skip is taken.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use vitals_core::fixtures;
use vitals_core::units::Celsius;
use vitals_mqtt::discovery::{DeviceInfo, discovery_payload};
use vitals_mqtt::feed::all_specs;
use vitals_mqtt::{MqttConfig, Publisher, parse_broker_url, readings};

use common::{TOPIC_PREFIX, broker_url};

/// Home Assistant's accepted keys, curated from its own source. See the
/// `_provenance` block in the file: it is Home Assistant's vocabulary, not a
/// copy of what Vitals publishes, so a key Vitals invents or misplaces fails.
const HA_KEYS: &str = include_str!("fixtures/ha_discovery_keys.json");

/// The first words of the skip line. CI greps for them and fails if the
/// acceptance test ever takes this path, because a silent skip would leave a
/// green job that proved nothing. Deliberately different from the broker
/// test's marker so the broker-only jobs are not tripped by it.
const SKIP_MARKER: &str = "SKIP the Home Assistant acceptance test";

/// Keys Vitals publishes at the entity level that Home Assistant does not know.
///
/// Home Assistant's sensor and `binary_sensor` discovery schemas are declared
/// `extra=REMOVE_EXTRA`, so an unknown top-level key is *dropped*, not
/// rejected - which is its own quiet bug, and the reason the assertion below
/// is strict rather than "whatever Home Assistant tolerates". The single
/// exception is `object_id`: Home Assistant derives an entity's object id from
/// the discovery *topic*, never from a payload field, so this key is dropped
/// from every document. It is listed here rather than deleted only because
/// removing it is a change to `src/discovery.rs`, which is frozen for the
/// 1.3.1 release. If a second key ever needs to appear in this list, the
/// document has grown another field Home Assistant will ignore - fix that in
/// `src/discovery.rs` instead of extending this list.
const DROPPED_BY_HOME_ASSISTANT: &[&str] = &["object_id"];

/// The keys of a JSON array of strings.
fn accepted_keys(value: &Value) -> HashSet<&str> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("the fixture entry {value} is not an array"))
        .iter()
        .map(|key| key.as_str().expect("every fixture key is a string"))
        .collect()
}

fn device() -> DeviceInfo {
    DeviceInfo {
        node: "vitals_pc".to_owned(),
        name: "Vitals on PC".to_owned(),
        identifier: "vitals_pc".to_owned(),
    }
}

/// Every way `body` departs from Home Assistant's accepted vocabulary.
///
/// Returned rather than asserted so the same check can be pointed at a
/// deliberately broken document: a pin's worth is the failure it produces, and
/// `the_pin_rejects_availability_nested_inside_the_device_block` below requires
/// this to reject the shape Home Assistant rejected in 1.3.0.
fn vocabulary_violations(
    body: &Value,
    component: &str,
    object: &str,
    top: &HashSet<&str>,
    device_keys: &HashSet<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();

    // An unknown top-level key is silently dropped, so a misspelled
    // `state_topic` would leave an entity that never updates - as wrong as the
    // nesting, only quieter.
    for key in body.as_object().expect("the payload is an object").keys() {
        if !top.contains(key.as_str()) && !DROPPED_BY_HOME_ASSISTANT.contains(&key.as_str()) {
            violations.push(format!(
                "`{key}` on `{component}` `{object}` is not a key Home Assistant accepts in a \
                 discovery document; it would be dropped silently"
            ));
        }
    }

    // The device block is the one Home Assistant validates strictly, and the
    // one whose unknown keys discarded every entity until e2a7f9b. Availability
    // is entity-level; nesting it here is what the log line this file quotes is
    // about.
    for key in body["device"].as_object().expect("a device object").keys() {
        if !device_keys.contains(key.as_str()) {
            violations.push(format!(
                "`device.{key}` on `{component}` `{object}` is not a key Home Assistant accepts \
                 inside `device`; it discards the whole document and creates no entity"
            ));
        }
    }

    violations
}

/// Pins every discovery document Vitals can send against Home Assistant's
/// accepted vocabulary.
///
/// This is the regression test for `e2a7f9b`: with availability nested in the
/// `device` block, the `device`-block assertion below fails for every spec, so
/// the defect cannot come back through a gate. It is deliberately a plain
/// `#[test]` with no broker, so it runs in every job that compiles this crate
/// rather than only where a broker happens to be reachable.
#[test]
fn every_key_in_a_discovery_document_is_one_home_assistant_accepts() {
    let fixture: Value = serde_json::from_str(HA_KEYS).expect("the curated fixture is JSON");
    let components = &fixture["components"];
    let device_keys = accepted_keys(&fixture["device"]);
    let device = device();

    for spec in all_specs() {
        let body = discovery_payload(TOPIC_PREFIX, &device, &spec);
        let component = spec.component.as_str();
        let top = accepted_keys(&components[component]);
        let object = spec.object;

        let violations = vocabulary_violations(&body, component, object, &top, &device_keys);
        assert!(
            violations.is_empty(),
            "Home Assistant would not accept `{component}` `{object}`:\n  {}\n\
             Accepted entity keys: {top:?}\nAccepted device keys: {device_keys:?}",
            violations.join("\n  ")
        );

        // Availability belongs on the entity, and must actually be there - a
        // document with neither the device block nor the entity carrying it
        // would create an entity that can never be marked unavailable.
        for key in [
            "availability_topic",
            "payload_available",
            "payload_not_available",
        ] {
            assert!(
                body.get(key).is_some(),
                "`{key}` is missing from the entity `{component}` `{object}` altogether"
            );
        }
        assert_eq!(
            body["availability_topic"],
            format!("{TOPIC_PREFIX}/vitals_pc/availability"),
            "availability_topic on `{object}`"
        );
    }

    // The exception list is part of the contract: if it grows, the document
    // gained another field Home Assistant ignores.
    assert_eq!(
        DROPPED_BY_HOME_ASSISTANT.len(),
        1,
        "only `object_id` may appear in DROPPED_BY_HOME_ASSISTANT"
    );
    assert_eq!(DROPPED_BY_HOME_ASSISTANT[0], "object_id");
}

/// Shows the pin has teeth by feeding it the document Home Assistant rejected.
///
/// The pin's value is the failure it produces, and a check that only ever sees
/// a correct document cannot demonstrate one - it could accept anything and
/// still pass. This takes the real payload, moves availability into the
/// `device` block exactly as the broken 1.3.0 builds did, and requires the
/// vocabulary check to reject it for that reason. It runs wherever this crate
/// compiles, so if the check is ever loosened - accepting unknown keys, or
/// losing the `device` walk - CI goes red here instead of the pin quietly
/// becoming decorative.
#[test]
fn the_pin_rejects_availability_nested_inside_the_device_block() {
    let fixture: Value = serde_json::from_str(HA_KEYS).expect("the curated fixture is JSON");
    let device_keys = accepted_keys(&fixture["device"]);
    let device = device();
    let spec = all_specs()
        .into_iter()
        .find(|spec| spec.object == "cpu")
        .expect("the feed still has a cpu spec");

    let mut broken = discovery_payload(TOPIC_PREFIX, &device, &spec);
    for key in [
        "availability_topic",
        "payload_available",
        "payload_not_available",
    ] {
        let value = broken[key].take();
        assert!(
            !value.is_null(),
            "`{key}` is not entity-level in the current payload, so this test no longer \
             reproduces the 1.3.0 defect"
        );
        broken["device"][key] = value;
    }

    let top = accepted_keys(&fixture["components"][spec.component.as_str()]);
    let violations = vocabulary_violations(
        &broken,
        spec.component.as_str(),
        spec.object,
        &top,
        &device_keys,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("device.availability_topic")),
        "the vocabulary check did not object to availability inside `device`, which is the \
         document Home Assistant discards in its entirety: the pin is decorative. \
         Violations found: {violations:?}"
    );
}

/// Where Home Assistant lives and how to ask it, from the environment.
fn home_assistant() -> Option<(String, String)> {
    let url = std::env::var("VITALS_HA_URL").ok()?;
    let token = std::env::var("VITALS_HA_TOKEN").ok()?;
    if url.trim().is_empty() || token.trim().is_empty() {
        return None;
    }
    Some((url.trim().to_owned(), token.trim().to_owned()))
}

/// A `GET`, over a bare socket, of one path on Home Assistant.
///
/// No HTTP client is a dependency of this crate, and adding one for a test
/// would put a new crate in the release lockfile, so this speaks the minimum
/// HTTP/1.1 the HA REST API needs: one request, `Connection: close`, read to
/// EOF. `Connection: close` is what makes reading to EOF safe; the chunked
/// branch exists only because a server may ignore that hint.
async fn http_get(base: &str, path: &str, token: &str) -> (u16, String) {
    let authority = base
        .strip_prefix("http://")
        .unwrap_or_else(|| panic!("VITALS_HA_URL must be http://, got {base}"));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().expect("a numeric port")),
        None => (authority, 80),
    };

    let mut stream = TcpStream::connect((host, port))
        .await
        .unwrap_or_else(|error| panic!("cannot reach Home Assistant at {base}: {error}"));
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {token}\r\n\
         Accept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.expect("send");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");

    let response = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("malformed HTTP response: {response}"));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(body)
    } else {
        body.to_owned()
    };
    (status, body)
}

/// Reassembles an HTTP/1.1 chunked body.
fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size_line, after)) = rest.split_once("\r\n") {
        let size =
            usize::from_str_radix(size_line.trim().split(';').next().unwrap_or("0").trim(), 16)
                .unwrap_or(0);
        if size == 0 || after.len() < size {
            out.push_str(&after[..after.len().min(size)]);
            break;
        }
        out.push_str(&after[..size]);
        rest = after[size..].strip_prefix("\r\n").unwrap_or(&after[size..]);
    }
    out
}

/// One `/api/states` read, parsed.
async fn states(base: &str, token: &str) -> Vec<Value> {
    let (status, body) = http_get(base, "/api/states", token).await;
    assert_eq!(
        status, 200,
        "Home Assistant answered {status} to /api/states, expected 200: {body}"
    );
    serde_json::from_str(&body).expect("/api/states is a JSON array of states")
}

/// The state entry for `entity_id`, if Home Assistant has created it.
fn entity<'a>(states: &'a [Value], entity_id: &str) -> Option<&'a Value> {
    states.iter().find(|state| state["entity_id"] == entity_id)
}

/// Boots against a live Home Assistant and asserts an entity appears with a
/// reading - the real consumer, not a stand-in for it.
///
/// The request is the one that failed: Home Assistant receives the retained
/// discovery document, validates it, and either creates `sensor.*` with the
/// attributes the document named or logs the rejection and creates nothing.
/// The assertions below read the created entity's own attributes, so a
/// document Home Assistant merely *tolerated* (dropped a key) fails here even
/// though an entity exists.
#[tokio::test(flavor = "multi_thread")]
async fn home_assistant_creates_an_entity_from_the_discovery_document() {
    let Some((base, token)) = home_assistant() else {
        println!(
            "{SKIP_MARKER}: set VITALS_HA_URL and VITALS_HA_TOKEN to a running Home Assistant \
             with its MQTT integration pointed at the broker to run it. No assertion ran."
        );
        return;
    };
    let Some(url) = broker_url() else {
        // Not the broker test's marker: this file is not the one CI's
        // broker-only jobs guard, and reusing their string would fail them.
        println!(
            "{SKIP_MARKER}: VITALS_MQTT_TEST_BROKER is unset, so there is no broker to \
             publish to. No assertion ran."
        );
        return;
    };
    parse_broker_url(&url).expect("VITALS_MQTT_TEST_BROKER is mqtt://host:port");

    // A fixed node, unlike the broker test's pid-suffixed one, so the entity
    // id this test looks for is predictable across runs. Home Assistant
    // derives `sensor.<device name>_<entity name>` from the document, so
    // "Vitals on <node>" + "CPU" is `sensor.vitals_on_<node>_cpu`.
    let node = std::env::var("VITALS_HA_TEST_NODE").unwrap_or_else(|_| "vitals_ha_ci".to_owned());
    let entity_id = format!("sensor.vitals_on_{node}_cpu");
    let seconds = std::env::var("VITALS_HA_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);

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
    let publisher = Publisher::spawn(config, device).expect("start the publisher");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Discovery is retained, so Home Assistant receives it whenever its MQTT
    // integration connects, before or after this publish. Readings are not
    // retained, so they are re-sent below until one lands.
    publisher
        .publish_discovery()
        .await
        .expect("publish the discovery entities");

    let mut machine = fixtures::system();
    machine.cpu.temperature = Some(Celsius(48.0));
    let reading = readings(&machine, 2);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let value = loop {
        // Re-publish on every attempt: Home Assistant may have connected to
        // the broker after the last one, and a reading is not retained.
        publisher
            .publish_readings(&reading)
            .await
            .expect("publish the readings");

        let known = states(&base, &token).await;
        if let Some(entry) = entity(&known, &entity_id) {
            // These attributes exist only because Home Assistant read them out
            // of the entity-level keys of the document. A rejected document
            // would have created nothing to read them from; a dropped key
            // would have left the attribute absent.
            assert_eq!(
                entry["attributes"]["unit_of_measurement"], "%",
                "Home Assistant did not apply the unit from the discovery document: {entry}"
            );
            assert_eq!(
                entry["attributes"]["state_class"], "measurement",
                "Home Assistant did not apply the state class from the document: {entry}"
            );
            assert_eq!(
                entry["attributes"]["friendly_name"],
                format!("Vitals on {node} CPU"),
                "the device and entity names did not reach Home Assistant: {entry}"
            );
            // And the entity is *live*: a reading on the state topic the
            // document named reached it. `unavailable` here would mean the
            // availability or state topic is wrong even though it was accepted.
            if let Some(reading) = entry["state"]
                .as_str()
                .filter(|text| text.parse::<f64>().is_ok())
            {
                break reading.to_owned();
            }
        }

        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out after {seconds}s waiting for Home Assistant to create {entity_id} \
             with a reading; it knows {} entities",
            states(&base, &token).await.len()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    println!("Home Assistant created {entity_id} with state {value}");

    publisher.shutdown().await;
}
