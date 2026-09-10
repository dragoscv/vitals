//! Proves the LAN API serves real sampled data over a real socket.
//!
//! Same reasoning as `vitals-store`'s prover: the integration tests use a
//! fixture, and a fixture cannot show that the sampler's actual output
//! survives serialisation, the HTTP layer and a client's parser. This runs
//! the whole path and prints what a phone would receive.
//!
//! Run with: `cargo run -p vitals-server --example prove_lan`

#![allow(clippy::expect_used, clippy::print_stdout, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use vitals_server::state::ServerLock;
use vitals_server::{ApiState, FrameSource, Scope, Token, TokenSet};

#[tokio::main]
async fn main() {
    let frames = FrameSource::new();
    let secret = vitals_server::auth::generate_secret();

    let state = ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(TokenSet {
            tokens: vec![Token {
                secret: secret.clone(),
                scope: Scope::Read,
                label: "prover".into(),
                created: 0,
            }],
        })),
        controller: Arc::new(vitals_server::control::NoControl),
        assets: None,
        host: Arc::new(|| None),
        version: "prove".into(),
    };

    let handle = vitals_server::serve(state, 0).await.expect("bind");
    let port = handle.addr.port();
    println!("listening on 127.0.0.1:{port}");

    // Feed it real frames on a background thread, as the sampler would.
    #[cfg(windows)]
    {
        let published = frames.clone();
        std::thread::spawn(move || {
            let mut sampler = vitals_win::SystemSampler::new();
            let mut builder = vitals_win::FrameBuilder::new();
            for _ in 0..5 {
                if let Ok(sample) = sampler.sample() {
                    published.publish(Arc::new(builder.build(sample)));
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        });
        tokio::time::sleep(Duration::from_millis(2500)).await;
    }
    #[cfg(not(windows))]
    let _ = &frames;

    let base = format!("127.0.0.1:{port}");

    println!("\n--- GET /api/v1/health (no token) ---");
    println!("{}", get(&base, "/api/v1/health", None).await);

    println!("\n--- GET /api/v1/snapshot (no token) ---");
    let anon = get(&base, "/api/v1/snapshot", None).await;
    println!("{}", first_line(&anon));

    println!("\n--- GET /api/v1/snapshot (with token) ---");
    let body = get(&base, "/api/v1/snapshot", Some(&secret)).await;
    summarise_snapshot(&body);

    println!("\n--- GET /metrics (with token) ---");
    let metrics = get(&base, "/metrics", Some(&secret)).await;
    for line in metrics.lines().filter(|l| l.starts_with("vitals_")).take(8) {
        println!("  {line}");
    }
    println!(
        "  ... {} series total",
        metrics.lines().filter(|l| l.starts_with("vitals_")).count()
    );
}

fn first_line(response: &str) -> &str {
    response.lines().next().unwrap_or("")
}

fn summarise_snapshot(response: &str) {
    let Some((_, body)) = response.split_once("\r\n\r\n") else {
        println!("  (no body)\n{response}");
        return;
    };
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(json) => {
            let payload = &json["payload"];
            println!("  status: {}", first_line(response));
            println!("  seq={} elapsedMs={}", json["seq"], json["elapsedMs"]);
            println!("  payload.kind={}", payload["kind"]);
            println!("  cpu.total={}", payload["system"]["cpu"]["total"]);
            println!("  memory.used={}", payload["system"]["memory"]["used"]);
            let procs = payload["processes"].as_array().map_or(0, Vec::len);
            println!("  processes={procs}");
            if let Some(first) = payload["processes"].get(0) {
                // The camelCase contract, at depth, on a real frame.
                println!(
                    "  first process: name={} key={}",
                    first["name"], first["key"]
                );
            }
        }
        Err(error) => println!(
            "  body did not parse: {error}\n  {}",
            &body[..body.len().min(200)]
        ),
    }
}

async fn get(addr: &str, path: &str, token: Option<&str>) -> String {
    use std::fmt::Write as _;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let mut head = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(token) = token {
        let _ = write!(head, "Authorization: Bearer {token}\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await.expect("write");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    String::from_utf8_lossy(&raw).into_owned()
}
