//! Proves the MCP endpoint against a real machine over a real socket.
//!
//! Same reasoning as `prove_lan`: the integration tests use a fixture, and a
//! fixture cannot show that a sampled machine survives JSON-RPC, the auth
//! layer and a client's parser. This one speaks the protocol an agent speaks
//! — initialize, tools/list, tools/call — and prints exactly what came back,
//! including the two refusals that matter: no token, and a tool that would
//! act.
//!
//! The host grants the tokenless loopback read scope on purpose, so the
//! refusal on show is `/mcp`'s own rather than the auth layer's. That is the
//! rule the route exists for.
//!
//! Run with: `cargo run -p vitals-server --example prove_mcp`

#![allow(clippy::expect_used, clippy::print_stdout, clippy::unwrap_used)]

use std::sync::Arc;
#[cfg(any(windows, target_os = "macos"))]
use std::time::Duration;

use vitals_server::state::ServerLock;
use vitals_server::{ApiState, FrameSource, Scope, Token, TokenSet};

#[tokio::main]
async fn main() {
    let frames = FrameSource::new();
    let secret = vitals_server::auth::generate_secret();

    let handle = vitals_server::serve(api_state(&frames, &secret), 0)
        .await
        .expect("bind");
    let addr = format!("127.0.0.1:{}", handle.addr.port());
    println!("listening on {addr}");

    feed_real_frames(&frames).await;

    prove_handshake(&addr, &secret).await;

    println!("\n--- tools/call summary, top 5, on this machine ---");
    let summary = call(
        &addr,
        &secret,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"summary","arguments":{"top":5}}}"#,
    )
    .await;
    summarise_summary(&summary);

    println!("\n--- tools/call processes, top 3 ---");
    let processes = call(
        &addr,
        &secret,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"processes","arguments":{"top":3}}}"#,
    )
    .await;
    summarise_processes(&processes);

    println!("\n--- tools/call diagnosis ---");
    let diagnosis = call(
        &addr,
        &secret,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"diagnosis"}}"#,
    )
    .await;
    println!("{}", trimmed(&diagnosis, 500));

    println!("\n--- tools/call sensors and history (the prover wires neither) ---");
    for (id, tool) in [(6, "sensors"), (7, "history")] {
        let reply = call(
            &addr,
            &secret,
            &format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"{tool}"}}}}"#
            ),
        )
        .await;
        println!("  {tool}: {}", trimmed(&reply, 220));
    }

    println!("\n--- resources/list, then resources/read ---");
    let resources = call(
        &addr,
        &secret,
        r#"{"jsonrpc":"2.0","id":8,"method":"resources/list"}"#,
    )
    .await;
    println!("  {}", trimmed(&resources, 400));
    let uri = serde_json::from_str::<serde_json::Value>(&resources)
        .ok()
        .and_then(|json| {
            json["result"]["resources"][0]["uri"]
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let read = call(
        &addr,
        &secret,
        &format!(
            r#"{{"jsonrpc":"2.0","id":9,"method":"resources/read","params":{{"uri":"{uri}"}}}}"#
        ),
    )
    .await;
    println!("  {}", trimmed(&read, 400));

    println!("\n--- tools/call end_process (the guarantee: there is no such tool) ---");
    for name in ["end_process", "suspend", "set_priority"] {
        let reply = call(
            &addr,
            &secret,
            &format!(
                r#"{{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{{"name":"{name}","arguments":{{"pid":1}}}}}}"#
            ),
        )
        .await;
        println!("  {name}: {}", trimmed(&reply, 200));
    }

    println!(
        "\n--- tools/call summary with a token in the read scope, then a resource we do not serve ---"
    );
    let missing = call(
        &addr,
        &secret,
        r#"{"jsonrpc":"2.0","id":11,"method":"resources/read","params":{"uri":"vitals://some-other-pc"}}"#,
    )
    .await;
    println!("  {}", trimmed(&missing, 220));
}

/// The state a host would hand the server, with the loopback read grant on so
/// the refusals printed below are the route's own rather than the auth
/// layer's.
fn api_state(frames: &FrameSource, secret: &str) -> ApiState {
    ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(TokenSet {
            tokens: vec![Token::new(secret, Scope::Read, "prover", 0)],
        })),
        controller: Arc::new(vitals_server::control::NoControl),
        assets: None,
        host: Arc::new(host_info),
        alerts: Arc::new(Vec::new),
        // The prover wires no store and no sensor service. An empty list is
        // the honest answer for that ("none measurable here"), which is what
        // the sensors and history calls below print — the desktop serves real
        // ones through the same tools.
        history: Arc::new(|_| Vec::new()),
        sensors: Arc::new(Vec::new),
        connectivity: Arc::new(|| {
            vitals_core::connectivity::ConnectivityReport::from_parts(None, Vec::new(), None)
        }),
        version: "prove".into(),
        loopback_scope: Some(Scope::Read),
        pairing: None,
        agents: None,
    }
}

// Both variants return `Option` so one host closure compiles on every
// platform. On Windows the value is always present, which is what the lint
// objects to, and why the allow is here rather than the type being changed.
#[allow(clippy::unnecessary_wraps)]
#[cfg(windows)]
fn host_info() -> Option<vitals_core::provider::HostInfo> {
    Some(vitals_win::hostinfo::read())
}

#[allow(clippy::unnecessary_wraps)]
#[cfg(not(windows))]
fn host_info() -> Option<vitals_core::provider::HostInfo> {
    None
}

/// The two refusals, then the handshake and the tool list: exactly what an
/// agent does when it connects.
async fn prove_handshake(addr: &str, secret: &str) {
    println!("\n--- POST /mcp with no token (the loopback grant applies) ---");
    let anon = post(
        addr,
        None,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    )
    .await;
    print_status_and_body(&anon);

    println!("\n--- POST /mcp with a wrong token ---");
    let wrong = post(
        addr,
        Some("not-a-real-token"),
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
    )
    .await;
    print_status_and_body(&wrong);

    println!("\n--- initialize ---");
    let reply = call(
        addr,
        secret,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"prove_mcp"}}}"#,
    )
    .await;
    println!("{}", trimmed(&reply, 700));

    println!("\n--- notifications/initialized (a notification is accepted, not answered) ---");
    let ack = post(
        addr,
        Some(secret),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    )
    .await;
    println!("  status: {}", first_line(&ack));
    println!(
        "  body:   {:?}",
        ack.split_once("\r\n\r\n").map_or("", |(_, b)| b)
    );

    println!("\n--- tools/list ---");
    let listed = call(
        addr,
        secret,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    )
    .await;
    let tools: serde_json::Value = serde_json::from_str(&listed).expect("a JSON reply");
    for tool in tools["result"]["tools"].as_array().expect("tools") {
        let properties = tool["inputSchema"]["properties"]
            .as_object()
            .map(|properties| {
                let keys: Vec<&str> = properties.keys().map(String::as_str).collect();
                if keys.is_empty() {
                    "none".to_owned()
                } else {
                    keys.join(", ")
                }
            })
            .unwrap_or_default();
        println!("  {}  (schema keys: {properties})", tool["name"]);
    }
}

/// Feeds the server real frames from this machine, as the sampler would.
async fn feed_real_frames(frames: &FrameSource) {
    #[cfg(windows)]
    let payback = {
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
        })
    };
    #[cfg(target_os = "macos")]
    let payback = {
        let published = frames.clone();
        std::thread::spawn(move || {
            let mut sampler = vitals_macos::SystemSampler::new();
            let mut builder = vitals_macos::FrameBuilder::new();
            for _ in 0..5 {
                if let Ok(sample) = sampler.sample() {
                    published.publish(Arc::new(builder.build(sample)));
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        })
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let payback = {
        let _ = frames;
        std::thread::spawn(|| {})
    };

    tokio::time::sleep(Duration::from_millis(2500)).await;
    let _ = payback.join();
}

fn summarise_summary(response: &str) {
    let json: serde_json::Value = match serde_json::from_str(response) {
        Ok(json) => json,
        Err(error) => {
            println!("  the reply did not parse: {error}");
            return;
        }
    };
    println!("  isError: {}", json["result"]["isError"]);
    let payload = &json["result"]["structuredContent"];
    println!(
        "  seq={} processCount={} returned={}",
        payload["seq"],
        payload["processCount"],
        payload["top"].as_array().map_or(0, Vec::len)
    );
    println!(
        "  cpu={}%  memory={} of {} bytes  cpuTemp={}",
        payload["system"]["cpu"]["total"],
        payload["system"]["memory"]["used"],
        payload["system"]["memory"]["total"],
        payload["system"]["cpu"]["temperature"]
    );
    for process in payload["top"].as_array().into_iter().flatten() {
        println!(
            "  {} pid={} cpu={}% privateMemory={}",
            process["name"], process["key"]["pid"], process["cpu"], process["memoryPrivate"]
        );
    }
    println!(
        "  text content is {} bytes, structuredContent is {} bytes",
        json["result"]["content"][0]["text"]
            .as_str()
            .map_or(0, str::len),
        payload.to_string().len()
    );
}

fn summarise_processes(response: &str) {
    let json: serde_json::Value = match serde_json::from_str(response) {
        Ok(json) => json,
        Err(error) => {
            println!("  the reply did not parse: {error}");
            return;
        }
    };
    let payload = &json["result"]["structuredContent"];
    println!(
        "  returned={} count={} (the count is every process on the machine)",
        payload["returned"], payload["count"]
    );
    for process in payload["processes"].as_array().into_iter().flatten() {
        println!("  {} pid={}", process["name"], process["key"]["pid"]);
    }
}

/// Prints the body of a `Result`-style reply, trimmed: the payloads are real
/// process lists and would otherwise fill a screen.
fn print_status_and_body(response: &str) {
    println!("  status: {}", first_line(response));
    let body = response.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    println!("  body:   {}", trimmed(body, 300));
}

fn first_line(response: &str) -> &str {
    response.lines().next().unwrap_or("")
}

fn trimmed(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    format!("{}… (+{} bytes)", &text[..max], text.len() - max)
}

/// One MCP message with a token.
async fn call(addr: &str, token: &str, body: &str) -> String {
    let response = post(addr, Some(token), body).await;
    response
        .split_once("\r\n\r\n")
        .map_or_else(|| response.clone(), |(_, body)| body.to_owned())
}

/// `POST /mcp`, as an agent's HTTP client would.
async fn post(addr: &str, token: Option<&str>, body: &str) -> String {
    use std::fmt::Write as _;

    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

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
    String::from_utf8_lossy(&raw).into_owned()
}
