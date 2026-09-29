//! `vitals serve` — the LAN server without the desktop.
//!
//! For a headless box, a server, or a machine where the tray app is not
//! wanted. Same crate, same routes, same token rules as the desktop's
//! Remote access; the only differences are that the token comes from the
//! command line (or is generated and printed once) and there is no mobile
//! page to serve — a phone pairs against the desktop build's `mobile.html`
//! or scrapes `/metrics`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use vitals_core::alerts::Alert;
use vitals_server::state::ServerLock;
use vitals_server::{ApiState, FrameSource, PairingDesk, Scope, Token, TokenSet};

use crate::source::Direct;

/// What the caller asked for.
#[derive(Debug, Clone)]
pub struct Options {
    pub port: u16,
    /// `None` generates one.
    pub token: Option<String>,
    /// Grant `control` scope to the token. Off by default: a token in a
    /// shell history is a remote kill switch for the machine.
    pub control: bool,
}

/// Builds the token set, and returns the secret beside it: the set keeps only
/// a hash, so this is the one place the caller can still read it to print.
/// The second value is `Some` only when the secret was generated.
#[must_use]
pub fn tokens(options: &Options) -> (TokenSet, String, Option<String>) {
    let generated = options
        .token
        .is_none()
        .then(vitals_server::auth::generate_secret);
    let secret = options
        .token
        .clone()
        .or_else(|| generated.clone())
        .unwrap_or_default();
    let set = TokenSet {
        tokens: vec![Token::new(
            &secret,
            scope(options),
            "vitals serve",
            unix_now(),
        )],
    };
    (set, secret, generated)
}

/// The scope both the token and the TV pairing code grant: one switch, so a
/// TV can never end up with more than the command line asked for.
fn scope(options: &Options) -> Scope {
    if options.control {
        Scope::Control
    } else {
        Scope::Read
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Runs until Ctrl-C.
///
/// # Errors
/// No sampler on this platform, or the port could not be bound.
pub fn run(options: &Options) -> Result<()> {
    // Fail before binding a port if this platform cannot sample at all. The
    // real sampler is built on its own thread below: `SystemSampler` holds a
    // PDH query with raw pointers and is not `Send`.
    Direct::new()?;
    let (token_set, secret, generated) = tokens(options);

    let frames = FrameSource::new();
    let alerts: Arc<ServerLock<Vec<Alert>>> = Arc::new(ServerLock::new(Vec::new()));
    // A no-op persist: the headless server keeps no token file (its own
    // token comes from the command line), so a TV paired here is forgotten
    // on exit, exactly like the generated token.
    let desk = Arc::new(PairingDesk::new(Arc::new(|_| {})));
    let code = desk.create(scope(options));

    let state = ApiState {
        frames: frames.clone(),
        tokens: Arc::new(ServerLock::new(token_set)),
        // Control is deliberately not wired: the CLI has no UI to confirm
        // ending a critical process, and a headless controller answering a
        // LAN request is exactly the surface the desktop keeps behind a
        // Settings toggle. `--control` scopes the token; acting still needs
        // the desktop.
        controller: Arc::new(vitals_server::control::NoControl),
        assets: None,
        host: Arc::new(host_info),
        alerts: {
            let alerts = Arc::clone(&alerts);
            Arc::new(move || alerts.read().clone())
        },
        // The headless server keeps no store and reads no sensors; both
        // answer with an empty list, which clients render as "not recorded".
        history: Arc::new(|_| Vec::new()),
        sensors: Arc::new(Vec::new),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        loopback_scope: None,
        pairing: Some(desk),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;

    runtime.block_on(async move {
        let handle = vitals_server::serve(state, options.port)
            .await
            .with_context(|| format!("binding 0.0.0.0:{}", options.port))?;
        let port = handle.addr.port();

        announce(port, &secret, generated.as_deref(), options.control);
        // stderr, like the token above, never `tracing`: a code is a
        // credential for five minutes and must not reach a log file.
        eprintln!(
            "TV pairing code (valid 5 minutes, single use): {} {}",
            &code.code[..3],
            &code.code[3..]
        );

        std::thread::spawn(move || sample_forever(&frames, &alerts));

        tokio::signal::ctrl_c()
            .await
            .context("waiting for Ctrl-C")?;
        eprintln!("stopping");
        let mut handle = handle;
        handle.stop();
        Ok(())
    })
}

/// The sampling thread. Constructs the sampler here because it cannot be
/// moved in, then publishes one frame a second until the process exits.
fn sample_forever(frames: &FrameSource, alerts: &ServerLock<Vec<Alert>>) {
    let Ok(mut direct) = Direct::new() else {
        // Checked in `run` before the port was bound; only a platform with
        // no sampler reaches here, and it already failed there.
        return;
    };
    loop {
        match direct.tick() {
            Ok(frame) => {
                *alerts.write() = direct_alerts(&direct);
                frames.publish(Arc::new(frame));
            }
            Err(error) => eprintln!("sampling failed: {error}"),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn announce(port: u16, secret: &str, generated: Option<&str>, control: bool) {
    let scope = if control { "control" } else { "read" };
    eprintln!("serving on 0.0.0.0:{port}  scope: {scope}");
    if control {
        // The controller is `NoControl` (see `run`): every control request
        // answers 501. Saying "scope: control" alone read as a promise.
        eprintln!(
            "note: this headless server cannot act on processes; control requests answer 501. \
             Pair the phone with the desktop app for that."
        );
    }
    if let Some(secret) = generated {
        eprintln!("generated token (shown once, not stored): {secret}");
        eprintln!("pass --token next time to keep the same pairing");
    }
    match vitals_server::interfaces().first() {
        Some(iface) => {
            eprintln!(
                "pairing URL for {}: {}",
                iface.name,
                vitals_server::pairing_url(iface.address, port, secret)
            );
            eprintln!(
                "Prometheus: curl -H 'Authorization: Bearer <token>' http://{}:{port}/metrics",
                iface.address
            );
        }
        None => eprintln!("no LAN interface found; reachable on this machine only"),
    }
}

#[allow(clippy::unnecessary_wraps)]
fn host_info() -> Option<vitals_core::provider::HostInfo> {
    #[cfg(windows)]
    {
        Some(vitals_win::hostinfo::read())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn direct_alerts(direct: &Direct) -> Vec<Alert> {
    direct.active_alerts()
}

#[cfg(not(windows))]
fn direct_alerts(_direct: &Direct) -> Vec<Alert> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_given_token_is_used_verbatim_and_nothing_is_generated() {
        let (set, secret, generated) = tokens(&Options {
            port: 0,
            token: Some("abc".into()),
            control: false,
        });
        assert_eq!(generated, None);
        assert_eq!(secret, "abc");
        assert_eq!(set.scope_for("abc"), Some(Scope::Read));
    }

    #[test]
    fn without_a_token_one_is_generated_and_control_only_when_asked() {
        let (set, _, generated) = tokens(&Options {
            port: 0,
            token: None,
            control: true,
        });
        let secret = generated.expect("generated");
        assert!(secret.len() >= 40, "256 bits base64url");
        assert_eq!(set.scope_for(&secret), Some(Scope::Control));
    }
}
