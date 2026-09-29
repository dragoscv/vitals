//! The routes, the auth layer, and the server lifecycle.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch};
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::Scope;
use crate::control::{ControlError, ControlRequest};
use crate::prometheus;
use crate::state::ApiState;

/// How many processes appear in `/metrics`.
const PROMETHEUS_TOP_N: usize = 20;

/// `/summary` without `top`: what a watch face or a widget shows.
const SUMMARY_DEFAULT_TOP: usize = 5;
/// The most a `/summary` caller can ask for. Beyond this a client wants the
/// process list, and should open the stream.
const SUMMARY_MAX_TOP: usize = 25;
/// The longest `/history` span: the store's five-minute tier covers a week.
const HISTORY_MAX_SECONDS: u32 = 7 * 24 * 3600;

/// A running server. Dropping it does **not** stop the server — call
/// [`ServeHandle::stop`]. Made explicit because a silent shutdown on drop
/// would make an unrelated refactor kill the phone's connection.
#[derive(Debug)]
pub struct ServeHandle {
    pub addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    /// Flipped to `true` on stop. Graceful shutdown alone waits for
    /// in-flight requests, and a stream is an in-flight request that never
    /// finishes: with only the oneshot, "Stop sharing" left every phone
    /// receiving live frames until it disconnected on its own.
    closing: watch::Sender<bool>,
}

impl ServeHandle {
    /// Asks the server to finish in-flight requests and stop accepting.
    /// Open streams end at once.
    pub fn stop(&mut self) {
        let _ = self.closing.send(true);
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

/// Binds `0.0.0.0:port` and starts serving on the current tokio runtime.
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
    serve_on(state, SocketAddr::from(([0, 0, 0, 0], port))).await
}

/// Binds exactly `addr` and starts serving on the current tokio runtime.
///
/// The desktop uses this for its always-on `127.0.0.1` listener, which must
/// never be reachable from the network — binding loopback is the mechanism,
/// not a filter applied afterwards. Port `0` asks the OS for a free one.
///
/// # Errors
///
/// Fails if the address is already in use or the OS refuses the bind.
pub async fn serve_on(state: ApiState, addr: SocketAddr) -> std::io::Result<ServeHandle> {
    let listener = TcpListener::bind(addr).await?;
    let addr = listener.local_addr()?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let (closing, _) = watch::channel(false);

    // The peer address is what `authorise` uses to decide whether the
    // loopback bypass applies, so the connect info must be attached here;
    // without it the extractor fails and every guarded route answers 500.
    let app = router_with_closing(state, closing.subscribe())
        .into_make_service_with_connect_info::<SocketAddr>();
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
        closing,
    })
}

/// Builds the router. Public so tests can drive it without a socket.
pub fn router(state: ApiState) -> Router {
    let (closing, rx) = watch::channel(false);
    // Kept alive for as long as the router: dropping the sender would flip
    // every receiver to "changed" and end streams immediately.
    std::mem::forget(closing);
    router_with_closing(state, rx)
}

/// Every stream handler watches `closing`; `true` ends it.
fn router_with_closing(state: ApiState, closing: watch::Receiver<bool>) -> Router {
    // `/health` is deliberately outside the auth layer: a client needs to be
    // able to tell "wrong address" from "wrong token", and it exposes only
    // the version and the fact that something is listening.
    //
    // `/pair` is public for the same kind of reason: it is how a caller
    // without a token gets one. Its own budget (one code, five guesses, five
    // minutes) is the protection, not the bearer layer.
    let public = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/pair", axum::routing::post(pair));

    let guarded = Router::new()
        .route("/api/v1/snapshot", get(snapshot))
        .route("/api/v1/summary", get(summary))
        .route("/api/v1/history", get(history))
        .route("/api/v1/sensors", get(sensors))
        .route("/api/v1/stream", get(stream))
        .route("/api/v1/ws", get(websocket))
        .route("/api/v1/host", get(host))
        .route("/api/v1/alerts", get(alerts))
        .route("/api/v1/control", axum::routing::post(control))
        .route("/metrics", get(metrics))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorise))
        .layer(axum::Extension(Closing(closing)));

    public.merge(guarded).fallback(assets).with_state(state)
}

/// The server's stop signal, shared by every connection.
#[derive(Debug, Clone)]
struct Closing(watch::Receiver<bool>);

impl Closing {
    /// Resolves when the server is stopping. Never, if it is not.
    async fn closed(mut self) {
        // `wait_for` returns Err only when the sender is gone, which for a
        // served router means the server task ended — also a reason to stop.
        let _ = self.0.wait_for(|closing| *closing).await;
    }
}

// ── Auth ───────────────────────────────────────────────────────────────

/// The scope a request was granted, attached to the request extensions,
/// together with the token it was granted for.
///
/// The token is kept so a **stream** can re-check it on every frame. Auth
/// runs once, at the handshake; without this, a phone whose pairing the user
/// revoked kept receiving frames — every process name on the machine —
/// until it happened to disconnect. `None` is the loopback bypass, which
/// has nothing to revoke.
#[derive(Debug, Clone)]
struct Granted {
    scope: Scope,
    token: Option<String>,
}

impl Granted {
    /// Whether the credential this request was granted on is still valid.
    fn still_valid(&self, state: &ApiState) -> bool {
        match &self.token {
            None => true,
            Some(token) => state.tokens.read().scope_for(token).is_some(),
        }
    }
}

/// Rejects anything without a valid bearer token, unless the caller is on the
/// loopback interface and the host has opted into [`ApiState::loopback_scope`].
///
/// Accepts the token in `Authorization: Bearer …` or, for `EventSource` and
/// `WebSocket` — neither of which can set headers in a browser — in a `token`
/// query parameter. That is a real trade: a query string can land in a log.
/// The server writes no access log, and the alternative is that the phone
/// cannot stream at all.
///
/// The loopback decision is made on the **peer** address of the connection,
/// never on which listener accepted it: a `0.0.0.0` listener receives
/// loopback connections too, and a `127.0.0.1` listener cannot receive any
/// other kind. Reading the peer is therefore both necessary and sufficient.
async fn authorise(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| *addr);
    if let Some(scope) = loopback_grant(&state, peer).filter(|_| is_local_caller(&request)) {
        let mut request = request;
        request
            .extensions_mut()
            .insert(Granted { scope, token: None });
        return next.run(request).await;
    }

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
    request.extensions_mut().insert(Granted {
        scope,
        token: Some(presented),
    });
    next.run(request).await
}

/// The scope a tokenless caller at `peer` is granted, if any.
///
/// A missing peer (the router driven without a socket, as in unit tests)
/// is treated as remote: failing closed is the only safe default for an
/// auth bypass.
fn loopback_grant(state: &ApiState, peer: Option<SocketAddr>) -> Option<Scope> {
    let scope = state.loopback_scope?;
    peer.filter(|addr| addr.ip().is_loopback()).map(|_| scope)
}

/// Whether a loopback request came from a local program rather than from a
/// web page the user happens to have open.
///
/// The loopback peer check alone proved nothing about *who* is calling: a
/// browser runs on this machine, so any page can open
/// `ws://127.0.0.1:7330/api/v1/ws` — `WebSocket`s are exempt from CORS — and
/// would have been granted control with no token, able to read every
/// process and end any of them. Two things tell a page apart:
///
/// - **`Origin`.** Browsers always send it on a `WebSocket` handshake and on
///   a cross-origin `fetch`, and a page cannot suppress it. The CLI and
///   scripts send none. Any `Origin` at all is refused: the desktop webview
///   talks to the backend over Tauri IPC, never over this socket, so no
///   legitimate caller of the bypass has one.
/// - **`Host`.** DNS rebinding points an attacker's name at 127.0.0.1, so
///   the request arrives from loopback carrying `Host: evil.example`. Only
///   a loopback host is accepted.
///
/// A refused request is not rejected here — it falls through to the token
/// check like a remote caller, so a page holding a real token (the paired
/// phone's PWA is served from this same server) still works.
fn is_local_caller(request: &Request) -> bool {
    if request.headers().contains_key(header::ORIGIN) {
        return false;
    }
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| request.uri().host());
    host.is_some_and(is_loopback_host)
}

/// `127.0.0.1`, `[::1]` or `localhost`, with or without a port.
fn is_loopback_host(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host.rsplit_once(':').map_or(host, |(name, _)| name)
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
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

/// Alerts currently raised, most serious first. An empty array is the
/// healthy answer, not 204: "nothing wrong" is a value.
async fn alerts(State(state): State<ApiState>) -> Response {
    Json((state.alerts)()).into_response()
}

async fn host(State(state): State<ApiState>) -> Response {
    match (state.host)() {
        Some(info) => Json(info).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct SummaryQuery {
    top: Option<usize>,
}

/// The machine now plus its busiest processes: about 3 KB against a
/// keyframe's 250 KB. For the watch, widgets and tiles (ADR-0033).
///
/// `top` is clamped, not rejected: a widget asking for 50 gets 25 and
/// renders, where a 422 would leave a blank tile on someone's home screen.
async fn summary(State(state): State<ApiState>, Query(q): Query<SummaryQuery>) -> Response {
    let top = q.top.unwrap_or(SUMMARY_DEFAULT_TOP).min(SUMMARY_MAX_TOP);
    match state.frames.summary(top) {
        Some(summary) => Json(summary).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    seconds: Option<u32>,
}

/// Machine-wide samples for the last `seconds` (default an hour, at most a
/// week), at whatever resolution the store keeps for that span.
///
/// Off the async runtime: the desktop's provider opens SQLite and reads a
/// few thousand rows, which would stall every stream on that worker.
async fn history(State(state): State<ApiState>, Query(q): Query<HistoryQuery>) -> Response {
    let seconds = q.seconds.unwrap_or(3600).clamp(60, HISTORY_MAX_SECONDS);
    let provider = Arc::clone(&state.history);
    match tokio::task::spawn_blocking(move || provider(seconds)).await {
        Ok(rows) => Json(rows).into_response(),
        Err(error) => {
            tracing::warn!(%error, "history provider panicked");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Every sensor reading the host can make. An empty list means none are
/// measurable on this machine, never "all zero".
async fn sensors(State(state): State<ApiState>) -> Response {
    state.frames.touch();
    let provider = Arc::clone(&state.sensors);
    match tokio::task::spawn_blocking(move || provider()).await {
        Ok(lines) => Json(lines).into_response(),
        Err(error) => {
            tracing::warn!(%error, "sensor provider panicked");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
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
async fn stream(
    State(state): State<ApiState>,
    axum::Extension(granted): axum::Extension<Granted>,
    axum::Extension(closing): axum::Extension<Closing>,
) -> Response {
    let initial = state.frames.initial();
    let live = BroadcastStream::new(state.frames.subscribe());

    // Send whatever we already have first, so a client that connects between
    // ticks renders immediately instead of showing a spinner for up to two
    // seconds.
    let head = futures_util::stream::iter(initial.into_iter().map(Ok));
    let frames = state.frames.clone();
    let tail = live.filter_map(move |received| {
        let frames = frames.clone();
        async move {
            match received {
                Ok(frame) => Some(Ok::<_, Infallible>(frame)),
                // Lagged: frames were dropped for this client. The next one it
                // would get is a delta against a frame it never saw, and a
                // delta on top of a missed delta is a process table that
                // never self-corrects (exited processes linger, new ones are
                // missing) until the sampler's own keyframe up to thirty
                // seconds later. Hand it the complete present instead.
                Err(_) => frames.initial().map(Ok),
            }
        }
    });

    // Revoked token: end the stream at the next frame. Checked per frame
    // rather than per tick of a timer so an idle server costs nothing.
    let revoked_state = state.clone();
    let live_while_valid = head
        .chain(tail)
        .take_while(move |_| futures_util::future::ready(granted.still_valid(&revoked_state)));
    // Server stopping: end the stream now, not at the next frame.
    let until_closed = live_while_valid.take_until(closing.closed());

    let events = until_closed.map(|frame: Result<_, Infallible>| {
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
    axum::Extension(granted): axum::Extension<Granted>,
    axum::Extension(closing): axum::Extension<Closing>,
) -> Response {
    ws.on_upgrade(move |socket| pump(socket, state, granted, closing))
}

/// Frames out, control commands in.
async fn pump(mut socket: WebSocket, state: ApiState, granted: Granted, closing: Closing) {
    let mut rx = state.frames.subscribe();
    let scope = granted.scope;
    let closed = closing.closed();
    let mut closed = std::pin::pin!(closed);

    if let Some(frame) = state.frames.initial()
        && send_frame(&mut socket, &frame).await.is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            () = &mut closed => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            received = rx.recv() => match received {
                Ok(frame) => {
                    if !granted.still_valid(&state) {
                        let _ = socket.send(Message::Close(None)).await;
                        return;
                    }
                    if send_frame(&mut socket, &frame).await.is_err() {
                        return;
                    }
                }
                // Same reasoning as the SSE path: a resync keyframe, not a skip.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if let Some(frame) = state.frames.initial()
                        && send_frame(&mut socket, &frame).await.is_err()
                    {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if !granted.still_valid(&state) {
                        let _ = socket.send(Message::Close(None)).await;
                        return;
                    }
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
///
/// Takes the body as raw `Bytes` and parses it by hand so the **scope check
/// happens first**. With `Json<ControlRequest>` as an extractor, axum rejects
/// a malformed body with 422 before the handler runs, so a read-only client
/// with a slightly wrong request learns nothing about why it is refused —
/// and a correct one gets 403. Authorisation before validation is also the
/// safer order: an unauthorised caller should not be able to probe the
/// schema.
async fn control(
    State(state): State<ApiState>,
    axum::Extension(granted): axum::Extension<Granted>,
    body: axum::body::Bytes,
) -> Response {
    if !granted.scope.allows_control() {
        return (StatusCode::FORBIDDEN, Json(ControlError::Forbidden)).into_response();
    }

    let request = match serde_json::from_slice::<ControlRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ControlError::Unsupported {
                    message: format!("could not read the request: {error}"),
                }),
            )
                .into_response();
        }
    };

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
struct PairBody {
    code: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Serialize)]
struct PairReply {
    token: String,
    scope: Scope,
}

/// The one body every refused redemption gets. A constant, so "no code",
/// "expired", "wrong" and "burnt" cannot drift apart and become an oracle.
const PAIR_REFUSED: &str = "invalid pairing code";

/// Exchanges a six-digit code for a bearer token (see [`crate::pairing`]).
///
/// The body is parsed by hand from `Bytes`, as in [`control`], so the order
/// of checks is ours: "does this host pair at all" first, then "is this a
/// code" — a malformed body is a 400 and spends none of the five attempts —
/// and only then the guess itself.
async fn pair(State(state): State<ApiState>, body: axum::body::Bytes) -> Response {
    let Some(desk) = state.pairing.as_ref() else {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "this server does not offer pairing codes".to_owned(),
            }),
        )
            .into_response();
    };

    let request = match serde_json::from_slice::<PairBody>(&body) {
        Ok(request) if crate::pairing::is_well_formed(&request.code) => request,
        Ok(_) => return bad_request("code must be exactly six digits"),
        Err(_) => return bad_request("expected {\"code\": \"123456\"}"),
    };

    match desk.redeem(&request.code, request.label.as_deref(), &state.tokens) {
        Ok(redeemed) => Json(PairReply {
            token: redeemed.secret,
            scope: redeemed.scope,
        })
        .into_response(),
        Err(crate::pairing::Refused) => (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: PAIR_REFUSED.to_owned(),
            }),
        )
            .into_response(),
    }
}

fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: message.to_owned(),
        }),
    )
        .into_response()
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use super::loopback_grant;
    use crate::auth::{Scope, TokenSet};
    use crate::state::{ApiState, FrameSource, ServerLock};

    fn state(loopback_scope: Option<Scope>) -> ApiState {
        ApiState {
            frames: FrameSource::new(),
            tokens: Arc::new(ServerLock::new(TokenSet::default())),
            controller: Arc::new(crate::control::NoControl),
            assets: None,
            host: Arc::new(|| None),
            alerts: Arc::new(Vec::new),
            history: Arc::new(|_| Vec::new()),
            sensors: Arc::new(Vec::new),
            version: "test".into(),
            loopback_scope,
            pairing: None,
        }
    }

    fn peer(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn a_lan_peer_is_never_granted_the_loopback_scope() {
        // The bypass is keyed on the peer, not the listener: a `0.0.0.0`
        // listener with the bypass on would otherwise hand control to
        // everything on the network.
        let s = state(Some(Scope::Control));
        assert_eq!(loopback_grant(&s, Some(peer("192.168.1.5:1234"))), None);
        assert_eq!(loopback_grant(&s, Some(peer("[fe80::1]:1234"))), None);
    }

    #[test]
    fn loopback_v4_and_v6_peers_get_exactly_the_configured_scope() {
        let s = state(Some(Scope::Read));
        assert_eq!(
            loopback_grant(&s, Some(peer("127.0.0.1:50000"))),
            Some(Scope::Read)
        );
        assert_eq!(
            loopback_grant(&s, Some(peer("[::1]:50000"))),
            Some(Scope::Read)
        );
    }

    #[test]
    fn loopback_grants_nothing_when_the_host_has_not_opted_in() {
        // The LAN server's configuration. Loopback callers there still need
        // a token, exactly as before this bypass existed.
        let s = state(None);
        assert_eq!(loopback_grant(&s, Some(peer("127.0.0.1:50000"))), None);
    }

    #[test]
    fn an_unknown_peer_fails_closed() {
        // No connect info means the router was driven without a socket;
        // an auth bypass must not fire on missing information.
        let s = state(Some(Scope::Control));
        assert_eq!(loopback_grant(&s, None), None);
    }
}
