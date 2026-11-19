//! `vitals serve`, asked for MCP: the headless box as an agent endpoint.
//!
//! The route lives in the shared router, so a headless server has always
//! served it — but "always served it" is exactly the kind of claim this repo
//! refuses to make without a request. This starts the real process, on a real
//! port, and speaks the protocol to it, including the refusal a caller with no
//! token gets.
//!
//! Windows-only, like every other test that needs the CLI's own sampler: on
//! any other platform `vitals serve` exits before it binds, because there is
//! nothing to sample.

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

const TOKEN: &str = "serve-mcp-test-token";

/// A port to serve on, distinct per server in this process.
///
/// Servers are started by tests that run concurrently, so a fixed port would
/// make them collide with each other; the counter and the pid together keep
/// two `cargo test` runs out of the same range.
fn next_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let offset = NEXT.fetch_add(1, Ordering::SeqCst);
    17_300 + (std::process::id() % 100) as u16 * 3 + offset % 3
}

/// A running headless server. Dropping it stops the process.
struct Server {
    child: Child,
    addr: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    // The child is reaped by the `Drop` impl below, which clippy's analysis of
    // this loop cannot see; without the allow it demands a wait() on the path
    // that returns a live server, which is the whole point of the type.
    #[allow(clippy::zombie_processes)]
    fn start() -> Self {
        let port = next_port();
        let child = Command::new(env!("CARGO_BIN_EXE_vitals"))
            .args([
                "serve",
                "--port",
                &port.to_string(),
                "--token",
                TOKEN,
            ])
            // Both streams are dropped: the banner names a token and the test
            // has no business printing it into a log.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn vitals serve");

        let addr = format!("127.0.0.1:{port}");
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Ok(mut stream) = TcpStream::connect(&addr) {
                let _ = stream.write_all(
                    b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                );
                let mut response = String::new();
                let _ = stream.read_to_string(&mut response);
                if response.starts_with("HTTP/1.1 200") {
                    return Self { child, addr };
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        panic!("vitals serve never answered /api/v1/health on {addr}");
    }

    /// One `POST /mcp`, as an agent's HTTP client makes it.
    fn post(&self, token: Option<&str>, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(&self.addr).expect("connect");
        let mut request = format!(
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n",
            self.addr,
            body.len()
        );
        if let Some(token) = token {
            write!(request, "Authorization: Bearer {token}\r\n")
                .expect("writing to a String cannot fail");
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream.write_all(request.as_bytes()).expect("write");

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).expect("read");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        let body = text
            .split_once("\r\n\r\n")
            .map_or(String::new(), |(_, body)| body.to_owned());
        (status, body)
    }

    fn call(&self, body: &str) -> serde_json::Value {
        let (status, body) = self.post(Some(TOKEN), body);
        assert_eq!(status, 200, "the call was refused: {body}");
        serde_json::from_str(&body).expect("one JSON message")
    }
}

#[test]
fn the_headless_server_refuses_a_tokenless_mcp_call() {
    let server = Server::start();
    let (status, body) = server.post(None, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    assert_eq!(
        status, 401,
        "the headless server has no loopback grant: {body}"
    );
    assert!(
        body.contains("a bearer token is required"),
        "the refusal must be the shared one, not a new one: {body}"
    );
}

#[test]
fn the_headless_server_answers_the_handshake_and_a_tool_call_with_its_token() {
    let server = Server::start();

    let initialized = server.call(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
    );
    assert_eq!(initialized["result"]["serverInfo"]["name"], "vitals");

    let listed = server.call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    assert_eq!(
        listed["result"]["tools"].as_array().expect("tools").len(),
        6
    );

    // The sampler publishes from its own thread, so the first summary may
    // arrive before the first keyframe. Waiting for it is the difference
    // between a test of the route and a test of a race.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let reply = server.call(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"summary","arguments":{"top":3}}}"#,
        );
        if reply["result"]["isError"] == false {
            let payload = &reply["result"]["structuredContent"];
            assert!(
                payload["processCount"].as_u64().unwrap_or(0) >= 1,
                "a machine running this test has processes: {payload}"
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the headless server never produced a summary: {reply}"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}
