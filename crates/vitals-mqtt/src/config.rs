//! The broker settings, and the parsing of a broker URL.
//!
//! One struct, serialised in `camelCase` exactly like the rest of the wire
//! contract, because the desktop's Settings screen stores it as JSON. It is
//! deserialisable on its own — `#[serde(default)]` at the container level —
//! so a settings file written before a field existed still loads, and a
//! field that is not set is off rather than an error.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::topics::node_slug;

/// The scheme a broker URL names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Plain TCP. Port 1883 by default.
    Mqtt,
    /// TLS over TCP. Port 8883 by default.
    Mqtts,
}

impl Scheme {
    /// The port used when the URL does not name one.
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Mqtt => 1883,
            Self::Mqtts => 8883,
        }
    }

    /// Whether the transport must be TLS.
    #[must_use]
    pub const fn is_tls(self) -> bool {
        matches!(self, Self::Mqtts)
    }
}

/// A parsed broker address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerUrl {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
}

impl BrokerUrl {
    /// Whether the connection must be made over TLS. Secrets must never cross
    /// a plaintext link, so a URL without `mqtts://` is a decision the user
    /// makes explicitly by writing `mqtt://`.
    #[must_use]
    pub const fn tls(&self) -> bool {
        self.scheme.is_tls()
    }
}

/// Why a URL could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("the broker address is empty")]
    Empty,
    #[error("`{0}` is not an MQTT address; it must start with `mqtt://` or `mqtts://`")]
    BadScheme(String),
    #[error("the broker address has no host")]
    NoHost,
    #[error("`{0}` is not a port number")]
    BadPort(String),
}

/// Turns `mqtt://user:pass@host:port` into its parts.
///
/// Hand-parsed rather than pulling in a URL crate: the grammar MQTT brokers
/// actually use is three fields and an optional userinfo part, the parsing is
/// fifteen lines, and every one of those lines is covered by a test below.
/// Credentials in the URL are accepted because that is how they are pasted
/// from a broker's documentation; they end up in the same fields as the
/// separate ones, and are never logged.
///
/// # Errors
/// [`ConfigError`] for an empty string, an unknown scheme, a missing host, or
/// a port that is not a number.
pub fn parse_broker_url(
    url: &str,
) -> Result<(BrokerUrl, Option<String>, Option<String>), ConfigError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(ConfigError::Empty);
    }
    let (scheme, rest) = match url.split_once("://") {
        Some(("mqtt", rest)) => (Scheme::Mqtt, rest),
        Some(("mqtts", rest)) => (Scheme::Mqtts, rest),
        _ => return Err(ConfigError::BadScheme(url.to_owned())),
    };
    // `/path` and `?query` are legal in a broker URI and mean nothing to a
    // plain TCP client; dropping them is kinder than refusing the paste.
    let rest = rest
        .split(['/', '?'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('@');
    let (userinfo, hostport) = match rest.rsplit_once('@') {
        Some((userinfo, hostport)) => (Some(userinfo), hostport),
        None => (None, rest),
    };
    let credentials = match userinfo {
        None => (None, None),
        Some(info) => match info.split_once(':') {
            Some((user, password)) => (
                non_empty(user).map(str::to_owned),
                non_empty(password).map(str::to_owned),
            ),
            None => (non_empty(info).map(str::to_owned), None),
        },
    };
    // An IPv6 literal has colons inside brackets, so `rsplit_once(':')` alone
    // would read `[fe80::1]` as host `[fe80` on port `:1]`. The brackets are
    // handled explicitly, and then a bare `host:port` is the simple case.
    let (host, port) = match hostport.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((host, after)) => (host, after.strip_prefix(':')),
            None => (rest, None),
        },
        None => match hostport.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (hostport, None),
        },
    };
    let host = host.trim();
    if host.is_empty() {
        return Err(ConfigError::NoHost);
    }
    let port = match port.and_then(non_empty) {
        Some(port) => port
            .parse::<u16>()
            .map_err(|_| ConfigError::BadPort(port.to_owned()))?,
        None => scheme.default_port(),
    };
    Ok((
        BrokerUrl {
            scheme,
            host: host.to_owned(),
            port,
        },
        credentials.0,
        credentials.1,
    ))
}

fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

/// How often a reading is published.
pub const MIN_INTERVAL_SECS: u64 = 1;
/// The slowest useful publish rate. A dashboard cannot show more, and every
/// extra publish is a write to the broker's retained store.
pub const MAX_INTERVAL_SECS: u64 = 3600;
/// The interval used when the settings file does not name one.
pub const DEFAULT_INTERVAL_SECS: u64 = 10;

/// Everything the MQTT publisher needs, as the Settings screen stores it.
///
/// **Off by default** and inert while `enabled` is false: no socket is opened
/// and no timer is started. That is the same rule as remote access itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MqttConfig {
    /// The user's switch. Nothing happens while this is false.
    pub enabled: bool,
    /// `mqtt://host:port` or `mqtts://host:port`, optionally with
    /// `user:password@` in front.
    pub url: String,
    /// Overrides the userinfo in `url` when set.
    pub username: Option<String>,
    /// The password. Never logged, never serialised into a diagnostic.
    pub password: Option<String>,
    /// Where discovery topics go. `homeassistant` unless a second instance
    /// shares the broker.
    pub discovery_prefix: String,
    /// The prefix of the state and availability topics.
    pub topic_prefix: String,
    /// The Home Assistant node id: `vitals_<host>`. Filled from the machine
    /// name by [`MqttConfig::prepare`] when empty, and persisted afterwards
    /// so the entity ids do not move when the machine is renamed.
    pub node: String,
    /// Seconds between publishes.
    pub interval_secs: u64,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            username: None,
            password: None,
            discovery_prefix: "homeassistant".to_owned(),
            topic_prefix: "vitals".to_owned(),
            node: String::new(),
            interval_secs: DEFAULT_INTERVAL_SECS,
        }
    }
}

impl MqttConfig {
    /// Whether the publisher should run at all. Requires a URL as well as the
    /// switch: `enabled` with an empty address is a half-finished Settings
    /// form, not an instruction to guess a broker.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && !self.url.trim().is_empty()
    }

    /// The parsed broker. Credentials in the URL are returned here rather
    /// than stored, so they are only ever read where they are used.
    ///
    /// # Errors
    /// See [`parse_broker_url`].
    pub fn broker(&self) -> Result<BrokerUrl, ConfigError> {
        parse_broker_url(&self.url).map(|(broker, _, _)| broker)
    }

    /// The credentials to authenticate with: the explicit fields win over the
    /// ones embedded in the URL.
    ///
    /// # Errors
    /// See [`parse_broker_url`].
    pub fn credentials(&self) -> Result<(Option<String>, Option<String>), ConfigError> {
        let (_, url_user, url_password) = parse_broker_url(&self.url)?;
        Ok((
            self.username.clone().or(url_user),
            self.password.clone().or(url_password),
        ))
    }

    /// Fills in the node id from the machine's host name when the settings do
    /// not already carry one. Idempotent, so it can be called on every start.
    pub fn prepare(&mut self, host: &str) {
        if self.node.is_empty() {
            self.node = node_slug(host);
        }
    }

    /// How long between publishes, clamped to something a broker and a
    /// dashboard can both live with.
    #[must_use]
    pub fn interval(&self) -> Duration {
        Duration::from_secs(
            self.interval_secs
                .clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS),
        )
    }

    /// A one-line description safe to log: the address and the node, never
    /// the password and never the userinfo that may hide one.
    #[must_use]
    pub fn describe(&self) -> String {
        match self.broker() {
            Ok(broker) => format!(
                "{scheme}://{host}:{port} node {node} every {secs}s",
                scheme = if broker.tls() { "mqtts" } else { "mqtt" },
                host = broker.host,
                port = broker.port,
                node = self.node,
                secs = self.interval().as_secs(),
            ),
            Err(error) => format!("unusable broker address: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broker(url: &str) -> BrokerUrl {
        parse_broker_url(url).expect("should parse").0
    }

    #[test]
    fn a_plain_url_takes_the_default_mqtt_port() {
        let parsed = broker("mqtt://broker.local");
        assert_eq!(parsed.scheme, Scheme::Mqtt);
        assert_eq!(parsed.host, "broker.local");
        assert_eq!(parsed.port, 1883);
        assert!(!parsed.tls());
    }

    #[test]
    fn mqtts_selects_tls_and_the_secure_default_port() {
        let parsed = broker("mqtts://broker.local");
        assert_eq!(parsed.scheme, Scheme::Mqtts);
        assert_eq!(parsed.port, 8883);
        assert!(parsed.tls());
        assert_eq!(broker("mqtts://broker.local:1884").port, 1884);
    }

    #[test]
    fn credentials_in_the_url_are_split_out_of_the_host() {
        let (parsed, user, password) =
            parse_broker_url("mqtt://alice:s3cret@broker.local:1884").expect("parses");
        assert_eq!(parsed.host, "broker.local");
        assert_eq!(parsed.port, 1884);
        assert_eq!(user.as_deref(), Some("alice"));
        assert_eq!(password.as_deref(), Some("s3cret"));
    }

    #[test]
    fn a_trailing_path_or_query_is_ignored_rather_than_refused() {
        assert_eq!(broker("mqtt://broker.local:1883/").host, "broker.local");
        assert_eq!(broker("mqtt://broker.local?x=1").host, "broker.local");
    }

    #[test]
    fn an_ipv6_literal_keeps_its_colons_and_loses_its_brackets() {
        let parsed = broker("mqtt://[fe80::1]:1884");
        assert_eq!(parsed.host, "fe80::1");
        assert_eq!(parsed.port, 1884);
    }

    #[test]
    fn a_bad_url_is_refused_with_a_reason() {
        assert_eq!(parse_broker_url("   "), Err(ConfigError::Empty));
        assert!(matches!(
            parse_broker_url("http://broker.local"),
            Err(ConfigError::BadScheme(_))
        ));
        assert!(matches!(
            parse_broker_url("mqtt://"),
            Err(ConfigError::NoHost)
        ));
        assert!(matches!(
            parse_broker_url("mqtt://broker.local:notaport"),
            Err(ConfigError::BadPort(_))
        ));
    }

    #[test]
    fn the_explicit_credentials_win_over_the_ones_in_the_url() {
        let config = MqttConfig {
            url: "mqtt://urluser:urlpass@broker.local".to_owned(),
            username: Some("explicit".to_owned()),
            ..MqttConfig::default()
        };
        let (user, password) = config.credentials().expect("parses");
        assert_eq!(user.as_deref(), Some("explicit"));
        // The password still comes from the URL it was pasted into: the two
        // sources are independent, which is what lets the user fix a user name
        // without retyping a secret.
        assert_eq!(password.as_deref(), Some("urlpass"));
    }

    #[test]
    fn nothing_is_active_until_the_switch_is_on_and_an_address_is_given() {
        let mut config = MqttConfig::default();
        assert!(!config.is_active(), "off by default");
        config.enabled = true;
        assert!(!config.is_active(), "no address yet");
        config.url = "mqtt://broker.local".to_owned();
        assert!(config.is_active());
    }

    #[test]
    fn preparing_fills_the_node_once_and_then_leaves_it_alone() {
        let mut config = MqttConfig {
            node: "vitals_custom".to_owned(),
            ..MqttConfig::default()
        };
        config.prepare("DESKTOP-ABC");
        assert_eq!(config.node, "vitals_custom", "a stored node is kept");
        let mut fresh = MqttConfig::default();
        fresh.prepare("DESKTOP-ABC");
        assert_eq!(fresh.node, "vitals_desktop_abc");
        fresh.prepare("RENAMED");
        assert_eq!(fresh.node, "vitals_desktop_abc", "idempotent");
    }

    #[test]
    fn the_interval_is_clamped_and_the_description_hides_the_password() {
        let config = MqttConfig {
            enabled: true,
            url: "mqtt://user:topsecret@broker.local".to_owned(),
            interval_secs: 0,
            node: "vitals_pc".to_owned(),
            ..MqttConfig::default()
        };
        assert_eq!(config.interval(), Duration::from_secs(MIN_INTERVAL_SECS));
        let described = config.describe();
        assert!(!described.contains("topsecret"), "{described}");
        assert!(!described.contains("user"), "{described}");
        assert!(described.contains("broker.local"), "{described}");
    }

    #[test]
    fn a_settings_file_without_the_new_fields_still_loads() {
        let config: MqttConfig = serde_json::from_str(r#"{"enabled":true}"#).expect("loads");
        assert!(config.enabled);
        assert_eq!(config.discovery_prefix, "homeassistant");
        assert_eq!(config.topic_prefix, "vitals");
        assert_eq!(config.interval_secs, DEFAULT_INTERVAL_SECS);
    }
}
