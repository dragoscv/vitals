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
