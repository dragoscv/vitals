//! `/api/v1/agents` over a real socket (ADR-0039, S18-07).
//!
//! The adversarial cases are the point: a read token must not act, must be
//! refused before the body is parsed, and nothing that looks like a home
//! directory or a credential may reach the wire.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vitals_core::agents::{
    AgentClient, AgentSession, AgentqAction, AgentqResource, AgentqTicket, AgentsSnapshot,
    Liveness, ResourceKind, SessionState,
};
use vitals_server::state::ServerLock;
use vitals_server::{
    ActionError, ActionOutcome, AgentsProvider, ApiState, FrameSource, Scope, Token, TokenSet,
    serve,
};

const READ: &str = "agents-read-token";
const CONTROL: &str = "agents-control-token";

#[derive(Default)]
struct Fake {
    snapshots: AtomicUsize,
    acted: Mutex<Vec<AgentqAction>>,
}

impl AgentsProvider for Fake {
    fn snapshot(&self) -> AgentsSnapshot {
        self.snapshots.fetch_add(1, Ordering::SeqCst);
        snapshot()
    }

    fn act(&self, action: AgentqAction) -> Result<ActionOutcome, ActionError> {
        if let AgentqAction::Note { message } = &action
            && message.len() < 4
        {
            return Err(ActionError::Invalid(
                "reason must be at least 4 characters".into(),
            ));
        }
        self.acted.lock().unwrap().push(action);
        Ok(ActionOutcome {
            ok: true,
            code: Some(0),
            output: r"noted by C:\Users\dragos\.copilot\bin\agentq.ps1".into(),
        })
    }
}

fn snapshot() -> AgentsSnapshot {
    AgentsSnapshot {
        taken_at_ms: 1,
        agentq_installed: true,
        resources: vec![AgentqResource {
            name: "build:codai".into(),
            kind: ResourceKind::Build,
            capacity: Some(1),
            tickets: vec![
                ticket("t1", true, "pnpm test --token hunter2hunter2"),
                ticket("t2", false, r"cargo build in C:\Users\dragos\gh"),
            ],
        }],
        sessions: vec![AgentSession {
            id: "sess-1".into(),
            client: AgentClient::VscodeCopilot,
            host: "local".into(),
            title: Some("fix the thing".into()),
            workspace: Some(r"C:\Users\dragos\gh\vitals".into()),
            repo: Some("vitals".into()),
            branch: None,
            model: None,
            state: SessionState::Working,
            started_ms: None,
            last_activity_ms: None,
            active_ms: Some(0),
            turns: Some(1),
            user_messages: Some(1),
            tool_calls: Some(0),
            tool_failures: Some(0),
            current_tool: None,
            top_tools: Vec::new(),
            tokens_in: None,
            tokens_out: None,
            pid: None,
        }],
        ..AgentsSnapshot::default()
    }
}

fn ticket(id: &str, held: bool, cmd: &str) -> AgentqTicket {
    AgentqTicket {
        id: id.into(),
        resource: "build:codai".into(),
        seq: 1,
        held,
        lane: None,
        priority: None,
        pid: None,
        session: None,
        purpose: None,
        cmd: Some(cmd.into()),
        repo: None,
        created_ms: None,
        acquired_ms: None,
        beat_ms: None,
        liveness: Liveness::Alive,
        yielded: false,
        wedged: false,
    }
}

struct Harness {
    base: String,
    fake: Arc<Fake>,
    frames: FrameSource,
    handle: vitals_server::ServeHandle,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

async fn start(with_agents: bool) -> Harness {
    start_with(with_agents, None).await
}

async fn start_with(with_agents: bool, loopback_scope: Option<Scope>) -> Harness {
    let fake = Arc::new(Fake::default());
    let frames = FrameSource::new();
    let state = ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(TokenSet {
            tokens: vec![
                Token::new(READ, Scope::Read, "phone", 0),
                Token::new(CONTROL, Scope::Control, "trusted", 0),
            ],
        })),
        controller: Arc::new(vitals_server::control::NoControl),
        assets: None,
        host: Arc::new(|| None),
        alerts: Arc::new(Vec::new),
        history: Arc::new(|_| Vec::new()),
        sensors: Arc::new(Vec::new),
        version: "0.0.0-test".into(),
        loopback_scope,
        pairing: None,
        agents: with_agents.then(|| Arc::clone(&fake) as Arc<dyn AgentsProvider>),
    };
    let handle = serve(state, 0).await.expect("bind");
    Harness {
        base: format!("127.0.0.1:{}", handle.addr.port()),
        fake,
        frames,
        handle,
    }
}

async fn request(
    addr: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> (u16, String) {
    use std::fmt::Write as _;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(token) = token {
        let _ = write!(head, "Authorization: Bearer {token}\r\n");
    }
    if let Some(body) = body {
        let _ = write!(
            head,
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        );
    }
    head.push_str("\r\n");
    if let Some(body) = body {
        head.push_str(body);
    }
    stream.write_all(head.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map_or("", |(_, b)| b)
        .to_owned();
    (status, body)
}

const NOTE: &str = r#"{"kind":"note","message":"phone says hello"}"#;

#[tokio::test]
async fn a_read_token_sees_the_snapshot_with_homes_and_secrets_redacted() {
    let h = start(true).await;
    let (status, body) = request(&h.base, "GET", "/api/v1/agents", Some(READ), None).await;
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(json["agentqInstalled"], true);
    assert_eq!(json["resources"][0]["tickets"][0]["id"], "t1");
    assert_eq!(json["sessions"][0]["state"], "working");
    assert!(!body.contains("dragos"), "home leaked: {body}");
    assert!(!body.contains("hunter2"), "secret leaked: {body}");
    assert!(body.contains(r"~\\gh\\vitals"), "{body}");
    // The shape still decodes as the protocol type a client parses.
    let back: AgentsSnapshot = serde_json::from_str(&body).expect("decodes as AgentsSnapshot");
    assert_eq!(back.resources[0].tickets.len(), 2);
}

#[tokio::test]
async fn a_host_without_an_ai_page_answers_404_not_an_empty_snapshot() {
    let h = start(false).await;
    let (status, _) = request(&h.base, "GET", "/api/v1/agents", Some(READ), None).await;
    assert_eq!(status, 404);
    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/agents/actions",
        Some(CONTROL),
        Some(NOTE),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_read_token_cannot_act_and_is_refused_before_the_body_is_parsed() {
    let h = start(true).await;
    let (valid, valid_body) = request(
        &h.base,
        "POST",
        "/api/v1/agents/actions",
        Some(READ),
        Some(NOTE),
    )
    .await;
    let (garbage, garbage_body) = request(
        &h.base,
        "POST",
        "/api/v1/agents/actions",
        Some(READ),
        Some("not json {"),
    )
    .await;
    assert_eq!((valid, garbage), (403, 403));
    // Identical answers, or the difference maps the schema.
    assert_eq!(valid_body, garbage_body);
    assert!(h.fake.acted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_read_token_is_refused_even_on_a_host_without_an_ai_page() {
    // 403 before 404: a read token must not learn whether the host offers
    // actions at all.
    let h = start(false).await;
    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/agents/actions",
        Some(READ),
        Some(NOTE),
    )
    .await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn a_control_token_runs_the_action_and_gets_redacted_output() {
    let h = start(true).await;
    let (status, body) = request(
        &h.base,
        "POST",
        "/api/v1/agents/actions",
        Some(CONTROL),
        Some(NOTE),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(r#""ok":true"#), "{body}");
    assert!(!body.contains("dragos"), "{body}");
    let acted = h.fake.acted.lock().unwrap();
    assert_eq!(
        *acted,
        vec![AgentqAction::Note {
            message: "phone says hello".into()
        }]
    );
}

#[tokio::test]
async fn a_malformed_or_refused_action_from_a_control_token_is_422() {
    let h = start(true).await;
    let (status, body) = request(
        &h.base,
        "POST",
        "/api/v1/agents/actions",
        Some(CONTROL),
        Some(r#"{"kind":"selfDestruct"}"#),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    let (status, body) = request(
        &h.base,
        "POST",
        "/api/v1/agents/actions",
        Some(CONTROL),
        Some(r#"{"kind":"note","message":"no"}"#),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("at least 4"), "{body}");
    assert!(h.fake.acted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn metrics_carry_agent_series_only_when_the_host_has_an_ai_page() {
    let with = start(true).await;
    with.publish_a_frame();
    let (_, body) = request(&with.base, "GET", "/metrics", Some(READ), None).await;
    assert!(
        body.contains("vitals_agents_sessions{client=\"vscodeCopilot\",state=\"working\"} 1"),
        "{body}"
    );
    assert!(
        body.contains("vitals_agentq_waiters{resource=\"build:codai\"} 1"),
        "{body}"
    );

    let without = start(false).await;
    without.publish_a_frame();
    let (_, body) = request(&without.base, "GET", "/metrics", Some(READ), None).await;
    assert!(body.contains("vitals_cpu_percent"), "{body}");
    assert!(!body.contains("vitals_agents"), "{body}");
    assert!(!body.contains("vitals_agentq"), "{body}");
}

impl Harness {
    /// `/metrics` answers 204 before the first frame; give it one.
    fn publish_a_frame(&self) {
        self.frames
            .publish(Arc::new(vitals_core::fixtures::keyframe(1, Vec::new())));
    }
}

// The desktop's 127.0.0.1:7330 listener grants a tokenless *read* to any
// loopback caller, and loopback is shared by every account on the PC. The
// AI snapshot is one user's session titles and command lines, so it must not
// ride that grant (ENG2, S21-04). Every request here comes from 127.0.0.1
// with a loopback Host and no Origin: exactly what the bypass accepts.

#[tokio::test]
async fn another_account_on_loopback_without_a_token_gets_no_agents_snapshot() {
    let h = start_with(true, Some(Scope::Read)).await;
    let (status, body) = request(&h.base, "GET", "/api/v1/agents", None, None).await;
    assert_eq!(status, 403, "{body}");
    assert!(!body.contains("fix the thing"), "{body}");
    assert_eq!(
        h.fake.snapshots.load(Ordering::SeqCst),
        0,
        "the provider must not even be asked"
    );
    // The same listener still serves the rest of the tokenless read scope.
    h.publish_a_frame();
    let (status, _) = request(&h.base, "GET", "/api/v1/snapshot", None, None).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn a_tokenless_loopback_caller_cannot_act_even_with_a_control_loopback_grant() {
    let h = start_with(true, Some(Scope::Control)).await;
    let (status, _) = request(&h.base, "POST", "/api/v1/agents/actions", None, Some(NOTE)).await;
    assert_eq!(status, 403);
    assert!(h.fake.acted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_tokenless_loopback_scrape_carries_no_agent_series() {
    let h = start_with(true, Some(Scope::Read)).await;
    h.publish_a_frame();
    let (status, body) = request(&h.base, "GET", "/metrics", None, None).await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("vitals_cpu_percent"), "{body}");
    assert!(!body.contains("vitals_agents"), "{body}");
    assert!(!body.contains("vitals_agentq"), "{body}");
    assert_eq!(h.fake.snapshots.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_paired_token_still_reads_agents_over_loopback_when_the_bypass_is_on() {
    let h = start_with(true, Some(Scope::Read)).await;
    let (status, body) = request(&h.base, "GET", "/api/v1/agents", Some(READ), None).await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("fix the thing"), "{body}");
}
