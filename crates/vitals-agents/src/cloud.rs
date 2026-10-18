//! Agents running in the cloud, from the codai account (ADR-0039).
//!
//! Read-only and opt-in: nothing is sent until the user signs in with the
//! codai OIDC device flow (RFC 8628, public client `codai-cli`, the same one
//! `codaid` uses). The resulting `codai_…` key is stored by the desktop in
//! Credential Manager; this module only builds requests and parses answers,
//! with the HTTP transport passed in so it stays OS- and runtime-agnostic.
//!
//! Endpoints (`apps/gateway` in the codai repo):
//! - `GET /v1/hosts` — the user's desktops connected right now;
//! - `GET /v1/sessions?v=2` — conversation sessions with `last_event_at`,
//!   `executor_device_id` and live `presence_count`;
//! - `POST /v1/keys/self/revoke` — the key revokes itself on sign-out.
//!
//! The key is minted under the `agents:read` scope (owner decision
//! 2026-10-18, S21): codai's gateway refuses it everywhere except those
//! reads and its own revocation, so a copied key can neither run inference,
//! spend money nor manage other keys. The device name sent at minting is a
//! fixed label, never the computer name.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Deserialize;
use serde_json::Value;
use vitals_core::agents::{AgentClient, AgentSession, SessionState};

use crate::time::parse_iso_ms;

pub const DEFAULT_GATEWAY: &str = "https://ai.codai.ro";
pub const DEFAULT_AUTH: &str = "https://auth.codai.ro";
pub const CLIENT_ID: &str = "codai-cli";
/// `agents:read` is the only codai resource scope requested: no `inference`,
/// no `keys:manage`, no `email`/`profile` (Vitals never shows who you are).
pub const SCOPE: &str = "openid agents:read";
/// What the key is called in the codai hub. Deliberately not the computer
/// name: that would be personal data sent for no feature (LEG2-3).
pub const KEY_LABEL: &str = "Vitals desktop";
/// Credential Manager target for the key.
pub const CREDENTIAL_TARGET: &str = "vitals/codai-api-key";

/// A request the transport must perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: &'static str,
    pub url: String,
    /// `application/x-www-form-urlencoded` body, when `Some`.
    pub form: Option<Vec<(String, String)>>,
    pub bearer: Option<String>,
    pub headers: Vec<(String, String)>,
}

/// `(status, body)`.
pub type Response = (u16, String);

/// Step one of the device flow.
#[must_use]
pub fn device_auth_request(auth: &str) -> Request {
    Request {
        method: "POST",
        url: format!("{}/device/auth", auth.trim_end_matches('/')),
        form: Some(vec![
            ("client_id".into(), CLIENT_ID.into()),
            ("scope".into(), SCOPE.into()),
        ]),
        bearer: None,
        headers: Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct DeviceAuth {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

const fn default_interval() -> u64 {
    5
}

/// Parses the `/device/auth` answer.
///
/// # Errors
/// A non-2xx status or a body without the RFC 8628 fields.
pub fn parse_device_auth(res: &Response) -> Result<DeviceAuth, String> {
    if !(200..300).contains(&res.0) {
        return Err(format!("device authorisation failed ({})", res.0));
    }
    serde_json::from_str(&res.1).map_err(|e| format!("unexpected device authorisation answer: {e}"))
}

#[must_use]
pub fn token_request(auth: &str, device_code: &str) -> Request {
    Request {
        method: "POST",
        url: format!("{}/token", auth.trim_end_matches('/')),
        form: Some(vec![
            (
                "grant_type".into(),
                "urn:ietf:params:oauth:grant-type:device_code".into(),
            ),
            ("device_code".into(), device_code.into()),
            ("client_id".into(), CLIENT_ID.into()),
        ]),
        bearer: None,
        headers: Vec::new(),
    }
}

/// What one poll of `/token` means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    /// The user has not approved yet: poll again after `interval`.
    Pending,
    /// The server asks for a longer interval (+5 s, RFC 8628 §3.5).
    SlowDown,
    Granted {
        access_token: String,
    },
    /// Denied or expired: stop.
    Failed(String),
}

#[must_use]
pub fn parse_token(res: &Response) -> Poll {
    let v: Value = serde_json::from_str(&res.1).unwrap_or(Value::Null);
    if (200..300).contains(&res.0) {
        return match v.get("access_token").and_then(Value::as_str) {
            Some(t) => Poll::Granted {
                access_token: t.to_owned(),
            },
            None => Poll::Failed("token answer without access_token".into()),
        };
    }
    match v.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => Poll::Pending,
        Some("slow_down") => Poll::SlowDown,
        Some(other) => Poll::Failed(other.to_owned()),
        None => Poll::Failed(format!("token endpoint answered {}", res.0)),
    }
}

/// Exchanges the access token for a long-lived `codai_…` key (returned once),
/// labelled [`KEY_LABEL`].
#[must_use]
pub fn key_request(auth: &str, access_token: &str) -> Request {
    Request {
        method: "GET",
        url: format!("{}/connect/key", auth.trim_end_matches('/')),
        form: None,
        bearer: Some(access_token.into()),
        headers: vec![("X-Codai-Device-Name".into(), KEY_LABEL.into())],
    }
}

/// # Errors
/// A non-2xx status or no key in the body.
pub fn parse_key(res: &Response) -> Result<String, String> {
    if !(200..300).contains(&res.0) {
        return Err(format!("key exchange failed ({})", res.0));
    }
    let v: Value = serde_json::from_str(&res.1).map_err(|e| e.to_string())?;
    ["key", "api_key", "plaintext"]
        .iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .filter(|k| k.starts_with("codai_"))
        .map(str::to_owned)
        .ok_or_else(|| "no codai key in the answer".into())
}

/// Sign-out: the key revokes itself so a copy stops working too.
#[must_use]
pub fn revoke_request(gateway: &str, key: &str) -> Request {
    Request {
        method: "POST",
        url: format!("{}/v1/keys/self/revoke", gateway.trim_end_matches('/')),
        form: None,
        bearer: Some(key.into()),
        headers: client_header(),
    }
}

/// Whether the server no longer accepts the key after a revoke call.
///
/// A 401 means it was already revoked (or never existed): signed out either way.
///
/// # Errors
/// Any other status, so the caller can say the server copy may still be live.
pub fn parse_revoke(res: &Response) -> Result<(), String> {
    match res.0 {
        200..=299 | 401 => Ok(()),
        s => Err(format!("codai answered {s}")),
    }
}

#[must_use]
pub fn hosts_request(gateway: &str, key: &str) -> Request {
    Request {
        method: "GET",
        url: format!("{}/v1/hosts", gateway.trim_end_matches('/')),
        form: None,
        bearer: Some(key.into()),
        headers: client_header(),
    }
}

#[must_use]
pub fn sessions_request(gateway: &str, key: &str) -> Request {
    Request {
        method: "GET",
        url: format!(
            "{}/v1/sessions?v=2&limit=100",
            gateway.trim_end_matches('/')
        ),
        form: None,
        bearer: Some(key.into()),
        headers: client_header(),
    }
}

fn client_header() -> Vec<(String, String)> {
    vec![(
        "x-codai-client".into(),
        format!("vitals/{}", env!("CARGO_PKG_VERSION")),
    )]
}

/// A codai host (a desktop connected to the account).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct Host {
    pub device_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
}

#[derive(Deserialize)]
struct HostsBody {
    #[serde(default)]
    hosts: Vec<Host>,
}

/// # Errors
/// 401 (key revoked) is reported distinctly so the UI can ask to sign in again.
pub fn parse_hosts(res: &Response) -> Result<Vec<Host>, String> {
    check(res)?;
    serde_json::from_str::<HostsBody>(&res.1)
        .map(|b| b.hosts)
        .map_err(|e| e.to_string())
}

/// Cloud sessions active in the last `window_ms`, as [`AgentSession`]s whose
/// `host` names the device that executes them (or `cloud`).
///
/// codai mirrors local conversations to the cloud: `session_key` is the local
/// session id (Copilot CLI) or `vscode:<workspace>:<id>` (VS Code). A row
/// whose key ends in an id `is_local` accepts is the same conversation the local
/// reader already lists with real counts, so it is dropped rather than shown
/// twice (measured 2026-10-17: 40 of 100 rows were such copies).
///
/// # Errors
/// As [`parse_hosts`].
pub fn parse_sessions(
    res: &Response,
    hosts: &[Host],
    is_local: &dyn Fn(&str) -> bool,
    now: u64,
    window_ms: u64,
) -> Result<Vec<AgentSession>, String> {
    check(res)?;
    let v: Value = serde_json::from_str(&res.1).map_err(|e| e.to_string())?;
    let rows = v
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(rows
        .iter()
        .filter_map(|s| {
            let last = s
                .get("last_event_at")
                .and_then(Value::as_str)
                .and_then(parse_iso_ms);
            if last.is_none_or(|l| now.saturating_sub(l) > window_ms) {
                return None;
            }
            if s.get("session_key")
                .and_then(Value::as_str)
                .and_then(|k| k.rsplit(':').next())
                .is_some_and(is_local)
            {
                return None;
            }
            let device = s.get("executor_device_id").and_then(Value::as_str);
            let host = device
                .and_then(|d| hosts.iter().find(|h| h.device_id == d))
                .and_then(|h| h.name.clone().or_else(|| h.hostname.clone()))
                .unwrap_or_else(|| "cloud".into());
            let presence = s.get("presence_count").and_then(Value::as_u64).unwrap_or(0);
            let lease_live = s
                .get("lease_expires_at")
                .and_then(Value::as_str)
                .and_then(parse_iso_ms)
                .is_some_and(|e| e > now);
            let idle = last.map_or(u64::MAX, |l| now.saturating_sub(l));
            let state = if lease_live && idle < 120_000 {
                SessionState::Working
            } else if lease_live || presence > 0 {
                SessionState::Waiting
            } else if idle < 6 * 3_600_000 {
                SessionState::Idle
            } else {
                SessionState::Ended
            };
            Some(AgentSession {
                id: s.get("id")?.as_str()?.to_owned(),
                client: AgentClient::Codai,
                host,
                title: s
                    .get("title")
                    .and_then(Value::as_str)
                    .map(|t| t.chars().take(80).collect()),
                workspace: None,
                repo: None,
                branch: None,
                model: None,
                state,
                started_ms: s
                    .get("created_at")
                    .and_then(Value::as_str)
                    .and_then(parse_iso_ms),
                last_activity_ms: last,
                // the session list carries no turn or tool counts
                active_ms: None,
                turns: None,
                user_messages: None,
                tool_calls: None,
                tool_failures: None,
                current_tool: None,
                top_tools: Vec::new(),
                tokens_in: None,
                tokens_out: None,
                pid: None,
            })
        })
        .collect())
}

fn check(res: &Response) -> Result<(), String> {
    match res.0 {
        200..=299 => Ok(()),
        401 | 403 => Err("signed-out".into()),
        s => Err(format!("codai answered {s}")),
    }
}

/// What the webview gets from step one of the device flow. The secret
/// `device_code` is not in it: it stays in [`SignIns`], and the webview holds
/// only `handle`, which is worthless outside this process.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInPrompt {
    pub handle: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
}

/// A device flow in progress, owned by Rust.
#[derive(Debug, Clone)]
pub struct PendingSignIn {
    pub device_code: String,
    pub interval: u64,
    pub cancelled: Arc<AtomicBool>,
}

/// At most one device flow at a time; starting another replaces (and
/// cancels) the previous one so an abandoned poll cannot store a key later.
#[derive(Debug, Default)]
pub struct SignIns {
    next: AtomicU64,
    current: std::sync::Mutex<Option<(String, PendingSignIn)>>,
}

impl SignIns {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
            current: std::sync::Mutex::new(None),
        }
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<(String, PendingSignIn)>> {
        // a panic while holding this lock leaves plain data behind: keep going
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records `auth` and returns what the webview may see.
    pub fn begin(&self, auth: DeviceAuth) -> SignInPrompt {
        let handle = format!("s{}", self.next.fetch_add(1, Ordering::Relaxed));
        let pending = PendingSignIn {
            device_code: auth.device_code,
            interval: auth.interval,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        if let Some((_, old)) = self.slot().replace((handle.clone(), pending)) {
            old.cancelled.store(true, Ordering::Relaxed);
        }
        SignInPrompt {
            handle,
            user_code: auth.user_code,
            verification_uri: auth.verification_uri,
            verification_uri_complete: auth.verification_uri_complete,
        }
    }

    /// The flow `handle` names, if it is still the current one.
    #[must_use]
    pub fn get(&self, handle: &str) -> Option<PendingSignIn> {
        self.slot()
            .as_ref()
            .filter(|(h, _)| h == handle)
            .map(|(_, p)| p.clone())
    }

    /// Stops the flow `handle` names; the poller sees it on its next tick.
    /// Returns whether there was one.
    pub fn cancel(&self, handle: &str) -> bool {
        let mut slot = self.slot();
        if slot.as_ref().is_some_and(|(h, _)| h == handle) {
            if let Some((_, p)) = slot.take() {
                p.cancelled.store(true, Ordering::Relaxed);
            }
            return true;
        }
        false
    }

    /// Forgets the flow `handle` names once it has finished, either way.
    pub fn finish(&self, handle: &str) {
        let mut slot = self.slot();
        if slot.as_ref().is_some_and(|(h, _)| h == handle) {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_flow_answers_map_to_pending_slow_down_granted_and_failed() {
        assert_eq!(
            parse_token(&(400, r#"{"error":"authorization_pending"}"#.into())),
            Poll::Pending
        );
        assert_eq!(
            parse_token(&(400, r#"{"error":"slow_down"}"#.into())),
            Poll::SlowDown
        );
        assert_eq!(
            parse_token(&(400, r#"{"error":"expired_token"}"#.into())),
            Poll::Failed("expired_token".into())
        );
        assert_eq!(
            parse_token(&(200, r#"{"access_token":"at","token_type":"Bearer"}"#.into())),
            Poll::Granted {
                access_token: "at".into()
            }
        );
    }

    #[test]
    fn only_a_codai_key_is_accepted_from_the_exchange() {
        // apps/auth/src/connect.ts answers { api_key, api_key_id, ... }; api_key is
        // null on a repeat call because the plaintext is returned once
        assert_eq!(
            parse_key(&(200, r#"{"api_key":"codai_abc","api_key_id":"k1"}"#.into())).unwrap(),
            "codai_abc"
        );
        assert!(parse_key(&(200, r#"{"api_key":null,"api_key_id":"k1"}"#.into())).is_err());
        assert!(parse_key(&(200, r#"{"api_key":"sk-other"}"#.into())).is_err());
        assert!(parse_key(&(500, String::new())).is_err());
    }

    #[test]
    fn a_revoked_key_is_reported_as_signed_out() {
        assert_eq!(
            parse_hosts(&(401, String::new())).unwrap_err(),
            "signed-out"
        );
    }

    #[test]
    fn a_session_executing_on_a_known_host_is_named_after_it_and_old_ones_drop() {
        let now = parse_iso_ms("2026-10-17T04:00:00Z").unwrap();
        let hosts = vec![Host {
            device_id: "d1".into(),
            name: Some("DRAGOS".into()),
            platform: None,
            hostname: None,
        }];
        let body = r#"{"sessions":[
            {"id":"a","title":"Refactor","created_at":"2026-10-17T03:00:00Z","last_event_at":"2026-10-17T03:59:30Z","executor_device_id":"d1","lease_expires_at":"2026-10-17T04:05:00Z","presence_count":1},
            {"id":"b","title":"Old","last_event_at":"2026-10-10T00:00:00Z"}
        ]}"#;
        let s =
            parse_sessions(&(200, body.into()), &hosts, &|_| false, now, 24 * 3_600_000).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].host, "DRAGOS");
        assert_eq!(s[0].state, SessionState::Working);
        assert_eq!(s[0].client, AgentClient::Codai);
    }

    #[test]
    fn a_cloud_copy_of_a_local_conversation_is_listed_once_and_counts_stay_unknown() {
        let now = parse_iso_ms("2026-10-17T04:00:00Z").unwrap();
        let body = r#"{"sessions":[
            {"id":"c1","session_key":"4dfcb7a2-e614-4995-a18c-5bc9e899e04f","last_event_at":"2026-10-17T03:59:00Z"},
            {"id":"c2","session_key":"vscode:fef8502d:9f20dbae-1111-2222-3333-444455556666","last_event_at":"2026-10-17T03:59:00Z"},
            {"id":"c3","session_key":"ux:only-in-the-cloud","last_event_at":"2026-10-17T03:59:00Z"}
        ]}"#;
        let local: std::collections::HashSet<&str> = [
            "4dfcb7a2-e614-4995-a18c-5bc9e899e04f",
            "9f20dbae-1111-2222-3333-444455556666",
        ]
        .into_iter()
        .collect();
        let s = parse_sessions(
            &(200, body.into()),
            &[],
            &|id| local.contains(id),
            now,
            24 * 3_600_000,
        )
        .unwrap();
        assert_eq!(s.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["c3"]);
        // the list endpoint reports no turns: unknown, not zero
        assert_eq!(s[0].turns, None);
        assert_eq!(s[0].active_ms, None);
    }

    #[test]
    fn device_auth_request_is_form_encoded_for_the_public_cli_client() {
        let r = device_auth_request("https://auth.codai.ro/");
        assert_eq!(r.url, "https://auth.codai.ro/device/auth");
        let form = r.form.unwrap();
        assert!(form.contains(&("client_id".into(), "codai-cli".into())));
        assert!(form.contains(&("scope".into(), SCOPE.into())));
    }

    #[test]
    fn sign_in_asks_for_read_only_access_and_nothing_that_runs_spends_or_manages_keys() {
        let scopes: Vec<&str> = SCOPE.split(' ').collect();
        assert!(scopes.contains(&"agents:read"));
        for wide in [
            "inference",
            "keys:manage",
            "email",
            "profile",
            "offline_access",
        ] {
            assert!(
                !scopes.contains(&wide),
                "scope {wide} must not be requested"
            );
        }
    }

    #[test]
    fn the_key_is_labelled_without_the_computer_name() {
        let r = key_request("https://auth.codai.ro", "at");
        assert_eq!(
            r.headers,
            vec![(
                "X-Codai-Device-Name".to_owned(),
                "Vitals desktop".to_owned()
            )]
        );
        if let Ok(host) = std::env::var("COMPUTERNAME") {
            assert!(r.headers.iter().all(|(_, v)| !v.contains(&host)));
        }
    }

    #[test]
    fn sign_out_revokes_the_key_with_itself_and_treats_401_as_already_gone() {
        let r = revoke_request("https://ai.codai.ro/", "codai_k");
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://ai.codai.ro/v1/keys/self/revoke");
        assert_eq!(r.bearer.as_deref(), Some("codai_k"));
        assert!(parse_revoke(&(200, r#"{"revoked":true}"#.into())).is_ok());
        assert!(parse_revoke(&(401, String::new())).is_ok());
        assert!(parse_revoke(&(503, String::new())).is_err());
        assert!(parse_revoke(&(403, String::new())).is_err());
    }

    fn auth(code: &str) -> DeviceAuth {
        DeviceAuth {
            device_code: code.into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://auth.codai.ro/device".into(),
            verification_uri_complete: None,
            interval: 5,
            expires_in: Some(600),
        }
    }

    #[test]
    fn the_device_code_never_reaches_the_webview_only_an_opaque_handle_does() {
        let s = SignIns::new();
        let p = s.begin(auth("secret-device-code"));
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("secret-device-code"), "{json}");
        assert!(json.contains("\"handle\""));
        assert_eq!(s.get(&p.handle).unwrap().device_code, "secret-device-code");
        assert!(s.get("s999").is_none());
    }

    #[test]
    fn cancelling_or_starting_again_stops_the_earlier_poll() {
        let s = SignIns::new();
        let first = s.begin(auth("one"));
        let flag = s.get(&first.handle).unwrap().cancelled;
        let second = s.begin(auth("two"));
        assert!(
            flag.load(Ordering::Relaxed),
            "a replaced flow must be cancelled"
        );
        assert!(s.get(&first.handle).is_none());
        let flag2 = s.get(&second.handle).unwrap().cancelled;
        assert!(!s.cancel(&first.handle), "a stale handle cancels nothing");
        assert!(!flag2.load(Ordering::Relaxed));
        assert!(s.cancel(&second.handle));
        assert!(flag2.load(Ordering::Relaxed));
        assert!(s.get(&second.handle).is_none());
    }
}
