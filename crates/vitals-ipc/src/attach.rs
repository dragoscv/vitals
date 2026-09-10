//! The attach pipe: how the CLI reads a running desktop's frames.
//!
//! One machine should run one sampler. When `vitals top` starts beside the
//! desktop app, sampling again would double the cost, and the second sampler
//! would disagree with the first for its opening seconds (rates need two
//! ticks; the first always reads 0 %). So the desktop listens on a per-user
//! local socket — a named pipe on Windows — and the CLI reads the frames the
//! app already has.
//!
//! ## Why there is no token
//!
//! The LAN server hands out bearer tokens because a socket bound to an
//! interface is reachable by every machine on the network. This pipe is not:
//! `interprocess` creates a Windows named pipe with the default security
//! descriptor, which grants access to the creating user's SID and to
//! administrators, and the name carries the user's account so two users on
//! one machine get two pipes. A process that can open it already runs as you
//! and could read the same counters itself. A token would prove nothing the
//! kernel has not already checked, and would make `vitals ps` unusable until
//! the user had copied one out of the app.
//!
//! ## Wire format
//!
//! Newline-delimited JSON, one request per line, kebab-case operations:
//!
//! ```text
//! → {"op":"hello"}
//! ← {"status":"ok","version":"0.1.0","modelVersion":1}
//! → {"op":"snapshot"}
//! ← <Frame>              (a keyframe of the complete current state)
//! → {"op":"subscribe"}
//! ← <Frame>              (first: the materialised keyframe, then every tick)
//! ← <Frame> …
//! ```
//!
//! A connection speaks exactly one of `snapshot` / `subscribe`; `hello` may
//! precede it. Frames are [`vitals_core::sample::Frame`] serialised exactly as
//! the desktop's own webview receives them, so a client's fold rule is the one
//! in `packages/protocol`.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;
use std::time::Duration;

use interprocess::local_socket::traits::{Listener as _, Stream as _};
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, ToNsName};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use vitals_core::ids::Pid;
use vitals_core::metrics::SystemMetrics;
use vitals_core::process::Process;
use vitals_core::sample::{Frame, FramePayload, FrameSeq};

type Stream = interprocess::local_socket::Stream;

/// How long `snapshot` waits for the sampler to produce a complete frame
/// before answering with an error.
///
/// Frames are not forwarded while nobody is attached, so a fresh connection
/// always waits at least one tick. That tick is as long as the app's current
/// sample rate, and `SampleRate::Background` — what a hidden window drops to
/// — is **20 seconds**. An earlier six-second limit looked generous and made
/// `vitals ps --source app` fail against a minimised app, which is precisely
/// when someone reaches for the CLI. Measured against a real app on
/// Background: first frame at 15 s.
const SNAPSHOT_WAIT: Duration = Duration::from_secs(25);

/// Per-subscriber queue depth. Small on purpose: a client that falls this far
/// behind wants current numbers, not a backlog.
const SUBSCRIBER_QUEUE: usize = 8;

// ── Name ─────────────────────────────────────────────────────────────────

/// The pipe name for the current user: `vitals-<account>`.
///
/// Per-user rather than global so that two accounts sharing a machine (fast
/// user switching, a Remote Desktop session) each attach to their own app
/// rather than the other person's. The account name is sanitised to the
/// characters every platform's namespace accepts.
#[must_use]
pub fn default_pipe_name() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "default".to_owned());
    let safe: String = user
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("vitals-{safe}")
}

fn resolve(name: &str) -> io::Result<Name<'static>> {
    name.to_owned().to_ns_name::<GenericNamespaced>()
}

// ── Protocol ─────────────────────────────────────────────────────────────

/// What a client may ask for. One line each, `{"op":"…"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    /// Version check. Answered with [`Hello`].
    Hello,
    /// Stream frames until the connection closes.
    Subscribe,
    /// One complete frame, then the connection closes.
    Snapshot,
}

/// Reply to [`Request::Hello`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub status: String,
    /// The serving app's version, for diagnostics.
    pub version: String,
    /// Must equal [`vitals_core::MODEL_VERSION`] or the client refuses to
    /// interpret frames.
    pub model_version: u32,
}

/// An error the server writes in place of a frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorReply {
    pub status: String,
    pub message: String,
}

/// A line that could not be understood.
#[derive(Debug)]
pub enum ProtocolError {
    /// Not JSON, or JSON that is not a known request.
    Malformed {
        line: String,
        reason: serde_json::Error,
    },
    /// The server said so.
    Server(String),
    /// The other side speaks a different frame model.
    ModelVersion { theirs: u32, ours: u32 },
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed { line, reason } => {
                write!(f, "unparseable line {line:?}: {reason}")
            }
            Self::Server(message) => write!(f, "server error: {message}"),
            Self::ModelVersion { theirs, ours } => write!(
                f,
                "the app speaks model version {theirs} but this client expects {ours}; upgrade one of them"
            ),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<ProtocolError> for io::Error {
    fn from(error: ProtocolError) -> Self {
        Self::new(io::ErrorKind::InvalidData, error)
    }
}

/// Splits a byte stream into requests at newlines.
///
/// Kept apart from the socket so the framing can be tested without one: the
/// three things that go wrong at this layer — a request arriving in two
/// writes, two requests arriving in one, and a line that is not a request —
/// are all properties of the buffer, not of the transport.
#[derive(Debug, Default)]
pub struct Decoder {
    pending: Vec<u8>,
}

impl Decoder {
    /// Appends bytes and returns every complete line they finish.
    ///
    /// A bad line yields an `Err` in its position and decoding continues with
    /// the next; the caller decides whether one bad request ends the
    /// connection. Empty lines are skipped so a client that sends `\r\n` or a
    /// trailing blank does not trip an error.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Result<Request, ProtocolError>> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            out.push(serde_json::from_str::<Request>(text).map_err(|reason| {
                ProtocolError::Malformed {
                    line: text.to_owned(),
                    reason,
                }
            }));
        }
        out
    }
}

// ── Materialised view ────────────────────────────────────────────────────

/// The complete present, rebuilt from the frames published so far.
///
/// Mirrors `vitals-server`'s `FrameSource` fold rather than importing it: the
/// server crate carries axum and tokio, which the helper — the other user of
/// this crate — must not link. The rule is the same one everywhere in this
/// repository, so it is stated here in words as well as code: **exits are
/// applied before changes**, because a PID that exited and was reused within
/// one tick appears in both lists, and removing afterwards would delete the
/// new owner.
#[derive(Debug, Clone)]
struct Materialised {
    seq: FrameSeq,
    timestamp_ms: u64,
    elapsed_ms: u32,
    system: SystemMetrics,
    processes: HashMap<Pid, Process>,
}

impl Materialised {
    fn to_frame(&self) -> Frame {
        let mut processes: Vec<_> = self.processes.values().cloned().collect();
        // Stable order so two snapshots do not appear to shuffle.
        processes.sort_by_key(|p| p.key.pid.0);
        Frame {
            seq: self.seq,
            timestamp_ms: self.timestamp_ms,
            elapsed_ms: self.elapsed_ms,
            payload: FramePayload::Keyframe {
                system: self.system.clone(),
                processes,
            },
        }
    }

    /// Folds one frame in. Returns `None` for a delta with no keyframe
    /// before it — nothing to fold onto — so the caller keeps the slot empty
    /// rather than inventing a partial machine.
    fn fold(current: Option<Self>, frame: &Frame) -> Option<Self> {
        match &frame.payload {
            FramePayload::Keyframe { system, processes } => Some(Self {
                seq: frame.seq,
                timestamp_ms: frame.timestamp_ms,
                elapsed_ms: frame.elapsed_ms,
                system: system.clone(),
                processes: processes.iter().map(|p| (p.key.pid, p.clone())).collect(),
            }),
            FramePayload::Delta {
                system,
                changed,
                exited,
            } => {
                let mut view = current?;
                view.seq = frame.seq;
                view.timestamp_ms = frame.timestamp_ms;
                view.elapsed_ms = frame.elapsed_ms;
                view.system = system.clone();
                for pid in exited {
                    view.processes.remove(pid);
                }
                for process in changed {
                    view.processes.insert(process.key.pid, process.clone());
                }
                Some(view)
            }
        }
    }
}

// ── Server ───────────────────────────────────────────────────────────────

/// One connected `subscribe` client.
#[derive(Debug)]
struct Subscriber {
    tx: SyncSender<Arc<Frame>>,
    /// Set when a frame was dropped because the client was not keeping up.
    /// The next publish sends it the materialised keyframe instead of the
    /// delta it would otherwise get, because a delta on top of a missed
    /// delta is a wrong process table that never self-corrects until the
    /// sampler's periodic keyframe thirty seconds later.
    lagged: AtomicBool,
}

#[derive(Debug, Default)]
struct Shared {
    current: Mutex<Option<Materialised>>,
    subscribers: Mutex<Vec<Arc<Subscriber>>>,
    /// Connections currently open, `subscribe` or not. The host reads this
    /// before cloning a frame.
    clients: AtomicUsize,
    stopping: AtomicBool,
}

/// Decrements the client count however the handler exits.
struct ClientGuard(Arc<Shared>);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.clients.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The desktop's end of the pipe.
///
/// Cheap while nobody is attached: [`has_clients`](Self::has_clients) is one
/// atomic load, and the host is expected to check it before cloning a frame.
/// The sampler runs continuously on someone's machine; work that is only
/// needed when a client is attached must not happen when none is.
#[derive(Debug)]
pub struct AttachServer {
    shared: Arc<Shared>,
    name: String,
}

impl AttachServer {
    /// Binds the pipe and starts accepting on a background thread.
    ///
    /// # Errors
    /// The name is invalid or already bound — a second desktop instance for
    /// this user, which the single-instance plugin should already prevent.
    pub fn start(name: &str, version: String) -> io::Result<Self> {
        let listener = ListenerOptions::new().name(resolve(name)?).create_sync()?;
        let shared = Arc::new(Shared::default());
        let accept_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("vitals-attach-accept".into())
            .spawn(move || accept_loop(&listener, &accept_shared, &version))?;
        Ok(Self {
            shared,
            name: name.to_owned(),
        })
    }

    /// The name this server was bound under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether anything is connected. One atomic load.
    #[must_use]
    pub fn has_clients(&self) -> bool {
        self.shared.clients.load(Ordering::Relaxed) > 0
    }

    /// True when a client is waiting and the server holds no complete view
    /// to give it. The host should force its next frame to be a keyframe:
    /// frames are skipped while nobody is attached, so the first one a new
    /// client sees is otherwise a delta against a frame the server never
    /// received, and nothing can be materialised from that.
    #[must_use]
    pub fn wants_keyframe(&self) -> bool {
        self.has_clients() && self.shared.current.lock().is_none()
    }

    /// Folds a frame into the view and forwards it to every subscriber.
    ///
    /// Only call this while [`has_clients`](Self::has_clients) — the fold
    /// clones every process in a keyframe. Non-blocking: a slow subscriber
    /// has the frame dropped and is marked to receive a keyframe next.
    pub fn publish(&self, frame: &Arc<Frame>) {
        let materialised = {
            let mut guard = self.shared.current.lock();
            *guard = Materialised::fold(guard.take(), frame);
            // Nothing to forward until a keyframe has been seen: a delta on
            // its own describes changes to a machine the client has never
            // seen, and every fold rule downstream would drop it anyway.
            let Some(view) = guard.as_ref() else { return };
            view.to_frame()
        };
        let materialised = Arc::new(materialised);

        let mut subscribers = self.shared.subscribers.lock();
        subscribers.retain(|sub| {
            let payload = if sub.lagged.swap(false, Ordering::SeqCst) {
                Arc::clone(&materialised)
            } else {
                Arc::clone(frame)
            };
            match sub.tx.try_send(payload) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    sub.lagged.store(true, Ordering::SeqCst);
                    true
                }
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
    }

    /// Stops accepting. Open connections end when their client leaves.
    pub fn stop(&self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        // `accept` blocks with nothing to interrupt it; connecting to
        // ourselves wakes it so it can observe the flag and return.
        if let Ok(name) = resolve(&self.name) {
            let _ = Stream::connect(name);
        }
    }
}

impl Drop for AttachServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(
    listener: &interprocess::local_socket::Listener,
    shared: &Arc<Shared>,
    version: &str,
) {
    loop {
        let conn = match listener.accept() {
            Ok(conn) => conn,
            Err(error) => {
                if shared.stopping.load(Ordering::SeqCst) {
                    return;
                }
                tracing::warn!(%error, "attach pipe accept failed");
                continue;
            }
        };
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        shared.clients.fetch_add(1, Ordering::SeqCst);
        let guard = ClientGuard(Arc::clone(shared));
        let version = version.to_owned();
        let spawned = thread::Builder::new()
            .name("vitals-attach-client".into())
            .spawn(move || {
                // Held for the whole connection so the count drops on any exit.
                let guard = guard;
                if let Err(error) = serve_connection(conn, &guard.0, &version) {
                    // A client that closed mid-stream is the normal way a
                    // `top` session ends; only a failure to speak is notable.
                    if error.kind() != io::ErrorKind::BrokenPipe {
                        tracing::debug!(%error, "attach client ended");
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not spawn attach client thread");
        }
    }
}

fn write_line<T: Serialize>(out: &mut impl Write, value: &T) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    out.write_all(&bytes)?;
    out.flush()
}

fn write_error(out: &mut impl Write, message: &str) -> io::Result<()> {
    write_line(
        out,
        &ErrorReply {
            status: "error".to_owned(),
            message: message.to_owned(),
        },
    )
}

fn serve_connection(conn: Stream, shared: &Arc<Shared>, version: &str) -> io::Result<()> {
    let mut reader = BufReader::new(conn);
    let mut decoder = Decoder::default();
    let mut buf = [0_u8; 512];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            return Ok(());
        }
        for request in decoder.push(&buf[..read]) {
            match request {
                Ok(Request::Hello) => write_line(
                    reader.get_mut(),
                    &Hello {
                        status: "ok".to_owned(),
                        version: version.to_owned(),
                        model_version: vitals_core::MODEL_VERSION,
                    },
                )?,
                Ok(Request::Snapshot) => return serve_snapshot(reader.get_mut(), shared),
                Ok(Request::Subscribe) => return serve_subscribe(reader.get_mut(), shared),
                Err(error) => {
                    write_error(reader.get_mut(), &error.to_string())?;
                    return Err(error.into());
                }
            }
        }
    }
}

fn register(shared: &Shared) -> Receiver<Arc<Frame>> {
    let (tx, rx) = mpsc::sync_channel(SUBSCRIBER_QUEUE);
    shared.subscribers.lock().push(Arc::new(Subscriber {
        tx,
        lagged: AtomicBool::new(false),
    }));
    rx
}

fn serve_snapshot(out: &mut Stream, shared: &Shared) -> io::Result<()> {
    if let Some(frame) = shared.current.lock().as_ref().map(Materialised::to_frame) {
        return write_line(out, &frame);
    }
    // Nothing folded yet — the sampler was idle for us. The next publish
    // (a keyframe, because the host reads `wants_keyframe`) arrives here.
    let rx = register(shared);
    match rx.recv_timeout(SNAPSHOT_WAIT) {
        Ok(_) => match shared.current.lock().as_ref().map(Materialised::to_frame) {
            Some(frame) => write_line(out, &frame),
            None => write_error(out, "the app has not produced a complete frame yet"),
        },
        Err(_) => write_error(
            out,
            "the app produced no frame within 25 s; its sampling is probably paused",
        ),
    }
}

fn serve_subscribe(out: &mut Stream, shared: &Shared) -> io::Result<()> {
    // Register before reading the view so no frame falls in the gap.
    let rx = register(shared);
    if let Some(frame) = shared.current.lock().as_ref().map(Materialised::to_frame) {
        write_line(out, &frame)?;
    }
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(frame) => write_line(out, &*frame)?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if shared.stopping.load(Ordering::SeqCst) {
                    return Ok(());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

// ── Client ───────────────────────────────────────────────────────────────

/// The CLI's end of the pipe.
#[derive(Debug)]
pub struct AttachClient {
    stream: BufReader<Stream>,
}

impl AttachClient {
    /// Connects and verifies the model version with a `hello`.
    ///
    /// # Errors
    /// Nothing is listening under `name`; or the app speaks another model
    /// version, in which case the error says which side to upgrade.
    pub fn connect(name: &str) -> io::Result<(Self, Hello)> {
        let stream = Stream::connect(resolve(name)?)?;
        let mut client = Self {
            stream: BufReader::new(stream),
        };
        let hello = client.hello()?;
        if hello.model_version != vitals_core::MODEL_VERSION {
            return Err(ProtocolError::ModelVersion {
                theirs: hello.model_version,
                ours: vitals_core::MODEL_VERSION,
            }
            .into());
        }
        Ok((client, hello))
    }

    fn send(&mut self, request: Request) -> io::Result<()> {
        write_line(self.stream.get_mut(), &request)
    }

    fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.stream.read_line(&mut line)?;
            if read == 0 {
                return Ok(None);
            }
            if !line.trim().is_empty() {
                return Ok(Some(line));
            }
        }
    }

    fn read_frame(&mut self) -> io::Result<Option<Frame>> {
        let Some(line) = self.read_line()? else {
            return Ok(None);
        };
        match serde_json::from_str::<Frame>(&line) {
            Ok(frame) => Ok(Some(frame)),
            Err(frame_error) => {
                // Not a frame: either the server's error object or noise.
                // Report the server's words when it has some.
                if let Ok(reply) = serde_json::from_str::<ErrorReply>(&line) {
                    return Err(ProtocolError::Server(reply.message).into());
                }
                Err(ProtocolError::Malformed {
                    line: line.trim().to_owned(),
                    reason: frame_error,
                }
                .into())
            }
        }
    }

    /// # Errors
    /// The connection failed or the reply was not a `Hello`.
    pub fn hello(&mut self) -> io::Result<Hello> {
        self.send(Request::Hello)?;
        let line = self
            .read_line()?
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "closed before hello"))?;
        serde_json::from_str(&line).map_err(|reason| {
            ProtocolError::Malformed {
                line: line.trim().to_owned(),
                reason,
            }
            .into()
        })
    }

    /// One complete keyframe. Consumes the connection.
    ///
    /// # Errors
    /// The app had nothing complete within a few seconds (paused?), or the
    /// connection failed.
    pub fn snapshot(mut self) -> io::Result<Frame> {
        self.send(Request::Snapshot)?;
        self.read_frame()?
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "closed before a frame"))
    }

    /// Streams frames until the app goes away. Consumes the connection.
    ///
    /// # Errors
    /// The connection failed.
    pub fn subscribe(mut self) -> io::Result<FrameStream> {
        self.send(Request::Subscribe)?;
        Ok(FrameStream { client: self })
    }
}

/// Frames as they arrive. The first is the complete current state.
#[derive(Debug)]
pub struct FrameStream {
    client: AttachClient,
}

impl Iterator for FrameStream {
    type Item = io::Result<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        self.client.read_frame().transpose()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use vitals_core::fixtures;

    fn ok_ops(results: Vec<Result<Request, ProtocolError>>) -> Vec<Request> {
        results.into_iter().map(Result::unwrap).collect()
    }

    #[test]
    fn a_request_split_across_two_writes_is_assembled_and_yielded_once() {
        let mut decoder = Decoder::default();
        assert!(decoder.push(br#"{"op":"sub"#).is_empty());
        let got = ok_ops(decoder.push(b"scribe\"}\n"));
        assert_eq!(got, vec![Request::Subscribe]);
    }

    #[test]
    fn two_requests_in_one_write_are_both_yielded_in_order() {
        let mut decoder = Decoder::default();
        let got = ok_ops(decoder.push(b"{\"op\":\"hello\"}\r\n{\"op\":\"snapshot\"}\n"));
        assert_eq!(got, vec![Request::Hello, Request::Snapshot]);
    }

    #[test]
    fn a_garbage_line_is_an_error_not_a_panic_and_decoding_continues_after_it() {
        let mut decoder = Decoder::default();
        let got =
            decoder.push(b"not json at all\n{\"op\":\"kill-everything\"}\n{\"op\":\"hello\"}\n");
        assert_eq!(got.len(), 3);
        assert!(matches!(got[0], Err(ProtocolError::Malformed { .. })));
        assert!(matches!(got[1], Err(ProtocolError::Malformed { .. })));
        assert_eq!(*got[2].as_ref().unwrap(), Request::Hello);
    }

    #[test]
    fn requests_are_kebab_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&Request::Subscribe).unwrap(),
            r#"{"op":"subscribe"}"#
        );
        let hello = Hello {
            status: "ok".into(),
            version: "1".into(),
            model_version: 1,
        };
        assert!(
            serde_json::to_string(&hello)
                .unwrap()
                .contains("modelVersion")
        );
    }

    #[test]
    fn a_delta_before_any_keyframe_leaves_the_view_empty() {
        let delta = Frame {
            seq: FrameSeq(1),
            timestamp_ms: 1,
            elapsed_ms: 1000,
            payload: FramePayload::Delta {
                system: fixtures::system(),
                changed: vec![fixtures::process("x.exe", 1, 0.0)],
                exited: vec![],
            },
        };
        assert!(Materialised::fold(None, &delta).is_none());
    }

    #[test]
    fn exits_apply_before_changes_so_a_recycled_pid_survives_in_the_view() {
        let key = fixtures::keyframe(1, vec![fixtures::process("old.exe", 100, 1.0)]);
        let view = Materialised::fold(None, &key);
        let delta = Frame {
            seq: FrameSeq(2),
            timestamp_ms: 2,
            elapsed_ms: 1000,
            payload: FramePayload::Delta {
                system: fixtures::system(),
                changed: vec![fixtures::process("new.exe", 100, 2.0)],
                exited: vec![Pid(100)],
            },
        };
        let view = Materialised::fold(view, &delta).unwrap();
        let FramePayload::Keyframe { processes, .. } = view.to_frame().payload else {
            panic!("materialised view renders as a keyframe");
        };
        let names: Vec<_> = processes.into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["new.exe"]);
    }

    #[test]
    fn a_stalled_subscriber_is_handed_a_keyframe_rather_than_a_delta_it_cannot_apply() {
        let shared = Arc::new(Shared::default());
        let server = AttachServer {
            shared: Arc::clone(&shared),
            name: "unused".into(),
        };
        let rx = register(&shared);
        server.publish(&Arc::new(fixtures::keyframe(1, vec![])));
        // Fill the queue without draining it.
        for seq in 2..=(SUBSCRIBER_QUEUE as u64 + 1) {
            server.publish(&Arc::new(delta_for(seq)));
        }
        // One more: dropped, subscriber marked as lagging.
        server.publish(&Arc::new(delta_for(50)));
        // Drain what was queued, then publish a delta — the client must get
        // a keyframe in its place.
        while rx.try_recv().is_ok() {}
        server.publish(&Arc::new(delta_for(51)));
        let next = rx.try_recv().expect("a frame after the lag");
        assert!(
            next.is_keyframe(),
            "a lagged subscriber must be re-synchronised with a keyframe"
        );
        assert_eq!(next.seq, FrameSeq(51));
        // Prevent `Drop::stop` from trying to connect to a nonexistent pipe.
        shared.stopping.store(true, Ordering::SeqCst);
        drop(server);
    }

    fn delta_for(seq: u64) -> Frame {
        Frame {
            seq: FrameSeq(seq),
            timestamp_ms: seq,
            elapsed_ms: 1000,
            payload: FramePayload::Delta {
                system: fixtures::system(),
                changed: vec![],
                exited: vec![],
            },
        }
    }

    #[test]
    fn the_default_pipe_name_carries_only_safe_characters() {
        let name = default_pipe_name();
        assert!(name.starts_with("vitals-"));
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "{name}"
        );
    }
}
