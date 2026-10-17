//! `POST /api/v1/pair` against callers a browser can produce on loopback.
//!
//! The route needs no token, and the desktop's loopback listener is reachable
//! from every page the user has open. These tests send what such a page
//! would — an `Origin`, or a rebound `Host` — over a real socket, and assert
//! both that it is refused and that the refusal spent none of the code's five
//! attempts. Without the second half, a page could still burn the code on the
//! TV's screen.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use vitals_server::state::ServerLock;
use vitals_server::{ApiState, FrameSource, PairingDesk, Scope, TokenSet, serve_on};

struct Harness {
    addr: String,
    desk: Arc<PairingDesk>,
    handle: vitals_server::ServeHandle,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.handle.stop();
    }
}

async fn start() -> Harness {
    let desk = Arc::new(PairingDesk::new(Arc::new(|_: &TokenSet| {})));
    let state = ApiState {
        frames: FrameSource::new(),
        tokens: Arc::new(ServerLock::new(TokenSet::default())),
        controller: Arc::new(vitals_server::control::NoControl),
        assets: None,
        host: Arc::new(|| None),
        alerts: Arc::new(Vec::new),
        history: Arc::new(|_| Vec::new()),
        sensors: Arc::new(Vec::new),
        version: "test".into(),
        // The desktop's loopback listener grants read without a token; the
        // guard must not depend on that setting.
        loopback_scope: Some(Scope::Read),
        pairing: Some(Arc::clone(&desk)),
        agents: None,
    };
    let handle = serve_on(state, "127.0.0.1:0".parse().unwrap())
        .await
        .expect("bind");
    Harness {
        addr: handle.addr.to_string(),
        desk,
        handle,
    }
}

/// POSTs `body` to `/api/v1/pair` with `Host: host` and the extra header
/// lines in `extra`, returning the status and body.
async fn pair(addr: &str, host: &str, extra: &str, body: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let head = format!(
        "POST /api/v1/pair HTTP/1.1\r\nHost: {host}\r\n{extra}Content-Type: text/plain\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
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

fn wrong(code: &str) -> String {
    let first = code.as_bytes()[0];
    let other = if first == b'9' {
        '0'
    } else {
        char::from(first + 1)
    };
    format!(r#"{{"code":"{other}{}"}}"#, &code[1..])
}

#[tokio::test]
async fn a_page_on_another_origin_cannot_spend_the_codes_attempts_through_loopback() {
    let h = start().await;
    let issued = h.desk.create(Scope::Control);
    // Twice the attempt budget, from a page. If any of these counted, the
    // real code would be burnt by the end.
    for _ in 0..10 {
        let (status, body) = pair(
            &h.addr,
            &h.addr,
            "Origin: https://evil.example\r\n",
            &wrong(&issued.code),
        )
        .await;
        assert_eq!(status, 403, "{body}");
        assert_eq!(body, r#"{"error":"invalid pairing code"}"#);
    }
    let (status, body) = pair(
        &h.addr,
        &h.addr,
        "",
        &format!(r#"{{"code":"{}"}}"#, issued.code),
    )
    .await;
    assert_eq!(
        status, 200,
        "the code must survive a page's guesses: {body}"
    );
}

#[tokio::test]
async fn a_page_cannot_redeem_a_correct_code_through_loopback() {
    // Guessing is only half of it: a page that knew the code (shoulder
    // surfing, a screenshot) must not walk away with a control token.
    let h = start().await;
    let issued = h.desk.create(Scope::Control);
    let body = format!(r#"{{"code":"{}"}}"#, issued.code);
    let (status, reply) = pair(&h.addr, &h.addr, "Origin: null\r\n", &body).await;
    assert_eq!(status, 403, "{reply}");
    assert!(!reply.contains("token"), "{reply}");
}

#[tokio::test]
async fn a_dns_rebound_host_cannot_pair_through_loopback() {
    let h = start().await;
    let issued = h.desk.create(Scope::Read);
    let body = format!(r#"{{"code":"{}"}}"#, issued.code);
    let (status, _) = pair(&h.addr, "rebind.evil.example:7330", "", &body).await;
    assert_eq!(status, 403);
    // And it was refused before the guess: the code is still live.
    assert!(h.desk.status().active);
}

#[tokio::test]
async fn a_refused_page_learns_nothing_about_the_body_shape() {
    // A malformed body from a local program is a 400; from a page it must be
    // the same 403 as everything else, or the page can probe the schema.
    let h = start().await;
    let _ = h.desk.create(Scope::Read);
    let (status, body) = pair(&h.addr, &h.addr, "Origin: https://evil.example\r\n", "{}").await;
    assert_eq!(status, 403);
    assert_eq!(body, r#"{"error":"invalid pairing code"}"#);
}
