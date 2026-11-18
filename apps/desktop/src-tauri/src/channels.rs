//! Sends alerts to the channels the user configured.
//!
//! The decision of *what goes where* lives in `vitals_core::channels`; this is
//! the part that has an HTTP client in it. The split matters because the
//! routing rules are the part that can be wrong in a way nobody notices
//! (an alert to the wrong place, or to nowhere), and they are testable as a
//! pure function.
//!
//! Everything here is best effort and bounded: a channel that is down must
//! not hold the sampler, must not retry forever, and must never be the reason
//! a reading is late. So a send is a spawned task with a short timeout and at
//! most two attempts.
//!
//! Nothing is sent anywhere until the user writes a URL down, and a secret is
//! refused over plain HTTP to any machine but this one (see
//! `Channel::may_send_secret`) — an alert body names processes and disks, and
//! a token in the clear is a credential on the wire.

use std::fmt::Write as _;
use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use parking_lot::Mutex;
use sha2::Sha256;

use vitals_core::alerts::Severity;
use vitals_core::channels::{Channel, ChannelKind, Notification};

/// How long one attempt gets. A hook that has not answered in five seconds is
/// not going to, and the alert is worth less than the reading after it.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Attempts per alert, including the first. Two, not three: a retry exists for
/// a blip, and an endpoint that misses twice is down.
const MAX_ATTEMPTS: u32 = 2;

/// Delay before the one retry.
const RETRY_DELAY: Duration = Duration::from_millis(750);

/// Why a channel could not be sent to. Never contains the URL: a Discord
/// webhook URL *is* the credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("this channel has no usable URL yet")]
    NoUrl,
    #[error("refusing to send a credential over plain HTTP to another machine")]
    SecretOverPlainHttp,
}

/// One HTTP request, built as a value so it can be asserted without a socket.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// What the sender kept from the last attempt, for the Settings screen.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    /// `None` when nothing has been sent yet.
    pub last_ok: Option<bool>,
    /// The HTTP status, when there was one (a transport failure has none).
    pub last_status: Option<u16>,
}

/// The configured channels, and the client that sends to them.
#[derive(Debug)]
pub struct Channels {
    config: Mutex<Vec<Channel>>,
    /// The machine name, so a dashboard with several can tell them apart.
    host: Mutex<String>,
    status: Mutex<std::collections::HashMap<String, ChannelStatus>>,
    /// Built on the first send, not here. `reqwest::Client::new` needs a Tokio
    /// reactor, and managed state is constructed during app setup, which is
    /// not inside one — and a test that only checks routing must not need a
    /// runtime at all.
    client: std::sync::OnceLock<reqwest::Client>,
}

impl Default for Channels {
    fn default() -> Self {
        Self::new()
    }
}

impl Channels {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Vec::new()),
            host: Mutex::new(String::new()),
            status: Mutex::new(std::collections::HashMap::new()),
            client: std::sync::OnceLock::new(),
        }
    }

    /// The HTTP client, built once on first use.
    fn client(&self) -> &reqwest::Client {
        self.client.get_or_init(|| {
            reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                // A redirect would carry the signature header to a host the
                // user did not name. There is no legitimate reason for a
                // webhook to redirect.
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default()
        })
    }

    pub fn set(&self, channels: Vec<Channel>) {
        // A channel that was removed takes its status with it, or the screen
        // shows the last result of something that no longer exists.
        let ids: Vec<String> = channels.iter().map(|c| c.id.clone()).collect();
        self.status.lock().retain(|id, _| ids.contains(id));
        *self.config.lock() = channels;
    }

    #[must_use]
    pub fn config(&self) -> Vec<Channel> {
        self.config.lock().clone()
    }

    pub fn set_host(&self, host: String) {
        *self.host.lock() = host;
    }

    /// The machine name, as it goes into every notification.
    #[must_use]
    pub fn host(&self) -> String {
        self.host.lock().clone()
    }

    /// What the Settings screen shows beside each channel.
    #[must_use]
    pub fn status(&self) -> std::collections::HashMap<String, ChannelStatus> {
        self.status.lock().clone()
    }

    /// Sends an alert to every channel that wants it, without blocking.
    ///
    /// Returns how many channels it was sent to, which is what a test asserts
    /// and a log line records; the sends themselves happen on the runtime.
    /// The decision alone, with nothing sent. Pure and free of a runtime, so
    /// the routing rules can be asserted without a socket.
    #[must_use]
    pub fn targets(&self, severity: Severity, routed: &[String]) -> Vec<Channel> {
        self.config
            .lock()
            .iter()
            .filter(|c| c.wants(severity, routed))
            .cloned()
            .collect()
    }

    /// Sends an alert to every channel that wants it, without blocking.
    ///
    /// Returns how many channels it was sent to, which a log line records;
    /// the sends themselves happen on the runtime.
    pub fn dispatch(
        self: &std::sync::Arc<Self>,
        notification: &Notification,
        severity: Severity,
        routed: &[String],
    ) -> usize {
        let targets = self.targets(severity, routed);
        for channel in &targets {
            let this = std::sync::Arc::clone(self);
            let channel = channel.clone();
            let notification = notification.clone();
            tauri::async_runtime::spawn(async move {
                this.send_one(&channel, &notification).await;
            });
        }
        targets.len()
    }

    /// Sends one alert to one channel, with the bounded retry.
    pub async fn send_one(&self, channel: &Channel, notification: &Notification) {
        let request = match request_for(channel, notification) {
            Ok(request) => request,
            Err(refused) => {
                tracing::warn!(
                    channel = %channel.name,
                    kind = channel.kind.slug(),
                    %refused,
                    "channel refused the alert"
                );
                self.record(&channel.id, false, None);
                return;
            }
        };
        let (ok, status) = self.attempt(request).await;
        self.record(&channel.id, ok, status);
        if !ok {
            tracing::warn!(
                channel = %channel.name,
                kind = channel.kind.slug(),
                status = ?status,
                "channel did not accept the alert"
            );
        }
    }

    /// One request, retried once on a transport error or a server fault.
    async fn attempt(&self, request: Request) -> (bool, Option<u16>) {
        let mut last_status = None;
        for attempt in 1..=MAX_ATTEMPTS {
            let mut builder = self.client().request(
                reqwest::Method::from_bytes(request.method.as_bytes())
                    .unwrap_or(reqwest::Method::POST),
                &request.url,
            );
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            match builder.body(request.body.clone()).send().await {
                Ok(response) => {
                    let status = response.status();
                    last_status = Some(status.as_u16());
                    if status.is_success() {
                        return (true, last_status);
                    }
                    // A 4xx is the user's configuration and will fail the
                    // same way next time; only a server fault is worth a
                    // second try.
                    if !worth_retrying(last_status, attempt) {
                        return (false, last_status);
                    }
                }
                Err(error) => {
                    last_status = None;
                    tracing::debug!(%error, "channel attempt failed");
                    if !worth_retrying(None, attempt) {
                        return (false, None);
                    }
                }
            }
            tokio::time::sleep(RETRY_DELAY).await;
        }
        (false, last_status)
    }

    fn record(&self, id: &str, ok: bool, status: Option<u16>) {
        self.status.lock().insert(
            id.to_owned(),
            ChannelStatus {
                last_ok: Some(ok),
                last_status: status,
            },
        );
    }

    /// Sends a harmless message so the user can see the wiring works.
    ///
    /// # Errors
    /// The refusal, or the HTTP status, as a sentence the screen can show.
    pub async fn test(&self, id: &str) -> Result<u16, String> {
        let channel = self
            .config
            .lock()
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| "that channel no longer exists".to_owned())?;
        let notification = Notification {
            title: "Vitals".to_owned(),
            body: "This is a test. Your alerts will arrive here.".to_owned(),
            severity: Severity::Info,
            kind: "test".to_owned(),
            subject: String::new(),
            host: self.host.lock().clone(),
        };
        let request = request_for(&channel, &notification).map_err(|e| e.to_string())?;
        let (ok, status) = self.attempt(request).await;
        self.record(&channel.id, ok, status);
        match (ok, status) {
            (true, Some(code)) => Ok(code),
            (false, Some(code)) => Err(format!("the server answered {code}")),
            _ => Err("the address could not be reached".to_owned()),
        }
    }
}

/// Builds the request for one channel.
///
/// # Errors
/// [`Refused`] when the channel has no URL, or would take a credential over
/// plain HTTP to another machine.
pub fn request_for(channel: &Channel, notification: &Notification) -> Result<Request, Refused> {
    if !channel.has_url() {
        return Err(Refused::NoUrl);
    }
    let url = channel.url.trim().to_owned();
    let notification = notification.bounded();
    let secret = channel
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if secret.is_some() && !channel.may_send_secret() {
        return Err(Refused::SecretOverPlainHttp);
    }

    match channel.kind {
        ChannelKind::Webhook => {
            let body = webhook_body(&notification);
            let mut headers = json_headers();
            if let Some(secret) = secret
                && let Some(signature) = signature(secret, &body)
            {
                headers.push((
                    "X-Vitals-Signature".to_owned(),
                    format!("sha256={signature}"),
                ));
            }
            Ok(Request {
                method: "POST",
                url,
                headers,
                body,
            })
        }
        ChannelKind::Ntfy => {
            let mut headers = vec![
                ("Title".to_owned(), notification.title.clone()),
                (
                    "Priority".to_owned(),
                    ntfy_priority(notification.severity).to_string(),
                ),
                (
                    "Tags".to_owned(),
                    ntfy_tag(notification.severity).to_owned(),
                ),
            ];
            if let Some(secret) = secret {
                headers.push(("Authorization".to_owned(), format!("Bearer {secret}")));
            }
            Ok(Request {
                method: "POST",
                url,
                headers,
                body: notification.body.clone(),
            })
        }
        ChannelKind::Discord => Ok(Request {
            method: "POST",
            url,
            headers: json_headers(),
            // No signature: Discord's own URL is the credential, and it
            // rejects unknown fields.
            body: discord_body(&notification),
        }),
        ChannelKind::HomeAssistant => {
            let mut headers = json_headers();
            if let Some(secret) = secret {
                headers.push(("Authorization".to_owned(), format!("Bearer {secret}")));
            }
            Ok(Request {
                method: "POST",
                url,
                headers,
                body: webhook_body(&notification),
            })
        }
    }
}

fn json_headers() -> Vec<(String, String)> {
    vec![("Content-Type".to_owned(), "application/json".to_owned())]
}

/// The generic body: every field a consumer could want, and nothing else.
///
/// The same object goes to Home Assistant, whose `/api/events/<name>` takes
/// an arbitrary payload and hands it to an automation as `trigger.event.data`.
fn webhook_body(notification: &Notification) -> String {
    serde_json::json!({
        "title": notification.title,
        "body": notification.body,
        "severity": severity_slug(notification.severity),
        "kind": notification.kind,
        "subject": notification.subject,
        "host": notification.host,
        // So a receiver can tell a Vitals alert from anything else on the
        // same endpoint without parsing the body.
        "source": "vitals",
    })
    .to_string()
}

fn discord_body(notification: &Notification) -> String {
    serde_json::json!({
        // Discord shows `content` as plain text and `embeds` as a card. The
        // card alone is easy to miss on a busy server, so the title also goes
        // in the text line.
        "content": format!("**{}**", notification.title),
        "embeds": [{
            "title": notification.title,
            "description": notification.body,
            "color": discord_colour(notification.severity),
        }],
    })
    .to_string()
}

/// The decimal colour Discord wants. Chosen for contrast against both of its
/// themes rather than for the exact brand value.
/// Whether a failed attempt is worth repeating.
///
/// Three answers, and each is a real distinction rather than a blanket retry:
/// a transport error (`None`) means the request never got an answer — a
/// sleeping laptop, a flaky link — and is worth one more try; a server fault
/// is the far side having a bad moment; a client error is the user's own
/// configuration and will fail byte-for-byte the same way, so retrying it
/// only doubles the requests it makes against an endpoint that already said
/// no.
///
/// The attempt cap is checked first so a 5xx that lasts cannot loop: two
/// attempts, then the user is told.
fn worth_retrying(status: Option<u16>, attempt: u32) -> bool {
    if attempt >= MAX_ATTEMPTS {
        return false;
    }
    match status {
        None => true,
        Some(code) => !(400..500).contains(&code),
    }
}

const fn discord_colour(severity: Severity) -> u32 {
    match severity {
        Severity::Info => 0x3B_82_F6,     // blue
        Severity::Warning => 0xF5_9E_0B,  // amber
        Severity::Critical => 0xDC_26_26, // red
    }
}

/// `ntfy`'s priority scale is 1–5, with 3 the default. A reading that "needs
/// attention" is `high`, and only critical reaches `max`, because `max`
/// bypasses a phone's do-not-disturb setting.
const fn ntfy_priority(severity: Severity) -> u8 {
    match severity {
        Severity::Info => 2,
        Severity::Warning => 4,
        Severity::Critical => 5,
    }
}

const fn ntfy_tag(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "information_source",
        Severity::Warning => "warning",
        Severity::Critical => "rotating_light",
    }
}

const fn severity_slug(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Critical => "critical",
    }
}

/// `sha256=<hex>` over the exact bytes of the body.
///
/// `None` only when the key length is rejected, which for HMAC cannot happen;
/// returning `Option` rather than `expect` keeps the deny-list honest.
fn signature(secret: &str, body: &str) -> Option<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(body.as_bytes());
    Some(hex(&mac.finalize().into_bytes()))
}

/// Lower-case hex. Written out rather than pulling in `hex` for four lines.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(kind: ChannelKind) -> Channel {
        Channel {
            id: "c1".into(),
            name: "Hook".into(),
            kind,
            url: "https://example.com/hook".into(),
            enabled: true,
            secret: None,
            min_severity: Severity::Warning,
        }
    }

    fn notification() -> Notification {
        Notification {
            title: "The processor is overheating".to_owned(),
            body: "96 °C. Check the fans.".to_owned(),
            severity: Severity::Critical,
            kind: "thermalCpu".to_owned(),
            subject: String::new(),
            host: "DESKTOP-ABC".to_owned(),
        }
    }

    fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
        request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn the_signature_matches_the_rfc_4231_vector() {
        // The published test case: key "Jefe", the sentence below, HMAC-SHA256.
        // Without this the only way to find a wrong construction would be a
        // receiver that rejects every request.
        assert_eq!(
            signature("Jefe", "what do ya want for nothing?").as_deref(),
            Some("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
        );
    }

    #[test]
    fn a_webhook_is_signed_over_the_exact_body_bytes() {
        let mut c = channel(ChannelKind::Webhook);
        c.secret = Some("topsecret".into());
        let request = request_for(&c, &notification()).expect("built");
        let sent = header(&request, "X-Vitals-Signature").expect("signed");
        assert!(sent.starts_with("sha256="), "{sent}");
        assert_eq!(
            sent,
            format!(
                "sha256={}",
                signature("topsecret", &request.body).expect("signs")
            ),
            "the signature must cover the body as sent"
        );
    }

    #[test]
    fn an_unsigned_webhook_has_no_signature_header_at_all() {
        let request = request_for(&channel(ChannelKind::Webhook), &notification()).expect("built");
        assert!(header(&request, "X-Vitals-Signature").is_none());
    }

    #[test]
    fn a_credential_over_plain_http_to_another_machine_is_refused() {
        let mut c = channel(ChannelKind::Webhook);
        c.url = "http://example.com/hook".into();
        c.secret = Some("topsecret".into());
        assert_eq!(
            request_for(&c, &notification()),
            Err(Refused::SecretOverPlainHttp)
        );
        // The same URL without a secret is allowed: there is nothing to leak.
        c.secret = None;
        assert!(request_for(&c, &notification()).is_ok());
        // And loopback is fine either way.
        c.url = "http://127.0.0.1:8123/api/events/x".into();
        c.secret = Some("token".into());
        assert!(request_for(&c, &notification()).is_ok());
    }

    #[test]
    fn a_channel_with_no_url_cannot_be_built() {
        let mut c = channel(ChannelKind::Webhook);
        c.url = String::new();
        assert_eq!(request_for(&c, &notification()), Err(Refused::NoUrl));
    }

    #[test]
    fn the_webhook_body_carries_every_field_an_automation_needs() {
        let request = request_for(&channel(ChannelKind::Webhook), &notification()).expect("built");
        let body: serde_json::Value = serde_json::from_str(&request.body).expect("json");
        assert_eq!(body["severity"], "critical");
        assert_eq!(body["kind"], "thermalCpu");
        assert_eq!(body["host"], "DESKTOP-ABC");
        assert_eq!(body["source"], "vitals", "so a receiver can filter");
        assert_eq!(body["title"], notification().title);
    }

    #[test]
    fn ntfy_puts_the_text_in_the_body_and_its_metadata_in_headers() {
        let mut c = channel(ChannelKind::Ntfy);
        c.secret = Some("tk_abc".into());
        let request = request_for(&c, &notification()).expect("built");
        assert_eq!(request.body, notification().body);
        assert_eq!(
            header(&request, "Title"),
            Some(notification().title.as_str())
        );
        assert_eq!(header(&request, "Priority"), Some("5"), "critical is max");
        assert_eq!(header(&request, "Authorization"), Some("Bearer tk_abc"));
        assert_eq!(
            header(&request, "Content-Type"),
            None,
            "ntfy takes plain text"
        );
    }

    #[test]
    fn the_three_severities_map_onto_ntfy_priority_and_discord_colour() {
        assert_eq!(ntfy_priority(Severity::Info), 2);
        assert_eq!(ntfy_priority(Severity::Warning), 4);
        assert_eq!(
            ntfy_priority(Severity::Critical),
            5,
            "max, which bypasses DND"
        );
        assert_ne!(
            discord_colour(Severity::Warning),
            discord_colour(Severity::Critical)
        );
    }

    #[test]
    fn a_discord_body_is_a_card_and_never_a_signature() {
        let mut c = channel(ChannelKind::Discord);
        // A secret on Discord is meaningless: the URL is the credential.
        c.secret = Some("ignored".into());
        let request = request_for(&c, &notification()).expect("built");
        assert!(header(&request, "X-Vitals-Signature").is_none());
        assert!(header(&request, "Authorization").is_none());
        let body: serde_json::Value = serde_json::from_str(&request.body).expect("json");
        assert_eq!(body["embeds"][0]["description"], notification().body);
        assert!(body["content"].as_str().expect("content").contains("**"));
    }

    #[test]
    fn home_assistant_takes_a_bearer_token_and_the_generic_body() {
        let mut c = channel(ChannelKind::HomeAssistant);
        c.url = "https://home.example.com:8123/api/events/vitals_alert".into();
        c.secret = Some("long-lived-token".into());
        let request = request_for(&c, &notification()).expect("built");
        assert_eq!(
            header(&request, "Authorization"),
            Some("Bearer long-lived-token")
        );
        let body: serde_json::Value = serde_json::from_str(&request.body).expect("json");
        assert_eq!(body["kind"], "thermalCpu");
    }

    #[test]
    fn a_long_alert_is_truncated_before_it_is_sent() {
        let mut n = notification();
        n.body = "ă".repeat(5000);
        let request = request_for(&channel(ChannelKind::Webhook), &n).expect("built");
        let body: serde_json::Value = serde_json::from_str(&request.body).expect("json");
        assert_eq!(
            body["body"].as_str().expect("body").chars().count(),
            Notification::MAX_BODY_CHARS
        );
    }

    #[test]
    fn routing_sends_only_to_the_channels_that_want_the_alert() {
        let channels = Channels::new();
        channels.set(vec![
            Channel {
                id: "loud".into(),
                enabled: true,
                min_severity: Severity::Warning,
                ..channel(ChannelKind::Webhook)
            },
            Channel {
                id: "quiet".into(),
                enabled: true,
                min_severity: Severity::Critical,
                ..channel(ChannelKind::Ntfy)
            },
        ]);
        // `targets` is the decision; `dispatch` only adds the spawning of it.
        // Asserting the decision keeps the test free of a socket, which is
        // what `dispatch` would open.
        let loud = channels.targets(Severity::Warning, &[]);
        assert_eq!(loud.len(), 1);
        assert_eq!(
            loud[0].id, "loud",
            "the critical-only channel stays quiet for a warning"
        );
        let routed = channels.targets(Severity::Critical, &["quiet".to_owned()]);
        assert_eq!(routed.len(), 1);
        assert_eq!(
            routed[0].id, "quiet",
            "a rule naming a channel reaches only it"
        );
        assert!(
            channels.targets(Severity::Info, &[]).is_empty(),
            "below both floors, nothing is sent"
        );
    }

    #[test]
    fn a_client_error_is_never_retried_because_it_will_fail_identically() {
        // The user typed the URL wrong, or the endpoint wants a token we did
        // not send: sending it twice only doubles the load on an endpoint
        // that has already answered.
        assert!(!worth_retrying(Some(404), 1));
        assert!(!worth_retrying(Some(401), 1));
        assert!(!worth_retrying(Some(429), 1), "a rate limit is a refusal");
    }

    #[test]
    fn a_server_fault_and_a_dead_link_get_one_more_try() {
        assert!(
            worth_retrying(Some(503), 1),
            "the far side is having a moment"
        );
        assert!(worth_retrying(Some(500), 1));
        assert!(worth_retrying(None, 1), "no answer at all");
    }

    #[test]
    fn the_attempt_cap_stops_even_a_fault_that_lasts() {
        // The point of the cap: a 503 that never clears must not turn into a
        // loop that fires at the user's endpoint until the app is closed.
        assert!(!worth_retrying(Some(503), MAX_ATTEMPTS));
        assert!(!worth_retrying(None, MAX_ATTEMPTS));
        assert!(!worth_retrying(None, MAX_ATTEMPTS + 1));
    }

    #[test]
    fn removing_a_channel_takes_its_last_result_with_it() {
        let channels = Channels::new();
        let one = channel(ChannelKind::Webhook);
        channels.set(vec![one.clone()]);
        channels.record("c1", true, Some(200));
        assert!(channels.status().contains_key("c1"));
        channels.set(Vec::new());
        assert!(
            !channels.status().contains_key("c1"),
            "the screen must not show a result for a channel that is gone"
        );
    }
}
