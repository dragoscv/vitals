//! The Home Assistant discovery documents, built as values.
//!
//! Home Assistant learns an entity exists by reading a retained message on a
//! discovery topic: `<prefix>/<component>/<node>/<object>/config`, whose body
//! names the state topic, the unit, the device the entity belongs to and the
//! `unique_id` that keeps the entity stable across restarts. Build the body as
//! data rather than a format string and a test can read it back as data.
//!
//! One rule from the rest of Vitals carries here unchanged: a reading that
//! cannot be measured is **not** announced and **not** published. A discovery
//! entry with a `0` in it would put a permanent zero on the user's dashboard,
//! which is exactly the "fabricated zero" the app refuses everywhere else.

use serde_json::{Value, json};

use crate::topics::{self, Component};

/// What one published reading looks like to Home Assistant.
///
/// `unit` and `device_class` are optional because a bare count (processes,
/// alerts) has neither; a percentage has both. `object` is the last segment of
/// the discovery topic, and `key` is the state key (`summary/cpu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntitySpec {
    /// The `<object>` segment, e.g. `cpu`. Also the base of `unique_id`.
    pub object: &'static str,
    /// The human name Home Assistant shows, e.g. `CPU`.
    pub name: &'static str,
    /// The state key under `<prefix>/<node>/state/`.
    pub key: &'static str,
    pub component: Component,
    pub unit: Option<&'static str>,
    pub device_class: Option<&'static str>,
    /// `measurement` for a gauge, in which Home Assistant keeps statistics.
    pub state_class: Option<&'static str>,
}

/// The device every entity hangs off, so Home Assistant groups them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// The node slug (`vitals_<host>`), which is also the topic segment.
    pub node: String,
    /// What the user sees as the device name, e.g. `Vitals on DESKTOP-ABC`.
    pub name: String,
    /// Present so two machines do not fight over one device.
    pub identifier: String,
}

impl DeviceInfo {
    /// The device block, with the availability topic attached to it.
    #[must_use]
    fn block(&self, availability_topic: &str) -> Value {
        json!({
            "identifiers": [self.identifier],
            "name": self.name,
            "manufacturer": "Vitals",
            "sw_version": env!("CARGO_PKG_VERSION"),
            "availability_topic": availability_topic,
            "payload_available": crate::policy::availability_payload(true),
            "payload_not_available": crate::policy::availability_payload(false),
        })
    }
}

/// The discovery body for one entity, ready to publish as JSON.
///
/// `availability_topic` is repeated on the entity as well as the device: an
/// entity added to a device block that predates this reading would otherwise
/// have no availability of its own, and Home Assistant shows such an entity as
/// permanently unavailable.
#[must_use]
pub fn discovery_payload(topic_prefix: &str, device: &DeviceInfo, spec: &EntitySpec) -> Value {
    let availability = topics::availability_topic(topic_prefix, &device.node);
    let unique_id = topics::object_id(&device.node, spec.object);
    let mut body = json!({
        "name": spec.name,
        "unique_id": unique_id,
        "object_id": unique_id,
        "state_topic": topics::state_topic(topic_prefix, &device.node, spec.key),
        "availability_topic": availability,
        "payload_available": crate::policy::availability_payload(true),
        "payload_not_available": crate::policy::availability_payload(false),
        "device": device.block(&availability),
    });
    // `unit_of_measurement` must be absent, not null: Home Assistant rejects a
    // discovery document that names a unit for a dimensionless sensor, and a
    // `null` is not the same as an omission.
    if let Some(map) = body.as_object_mut() {
        if let Some(unit) = spec.unit {
            map.insert("unit_of_measurement".to_owned(), Value::from(unit));
        }
        if let Some(class) = spec.device_class {
            map.insert("device_class".to_owned(), Value::from(class));
        }
        if let Some(state_class) = spec.state_class {
            map.insert("state_class".to_owned(), Value::from(state_class));
        }
    }
    body
}

/// The discovery topic a [`discovery_payload`] belongs on.
#[must_use]
pub fn discovery_topic(discovery_prefix: &str, device: &DeviceInfo, spec: &EntitySpec) -> String {
    topics::discovery_topic(
        discovery_prefix,
        spec.component,
        &device.node,
        &topics::object_id(&device.node, spec.object),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn device() -> DeviceInfo {
        DeviceInfo {
            node: "vitals_pc".to_owned(),
            name: "Vitals on PC".to_owned(),
            identifier: "vitals_pc".to_owned(),
        }
    }

    fn cpu() -> EntitySpec {
        EntitySpec {
            object: "cpu",
            name: "CPU",
            key: "summary/cpu",
            component: Component::Sensor,
            unit: Some("%"),
            device_class: None,
            state_class: Some("measurement"),
        }
    }

    #[test]
    fn a_reading_announces_its_state_topic_unit_and_device() {
        let body = discovery_payload("vitals", &device(), &cpu());
        assert_eq!(body["state_topic"], "vitals/vitals_pc/state/summary/cpu");
        assert_eq!(body["unit_of_measurement"], "%");
        assert_eq!(body["unique_id"], "vitals_pc_cpu");
        assert_eq!(body["device"]["identifiers"], json!(["vitals_pc"]));
        assert_eq!(body["availability_topic"], "vitals/vitals_pc/availability");
        assert_eq!(
            discovery_topic("homeassistant", &device(), &cpu()),
            "homeassistant/sensor/vitals_pc/vitals_pc_cpu/config"
        );
    }

    #[test]
    fn a_dimensionless_reading_has_no_unit_key_at_all() {
        let spec = EntitySpec {
            object: "processes",
            name: "Processes",
            key: "summary/processes",
            component: Component::Sensor,
            unit: None,
            device_class: None,
            state_class: None,
        };
        let body = discovery_payload("vitals", &device(), &spec);
        let map = body.as_object().expect("an object");
        assert!(
            !map.contains_key("unit_of_measurement"),
            "a null unit is not the same as no unit"
        );
        assert!(!map.contains_key("device_class"));
    }

    #[test]
    fn the_discovery_topic_keeps_the_trailing_config_segment() {
        let spec = EntitySpec {
            object: "problem",
            name: "Problem",
            key: "alerts/active",
            component: Component::BinarySensor,
            unit: None,
            device_class: Some("problem"),
            state_class: None,
        };
        assert_eq!(
            discovery_topic("homeassistant", &device(), &spec),
            "homeassistant/binary_sensor/vitals_pc/vitals_pc_problem/config"
        );
    }
}
