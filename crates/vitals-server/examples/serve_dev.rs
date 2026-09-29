//! Serves live metrics on a fixed port for developing the mobile page.
//!
//! `prove_lan` proves and exits; this one stays up. Token is `dev`, port is
//! 7332 (not the app's 7331, so both can run), and it advertises nothing.
//! Pair a browser with `http://localhost:5273/mobile.html#t=dev` after
//! pointing the pairing at `http://localhost:7332` — or simply open
//! `http://localhost:7332/mobile.html#t=dev` once `dist/` exists.
//!
//! It also prints a six-digit pairing code (`control` scope) at startup and
//! a fresh one every four and a half minutes, so a TV client can be paired
//! with `POST /api/v1/pair` without the desktop app.
//!
//! Run with: `cargo run -p vitals-server --example serve_dev`

#![allow(clippy::expect_used, clippy::print_stdout)]

use std::sync::Arc;
use std::time::Duration;

use vitals_server::state::ServerLock;
use vitals_server::{ApiState, FrameSource, Scope, Token, TokenSet};

#[tokio::main]
async fn main() {
    let frames = FrameSource::new();
    // `dev` is convenient for curl, but the Android app validates the shape
    // of a real token (43 base64url characters) before it will pair, so a
    // native client needs a real-looking one: set VITALS_DEV_TOKEN.
    let secret = std::env::var("VITALS_DEV_TOKEN").unwrap_or_else(|_| "dev".into());
    let desk = Arc::new(vitals_server::PairingDesk::new(Arc::new(|_| {})));

    let state = ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(TokenSet {
            tokens: vec![Token::new(&secret, Scope::Control, "dev", 0)],
        })),
        controller: Arc::new(vitals_server::control::NoControl),
        assets: Some(Arc::new(|path: &str| {
            // Serve a built dist/ if there is one, so the whole flow can be
            // tried without Tauri. Missing dist just means 404 for assets.
            let file = std::path::Path::new("apps/desktop/dist").join(path);
            std::fs::read(&file)
                .ok()
                .map(|bytes| (bytes, vitals_server::router::mime_for(path)))
        })),
        host: Arc::new(|| None),
        alerts: Arc::new(Vec::new),
        history: Arc::new(|_| Vec::new()),
        // Real readings on Windows so the phone and watch sensor screens can
        // be developed against this machine rather than a fixture.
        sensors: Arc::new(|| {
            #[cfg(windows)]
            {
                vitals_win::sensors::read_all()
                    .readings
                    .iter()
                    .map(vitals_win::sensors::SensorReading::to_line)
                    .collect()
            }
            #[cfg(not(windows))]
            {
                Vec::new()
            }
        }),
        version: "dev".into(),
        loopback_scope: None,
        // Nothing persists: a TV paired against this example is forgotten
        // when it exits, which is what a development server should do.
        pairing: Some(Arc::clone(&desk)),
    };

    let handle = vitals_server::serve(state, 7332).await.expect("bind 7332");
    println!(
        "dev server on http://127.0.0.1:{}  token={}...",
        handle.addr.port(),
        secret.chars().take(8).collect::<String>()
    );

    // A fresh code every four and a half minutes, so there is always a live
    // one on screen for a TV client under development — codes last five.
    tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_secs(270));
        loop {
            every.tick().await;
            let issued = desk.create(Scope::Control);
            println!(
                "pairing code (control, POST /api/v1/pair): {} {}",
                &issued.code[..3],
                &issued.code[3..]
            );
        }
    });

    #[cfg(windows)]
    {
        std::thread::spawn(move || {
            let mut sampler = vitals_win::SystemSampler::new();
            let mut builder = vitals_win::FrameBuilder::new();
            loop {
                if let Ok(sample) = sampler.sample() {
                    frames.publish(Arc::new(builder.build(sample)));
                }
                std::thread::sleep(Duration::from_millis(1000));
            }
        });
    }
    #[cfg(not(windows))]
    let _ = frames;

    // CORS is deliberately absent from the server: on a phone the page is
    // served from the same origin. For the dev page on :5273 the browser will
    // block cross-origin fetches — so open the built page from :7332 instead.
    tokio::signal::ctrl_c().await.expect("ctrl-c");
}
