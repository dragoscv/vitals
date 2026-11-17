//! Topic names, built as pure functions.
//!
//! Every topic Vitals publishes is derived here rather than assembled at the
//! call site, because the shapes are a contract with Home Assistant's
//! discovery: `<discovery_prefix>/<component>/<node>/<object>/config` is what
//! makes an entity appear, and a typo in one segment silently creates a
//! second entity that never updates. Keeping them in one file means the
//! tests can assert the exact string a broker sees (see
//! `crates/vitals-server/tests/mqtt.rs`).

/// The Home Assistant component a discovery topic belongs to.
///
/// Only two are used: a reading is a `sensor`, something that is on or off is
/// a `binary_sensor`. Control is deliberately not part of this: the MQTT
/// integration is read-only in v1, so no `switch`, `button` or `number`
/// entity is ever announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Sensor,
    BinarySensor,
}

impl Component {
    /// The segment Home Assistant expects in the topic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sensor => "sensor",
            Self::BinarySensor => "binary_sensor",
        }
    }
}

/// `<discovery_prefix>/<component>/<node>/<object>/config`.
///
/// The trailing `config` is what tells Home Assistant this is an
/// announcement rather than a state; `homeassistant` is the default prefix
/// and is configurable because a second Home Assistant instance sharing the
/// broker needs its own.
#[must_use]
pub fn discovery_topic(
    discovery_prefix: &str,
    component: Component,
    node: &str,
    object: &str,
) -> String {
    format!(
        "{discovery_prefix}/{component}/{node}/{object}/config",
        component = component.as_str()
    )
}

/// `<topic_prefix>/<node>/availability` — the retained Last Will topic.
///
/// Retained on purpose: a broker that has just started, or a Home Assistant
/// that has just restarted, must learn whether this machine is publishing
/// without waiting for the next publish. The broker writes `offline` here on
/// our behalf when the connection drops without a clean `DISCONNECT`.
#[must_use]
pub fn availability_topic(topic_prefix: &str, node: &str) -> String {
    format!("{topic_prefix}/{node}/availability")
}

/// `<topic_prefix>/<node>/state/<key>`.
///
/// `key` may contain slashes, which is how the readings are grouped
/// (`summary/cpu`, `alerts/count`, `sensors/<sensor key>`). Home Assistant
/// does not care about the depth — a discovery entry names the full topic.
#[must_use]
pub fn state_topic(topic_prefix: &str, node: &str, key: &str) -> String {
    format!("{topic_prefix}/{node}/state/{key}")
}

/// The node segment: `vitals_<host>`, slugged to what Home Assistant accepts.
///
/// This is the `<node>` in every topic and the `device` every entity is
/// attached to, so it must be stable across restarts or the user's dashboard
/// loses every entity each time the app is reinstalled. The host name is
/// stable; the slug therefore is too.
#[must_use]
pub fn node_slug(host: &str) -> String {
    let mut host_slug = String::with_capacity(host.len());
    for ch in host.chars() {
        if ch.is_ascii_alphanumeric() {
            host_slug.push(ch.to_ascii_lowercase());
        } else {
            host_slug.push('_');
        }
    }
    // A name that was empty or entirely punctuation would otherwise produce
    // `vitals_`, which collides with every other machine that did the same.
    if !host_slug.chars().any(|ch| ch != '_') {
        host_slug.clear();
        host_slug.push_str("machine");
    }
    format!("vitals_{host_slug}")
}

/// The Home Assistant `object_id` / `unique_id` for one entity.
///
/// `<node>_<object>`: Home Assistant keys entities by unique id and derives
/// the display entity id from it, so this is the string that decides whether
/// the user sees `sensor.vitals_desktop_abc_cpu` or a suffix with `_2` on it.
#[must_use]
pub fn object_id(node: &str, object: &str) -> String {
    format!("{node}_{object}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_discovery_topic_has_the_segments_home_assistant_looks_for() {
        assert_eq!(
            discovery_topic(
                "homeassistant",
                Component::Sensor,
                "vitals_pc",
                "vitals_pc_cpu"
            ),
            "homeassistant/sensor/vitals_pc/vitals_pc_cpu/config"
        );
        assert_eq!(
            discovery_topic(
                "homeassistant",
                Component::BinarySensor,
                "vitals_pc",
                "vitals_pc_problem"
            ),
            "homeassistant/binary_sensor/vitals_pc/vitals_pc_problem/config"
        );
    }

    #[test]
    fn a_host_name_becomes_a_stable_slug() {
        assert_eq!(node_slug("DESKTOP-ABC"), "vitals_desktop_abc");
        assert_eq!(node_slug("mac.local"), "vitals_mac_local");
        // A name with nothing alphanumeric in it must not collide with every
        // other such machine.
        assert_eq!(node_slug("---"), "vitals_machine");
        assert_eq!(node_slug(""), "vitals_machine");
    }

    #[test]
    fn state_and_availability_topics_keep_their_grouping() {
        assert_eq!(
            state_topic("vitals", "vitals_pc", "summary/cpu"),
            "vitals/vitals_pc/state/summary/cpu"
        );
        assert_eq!(
            availability_topic("vitals", "vitals_pc"),
            "vitals/vitals_pc/availability"
        );
    }
}
