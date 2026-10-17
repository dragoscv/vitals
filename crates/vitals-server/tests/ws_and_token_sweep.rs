//! Control over the `WebSocket`, and the wrong-token oracle, over a real
//! socket (QA-7).
//!
//! `api.rs` proves the HTTP `/control` route refuses a read-only token. The
//! `WebSocket` is a second door to the same controller with its own scope
//! check (`handle_control`), reached only after an upgrade — a path the
//! router's unit test exercises without a socket and nothing exercised with
//! one. These tests speak RFC 6455 by hand rather than adding a client crate
//! for a handshake and two frames.
//!
//! The sweep reads the guarded route list out of `router.rs`, so a route
//! added there is swept here without anyone remembering to.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use vitals_server::state::ServerLock;
use vitals_server::{
    ApiState, ControlError, ControlRequest, Controller, FrameSource, Scope, Token, TokenSet, serve,
};

const READ_TOKEN: &str = "ws-read-token";
const CONTROL_TOKEN: &str = "ws-control-token";
const TERMINATE: &str = r#"{"action":"terminate","key":{"pid":4242,"startTime":1}}"#;

#[derive(Debug, Default)]
struct RecordingController {
    calls: AtomicUsize,
    last: Mutex<Option<ControlRequest>>,
}

impl Controller for RecordingController {
    fn apply(&self, request: ControlRequest) -> Result<(), ControlError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last.lock().unwrap() = Some(request);
        Ok(())
    }
}

struct Harness {
    addr: String,
    controller: Arc<RecordingController>,
    handle: vitals_server::ServeHandle,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

/// No frame is ever published, so the server sends nothing on a socket
/// until it answers a message: the first frame read is the reply.
async fn start() -> Harness {
    let controller = Arc::new(RecordingController::default());
    let state = ApiState {
        frames: FrameSource::new(),
        tokens: Arc::new(ServerLock::new(TokenSet {
            tokens: vec![
                Token::new(READ_TOKEN, Scope::Read, "phone", 0),
                Token::new(CONTROL_TOKEN, Scope::Control, "trusted", 0),
            ],
        })),
        controller: controller.clone(),
        assets: None,
        host: Arc::new(|| None),
        alerts: Arc::new(Vec::new),
        history: Arc::new(|_| Vec::new()),
        sensors: Arc::new(Vec::new),
        version: "0.0.0-test".into(),
        loopback_scope: None,
        pairing: None,
    };
    let handle = serve(state, 0).await.expect("bind");
    Harness {
        addr: format!("127.0.0.1:{}", handle.addr.port()),
        controller,
        handle,
    }
}

/// A completed `WebSocket` handshake, plus whatever followed the response
/// head in the same read.
struct Socket {
    stream: TcpStream,
    pending: Vec<u8>,
}

/// Upgrades `/api/v1/ws` with the token in the query, as the phone does.
/// Returns the status line's code and, on 101, the open socket.
async fn upgrade(addr: &str, token: &str) -> (u16, Option<Socket>) {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let head = format!(
        "GET /api/v1/ws?token={token} HTTP/1.1\r\nHost: {addr}\r\nConnection: Upgrade\r\n\
         Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    stream.write_all(head.as_bytes()).await.expect("write");

    let mut buf = Vec::new();
    let end = loop {
        if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        let mut chunk = [0_u8; 1024];
        let n = stream.read(&mut chunk).await.expect("read head");
        assert!(n > 0, "connection closed before a response head");
        buf.extend_from_slice(&chunk[..n]);
    };
    let text = String::from_utf8_lossy(&buf[..end]).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if status != 101 {
        return (status, None);
    }
    let pending = buf[end..].to_vec();
    (status, Some(Socket { stream, pending }))
}

impl Socket {
    /// Sends one masked text frame. Clients must mask (RFC 6455 §5.3); an
    /// unmasked frame would be refused and prove nothing about scope.
    async fn send_text(&mut self, text: &str) {
        let payload = text.as_bytes();
        let mask = [0x37_u8, 0xfa, 0x21, 0x3d];
        let mut frame = vec![0x81_u8];
        if payload.len() < 126 {
            frame.push(0x80 | u8::try_from(payload.len()).unwrap());
        } else {
            // 126 = "16-bit length follows".
            frame.push(0x80 | 0x7e);
            frame.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&frame).await.expect("send frame");
    }

    async fn read_exact(&mut self, n: usize) -> Vec<u8> {
        while self.pending.len() < n {
            let mut chunk = [0_u8; 4096];
            let read = self.stream.read(&mut chunk).await.expect("read frame");
            assert!(read > 0, "socket closed mid-frame");
            self.pending.extend_from_slice(&chunk[..read]);
        }
        self.pending.drain(..n).collect()
    }

    /// Reads the next server frame: (opcode, payload). Server frames are
    /// unmasked.
    async fn recv(&mut self) -> (u8, Vec<u8>) {
        let head = self.read_exact(2).await;
        let opcode = head[0] & 0x0f;
        let len = match head[1] & 0x7f {
            126 => usize::from(u16::from_be_bytes(
                self.read_exact(2).await.try_into().unwrap(),
            )),
            127 => usize::try_from(u64::from_be_bytes(
                self.read_exact(8).await.try_into().unwrap(),
            ))
            .unwrap(),
            n => usize::from(n),
        };
        (opcode, self.read_exact(len).await)
    }

    /// The reply to one control message, bounded so a server that never
    /// answers fails the test instead of hanging the suite.
    async fn control(&mut self, text: &str) -> serde_json::Value {
        self.send_text(text).await;
        let (opcode, payload) =
            tokio::time::timeout(std::time::Duration::from_secs(10), self.recv())
                .await
                .expect("the server answered within 10 s");
        assert_eq!(opcode, 0x1, "a control reply is a text frame");
        serde_json::from_slice(&payload).expect("the reply is JSON")
    }
}

#[tokio::test]
async fn a_read_scope_websocket_cannot_terminate_and_the_controller_is_never_reached() {
    let h = start().await;
    let (status, socket) = upgrade(&h.addr, READ_TOKEN).await;
    assert_eq!(status, 101, "a read token may stream");
    let mut socket = socket.unwrap();

    let reply = socket.control(TERMINATE).await;
    assert_eq!(
        reply,
        serde_json::json!({ "ok": false, "error": { "kind": "forbidden" } })
    );
    // Malformed after valid: still forbidden, never "could not read", so the
    // schema cannot be probed through a read-only socket.
    let reply = socket.control("not json {").await;
    assert_eq!(reply["error"]["kind"], "forbidden", "{reply}");

    assert_eq!(
        h.controller.calls.load(Ordering::SeqCst),
        0,
        "no process was touched"
    );
}

#[tokio::test]
async fn a_control_scope_websocket_reaches_the_controller_with_the_request_intact() {
    // The counterpart that makes the refusal above meaningful: the same
    // frame over the same socket path does arrive when the scope allows it.
    let h = start().await;
    let (status, socket) = upgrade(&h.addr, CONTROL_TOKEN).await;
    assert_eq!(status, 101);
    let mut socket = socket.unwrap();

    let reply = socket.control(TERMINATE).await;
    assert_eq!(reply, serde_json::json!({ "ok": true }));
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        h.controller.last.lock().unwrap().clone(),
        Some(ControlRequest::Terminate {
            key: vitals_core::ids::ProcessKey {
                pid: vitals_core::ids::Pid(4242),
                start_time: 1,
            },
        })
    );
}

#[tokio::test]
async fn a_websocket_upgrade_with_a_wrong_token_is_refused_before_it_opens() {
    let h = start().await;
    let (status, socket) = upgrade(&h.addr, "not-a-token").await;
    assert_eq!(status, 401);
    assert!(socket.is_none());
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 0);
}

/// Every `(method, path)` behind the auth layer, read from `router.rs`.
fn guarded_routes() -> Vec<(&'static str, String)> {
    let source = include_str!("../src/router.rs");
    let start = source
        .find("let guarded = Router::new()")
        .expect("guarded router");
    let end = start + source[start..].find(".route_layer(").expect("auth layer");
    source[start..end]
        .split(".route(\"")
        .skip(1)
        .filter_map(|chunk| {
            let (path, rest) = chunk.split_once('"')?;
            let handler = rest.split(')').next().unwrap_or_default();
            let method = if handler.contains("post(") {
                "POST"
            } else {
                "GET"
            };
            Some((method, path.to_owned()))
        })
        .collect()
}

/// The whole response, minus `Date` (which ticks between two requests). Read
/// to close: everything a caller could compare, not just the status.
async fn raw_response(addr: &str, method: &str, path: &str, auth: Option<&str>) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(token) = auth {
        head.push_str("Authorization: Bearer ");
        head.push_str(token);
        head.push_str("\r\n");
    }
    if method == "POST" {
        head.push_str("Content-Type: application/json\r\nContent-Length: ");
        head.push_str(&TERMINATE.len().to_string());
        head.push_str("\r\n\r\n");
        head.push_str(TERMINATE);
    } else {
        head.push_str("\r\n");
    }
    stream.write_all(head.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    String::from_utf8_lossy(&raw)
        .split("\r\n")
        .filter(|line| !line.to_ascii_lowercase().starts_with("date:"))
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[tokio::test]
async fn every_guarded_route_answers_a_wrong_token_byte_for_byte_like_a_missing_one() {
    // A response that differs in any byte — a header, a length, a word of
    // the body — tells a guesser that a token was structurally right, or
    // that one route checks differently from another.
    let h = start().await;
    let routes = guarded_routes();
    assert!(
        routes.len() >= 10,
        "parsed too few routes from router.rs: {routes:?}"
    );
    assert!(
        routes
            .iter()
            .any(|(m, p)| *m == "POST" && p == "/api/v1/control")
    );

    let mut reference: Option<String> = None;
    for (method, path) in &routes {
        let missing = raw_response(&h.addr, method, path, None).await;
        assert!(
            missing.starts_with("HTTP/1.1 401 "),
            "{method} {path} without a token: {missing}"
        );
        let query = format!("{path}?token=wrong-in-query");
        for (label, response) in [
            (
                "wrong bearer",
                raw_response(&h.addr, method, path, Some("wrong-token")).await,
            ),
            (
                "near-miss bearer",
                raw_response(
                    &h.addr,
                    method,
                    path,
                    Some(&READ_TOKEN[..READ_TOKEN.len() - 1]),
                )
                .await,
            ),
            (
                "wrong query token",
                raw_response(&h.addr, method, &query, None).await,
            ),
        ] {
            assert_eq!(
                response, missing,
                "{method} {path}: a {label} differs from no token"
            );
        }
        // And every route refuses identically, so the refusal does not
        // reveal which routes exist behind it.
        match &reference {
            None => reference = Some(missing),
            Some(first) => assert_eq!(&missing, first, "{method} {path} refuses differently"),
        }
    }
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 0);
}
