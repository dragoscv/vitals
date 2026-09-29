//! End-to-end tests over a real socket.
//!
//! Deliberately not `tower::ServiceExt::oneshot` against the router: that
//! skips the listener, the HTTP parser and the header handling, which is
//! where an auth bug would actually live. These bind a port, speak HTTP, and
//! assert on what a phone would receive.
//!
//! The security assertions here are the point of the file. A test that only
//! proves the happy path would have passed just as well with the auth layer
//! deleted.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vitals_core::fixtures;
use vitals_core::ids::{Pid, ProcessKey};
use vitals_core::sample::Frame;
use vitals_server::state::ServerLock;
use vitals_server::{
    ApiState, ControlError, ControlRequest, Controller, FrameSource, PairingDesk, Scope, Token,
    TokenSet, serve, serve_on,
};

const READ_TOKEN: &str = "read-token-value";
const CONTROL_TOKEN: &str = "control-token-value";

#[derive(Debug, Default)]
struct RecordingController {
    calls: AtomicUsize,
    /// What the last call carried, so a test can prove the request survived
    /// deserialisation intact rather than merely arriving.
    last: Mutex<Option<ControlRequest>>,
}

impl Controller for RecordingController {
    fn apply(&self, request: ControlRequest) -> Result<(), ControlError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last.lock().unwrap() = Some(request);
        Ok(())
    }
}

fn frame(seq: u64) -> Frame {
    fixtures::keyframe(seq, vec![fixtures::process("chrome.exe", 4242, 12.0)])
}

struct Harness {
    base: String,
    frames: FrameSource,
    tokens: Arc<ServerLock<TokenSet>>,
    controller: Arc<RecordingController>,
    handle: vitals_server::ServeHandle,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

async fn start() -> Harness {
    start_with(None).await
}

/// Builds the state the harness serves. Shared with the loopback tests so
/// the two configurations differ in exactly one field.
fn state_with(
    frames: &FrameSource,
    controller: &Arc<RecordingController>,
    loopback_scope: Option<Scope>,
) -> ApiState {
    let tokens = TokenSet {
        tokens: vec![
            Token::new(READ_TOKEN, Scope::Read, "phone", 0),
            Token::new(CONTROL_TOKEN, Scope::Control, "trusted", 0),
        ],
    };

    ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(tokens)),
        controller: controller.clone(),
        assets: Some(Arc::new(|path: &str| {
            (path == "mobile.html")
                .then(|| (b"<!doctype html>ok".to_vec(), "text/html; charset=utf-8"))
        })),
        host: Arc::new(|| None),
        alerts: Arc::new(Vec::new),
        history: Arc::new(|seconds| {
            vec![vitals_core::MachineSample::from_metrics(
                i64::from(seconds),
                &fixtures::system(),
            )]
        }),
        sensors: Arc::new(|| {
            vec![vitals_core::remote::SensorLine {
                key: "cpu.package".into(),
                label: "CPU package".into(),
                value: 81.0,
                unit: "temperature".into(),
                source: "kernelDriver".into(),
                quality: "measured".into(),
            }]
        }),
        version: "0.0.0-test".into(),
        loopback_scope,
        pairing: None,
    }
}

async fn start_with(loopback_scope: Option<Scope>) -> Harness {
    let frames = FrameSource::new();
    let controller = Arc::new(RecordingController::default());
    let state = state_with(&frames, &controller, loopback_scope);
    let tokens = Arc::clone(&state.tokens);

    // Port 0: the OS picks a free one, so tests never collide with a real
    // server or with each other.
    let handle = serve(state, 0).await.expect("bind");
    let base = format!("http://127.0.0.1:{}", handle.addr.port());
    Harness {
        base,
        frames,
        tokens,
        controller,
        handle,
    }
}

/// A minimal HTTP/1.1 client. Avoids adding reqwest (and rustls, and hyper's
/// client stack) as a dev-dependency for a handful of requests.
async fn request(
    base: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> (u16, String) {
    use std::fmt::Write as _;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let addr = base.trim_start_matches("http://");
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");

    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(token) = token {
        let _ = write!(head, "Authorization: Bearer {token}\r\n");
    }
    if let Some(body) = body {
        head.push_str("Content-Type: application/json\r\n");
        let _ = write!(head, "Content-Length: {}\r\n", body.len());
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

#[tokio::test]
async fn health_needs_no_token_so_a_client_can_tell_wrong_address_from_wrong_token() {
    let h = start().await;
    let (status, body) = request(&h.base, "GET", "/api/v1/health", None, None).await;
    assert_eq!(status, 200);
    assert!(body.contains("\"ok\":true"), "{body}");
    assert!(body.contains("modelVersion"), "{body}");
}

#[tokio::test]
async fn every_data_route_refuses_an_anonymous_request() {
    // The single most important assertion in this file: without it, the auth
    // layer could be deleted and the happy-path tests would still pass.
    let h = start().await;
    h.frames.publish(Arc::new(frame(1)));

    for path in [
        "/api/v1/snapshot",
        "/api/v1/summary",
        "/api/v1/history",
        "/api/v1/sensors",
        "/api/v1/stream",
        "/api/v1/host",
        "/metrics",
    ] {
        let (status, body) = request(&h.base, "GET", path, None, None).await;
        assert_eq!(status, 401, "{path} allowed an anonymous request: {body}");
    }
}

#[tokio::test]
async fn a_wrong_token_is_refused_identically_to_a_missing_one() {
    // Distinguishing them tells an attacker when a guess is structurally
    // right, which is a free oracle.
    let h = start().await;
    let (missing, missing_body) = request(&h.base, "GET", "/api/v1/snapshot", None, None).await;
    let (wrong, wrong_body) = request(&h.base, "GET", "/api/v1/snapshot", Some("nope"), None).await;
    assert_eq!(missing, 401);
    assert_eq!(wrong, 401);
    assert_eq!(missing_body, wrong_body);
}

#[tokio::test]
async fn snapshot_is_no_content_before_the_first_frame_then_the_frame() {
    let h = start().await;

    let (status, _) = request(&h.base, "GET", "/api/v1/snapshot", Some(READ_TOKEN), None).await;
    assert_eq!(
        status, 204,
        "an empty object would look like an idle machine"
    );

    h.frames.publish(Arc::new(frame(7)));
    let (status, body) = request(&h.base, "GET", "/api/v1/snapshot", Some(READ_TOKEN), None).await;
    assert_eq!(status, 200);
    // The wire contract is camelCase at every depth; this is the regression
    // that once rendered the whole desktop app blank.
    assert!(body.contains("\"timestampMs\""), "{body}");
    assert!(body.contains("\"elapsedMs\""), "{body}");
    assert!(!body.contains("timestamp_ms"), "{body}");
}

#[tokio::test]
async fn metrics_is_prometheus_text_and_carries_real_values() {
    let h = start().await;
    h.frames.publish(Arc::new(frame(1)));

    let (status, body) = request(&h.base, "GET", "/metrics", Some(READ_TOKEN), None).await;
    assert_eq!(status, 200);
    assert!(body.contains("# TYPE vitals_cpu_percent gauge"), "{body}");
    assert!(body.contains("vitals_cpu_percent 33.500"), "{body}");
    assert!(
        body.contains(r#"vitals_cpu_core_percent{core="1"} 37.000"#),
        "{body}"
    );
}

#[tokio::test]
async fn a_read_token_cannot_control_anything() {
    let h = start().await;
    let body = r#"{"action":"terminate","key":{"pid":4242,"startTime":1}}"#;

    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(READ_TOKEN),
        Some(body),
    )
    .await;
    assert_eq!(
        status, 403,
        "a phone paired read-only must not be a kill switch"
    );
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_control_token_reaches_the_controller() {
    let h = start().await;
    let body = r#"{"action":"suspend","key":{"pid":4242,"startTime":1}}"#;

    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(CONTROL_TOKEN),
        Some(body),
    )
    .await;
    assert_eq!(status, 204);
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_read_token_is_refused_before_the_body_is_even_parsed() {
    // Scope before schema. With the body as an extractor, axum answered 422
    // first, so a read-only caller with a slightly wrong request never
    // learned it was read-only — and could probe the schema by watching
    // 422 turn into 403.
    let h = start().await;
    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(READ_TOKEN),
        Some(r#"{"action":"nonsense"}"#),
    )
    .await;
    assert_eq!(status, 403, "scope must be checked before the body");
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_read_token_cannot_set_efficiency_mode_even_with_a_malformed_body() {
    // Same guarantee for the newest action: 403 arrives before the body is
    // looked at, so `enabled` being missing here must not turn into a 422.
    let h = start().await;
    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(READ_TOKEN),
        Some(r#"{"action":"set-efficiency-mode","key":{"pid":4242,"startTime":1}}"#),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_control_token_can_set_efficiency_mode_and_the_controller_sees_the_flag() {
    let h = start().await;
    let body =
        r#"{"action":"set-efficiency-mode","key":{"pid":4242,"startTime":1},"enabled":true}"#;
    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(CONTROL_TOKEN),
        Some(body),
    )
    .await;
    assert_eq!(status, 204);
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        h.controller.last.lock().unwrap().clone(),
        Some(ControlRequest::SetEfficiencyMode {
            key: ProcessKey {
                pid: Pid(4242),
                start_time: 1
            },
            enabled: true,
        })
    );
}

#[tokio::test]
async fn alerts_are_an_array_and_empty_means_healthy() {
    // Not 204: "nothing wrong" is a value a client renders as a green tick,
    // not an absence it has to special-case.
    let h = start().await;
    let (status, body) = request(&h.base, "GET", "/api/v1/alerts", Some(READ_TOKEN), None).await;
    assert_eq!(status, 200);
    assert_eq!(body.trim(), "[]");

    let (status, _) = request(&h.base, "GET", "/api/v1/alerts", None, None).await;
    assert_eq!(
        status, 401,
        "alerts are behind the same auth as everything else"
    );
}

#[tokio::test]
async fn a_control_token_with_a_malformed_body_gets_a_reason() {
    let h = start().await;
    let (status, body) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(CONTROL_TOKEN),
        Some(r#"{"action":"nonsense"}"#),
    )
    .await;
    assert_eq!(status, 422);
    assert!(body.contains("could not read the request"), "{body}");
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_web_app_is_served_without_a_token_but_carries_no_data() {
    // The page has to load before it can read the token out of the fragment.
    let h = start().await;
    let (status, body) = request(&h.base, "GET", "/mobile.html", None, None).await;
    assert_eq!(status, 200);
    assert!(body.contains("<!doctype html>"), "{body}");
}

#[tokio::test]
async fn path_traversal_is_refused() {
    let h = start().await;
    let (status, _) = request(&h.base, "GET", "/../../etc/passwd", None, None).await;
    assert!(status == 400 || status == 404, "status was {status}");
}

#[tokio::test]
async fn a_token_in_the_query_works_for_streams_that_cannot_set_headers() {
    // EventSource and WebSocket in a browser cannot send an Authorization
    // header. Without this the phone cannot stream at all.
    let h = start().await;
    h.frames.publish(Arc::new(frame(1)));
    let (status, _) = request(
        &h.base,
        "GET",
        &format!("/api/v1/snapshot?token={READ_TOKEN}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn a_client_joining_mid_stream_receives_a_keyframe_not_a_delta() {
    // Found against a live sampler: connecting 2.5 s in returned
    // `processes=0`, because the newest frame was a delta describing changes
    // against state the client had never received.
    let h = start().await;

    h.frames.publish(Arc::new(fixtures::keyframe(
        1,
        vec![fixtures::process("chrome.exe", 100, 5.0)],
    )));

    let delta = Frame {
        seq: vitals_core::sample::FrameSeq(2),
        timestamp_ms: 1_700_000_001_000,
        elapsed_ms: 1_000,
        payload: vitals_core::sample::FramePayload::Delta {
            system: fixtures::system(),
            changed: vec![fixtures::process("chrome.exe", 100, 9.0)],
            exited: Vec::new(),
        },
    };
    h.frames.publish(Arc::new(delta));

    let (status, body) = request(&h.base, "GET", "/api/v1/snapshot", Some(READ_TOKEN), None).await;
    assert_eq!(status, 200);
    assert!(
        body.contains("\"kind\":\"keyframe\""),
        "a new client must be given complete state: {body}"
    );
    // And CURRENT state: the delta raised chrome to 9%, and the snapshot
    // must say so. Serving the stale keyframe would pass the assertion above
    // and still show a phone thirty-second-old numbers.
    assert!(body.contains("\"seq\":2"), "{body}");
    assert!(
        body.contains("\"cpu\":9.0"),
        "the delta was not applied: {body}"
    );
    assert!(
        !body.contains("\"cpu\":5.0"),
        "stale keyframe value survived: {body}"
    );
}

#[tokio::test]
async fn a_stream_client_that_fell_behind_is_resynced_with_a_keyframe_not_left_on_a_stale_delta() {
    // The broadcast channel holds 16 frames. A client that stalls for longer
    // than that misses frames; before this test, the server silently skipped
    // the gap and the next thing the client saw was a delta against a frame
    // it never received — a process table that stayed wrong until the
    // sampler's own keyframe, up to thirty seconds later.
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let h = start().await;
    h.frames.publish(Arc::new(fixtures::keyframe(
        1,
        vec![fixtures::process("old.exe", 100, 5.0)],
    )));

    let addr = h.base.trim_start_matches("http://");
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let head = format!(
        "GET /api/v1/stream?token={READ_TOKEN} HTTP/1.1\r\nHost: {addr}\r\nAccept: text/event-stream\r\n\r\n"
    );
    stream.write_all(head.as_bytes()).await.expect("write");
    // Wait for the initial keyframe so the subscription exists before the flood.
    let mut buf = vec![0u8; 64 * 1024];
    let mut received = String::new();
    while !received.contains("\"seq\":1") {
        let n = stream.read(&mut buf).await.expect("read");
        assert!(n > 0, "server closed the stream");
        received.push_str(&String::from_utf8_lossy(&buf[..n]));
    }

    // Flood: 40 deltas while the client reads nothing. Every one of them
    // replaces old.exe with new.exe; the last few are what a skip would
    // deliver, and a delta cannot be applied to the seq-1 view the client has.
    for seq in 2..42 {
        h.frames.publish(Arc::new(Frame {
            seq: vitals_core::sample::FrameSeq(seq),
            timestamp_ms: seq,
            elapsed_ms: 1_000,
            payload: vitals_core::sample::FramePayload::Delta {
                system: fixtures::system(),
                changed: vec![fixtures::process("new.exe", 200, 1.0)],
                exited: vec![Pid(100)],
            },
        }));
    }
    // Kernel buffers are large, so the frames the client "did not read" are
    // in fact in flight; what matters is which ones the broadcast dropped.
    // Read until the newest seq shows up, then inspect the whole transcript.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !received.contains("\"seq\":41") {
        assert!(
            tokio::time::Instant::now() < deadline,
            "never saw seq 41:\n{received}"
        );
        let n = tokio::time::timeout(std::time::Duration::from_secs(1), stream.read(&mut buf))
            .await
            .expect("read timed out")
            .expect("read");
        assert!(n > 0, "server closed the stream");
        received.push_str(&String::from_utf8_lossy(&buf[..n]));
    }

    let events: Vec<&str> = received
        .split("\n\n")
        .filter(|e| e.contains("data:"))
        .collect();
    let seqs: Vec<u64> = events
        .iter()
        .filter_map(|e| {
            e.split("\"seq\":")
                .nth(1)
                .and_then(|rest| rest.split(',').next())
                .and_then(|n| n.parse().ok())
        })
        .collect();
    // Find the first gap in what the client received; the frame right
    // after it must be a keyframe that already reflects every missed delta.
    let gap = seqs
        .windows(2)
        .position(|w| w[1] > w[0] + 1)
        .expect("the client was meant to fall behind; widen the flood if the channel grew");
    let after_gap = events[gap + 1];
    assert!(
        after_gap.contains("\"kind\":\"keyframe\""),
        "after a gap the client must be resynced, got: {after_gap}"
    );
    assert!(
        after_gap.contains("new.exe") && !after_gap.contains("old.exe"),
        "{after_gap}"
    );
}

/// Opens `/api/v1/stream` with `token` and reads until the initial keyframe
/// has arrived, so the subscription is provably live before the test acts.
async fn open_stream(base: &str, token: &str) -> tokio::net::TcpStream {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let h = base.trim_start_matches("http://");
    let mut stream = tokio::net::TcpStream::connect(h).await.expect("connect");
    let head = format!(
        "GET /api/v1/stream?token={token} HTTP/1.1\r\nHost: {h}\r\nAccept: text/event-stream\r\n\r\n"
    );
    stream.write_all(head.as_bytes()).await.expect("write");
    let mut buf = vec![0u8; 16 * 1024];
    let mut received = String::new();
    while !received.contains("\"seq\":1") {
        let n = stream.read(&mut buf).await.expect("read");
        assert!(n > 0, "server closed the stream early:\n{received}");
        received.push_str(&String::from_utf8_lossy(&buf[..n]));
    }
    stream
}

/// Reads until the server ends the response — the closing `0\r\n\r\n` chunk
/// (HTTP keep-alive leaves the socket open) or a socket close — and returns
/// what arrived. `None` if the response is still open after the timeout.
async fn read_to_close(stream: &mut tokio::net::TcpStream) -> Option<String> {
    use tokio::io::AsyncReadExt as _;

    let mut buf = vec![0u8; 16 * 1024];
    let mut received = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if received.ends_with("0\r\n\r\n") {
            return Some(received);
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, stream.read(&mut buf)).await {
            Ok(Ok(0) | Err(_)) => return Some(received),
            Ok(Ok(n)) => received.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(_) => return None,
        }
    }
}

#[tokio::test]
async fn revoking_a_token_ends_the_stream_it_opened_instead_of_feeding_it_until_it_leaves() {
    // Auth ran once at the handshake. Before this test, a revoked phone kept
    // receiving every process name until it disconnected on its own.
    let h = start().await;
    h.frames.publish(Arc::new(frame(1)));
    let mut stream = open_stream(&h.base, READ_TOKEN).await;

    h.tokens.write().revoke(READ_TOKEN);
    // The check is per frame, so one more tick is what ends it.
    h.frames.publish(Arc::new(frame(2)));

    let after = read_to_close(&mut stream)
        .await
        .expect("the stream must close after the token is revoked");
    assert!(
        !after.contains("\"seq\":2"),
        "a frame was delivered on a revoked token: {after}"
    );
}

#[tokio::test]
async fn a_control_token_keeps_streaming_when_a_different_token_is_revoked() {
    // The guard must be per credential, not "any revocation ends everything".
    let h = start().await;
    h.frames.publish(Arc::new(frame(1)));
    let mut stream = open_stream(&h.base, CONTROL_TOKEN).await;

    h.tokens.write().revoke(READ_TOKEN);
    h.frames.publish(Arc::new(frame(2)));

    let outcome = read_to_close(&mut stream).await;
    assert!(
        outcome.is_none(),
        "the stream closed although its own token is still valid: {outcome:?}"
    );
}

#[tokio::test]
async fn stopping_the_server_ends_open_streams_at_once() {
    // Graceful shutdown waits for in-flight requests, and a stream is an
    // in-flight request that never ends: "Stop sharing" left the phone live.
    let mut h = start().await;
    h.frames.publish(Arc::new(frame(1)));
    let mut stream = open_stream(&h.base, READ_TOKEN).await;

    h.handle.stop();

    assert!(
        read_to_close(&mut stream).await.is_some(),
        "the stream must close when the server is stopped"
    );
}

// ── Loopback bypass ────────────────────────────────────────────────────
//
// Every request in this file arrives from 127.0.0.1, so the existing 401
// assertions above are also the proof that `loopback_scope: None` — the LAN
// server's setting — leaves loopback callers exactly as strict as before.

#[tokio::test]
async fn with_a_read_loopback_scope_a_tokenless_local_caller_can_read_but_not_control() {
    let h = start_with(Some(Scope::Read)).await;

    let (status, _) = request(&h.base, "GET", "/api/v1/snapshot", None, None).await;
    assert_eq!(
        status, 204,
        "before the first frame the answer is 'nothing yet'"
    );

    h.frames.publish(Arc::new(frame(1)));
    let (status, body) = request(&h.base, "GET", "/api/v1/snapshot", None, None).await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"kind\":\"keyframe\""), "{body}");

    // Read scope cannot control, token or no token.
    let control = r#"{"action":"terminate","key":{"pid":4242,"startTime":1}}"#;
    let (status, _) = request(&h.base, "POST", "/api/v1/control", None, Some(control)).await;
    assert_eq!(status, 403);
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn with_a_control_loopback_scope_a_tokenless_local_caller_reaches_the_controller() {
    let h = start_with(Some(Scope::Control)).await;
    let control = r#"{"action":"suspend","key":{"pid":4242,"startTime":1}}"#;
    let (status, _) = request(&h.base, "POST", "/api/v1/control", None, Some(control)).await;
    assert_eq!(status, 204);
    assert_eq!(h.controller.calls.load(Ordering::SeqCst), 1);
}

/// Sends a raw request with the given extra header lines.
async fn raw(base: &str, head: &str) -> u16 {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let addr = base.trim_start_matches("http://");
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    stream.write_all(head.as_bytes()).await.expect("write");
    let mut buf = [0_u8; 256];
    let n = stream.read(&mut buf).await.expect("read");
    String::from_utf8_lossy(&buf[..n])
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[tokio::test]
async fn a_web_page_cannot_use_the_loopback_bypass_to_end_a_process() {
    // The attack: any page the user has open can POST to 127.0.0.1 (a
    // "simple" request needs no preflight) and the browser connects from
    // loopback. It carries an Origin the page cannot suppress.
    let h = start_with(Some(Scope::Control)).await;
    let addr = h.base.trim_start_matches("http://");
    let body = r#"{"action":"terminate","key":{"pid":4242,"startTime":1}}"#;
    let head = format!(
        "POST /api/v1/control HTTP/1.1\r\nHost: {addr}\r\nOrigin: https://evil.example\r\n\
         Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    assert_eq!(raw(&h.base, &head).await, 401);
    assert_eq!(
        h.controller.calls.load(Ordering::SeqCst),
        0,
        "nothing was ended"
    );
}

#[tokio::test]
async fn a_websocket_handshake_from_a_page_gets_no_tokenless_access() {
    // WebSockets are exempt from CORS, so this is the path that mattered
    // most: a page could read every process with its start time, then end
    // any of them.
    let h = start_with(Some(Scope::Control)).await;
    let addr = h.base.trim_start_matches("http://");
    let head = format!(
        "GET /api/v1/ws HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://localhost:3000\r\n\
         Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    assert_eq!(raw(&h.base, &head).await, 401);
}

#[tokio::test]
async fn a_dns_rebound_host_gets_no_tokenless_access() {
    // Rebinding points an attacker's name at 127.0.0.1: the peer is
    // loopback, the Host header is not.
    let h = start_with(Some(Scope::Control)).await;
    let head = "GET /api/v1/snapshot HTTP/1.1\r\nHost: rebind.evil.example:7330\r\nConnection: close\r\n\r\n";
    assert_eq!(raw(&h.base, head).await, 401);
}

#[tokio::test]
async fn the_cli_shape_of_request_still_gets_the_bypass() {
    // No Origin, loopback Host in every spelling: this is what the CLI and
    // scripts send, and it must keep working.
    let h = start_with(Some(Scope::Read)).await;
    let port = h.base.rsplit(':').next().expect("port");
    for host in [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        "[::1]".to_owned(),
    ] {
        let head =
            format!("GET /api/v1/snapshot HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        assert_eq!(raw(&h.base, &head).await, 204, "Host: {host}");
    }
}

#[tokio::test]
async fn a_presented_token_still_decides_scope_on_loopback_when_the_bypass_is_off() {
    // The bypass must not have changed what a token means: the same requests
    // that succeed tokenlessly above are refused here, and a wrong token is
    // still refused identically to a missing one.
    let h = start_with(None).await;
    let (missing, missing_body) = request(&h.base, "GET", "/api/v1/snapshot", None, None).await;
    let (wrong, wrong_body) = request(&h.base, "GET", "/api/v1/snapshot", Some("nope"), None).await;
    assert_eq!(missing, 401);
    assert_eq!(wrong, 401);
    assert_eq!(missing_body, wrong_body);
}

#[tokio::test]
async fn a_non_loopback_peer_is_refused_even_when_the_bypass_is_on() {
    // The adversarial case. Connecting to the machine's own LAN address
    // gives a peer of that address, not 127.0.0.1, so this is a real
    // network-path request as the server sees it. Skipped, not faked, on a
    // machine with no LAN interface.
    let Some(interface) = vitals_server::interfaces().into_iter().next() else {
        eprintln!("no LAN interface on this machine; cannot exercise the non-loopback path");
        return;
    };

    let frames = FrameSource::new();
    let controller = Arc::new(RecordingController::default());
    let state = state_with(&frames, &controller, Some(Scope::Control));
    let mut handle = serve_on(state, std::net::SocketAddr::from((interface.address, 0)))
        .await
        .expect("bind the LAN address");
    let base = format!("http://{}", handle.addr);

    let (status, _) = request(&base, "GET", "/api/v1/snapshot", None, None).await;
    assert_eq!(
        status, 401,
        "a LAN peer must never inherit the loopback grant"
    );
    let control = r#"{"action":"suspend","key":{"pid":4242,"startTime":1}}"#;
    let (status, _) = request(&base, "POST", "/api/v1/control", None, Some(control)).await;
    assert_eq!(status, 401);
    assert_eq!(controller.calls.load(Ordering::SeqCst), 0);

    // And a real token still works over that path, so the bypass has not
    // replaced authentication, only supplemented it on loopback.
    let (status, _) = request(
        &base,
        "POST",
        "/api/v1/control",
        Some(CONTROL_TOKEN),
        Some(control),
    )
    .await;
    assert_eq!(status, 204);
    handle.stop();
}

// ── Short-code pairing ─────────────────────────────────────────────────────────────────

struct PairingHarness {
    base: String,
    desk: Arc<PairingDesk>,
    now: Arc<AtomicU64>,
    saves: Arc<AtomicUsize>,
    tokens: Arc<ServerLock<TokenSet>>,
    handle: vitals_server::ServeHandle,
}

impl Drop for PairingHarness {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

const PAIR_START_MS: u64 = 1_700_000_000_000;

async fn start_pairing() -> PairingHarness {
    let frames = FrameSource::new();
    let controller = Arc::new(RecordingController::default());
    let mut state = state_with(&frames, &controller, None);
    let now = Arc::new(AtomicU64::new(PAIR_START_MS));
    let saves = Arc::new(AtomicUsize::new(0));
    let desk = {
        let now = Arc::clone(&now);
        let saves = Arc::clone(&saves);
        Arc::new(PairingDesk::with_clock(
            Arc::new(move |_| {
                saves.fetch_add(1, Ordering::SeqCst);
            }),
            Arc::new(move || now.load(Ordering::SeqCst)),
        ))
    };
    state.pairing = Some(Arc::clone(&desk));
    let tokens = Arc::clone(&state.tokens);
    let handle = serve(state, 0).await.expect("bind");
    PairingHarness {
        base: format!("http://127.0.0.1:{}", handle.addr.port()),
        desk,
        now,
        saves,
        tokens,
        handle,
    }
}

async fn pair(base: &str, body: &str) -> (u16, String) {
    request(base, "POST", "/api/v1/pair", None, Some(body)).await
}

fn pair_body(code: &str) -> String {
    format!(r#"{{"code":"{code}","label":"Living room TV"}}"#)
}

/// A well-formed code that is certainly not `code`.
fn other_code(code: &str) -> String {
    let first = code.as_bytes()[0];
    let other = if first == b'9' {
        '0'
    } else {
        char::from(first + 1)
    };
    format!("{other}{}", &code[1..])
}

#[tokio::test]
async fn five_wrong_codes_over_the_network_burn_the_code_so_the_right_one_is_then_refused() {
    let h = start_pairing().await;
    let issued = h.desk.create(Scope::Read);
    for _ in 0..5 {
        let (status, _) = pair(&h.base, &pair_body(&other_code(&issued.code))).await;
        assert_eq!(status, 403);
    }
    let (status, body) = pair(&h.base, &pair_body(&issued.code)).await;
    assert_eq!(status, 403, "a burnt code must not pair: {body}");
    assert_eq!(h.saves.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_expired_code_is_refused_over_the_network() {
    let h = start_pairing().await;
    let issued = h.desk.create(Scope::Read);
    h.now.store(issued.expires_at_ms, Ordering::SeqCst);
    let (status, _) = pair(&h.base, &pair_body(&issued.code)).await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn a_code_pairs_exactly_once() {
    let h = start_pairing().await;
    let issued = h.desk.create(Scope::Read);
    let (first, _) = pair(&h.base, &pair_body(&issued.code)).await;
    let (second, _) = pair(&h.base, &pair_body(&issued.code)).await;
    assert_eq!(first, 200);
    assert_eq!(second, 403);
    assert_eq!(
        h.tokens.read().tokens.len(),
        3,
        "two fixtures plus one pairing"
    );
}

#[tokio::test]
async fn every_refused_pairing_gets_a_byte_identical_answer_whatever_the_cause() {
    // No code, expired, wrong, burnt: if any of these differed a guesser
    // could learn which state the desk is in.
    let h = start_pairing().await;
    let (s_none, none) = pair(&h.base, &pair_body("123456")).await;

    let issued = h.desk.create(Scope::Read);
    let (s_wrong, wrong) = pair(&h.base, &pair_body(&other_code(&issued.code))).await;
    for _ in 0..4 {
        let _ = pair(&h.base, &pair_body(&other_code(&issued.code))).await;
    }
    let (s_burnt, burnt) = pair(&h.base, &pair_body(&issued.code)).await;

    let issued = h.desk.create(Scope::Read);
    h.now.store(issued.expires_at_ms + 1, Ordering::SeqCst);
    let (s_expired, expired) = pair(&h.base, &pair_body(&issued.code)).await;

    for status in [s_none, s_wrong, s_burnt, s_expired] {
        assert_eq!(status, 403);
    }
    assert_eq!(none, r#"{"error":"invalid pairing code"}"#);
    assert_eq!(none, wrong);
    assert_eq!(none, burnt);
    assert_eq!(none, expired);
}

#[tokio::test]
async fn a_malformed_pairing_body_is_a_400_and_spends_no_attempt() {
    let h = start_pairing().await;
    let issued = h.desk.create(Scope::Read);
    // Ten malformed bodies, twice the attempt budget: if any of them counted,
    // the right code would be refused at the end.
    for body in [
        "not json",
        "{}",
        r#"{"label":"TV"}"#,
        r#"{"code":123456}"#,
        r#"{"code":"12345"}"#,
        r#"{"code":"1234567"}"#,
        r#"{"code":"12a456"}"#,
        r#"{"code":" 23456"}"#,
        r#"{"code":""}"#,
        "",
    ] {
        let (status, reply) = pair(&h.base, body).await;
        assert_eq!(status, 400, "{body:?} -> {reply}");
        assert!(reply.contains("\"error\""), "{reply}");
    }
    let (status, _) = pair(&h.base, &pair_body(&issued.code)).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn a_redeemed_token_reads_with_exactly_the_codes_scope_and_a_read_one_cannot_control() {
    let h = start_pairing().await;
    let issued = h.desk.create(Scope::Read);
    let (status, body) = pair(&h.base, &pair_body(&issued.code)).await;
    assert_eq!(status, 200, "{body}");
    let reply: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(reply["scope"], "read");
    let token = reply["token"].as_str().expect("token").to_owned();
    assert_eq!(token.len(), 43);
    assert_eq!(h.saves.load(Ordering::SeqCst), 1, "the set is persisted");
    let label = h.tokens.read().tokens.last().map(|t| t.label.clone());
    assert_eq!(label.as_deref(), Some("Living room TV"));

    let (status, _) = request(&h.base, "GET", "/api/v1/summary", Some(&token), None).await;
    assert_eq!(status, 204, "authenticated: nothing sampled yet, not 401");
    let control = r#"{"action":"suspend","key":{"pid":4242,"startTime":1}}"#;
    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(&token),
        Some(control),
    )
    .await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn a_control_code_redeems_for_a_token_that_reaches_the_controller() {
    let h = start_pairing().await;
    let issued = h.desk.create(Scope::Control);
    let (_, body) = pair(&h.base, &format!(r#"{{"code":"{}"}}"#, issued.code)).await;
    let reply: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(reply["scope"], "control");
    let token = reply["token"].as_str().expect("token").to_owned();
    let label = h.tokens.read().tokens.last().map(|t| t.label.clone());
    assert_eq!(
        label.as_deref(),
        Some("TV"),
        "a missing label defaults to TV"
    );

    let control = r#"{"action":"suspend","key":{"pid":4242,"startTime":1}}"#;
    let (status, _) = request(
        &h.base,
        "POST",
        "/api/v1/control",
        Some(&token),
        Some(control),
    )
    .await;
    assert_eq!(status, 204);
}

#[tokio::test]
async fn the_pair_route_answers_404_on_a_host_without_pairing_codes() {
    let h = start().await;
    let (status, body) = request(
        &h.base,
        "POST",
        "/api/v1/pair",
        None,
        Some(r#"{"code":"123456"}"#),
    )
    .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test]
async fn serve_on_binds_exactly_the_address_it_is_given() {
    let frames = FrameSource::new();
    let controller = Arc::new(RecordingController::default());
    let state = state_with(&frames, &controller, None);
    let mut handle = serve_on(state, "127.0.0.1:0".parse().unwrap())
        .await
        .expect("bind");
    assert!(handle.addr.ip().is_loopback(), "{}", handle.addr);
    assert_ne!(handle.addr.port(), 0);
    handle.stop();
}

#[tokio::test]
async fn a_summary_carries_the_busiest_processes_and_the_true_count_in_a_fraction_of_a_keyframe() {
    let h = start().await;
    let processes: Vec<_> = (0..200)
        .map(|i| fixtures::process(&format!("p{i}.exe"), 1000 + i, (i % 50) as f32))
        .collect();
    h.frames.publish(Arc::new(fixtures::keyframe(3, processes)));

    let (status, body) = request(
        &h.base,
        "GET",
        "/api/v1/summary?top=3",
        Some(READ_TOKEN),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let summary: vitals_core::remote::Summary = serde_json::from_str(&body).expect("summary json");
    assert_eq!(summary.process_count, 200);
    assert_eq!(summary.top.len(), 3);
    assert!(
        summary
            .top
            .iter()
            .all(|p| (p.cpu.get() - 49.0).abs() < f32::EPSILON),
        "{body}"
    );

    let (_, snapshot) = request(&h.base, "GET", "/api/v1/snapshot", Some(READ_TOKEN), None).await;
    assert!(
        body.len() * 20 < snapshot.len(),
        "summary {} vs keyframe {}",
        body.len(),
        snapshot.len()
    );
}

#[tokio::test]
async fn a_greedy_summary_is_clamped_rather_than_refused() {
    let h = start().await;
    let processes: Vec<_> = (0..40)
        .map(|i| fixtures::process("x.exe", 10 + i, 1.0))
        .collect();
    h.frames.publish(Arc::new(fixtures::keyframe(1, processes)));
    let (status, body) = request(
        &h.base,
        "GET",
        "/api/v1/summary?top=500",
        Some(READ_TOKEN),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let summary: vitals_core::remote::Summary = serde_json::from_str(&body).unwrap();
    assert_eq!(summary.top.len(), 25);
}

#[tokio::test]
async fn a_summary_before_the_first_sample_is_no_content_not_an_empty_machine() {
    let h = start().await;
    let (status, _) = request(&h.base, "GET", "/api/v1/summary", Some(READ_TOKEN), None).await;
    assert_eq!(status, 204);
}

#[tokio::test]
async fn history_passes_the_clamped_span_to_the_host() {
    let h = start().await;
    // The fixture provider echoes the span it was asked for as `ts`.
    let (status, body) = request(
        &h.base,
        "GET",
        "/api/v1/history?seconds=99999999",
        Some(READ_TOKEN),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let rows: Vec<vitals_core::MachineSample> = serde_json::from_str(&body).unwrap();
    assert_eq!(rows[0].ts, 7 * 24 * 3600);
    let (_, body) = request(
        &h.base,
        "GET",
        "/api/v1/history?seconds=1",
        Some(READ_TOKEN),
        None,
    )
    .await;
    let rows: Vec<vitals_core::MachineSample> = serde_json::from_str(&body).unwrap();
    assert_eq!(rows[0].ts, 60);
}

#[tokio::test]
async fn sensors_are_served_as_the_host_reads_them() {
    let h = start().await;
    let (status, body) = request(&h.base, "GET", "/api/v1/sensors", Some(READ_TOKEN), None).await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(r#""unit":"temperature""#), "{body}");
    assert!(body.contains(r#""key":"cpu.package""#), "{body}");
}

#[tokio::test]
async fn a_poll_counts_as_a_viewer_so_the_desktop_samples_live() {
    let h = start().await;
    h.frames.publish(Arc::new(frame(1)));
    assert!(!h.frames.has_viewers(), "nobody has asked yet");
    let _ = request(&h.base, "GET", "/api/v1/summary", Some(READ_TOKEN), None).await;
    assert!(h.frames.has_viewers());
}
