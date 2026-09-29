//! # vitals-server
//!
//! The local HTTP API. One axum router serving:
//!
//! | Path                 | What                                             |
//! |----------------------|--------------------------------------------------|
//! | `GET /api/v1/snapshot` | The latest full frame as JSON                  |
//! | `GET /api/v1/summary`  | System metrics + top N processes (~3 KB)       |
//! | `GET /api/v1/history`  | Machine-wide history from the host's store     |
//! | `GET /api/v1/sensors`  | Every sensor reading, flattened                |
//! | `GET /api/v1/stream`   | Server-sent events, one per frame              |
//! | `GET /api/v1/ws`       | WebSocket: frames out, control commands in     |
//! | `GET /api/v1/host`     | Host facts                                      |
//! | `GET /api/v1/health`   | Liveness, unauthenticated                       |
//! | `POST /api/v1/pair`    | Six-digit code for a token, unauthenticated     |
//! | `GET /metrics`         | Prometheus text exposition                     |
//! | `/*`                   | The mobile web app (static, from the caller)   |
//!
//! Alongside the router, [`mdns`] can announce the server as
//! `_vitals._tcp.local.` so a paired phone finds it again without a rescan.
//! The announcement lives exactly as long as the listener does.
//!
//! **Off by default.** Nothing here binds a socket until the host application
//! decides to. When it does, every route except `/health` and `/pair`
//! requires a bearer token, compared in constant time. `/pair` is how a TV,
//! which cannot scan the QR code, obtains one: see [`pairing`]. Tokens carry a scope: `read` can see,
//! `control` can also act.
//!
//! Framework choice: axum rather than actix-web, because Tauri already runs a
//! multi-thread tokio runtime and axum adds no second executor. `tiny_http`
//! has no WebSocket upgrade. See the workspace `Cargo.toml` for the longer
//! note.
//!
//! This crate knows nothing about Tauri or Windows. It is handed a
//! [`FrameSource`] and a [`Controller`] and serves them; the desktop app and
//! the CLI's `serve` command are both callers.

pub mod auth;
pub mod control;
pub mod lan;
pub mod mdns;
pub mod pairing;
pub mod prometheus;
pub mod router;
pub mod state;

pub use auth::{Scope, Token, TokenSet};
pub use control::{ControlError, ControlRequest, Controller};
pub use lan::{Interface, interfaces, pairing_qr_svg, pairing_url};
pub use mdns::{Advertisement, advertise};
pub use pairing::{IssuedCode, PairingDesk, PairingStatus};
pub use router::{ServeHandle, serve, serve_on};
pub use state::{ApiState, FrameSource, StaticAssets};
