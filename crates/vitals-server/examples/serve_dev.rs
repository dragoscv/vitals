//! Serves live metrics on a fixed port for developing the mobile page.
//!
//! `prove_lan` proves and exits; this one stays up. Token is `dev`, port is
//! 7332 (not the app's 7331, so both can run), and it advertises nothing.
//! Pair a browser with `http://localhost:5273/mobile.html#t=dev` after
//! pointing the pairing at `http://localhost:7332` — or simply open
//! `http://localhost:7332/mobile.html#t=dev` once `dist/` exists.
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

    let state = ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(TokenSet {
            tokens: vec![Token {
                secret: "dev".into(),
                scope: Scope::Control,
                label: "dev".into(),
                created: 0,
            }],
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
        version: "dev".into(),
    };

    let handle = vitals_server::serve(state, 7332).await.expect("bind 7332");
    println!(
        "dev server on http://127.0.0.1:{}  token=dev",
        handle.addr.port()
    );

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
