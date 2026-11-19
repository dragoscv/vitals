//! `/mcp` over a real socket: the handshake, the tools, and the refusal.
//!
//! The route is the one surface here that speaks to a program which decides
//! what to do next, so the assertions that matter are the refusals: no token,
//! a wrong token, and a tool that would act. A file that only proved
//! `tools/list` returns six tools would have passed just as well with the
//! auth layer deleted.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use vitals_core::fixtures;
use vitals_server::state::ServerLock;
use vitals_server::{ApiState, FrameSource, Scope, Token, TokenSet, serve};

const READ_TOKEN: &str = "mcp-read-token-value";
const WRONG_TOKEN: &str = "not-a-real-token";

struct Harness {
    base: String,
    handle: vitals_server::ServeHandle,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

/// A running server that grants the tokenless loopback read scope, so the
/// "needs a token even on loopback" rule is actually exercised rather than
/// accidentally satisfied by the auth layer.
async fn start() -> Harness {
    start_with(Some(Scope::Read)).await
}

/// The same server with the loopback grant switched on or off.
///
/// The two configurations differ in exactly one field, which is what makes
/// the pair of refusal tests meaningful: with the grant off, the auth layer
/// answers; with it on, the route itself must.
async fn start_with(loopback_scope: Option<Scope>) -> Harness {
    let frames = FrameSource::new();
    let mut frame = fixtures::keyframe(
        1,
        vec![
            fixtures::process("chrome.exe", 4242, 71.0),
            fixtures::process("explorer.exe", 100, 4.0),
        ],
    );
    // The machine's own count has to match the list it serves.
    if let vitals_core::sample::FramePayload::Keyframe { system, .. } = &mut frame.payload {
        system.cpu.process_count = Some(2);
    }
    frames.publish(Arc::new(frame));
    let state = ApiState {
        frames,
        tokens: Arc::new(ServerLock::new(TokenSet {
            tokens: vec![Token::new(READ_TOKEN, Scope::Read, "agent", 0)],
        })),
        controller: Arc::new(vitals_server::control::NoControl),
        assets: None,
        host: Arc::new(|| {
            Some(vitals_core::provider::HostInfo {
                hostname: "mcp-test-pc".to_owned(),
                os_name: "Windows 11".to_owned(),
                os_version: "10.0".to_owned(),
                kernel_version: "10.0".to_owned(),
                architecture: "x86_64".to_owned(),
                cpu_model: "Test".to_owned(),
                cpu_vendor: "Test".to_owned(),
                physical_cores: 4,
                logical_cores: 8,
                core_topology: None,
                total_memory: vitals_core::units::Bytes(8 * 1024 * 1024 * 1024),
                boot_time_ms: 0,
                is_virtual_machine: false,
                motherboard: None,
                bios_version: None,
            })
        }),
        alerts: Arc::new(Vec::new),
        history: Arc::new(|seconds| {
            vec![vitals_core::MachineSample::from_metrics(
                i64::from(seconds),
                &fixtures::system(),
            )]
        }),
        sensors: Arc::new(|| {
            vec![vitals_core::remote::SensorLine {
                key: "cpu.package".to_owned(),
                label: "CPU package".to_owned(),
                value: 81.0,
                unit: "temperature".to_owned(),
                source: "kernelDriver".to_owned(),
                quality: "measured".to_owned(),
            }]
        }),
        connectivity: Arc::new(|| {
            vitals_core::connectivity::ConnectivityReport::from_parts(None, Vec::new(), None)
        }),
        version: "0.0.0-test".to_owned(),
        loopback_scope,
        pairing: None,
        agents: None,
    };

    let handle = serve(state, 0).await.expect("bind a free port");
    let base = format!("http://127.0.0.1:{}", handle.addr.port());
    Harness { base, handle }
}

/// A minimal HTTP/1.1 client, as in `api.rs`: a dev-dependency on reqwest and
/// its TLS stack would be more code than the requests it makes.
async fn post(base: &str, token: Option<&str>, body: &str) -> (u16, String) {
    use std::fmt::Write as _;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let addr = base.trim_start_matches("http://");
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let mut head = format!("POST /mcp HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(token) = token {
        let _ = write!(head, "Authorization: Bearer {token}\r\n");
    }
    head.push_str("Content-Type: application/json\r\n");
    let _ = write!(head, "Content-Length: {}\r\n\r\n", body.len());
    head.push_str(body);

    stream.write_all(head.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map_or(String::new(), |(_, b)| b.to_owned());
    (status, body)
}

async fn call(base: &str, message: serde_json::Value) -> serde_json::Value {
    let (status, body) = post(base, Some(READ_TOKEN), &message.to_string()).await;
    assert_eq!(status, 200, "the call was refused: {body}");
    serde_json::from_str(&body).expect("the reply is one JSON message")
}

#[tokio::test]
async fn a_tokenless_call_is_refused_even_on_loopback() {
    // The route's whole reason for requiring a token: an agent should be a
    // pairing the user can list and revoke, and a process on the machine is
    // not one. The auth layer already grants this caller read scope without a
    // token, so a 403 here is the route's own refusal, not the layer's.
    let h = start().await;
    let (status, body) = post(
        &h.base,
        None,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    )
    .await;
    assert_eq!(status, 403, "a tokenless MCP call must be refused: {body}");
    // The exact bytes: a client that shows the reason to a user should not
    // see it change shape between releases.
    assert_eq!(
        body.trim(),
        r#"{"error":"the MCP endpoint needs a paired token, also on loopback"}"#
    );
}

#[tokio::test]
async fn a_wrong_token_is_refused_by_the_auth_layer_when_the_loopback_grant_is_off() {
    // With no tokenless grant to fall back on, a bad token is the auth
    // layer's business and never reaches the route.
    let h = start_with(None).await;
    let (status, _) = post(
        &h.base,
        Some(WRONG_TOKEN),
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn a_wrong_token_on_loopback_still_does_not_reach_the_tools() {
    // A wrong token falls back to the tokenless grant on loopback (by
    // design: the grant exists so `vitals ps` works), and the grant carries
    // no token — so the route refuses it for the same reason it refuses a
    // caller who sent nothing at all.
    let h = start().await;
    let (status, body) = post(
        &h.base,
        Some(WRONG_TOKEN),
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("needs a paired token"), "{body}");
}

#[tokio::test]
async fn the_initialize_handshake_names_the_server_and_its_tools_capability() {
    let h = start().await;
    let reply = call(
        &h.base,
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }),
    )
    .await;
    assert_eq!(reply["result"]["serverInfo"]["name"], "vitals");
    assert!(reply["result"]["protocolVersion"].is_string());
    assert_eq!(
        reply["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
}

#[tokio::test]
async fn a_notification_is_accepted_with_no_body() {
    let h = start().await;
    let (status, body) = post(
        &h.base,
        Some(READ_TOKEN),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    )
    .await;
    assert_eq!(status, 202, "{body}");
    assert!(
        body.trim().is_empty(),
        "a notification gets no reply: {body}"
    );
}

#[tokio::test]
async fn tools_list_offers_the_six_readings_and_nothing_that_acts() {
    let h = start().await;
    let reply = call(
        &h.base,
        serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await;
    let names: Vec<&str> = reply["result"]["tools"]
        .as_array()
        .expect("an array of tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        names,
        vec![
            "summary",
            "processes",
            "sensors",
            "alerts",
            "history",
            "diagnosis"
        ]
    );
}

#[tokio::test]
async fn a_summary_call_reaches_the_materialised_frame() {
    let h = start().await;
    let reply = call(
        &h.base,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "summary", "arguments": { "top": 1 } },
        }),
    )
    .await;
    assert_eq!(reply["result"]["isError"], false);
    let payload = &reply["result"]["structuredContent"];
    assert_eq!(payload["processCount"], 2);
    assert_eq!(payload["top"][0]["name"], "chrome.exe");
}

#[tokio::test]
async fn a_read_token_cannot_reach_a_control_tool_because_there_is_none() {
    // The guarantee the row asks for, over the transport a remote agent uses:
    // a read-scope token is refused for every verb, and the refusal is the
    // same "no such tool" a typo gets, so no verb is discoverable by probing.
    let h = start().await;
    for name in ["end_process", "kill", "suspend", "resume", "set_priority"] {
        let reply = call(
            &h.base,
            serde_json::json!({
                "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": name },
            }),
        )
        .await;
        assert_eq!(
            reply["error"]["code"], -32602,
            "`{name}` must not exist: {reply}"
        );
    }
}

#[tokio::test]
async fn the_machine_resource_is_listed_and_readable() {
    let h = start().await;
    let listed = call(
        &h.base,
        serde_json::json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" }),
    )
    .await;
    let uri = listed["result"]["resources"][0]["uri"]
        .as_str()
        .expect("a URI")
        .to_owned();
    assert_eq!(uri, "vitals://mcp-test-pc");

    let read = call(
        &h.base,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 6, "method": "resources/read",
            "params": { "uri": uri },
        }),
    )
    .await;
    let text = read["result"]["contents"][0]["text"]
        .as_str()
        .expect("a document");
    let document: serde_json::Value = serde_json::from_str(text).expect("JSON");
    assert_eq!(document["readOnly"], true);
    assert_eq!(document["tools"].as_array().expect("tools").len(), 6);
}

#[tokio::test]
async fn a_body_that_is_not_json_gets_a_json_rpc_parse_error_and_the_route_keeps_working() {
    // A malformed message must not be an HTTP failure: the agent reads the
    // reason from the protocol, and the next message still works.
    let h = start().await;
    let (status, body) = post(&h.base, Some(READ_TOKEN), "this is not json").await;
    assert_eq!(status, 200, "{body}");
    let reply: serde_json::Value = serde_json::from_str(&body).expect("a JSON reply");
    assert_eq!(reply["error"]["code"], -32700);
    assert_eq!(reply["id"], serde_json::Value::Null);

    let healthy = call(
        &h.base,
        serde_json::json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" }),
    )
    .await;
    assert_eq!(healthy["id"], 7);
}
