//! Where an alert goes when it leaves the machine.
//!
//! Config and routing only — the request that actually leaves is built and
//! sent by the host (`apps/desktop/src-tauri/src/channels.rs`), because that
//! is where the HTTP client lives. Keeping the decision here means the rule
//! "does this alert belong on this channel?" is a pure function with tests,
//! rather than a condition buried in a request handler.
//!
//! Four destinations, chosen because each is already a thing people run:
//! a generic JSON webhook, `ntfy`, a Discord webhook, and Home Assistant's
//! event API. Nothing here is a vendor SDK and nothing is required: a channel
//! with no URL configured is inert, and the whole feature is off until the
//! user writes one down.

use serde::{Deserialize, Serialize};

use crate::alerts::Severity;

/// A destination kind, which decides the shape of the body sent to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum ChannelKind {
    /// `POST` of one JSON object describing the alert.
    Webhook,
    /// `POST` to an `ntfy` topic URL, with the text in the body.
    Ntfy,
    /// `POST` to a Discord webhook URL.
    Discord,
    /// `POST` to Home Assistant's `/api/events/<name>`.
    HomeAssistant,
}

impl ChannelKind {
    /// The i18n key suffix and the word used in a log line.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::Ntfy => "ntfy",
            Self::Discord => "discord",
            Self::HomeAssistant => "homeAssistant",
        }
    }

    /// Whether this kind carries a credential.
    ///
    /// Discord takes its secret in the URL, so it has nothing to send in a
    /// header; the other three can be given one.
    #[must_use]
    pub const fn uses_secret(self) -> bool {
        matches!(self, Self::Webhook | Self::Ntfy | Self::HomeAssistant)
    }

    /// What the user is expected to paste, shown as the placeholder.
    #[must_use]
    pub const fn example_url(self) -> &'static str {
        match self {
            Self::Webhook => "https://example.com/hooks/vitals",
            Self::Ntfy => "https://ntfy.sh/my-vitals-topic",
            Self::Discord => "https://discord.com/api/webhooks/…",
            Self::HomeAssistant => "https://home.example.com:8123/api/events/vitals_alert",
        }
    }
}

/// One configured destination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    /// Stable across edits, and what a rule routes to.
    pub id: String,
    /// What the user calls it in the list.
    pub name: String,
    pub kind: ChannelKind,
    /// Empty means not configured, which is the default.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub enabled: bool,
    /// The HMAC key (webhook), the bearer token (`ntfy`, Home Assistant).
    /// Never logged and never sent anywhere but this channel's own URL.
    #[serde(default)]
    pub secret: Option<String>,
    /// Only alerts at least this serious go here. A phone alarm for an
    /// informational rule is how people learn to ignore all of them.
    #[serde(default = "default_min_severity")]
    pub min_severity: Severity,
}

fn default_min_severity() -> Severity {
    Severity::Warning
}

impl Channel {
    /// Whether this channel can be sent to at all.
    ///
    /// A disabled channel and one whose URL was never finished are both
    /// inert, and neither is an error: the Settings screen saves as the user
    /// types, so a half-written URL is a normal intermediate state.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.enabled && !self.id.trim().is_empty() && self.has_url()
    }

    /// Whether a URL has been written that could plausibly be a URL.
    ///
    /// The scheme is required: `example.com/hook` pasted without one would
    /// otherwise be attempted as a relative path and fail with a message
    /// about a missing scheme that mentions neither the field nor the reason.
    #[must_use]
    pub fn has_url(&self) -> bool {
        let url = self.url.trim();
        ["https://", "http://"]
            .iter()
            .any(|scheme| url.len() > scheme.len() && url.starts_with(scheme))
    }

    /// Whether a credential may be sent to this URL.
    ///
    /// A secret over plain HTTP is readable by anything on the path. Loopback
    /// is exempt because that is the user's own machine, and Home Assistant
    /// on a home LAN is the case that would otherwise be impossible; anything
    /// else must be `https://` before a token goes on the wire.
    #[must_use]
    pub fn may_send_secret(&self) -> bool {
        let url = self.url.trim();
        if url.starts_with("https://") {
            return true;
        }
        is_loopback(url)
    }

    /// Whether this alert should be sent here.
    ///
    /// `routed` is the rule's channel list: empty means every channel, which
    /// is what a rule written before channels existed behaves as.
    #[must_use]
    pub fn wants(&self, severity: Severity, routed: &[String]) -> bool {
        self.is_usable()
            && severity >= self.min_severity
            && (routed.is_empty() || routed.iter().any(|id| id == &self.id))
    }
}

/// Whether a URL points at this machine.
fn is_loopback(url: &str) -> bool {
    let Some(rest) = url.trim().strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    // A port is not part of the host, and an IPv6 literal keeps its brackets
    // in the URL form.
    let host = match authority.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

/// What a channel is told about an alert.
///
/// The title and body arrive already localised, exactly as the toast gets
/// them: the strings live in the webview because that is where i18n is, and
/// a channel must not send English to a user who reads Romanian.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub severity: Severity,
    /// The wire slug of the alert kind, so an automation can filter on it.
    pub kind: String,
    /// The disk, the adapter, the rule name. Empty for machine-wide alerts.
    pub subject: String,
    /// The machine, so a dashboard with several can tell them apart.
    pub host: String,
}

impl Notification {
    /// The longest a channel may be told to display. Discord truncates at
    /// 4096 for an embed description and silently drops the rest, so a body
    /// is bounded here rather than discovered by a missing sentence.
    pub const MAX_BODY_CHARS: usize = 1800;
    /// See [`Notification::MAX_BODY_CHARS`].
    pub const MAX_TITLE_CHARS: usize = 200;
    /// The disk, the adapter or the rule name. Long enough for any real one.
    pub const MAX_SUBJECT_CHARS: usize = 200;
    /// The machine name.
    pub const MAX_HOST_CHARS: usize = 200;

    /// The text as it goes on the wire, with the bounds applied.
    #[must_use]
    pub fn bounded(&self) -> Self {
        Self {
            title: truncate(&self.title, Self::MAX_TITLE_CHARS),
            body: truncate(&self.body, Self::MAX_BODY_CHARS),
            severity: self.severity,
            kind: self.kind.clone(),
            subject: truncate(&self.subject, Self::MAX_SUBJECT_CHARS),
            host: truncate(&self.host, Self::MAX_HOST_CHARS),
        }
    }
}

/// Truncates on a character boundary.
///
/// Byte slicing would panic on a multi-byte character and a `char` count is
/// what the limits above are written in.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(kind: ChannelKind) -> Channel {
        Channel {
            id: "c1".into(),
            name: "My hook".into(),
            kind,
            url: "https://example.com/hook".into(),
            enabled: true,
            secret: None,
            min_severity: Severity::Warning,
        }
    }

    #[test]
    fn a_channel_with_no_url_or_switched_off_is_inert() {
        let mut c = channel(ChannelKind::Webhook);
        c.url = String::new();
        assert!(!c.is_usable(), "no URL, nothing to send to");
        c.url = "   ".into();
        assert!(!c.is_usable());
        c.url = "example.com/hook".into();
        assert!(!c.is_usable(), "a missing scheme is not a URL");
        c.url = "https://example.com/hook".into();
        assert!(c.is_usable());
        c.enabled = false;
        assert!(!c.is_usable(), "switched off");
    }

    #[test]
    fn a_secret_never_goes_over_plain_http_to_another_machine() {
        let mut c = channel(ChannelKind::Webhook);
        c.url = "https://example.com/hook".into();
        assert!(c.may_send_secret());

        c.url = "http://example.com/hook".into();
        assert!(
            !c.may_send_secret(),
            "a token would be readable on the path"
        );

        // Loopback is the user's own machine, and a Home Assistant on the
        // home LAN is the case that would otherwise be impossible.
        for url in [
            "http://localhost:8123/api/events/x",
            "http://127.0.0.1:8123/api/events/x",
            "http://[::1]:8123/api/events/x",
            "http://LOCALHOST:8123/api/events/x",
        ] {
            c.url = url.into();
            assert!(c.may_send_secret(), "{url} is this machine");
        }
        // A host that merely starts with the word must not pass.
        c.url = "http://localhost.example.com/hook".into();
        assert!(!c.may_send_secret(), "not loopback at all");
    }

    #[test]
    fn only_serious_enough_alerts_reach_a_channel() {
        let mut c = channel(ChannelKind::Webhook);
        c.min_severity = Severity::Warning;
        assert!(!c.wants(Severity::Info, &[]), "below the floor");
        assert!(c.wants(Severity::Warning, &[]));
        assert!(c.wants(Severity::Critical, &[]));
    }

    #[test]
    fn an_empty_route_means_every_channel_and_a_named_one_means_only_that() {
        let c = channel(ChannelKind::Webhook);
        assert!(
            c.wants(Severity::Warning, &[]),
            "a rule written before channels existed"
        );
        assert!(c.wants(Severity::Warning, &["c1".to_owned()]));
        assert!(!c.wants(Severity::Warning, &["other".to_owned()]));
    }

    #[test]
    fn a_long_body_is_cut_on_a_character_boundary_not_a_byte() {
        // The trap: slicing at a byte index panics on a multi-byte character,
        // and a Romanian body is full of them.
        let mut n = Notification {
            title: "t".into(),
            body: "ă".repeat(5000),
            severity: Severity::Warning,
            kind: "custom".into(),
            subject: "ș".repeat(500),
            host: "PC".into(),
        };
        let bounded = n.bounded();
        assert_eq!(bounded.body.chars().count(), Notification::MAX_BODY_CHARS);
        assert!(bounded.body.chars().all(|c| c == 'ă'));
        assert_eq!(
            bounded.subject.chars().count(),
            Notification::MAX_SUBJECT_CHARS
        );
        n.body = "short".into();
        assert_eq!(n.bounded().body, "short");
    }

    #[test]
    fn every_kind_says_which_of_them_carries_a_credential() {
        assert!(ChannelKind::Webhook.uses_secret());
        assert!(ChannelKind::Ntfy.uses_secret());
        assert!(ChannelKind::HomeAssistant.uses_secret());
        assert!(
            !ChannelKind::Discord.uses_secret(),
            "the secret is in the URL"
        );
        for kind in [
            ChannelKind::Webhook,
            ChannelKind::Ntfy,
            ChannelKind::Discord,
            ChannelKind::HomeAssistant,
        ] {
            assert!(!kind.example_url().is_empty());
            assert!(!kind.slug().is_empty());
        }
    }
}
