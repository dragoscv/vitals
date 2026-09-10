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

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vitals_core::fixtures;
use vitals_core::sample::Frame;
use vitals_server::state::ServerLock;
use vitals_server::{
    ApiState, ControlError, ControlRequest, Controller, FrameSource, Scope, Token, TokenSet, serve,
};

const READ_TOKEN: &str = "read-token-value";
const CONTROL_TOKEN: &str = "control-token-value";

#[derive(Debug, Default)]
struct RecordingController {
    calls: AtomicUsize,
}

impl Controller for RecordingController {
    fn apply(&self, _request: ControlRequest) -> Result<(), ControlError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn frame(seq: u64) -> Frame {
    fixtures::keyframe(seq, vec![fixtures::process("chrome.exe", 4242, 12.0)])
}

struct Harness {
    base: String,
    frames: FrameSource,
    controller: Arc<RecordingController>,
    handle: vitals_server::ServeHandle,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

async fn start() -> Harness {
    let frames = FrameSource::new();
    let controller = Arc::new(RecordingController::default());
    let tokens = TokenSet {
        tokens: vec![
            Token {
                secret: READ_TOKEN.into(),
                scope: Scope::Read,
                label: "phone".into(),
                created: 0,
            },
            Token {
                secret: CONTROL_TOKEN.into(),
                scope: Scope::Control,
                label: "trusted".into(),
                created: 0,
            },
        ],
    };

    let state = ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(tokens)),
        controller: controller.clone(),
        assets: Some(Arc::new(|path: &str| {
            (path == "mobile.html")
                .then(|| (b"<!doctype html>ok".to_vec(), "text/html; charset=utf-8"))
        })),
        host: Arc::new(|| None),
        version: "0.0.0-test".into(),
    };

    // Port 0: the OS picks a free one, so tests never collide with a real
    // server or with each other.
    let handle = serve(state, 0).await.expect("bind");
    let base = format!("http://127.0.0.1:{}", handle.addr.port());
    Harness {
        base,
        frames,
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
