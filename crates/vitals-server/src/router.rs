//! The routes, the auth layer, and the server lifecycle.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::Scope;
use crate::control::{ControlError, ControlRequest};
use crate::prometheus;
use crate::state::ApiState;

/// How many processes appear in `/metrics`.
const PROMETHEUS_TOP_N: usize = 20;

/// A running server. Dropping it does **not** stop the server — call
/// [`ServeHandle::stop`]. Made explicit because a silent shutdown on drop
/// would make an unrelated refactor kill the phone's connection.
#[derive(Debug)]
pub struct ServeHandle {
    pub addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
}

impl ServeHandle {
    /// Asks the server to finish in-flight requests and stop accepting.
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

/// Binds and starts serving on the current tokio runtime.
///
/// Port `0` asks the OS for a free one; the real port is on the returned
/// handle. Binding `0.0.0.0` is what makes this reachable from a phone, and
/// is exactly why the feature is off until the user turns it on.
///
/// # Errors
///
/// Fails if the address is already in use or the OS refuses the bind. On
/// Windows the first bind also raises the firewall prompt; a user who
/// declines it gets a server that starts and is unreachable, which the UI
/// must explain rather than showing a spinner.
pub async fn serve(state: ApiState, port: u16) -> std::io::Result<ServeHandle> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    let addr = listener.local_addr()?;
    let (stop_tx, stop_rx) = oneshot::channel();

    let app = router(state);
    tokio::spawn(async move {
        let served = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stop_rx.await;
            })
            .await;
        if let Err(error) = served {
            tracing::warn!(%error, "local API server stopped");
        }
    });

    Ok(ServeHandle {
        addr,
        stop: Some(stop_tx),
    })
}

/// Builds the router. Public so tests can drive it without a socket.
pub fn router(state: ApiState) -> Router {
    // `/health` is deliberately outside the auth layer: a client needs to be
    // able to tell "wrong address" from "wrong token", and it exposes only
    // the version and the fact that something is listening.
    let public = Router::new().route("/api/v1/health", get(health));

    let guarded = Router::new()
        .route("/api/v1/snapshot", get(snapshot))
        .route("/api/v1/stream", get(stream))
        .route("/api/v1/ws", get(websocket))
        .route("/api/v1/host", get(host))
        .route("/api/v1/control", axum::routing::post(control))
        .route("/metrics", get(metrics))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorise));

    public.merge(guarded).fallback(assets).with_state(state)
}

// ── Auth ───────────────────────────────────────────────────────────────

/// The scope a request was granted, attached to the request extensions.
#[derive(Debug, Clone, Copy)]
struct Granted(Scope);

/// Rejects anything without a valid bearer token.
///
/// Accepts the token in `Authorization: Bearer …` or, for `EventSource` and
/// `WebSocket` — neither of which can set headers in a browser — in a `token`
/// query parameter. That is a real trade: a query string can land in a log.
/// The server writes no access log, and the alternative is that the phone
/// cannot stream at all.
async fn authorise(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    let presented = bearer(&request).or_else(|| query_token(&request));

    let Some(presented) = presented else {
        return unauthorised("a bearer token is required");
    };

    let Some(scope) = state.tokens.read().scope_for(&presented) else {
        // Deliberately identical to the missing-token response: telling a
        // caller that a token exists but is wrong is a free oracle.
        return unauthorised("a bearer token is required");
    };

    let mut request = request;
    request.extensions_mut().insert(Granted(scope));
    next.run(request).await
}

fn bearer(request: &Request) -> Option<String> {
    request
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::to_owned)
}

fn query_token(request: &Request) -> Option<String> {
    let query = request.uri().query()?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "token").then(|| value.to_owned())
    })
}

fn unauthorised(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"vitals\""),
        )],
        Json(ApiError {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

// ── Handlers ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    version: String,
    /// So a client can refuse to parse a frame shape it does not know.
    model_version: u32,
}

async fn health(State(state): State<ApiState>) -> Json<Health> {
    Json(Health {
        ok: true,
        version: state.version.clone(),
        model_version: vitals_core::MODEL_VERSION,
    })
}

/// The most recent frame.
///
/// `204 No Content` before the first tick, rather than an empty object: a
/// client can tell "not sampling yet" from "sampling, everything is zero".
async fn snapshot(State(state): State<ApiState>) -> Response {
    // The keyframe, not the last frame: a one-shot caller asking "what is
    // running" must get the whole list, not the handful of processes that
    // happened to change since the previous tick.
    match state.frames.initial() {
        Some(frame) => Json(&*frame).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn host(State(state): State<ApiState>) -> Response {
    match (state.host)() {
        Some(info) => Json(info).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn metrics(State(state): State<ApiState>) -> Response {
    // System-wide numbers from the newest frame (a delta still carries the
    // full `system` tree), but the process list from the last keyframe:
    // ranking "top processes" from a delta would rank whichever happened to
    // change, not whichever is busiest.
    let Some(frame) = state.frames.latest() else {
        return StatusCode::NO_CONTENT.into_response();
    };
    let system = match &frame.payload {
        vitals_core::sample::FramePayload::Keyframe { system, .. }
        | vitals_core::sample::FramePayload::Delta { system, .. } => system,
    };
    let keyframe = state.frames.initial();
    let processes = match keyframe.as_deref().map(|f| &f.payload) {
        Some(vitals_core::sample::FramePayload::Keyframe { processes, .. }) => processes.as_slice(),
        _ => [].as_slice(),
    };

    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        prometheus::render(system, processes, PROMETHEUS_TOP_N),
    )
        .into_response()
}

/// Frames as server-sent events.
async fn stream(State(state): State<ApiState>) -> Response {
    let initial = state.frames.initial();
    let live = BroadcastStream::new(state.frames.subscribe());

    // Send whatever we already have first, so a client that connects between
    // ticks renders immediately instead of showing a spinner for up to two
    // seconds.
    let head = futures_util::stream::iter(initial.into_iter().map(Ok));
    let tail = live.filter_map(|received| async move {
        match received {
            Ok(frame) => Some(Ok::<_, Infallible>(frame)),
            // Lagged: this client fell behind and frames were dropped for it.
            // Skipping is correct for live metrics — it wants the current
            // numbers, not a backlog.
            Err(_) => None,
        }
    });

    let events = head.chain(tail).map(|frame: Result<_, Infallible>| {
        let frame = frame.unwrap_or_else(|never| match never {});
        Event::default()
            .json_data(&*frame)
            .unwrap_or_else(|_| Event::default().comment("unserialisable frame"))
    });

    let events = events.map(Ok::<_, Infallible>);

    (
        // Proxies buffer SSE by default. Irrelevant on a LAN, harmless to set,
        // and it saves a confusing report the first time someone runs this
        // behind one.
        [(
            header::HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        )],
        Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))),
    )
        .into_response()
}

async fn websocket(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
    axum::Extension(Granted(scope)): axum::Extension<Granted>,
) -> Response {
    ws.on_upgrade(move |socket| pump(socket, state, scope))
}

/// Frames out, control commands in.
async fn pump(mut socket: WebSocket, state: ApiState, scope: Scope) {
    let mut rx = state.frames.subscribe();

    if let Some(frame) = state.frames.initial()
        && send_frame(&mut socket, &frame).await.is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            received = rx.recv() => match received {
                Ok(frame) => {
                    if send_frame(&mut socket, &frame).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let reply = handle_control(&state, scope, &text);
                    if socket.send(Message::Text(reply.into())).await.is_err() {
                        return;
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

async fn send_frame(
    socket: &mut WebSocket,
    frame: &vitals_core::sample::Frame,
) -> Result<(), axum::Error> {
    match serde_json::to_string(frame) {
        Ok(text) => socket.send(Message::Text(text.into())).await,
        // A frame that will not serialise is a bug on our side, not a reason
        // to drop the client's connection.
        Err(error) => {
            tracing::warn!(%error, "frame did not serialise for a websocket client");
            Ok(())
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControlReply {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ControlError>,
}

fn handle_control(state: &ApiState, scope: Scope, text: &str) -> String {
    let reply = match serde_json::from_str::<ControlRequest>(text) {
        Err(error) => ControlReply {
            ok: false,
            error: Some(ControlError::Unsupported {
                message: format!("could not read the request: {error}"),
            }),
        },
        Ok(_) if !scope.allows_control() => ControlReply {
            ok: false,
            error: Some(ControlError::Forbidden),
        },
        Ok(request) => match state.controller.apply(request) {
            Ok(()) => ControlReply {
                ok: true,
                error: None,
            },
            Err(error) => ControlReply {
                ok: false,
                error: Some(error),
            },
        },
    };
    serde_json::to_string(&reply).unwrap_or_else(|_| r#"{"ok":false}"#.to_owned())
}

/// The same control surface over plain HTTP, for clients that do not want a
/// socket (a shell script with curl, Home Assistant).
async fn control(
    State(state): State<ApiState>,
    axum::Extension(Granted(scope)): axum::Extension<Granted>,
    Json(request): Json<ControlRequest>,
) -> Response {
    if !scope.allows_control() {
        return (StatusCode::FORBIDDEN, Json(ControlError::Forbidden)).into_response();
    }
    match state.controller.apply(request) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            let status = match error {
                ControlError::Forbidden | ControlError::AccessDenied => StatusCode::FORBIDDEN,
                ControlError::NotFound => StatusCode::NOT_FOUND,
                ControlError::Unsupported { .. } => StatusCode::NOT_IMPLEMENTED,
                ControlError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(error)).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct AssetQuery {
    #[allow(dead_code)]
    token: Option<String>,
}

/// Serves the mobile web app.
///
/// Unauthenticated on purpose: these are the same static files anyone could
/// take from the installer, and the page cannot show anything until it has a
/// token to call the API with. Requiring auth here would mean the browser
/// prompting before the page that reads the token from the fragment has even
/// loaded.
async fn assets(
    State(state): State<ApiState>,
    Query(_q): Query<AssetQuery>,
    request: Request,
) -> Response {
    let Some(assets) = state.assets.as_ref() else {
        return (
            StatusCode::NOT_FOUND,
            "no web app is bundled with this server",
        )
            .into_response();
    };

    let path = request.uri().path();
    let candidate = if path == "/" { "/mobile.html" } else { path };

    // `..` cannot escape: the resolver is given a path and returns bytes it
    // already holds, but a caller may back it with a directory, so refuse
    // traversal here rather than trusting every future implementation.
    if candidate.contains("..") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match assets(candidate.trim_start_matches('/')) {
        Some((bytes, mime)) => (
            [(header::CONTENT_TYPE, HeaderValue::from_static(mime))],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Guesses a content type from an extension.
///
/// A short table rather than a crate: the server only ever hands back files
/// this project built, and the set of extensions is fixed.
#[must_use]
pub fn mime_for(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "webmanifest") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
