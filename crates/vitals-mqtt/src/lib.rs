//! Publishes Vitals to an MQTT broker, with Home Assistant discovery.
//!
//! Split into pure logic and the socket, so everything that can be wrong about
//! the payloads can be asserted without a broker:
//!
//! - [`topics`] builds every topic name as a string.
//! - [`policy`] holds the few numbers that define the behaviour (`QoS`,
//!   retention, backoff).
//! - [`discovery`] renders the Home Assistant discovery documents.
//! - [`feed`] turns one machine sample into the readings that are published.
//! - [`publisher`] is the only part that opens a socket, and it consumes the
//!   other four.
//!
//! Read-only by design: no `switch`, `button` or `number` entity is ever
//! announced, so Home Assistant can watch this machine and never act on it.
//!
//! # Testing against a broker
//!
//! `tests/broker.rs` drives the real [`Publisher`] against an MQTT 3.1.1
//! broker and asserts the topics and payload fields Home Assistant consumes —
//! including that the entity announcements are retained and the readings are
//! not, which only a subscriber that arrives *later* can observe. It needs a
//! broker, so it reads `VITALS_MQTT_TEST_BROKER` and skips loudly when that is
//! unset: a plain `cargo test -p vitals-mqtt` passes with no broker. To run it:
//!
//! ```text
//! docker run --rm -p 1883:1883 eclipse-mosquitto:2
//! # or, with no image to pull: npx --yes aedes-cli@1.0.2 --port 1883
//! VITALS_MQTT_TEST_BROKER=mqtt://127.0.0.1:1883 cargo test -p vitals-mqtt -- --nocapture
//! ```
//!
//! `.github/workflows/mqtt.yml` runs it on every push and pull request that
//! touches this crate: mosquitto as a service container on Linux, aedes
//! started in a step on Windows.

pub mod config;
pub mod discovery;
pub mod feed;
pub mod policy;
pub mod publisher;
pub mod topics;

pub use config::{BrokerUrl, ConfigError, MqttConfig, Scheme, parse_broker_url};
pub use discovery::{DeviceInfo, EntitySpec, discovery_payload};
pub use feed::{Reading, readings};
pub use publisher::{PublishError, Publisher};
pub use topics::{Component, object_id};
