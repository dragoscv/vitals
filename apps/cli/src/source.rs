//! Where the numbers come from: the running desktop app (over its named pipe
//! or its loopback HTTP API), or our own sampler.
//!
//! Attaching is preferred when the app is up. It is already paying for the
//! sampler, its rates have a baseline (a fresh sampler's first tick reads 0 %
//! for everything), and it owns the alert state — a second engine in the CLI
//! would disagree with the one on screen. Direct sampling is the fallback for
//! a machine where the app is not running, and the only mode for `serve`.
//!
//! Of the two ways to attach, the pipe is tried first: it needs no discovery
//! file, no port, no HTTP, and cannot be reached from another machine. The
//! loopback API remains for `--attach <url>` and for a desktop build that
//! predates the pipe.

use std::io::IsTerminal;
use std::time::Duration;
// Only the Windows sensor cache below needs an Instant, and its two platforms
// that do not have one compile this file too: CI's macOS and Linux clippy jobs
// (both `-D warnings`) refuse an import no live code path uses.
#[cfg(windows)]
use std::time::Instant;

use anyhow::{Context, Result};
use vitals_core::MachineSample;
use vitals_core::alerts::Alert;
use vitals_core::ids::ProcessKey;
use vitals_core::provider::HostInfo;
use vitals_core::remote::SensorLine;
use vitals_core::sample::Frame;
use vitals_ipc::attach::{AttachClient, FrameStream, ProtocolError, WidgetAction, WidgetsReply};
use vitals_server::ControlRequest;

use crate::client::Client;
use crate::discovery;
use crate::exit;
use crate::fold::View;
use crate::sse::Events;

/// How the caller asked to connect.
#[derive(Debug, Clone, Default)]
pub enum Preference {
    /// Attach if the desktop is found (pipe, then loopback API), else sample
    /// directly.
    #[default]
    Auto,
    /// Attach to the running app over its pipe, or fail.
    App,
    /// Attach to exactly this base URL, or fail.
    Attach(String),
    /// Never attach.
    Direct,
}

/// A connection to the desktop over its per-user named pipe.
pub struct Piped {
    pipe: String,
    version: String,
    /// Opened on first use by [`Source::next_frame`]; one connection per
    /// `top` session.
    frames: Option<FrameStream>,
    view: View,
    /// The pipe carries frames only. Alerts are re-derived here from the
    /// same system metrics the app evaluates, with the same engine, so the
    /// count `top` shows matches the app's once both have seen the same
    /// history. Not identical from the first tick — the app has been
    /// watching longer — but honest about what it has seen.
    engine: vitals_core::alerts::Engine,
    /// The desktop's loopback API, when it advertises one.
    ///
    /// The pipe carries frames and nothing else: sensors and history live
    /// behind HTTP, and a caller that wants them (the MCP tools) should not
    /// have to know which transport happened to win.
    http: Option<Client>,
}

/// A connection to the desktop's loopback API.
pub struct Attached {
    client: Client,
    version: String,
    /// Opened on first use by [`Source::next_frame`]. Kept across calls so a
    /// `top` session is one HTTP request, not one per tick.
    events: Option<Events<Box<dyn std::io::Read + Send + Sync>>>,
    view: View,
}

/// Both variants are boxed: a `SystemMetrics` alone is ~700 bytes and the
/// sampler is several kilobytes of baselines; the enum is passed around by
/// value and should stay a pointer.
pub enum Source {
    Piped(Box<Piped>),
    Attached(Box<Attached>),
    Direct(Box<Direct>),
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Piped(p) => f
                .debug_struct("Piped")
                .field("pipe", &p.pipe)
                .field("version", &p.version)
                .finish_non_exhaustive(),
            Self::Attached(a) => f
                .debug_struct("Attached")
                .field("base", &a.client.base())
                .field("version", &a.version)
                .finish_non_exhaustive(),
            Self::Direct(_) => f.write_str("Direct"),
        }
    }
}

impl Source {
    /// Resolves the preference against the machine.
    ///
    /// # Errors
    /// An explicit `--source app` with no app listening, an explicit
    /// `--attach` that does not answer `/health`; or direct mode
    /// on a platform with no sampler.
    pub fn discover(preference: &Preference) -> Result<Self> {
        match preference {
            Preference::App => Self::attach_pipe(&vitals_ipc::attach::default_pipe_name())
                .context("Vitals is not running (no attach pipe for this user)"),
            Preference::Attach(base) => Self::attach(&Client::new(base))
                .with_context(|| format!("no Vitals local API at {base}")),
            Preference::Direct => Direct::new().map(|d| Self::Direct(Box::new(d))),
            Preference::Auto => {
                if let Ok(source) = Self::attach_pipe(&vitals_ipc::attach::default_pipe_name()) {
                    return Ok(source);
                }
                let found = discovery::read(&discovery::discovery_path())
                    .map(|d| Client::new(&format!("http://127.0.0.1:{}", d.port)))
                    .and_then(|client| Self::attach(&client).ok());
                if let Some(source) = found {
                    return Ok(source);
                }
                // Dim, on stderr, once: the numbers are still right, but they
                // come from a cold sampler and the user should know why the
                // first tick reads 0 %.
                //
                // Styled only when that is allowed. A redirected run
                // (`vitals ps --json > file`, a CI log) must not collect
                // escape bytes, and `NO_COLOR` is the convention the rest of
                // these tools follow.
                eprintln!("{}", sampling_note(note_is_styled()));
                Direct::new().map(|d| Self::Direct(Box::new(d)))
            }
        }
    }

    fn attach_pipe(name: &str) -> Result<Self> {
        let (_client, hello) = AttachClient::connect(name)?;
        // The handshake connection is dropped here on purpose: `snapshot`
        // and `subscribe` each consume a connection, and holding one open
        // that we may never use would keep the app cloning frames for us.
        Ok(Self::Piped(Box::new(Piped {
            pipe: name.to_owned(),
            version: hello.version,
            frames: None,
            view: View::default(),
            engine: vitals_core::alerts::Engine::new(),
            http: discovery::read(&discovery::discovery_path())
                .map(|found| Client::new(&format!("http://127.0.0.1:{}", found.port))),
        })))
    }

    fn attach(client: &Client) -> Result<Self> {
        let health = client.health()?;
        anyhow::ensure!(health.ok, "{} reports not ok", client.base());
        anyhow::ensure!(
            health.model_version == vitals_core::MODEL_VERSION,
            "{} speaks model version {} but this CLI expects {}; upgrade one of them",
            client.base(),
            health.model_version,
            vitals_core::MODEL_VERSION
        );
        Ok(Self::Attached(Box::new(Attached {
            client: client.clone(),
            version: health.version,
            events: None,
            view: View::default(),
        })))
    }

    /// One line for stderr saying where the numbers come from.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Piped(p) => format!("source: attached to Vitals {} over {}", p.version, p.pipe),
            Self::Attached(a) => {
                let port = a
                    .client
                    .base()
                    .rsplit_once(':')
                    .map_or("", |(_, port)| port);
                format!("source: attached to Vitals {} on :{port}", a.version)
            }
            Self::Direct(_) => "source: sampling directly".to_owned(),
        }
    }

    /// A short tag for JSON output.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Piped(_) | Self::Attached(_) => "attached",
            Self::Direct(_) => "direct",
        }
    }

    /// The complete current state, for one-shot commands.
    ///
    /// # Errors
    /// The server answered 204 (not sampling yet) or failed; or the sampler
    /// failed.
    pub fn snapshot(&mut self) -> Result<View> {
        match self {
            Self::Piped(p) => {
                let (client, _) = AttachClient::connect(&p.pipe)?;
                let frame = client
                    .snapshot()
                    .context("Vitals is running but had no complete frame to give")?;
                p.engine.poll(frame.system());
                let mut view = View::default();
                view.apply(&frame);
                Ok(view)
            }
            Self::Attached(a) => {
                let frame = a
                    .client
                    .snapshot()?
                    .context("Vitals is running but has not sampled yet; try again in a second")?;
                let mut view = View::default();
                view.apply(&frame);
                Ok(view)
            }
            Self::Direct(direct) => direct.snapshot(),
        }
    }

    /// The next frame, folded into the running view, which is returned.
    ///
    /// In attached mode `interval` is ignored — the server sets the rate. In
    /// direct mode the call sleeps for `interval` before sampling so that
    /// rates are differenced over a real gap.
    ///
    /// # Errors
    /// Stream closed or unparseable; sampler failure.
    pub fn next_frame(&mut self, interval: Duration) -> Result<Option<&View>> {
        match self {
            Self::Piped(p) => {
                let Piped {
                    pipe,
                    frames,
                    view,
                    engine,
                    ..
                } = p.as_mut();
                if frames.is_none() {
                    let (client, _) = AttachClient::connect(pipe)?;
                    *frames = Some(client.subscribe()?);
                }
                let Some(stream) = frames.as_mut() else {
                    return Ok(None);
                };
                loop {
                    let Some(frame) = stream.next() else {
                        return Ok(None);
                    };
                    let frame = frame.context("reading the attach pipe")?;
                    engine.poll(frame.system());
                    if view.apply(&frame) {
                        return Ok(Some(view));
                    }
                    // A delta before the keyframe: keep reading.
                }
            }
            Self::Attached(a) => {
                let Attached {
                    client,
                    events,
                    view,
                    ..
                } = a.as_mut();
                let stream = match events {
                    Some(stream) => stream,
                    None => events.insert(Events::new(client.stream()?)),
                };
                loop {
                    let Some(data) = stream.next() else {
                        return Ok(None);
                    };
                    let data = data.context("reading /api/v1/stream")?;
                    let frame: Frame = serde_json::from_str(&data)
                        .context("a frame on the stream did not parse")?;
                    if view.apply(&frame) {
                        return Ok(Some(view));
                    }
                    // A delta before the keyframe: keep reading.
                }
            }
            Self::Direct(direct) => {
                std::thread::sleep(interval);
                direct.tick()?;
                Ok(Some(&direct.view))
            }
        }
    }

    /// # Errors
    /// Connection failure.
    pub fn host(&mut self) -> Result<Option<HostInfo>> {
        match self {
            // Same machine: the platform answers directly, as it does in
            // direct mode. The pipe does not carry host info.
            Self::Piped(_) => Ok(Direct::local_host()),
            Self::Attached(a) => a.client.host(),
            Self::Direct(direct) => Ok(direct.host()),
        }
    }

    /// # Errors
    /// Connection failure.
    pub fn alerts(&mut self) -> Result<Vec<Alert>> {
        match self {
            Self::Piped(p) => Ok(p.engine.active()),
            Self::Attached(a) => a.client.alerts(),
            Self::Direct(direct) => Ok(direct.alerts()),
        }
    }

    /// Every sensor reading the host can make.
    ///
    /// An attached desktop is asked over HTTP, because it answers for *its*
    /// machine — which may not be this one. Every other source means the
    /// machine under the CLI's own feet, so the readings are taken here: the
    /// pipe carries frames and nothing else, and a direct sampler is this
    /// same machine. [`local_sensor_lines`] does the reading, through the
    /// platform module the desktop uses, so both surfaces report one set of
    /// numbers rather than two opinions about the same service.
    ///
    /// # Errors
    /// The attached host refused, or this platform has no sensor backend. The
    /// message reaches a user as an MCP tool result, so it is written for one.
    pub fn sensors(&self) -> Result<Vec<SensorLine>> {
        match self {
            Self::Attached(a) => a.client.sensors(),
            Self::Piped(_) | Self::Direct(_) => local_sensor_lines(),
        }
    }

    /// Machine-wide samples covering the last `seconds`.
    ///
    /// Same shape as [`Source::sensors`]: the store belongs to the desktop,
    /// so a direct sampler has no history to give and says so.
    ///
    /// # Errors
    /// The source cannot answer; see [`Source::sensors`].
    pub fn history(&self, seconds: u32) -> Result<Vec<MachineSample>> {
        match self {
            Self::Piped(p) => match &p.http {
                Some(client) => client.history(seconds),
                None => Err(anyhow::anyhow!(
                    "history is recorded by the desktop app, whose local API is not \
                     advertising itself; run `vitals mcp --attach http://127.0.0.1:<port>`"
                )),
            },
            Self::Attached(a) => a.client.history(seconds),
            Self::Direct(_) => Err(anyhow::anyhow!(
                "history is recorded by the desktop app's store, and this CLI is \
                 sampling the machine directly"
            )),
        }
    }

    /// Acts on a process: through a remote desktop's controller when attached
    /// over HTTP to another machine; through the platform actions directly
    /// on this one. The pipe is read-only by design, and the local API grants
    /// a tokenless caller `read` only — loopback TCP is reachable from every
    /// account on the PC — so on the same machine the CLI acts with its own
    /// rights, which Windows enforces per user.
    ///
    /// # Errors
    /// The host refused (read-only scope, protected process, PID recycled),
    /// or this platform has no process backend.
    pub fn control(&mut self, request: ControlRequest) -> Result<()> {
        if self.is_local() {
            return Direct::local_control(request);
        }
        match self {
            Self::Attached(a) => a.client.control(&request),
            Self::Piped(_) | Self::Direct(_) => Direct::local_control(request),
        }
    }

    /// Whether the processes this source describes run on *this* machine, so
    /// a platform call by PID here means the same process it means there.
    /// True for direct sampling and for an attachment to a loopback address;
    /// false for `--attach http://some-other-host`.
    #[must_use]
    pub fn is_local(&self) -> bool {
        match self {
            Self::Attached(a) => {
                let base = a.client.base();
                base.contains("127.0.0.1") || base.contains("localhost") || base.contains("[::1]")
            }
            Self::Piped(_) | Self::Direct(_) => true,
        }
    }

    /// Reads a process's efficiency-mode state off the local kernel. `None`
    /// when it cannot be read — the source is remote, the platform has no
    /// backend, or the process denies a handle — never a guessed `false`.
    ///
    /// # Errors
    /// The process has exited or its PID was reused.
    pub fn efficiency_mode(&self, key: ProcessKey) -> Result<Option<bool>> {
        if !self.is_local() {
            return Ok(None);
        }
        Direct::efficiency_mode(key)
    }

    /// Runs one widget action against the desktop over the attach pipe and
    /// returns its reply.
    ///
    /// Unlike [`Source::next_frame`], this is a request/response exchange:
    /// the caller gets the widgets the desktop would draw, or a refusal, and
    /// nothing left to drain. Only the pipe carries it — the local HTTP API
    /// answers for another machine and does not expose widget actions, and a
    /// direct sampler has no widgets at all.
    ///
    /// The two failures a caller must tell apart are separated here: a
    /// refusal (`UNSUPPORTED`, 6) comes from an app that is running and
    /// answered — an older build, or a host with no widget support — while a
    /// missing app (`LINK_FAILURE`, 3) means the attach was never made.
    ///
    /// # Errors
    /// The source cannot carry a widget action, Vitals is not running, or the
    /// desktop refused the action — reported to the user with the app's own
    /// words.
    pub fn widget(&mut self, action: WidgetAction) -> Result<WidgetsReply> {
        let kind = self.kind();
        let pipe = match self {
            Self::Piped(p) => p.pipe.clone(),
            Self::Attached(_) | Self::Direct(_) => {
                return Err(exit::unsupported(anyhow::anyhow!(
                    "widgets are windows, and only the attach pipe reaches the desktop \
                     that owns them; this source is {kind}"
                )));
            }
        };
        let (client, _) = AttachClient::connect(&pipe).map_err(|error| {
            exit::link_failure(anyhow::Error::new(error).context("Vitals is not running"))
        })?;
        client
            .widget(action)
            .map_err(|error| match protocol_cause(&error) {
                // The desktop heard the request and would not carry it out: an
                // older build that does not understand the message, no widget
                // host registered, or two ends that disagree on the version.
                // The sentence comes before the app's own words, which are
                // kept in the chain: an older app answers with a parser error
                // quoting the request, and that alone does not read as "your
                // app is too old".
                Some(_) => exit::unsupported(
                    anyhow::Error::new(error)
                        .context("the app did not carry the widget action out"),
                ),
                // Nothing on the other end of the pipe: closed, or never there.
                None => exit::link_failure(
                    anyhow::Error::new(error)
                        .context("sending a widget action over the attach pipe"),
                ),
            })
    }
}

/// The attach-protocol failure an I/O error is carrying, when it carries one.
///
/// `AttachClient` reports a reply it could not use as an `io::Error` with the
/// [`ProtocolError`] as its source, because that is the type every transport
/// call returns. Reading it back is what separates "the app answered and
/// refused" from "the pipe broke".
fn protocol_cause(error: &std::io::Error) -> Option<&ProtocolError> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ProtocolError>())
}

// ── Direct sampling ────────────────────────────────────────────────────

/// Our own sampler, frame builder and alert engine.
#[cfg(windows)]
pub struct Direct {
    sampler: vitals_win::SystemSampler,
    frames: vitals_win::FrameBuilder,
    engine: vitals_core::alerts::Engine,
    view: View,
}

#[cfg(windows)]
impl Direct {
    /// # Errors
    /// Never on Windows; the signature matches the other platforms so
    /// callers do not need a `cfg`.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new() -> Result<Self> {
        Ok(Self {
            sampler: vitals_win::SystemSampler::new(),
            frames: vitals_win::FrameBuilder::new(),
            engine: vitals_core::alerts::Engine::new(),
            view: View::default(),
        })
    }

    /// Samples once and folds the frame in. Returns the frame, which `serve`
    /// publishes.
    ///
    /// # Errors
    /// The platform sampler failed.
    pub fn tick(&mut self) -> Result<Frame> {
        let sample = self.sampler.sample().context("sampling the system")?;
        let frame = self.frames.build(sample);
        self.engine.poll(frame.system());
        self.view.apply(&frame);
        Ok(frame)
    }

    /// Two ticks a short interval apart, so the rates are real.
    ///
    /// The first sample only primes baselines and reports 0 % CPU for every
    /// process — honest, but useless as an answer to "what is busy".
    fn snapshot(&mut self) -> Result<View> {
        self.tick()?;
        std::thread::sleep(Duration::from_millis(500));
        self.tick()?;
        Ok(self.view.clone())
    }

    #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
    fn host(&self) -> Option<HostInfo> {
        Self::local_host()
    }

    #[allow(clippy::unnecessary_wraps)]
    fn local_host() -> Option<HostInfo> {
        Some(vitals_win::hostinfo::read())
    }

    /// Alerts the engine currently holds raised.
    #[must_use]
    pub fn active_alerts(&self) -> Vec<Alert> {
        self.engine.active()
    }

    fn alerts(&self) -> Vec<Alert> {
        self.active_alerts()
    }

    fn local_control(request: ControlRequest) -> Result<()> {
        use vitals_win::actions;
        // Never `Consent::Confirmed`: a script cannot see the risk dialog the
        // desktop shows, so a critical or protected process is refused here
        // as it is for a phone.
        let outcome = match request {
            ControlRequest::Terminate { key } => {
                actions::terminate(key, 1, actions::Consent::Unconfirmed)
            }
            ControlRequest::Suspend { key } => actions::suspend(key, actions::Consent::Unconfirmed),
            ControlRequest::Resume { key } => actions::resume(key),
            ControlRequest::SetPriority { key, priority } => {
                let level = parse_priority(&priority)
                    .with_context(|| format!("unknown priority {priority:?}"))?;
                actions::set_priority(key, level)
            }
            ControlRequest::SetEfficiencyMode { key, enabled } => {
                actions::set_efficiency_mode(key, enabled)
            }
        };
        outcome.map_err(|error| match error {
            vitals_core::Error::Refused(_) => anyhow::anyhow!(
                "refused: ending or freezing this process could crash Windows; \
                 use the desktop app, which asks first"
            ),
            vitals_core::Error::AccessDenied { .. } => anyhow::anyhow!(
                "access denied: the process belongs to another user or is protected; \
                 try again from an elevated prompt"
            ),
            other => other.into(),
        })
    }

    fn efficiency_mode(key: ProcessKey) -> Result<Option<bool>> {
        vitals_win::actions::efficiency_mode(key).map_err(Into::into)
    }
}

/// The control endpoint's priority spelling, parsed the way the desktop's
/// controller parses it. Locally `realtime` is allowed: the person typing
/// asked for it by name, which is the warning a phone never gets.
#[cfg(windows)]
fn parse_priority(value: &str) -> Option<vitals_win::Priority> {
    use vitals_win::Priority as P;
    Some(match value {
        "idle" => P::Idle,
        "below-normal" => P::BelowNormal,
        "normal" => P::Normal,
        "above-normal" => P::AboveNormal,
        "high" => P::High,
        "realtime" => P::Realtime,
        _ => return None,
    })
}

/// macOS: the same shape as the Windows sampler, over `vitals-macos`.
#[cfg(target_os = "macos")]
pub struct Direct {
    sampler: vitals_macos::SystemSampler,
    frames: vitals_macos::FrameBuilder,
    engine: vitals_core::alerts::Engine,
    view: View,
}

#[cfg(target_os = "macos")]
impl Direct {
    /// # Errors
    /// Never; the signature matches the other platforms.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new() -> Result<Self> {
        Ok(Self {
            sampler: vitals_macos::SystemSampler::new(),
            frames: vitals_macos::FrameBuilder::new(),
            engine: vitals_core::alerts::Engine::new(),
            view: View::default(),
        })
    }

    /// # Errors
    /// The platform sampler failed.
    pub fn tick(&mut self) -> Result<Frame> {
        let sample = self.sampler.sample().context("sampling the system")?;
        let frame = self.frames.build(sample);
        self.engine.poll(frame.system());
        self.view.apply(&frame);
        Ok(frame)
    }

    fn snapshot(&mut self) -> Result<View> {
        self.tick()?;
        std::thread::sleep(Duration::from_millis(500));
        self.tick()?;
        Ok(self.view.clone())
    }

    #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
    fn host(&self) -> Option<HostInfo> {
        Self::local_host()
    }

    #[allow(clippy::unnecessary_wraps)]
    fn local_host() -> Option<HostInfo> {
        Some(vitals_macos::host_info())
    }

    /// Alerts the engine currently holds raised.
    #[must_use]
    pub fn active_alerts(&self) -> Vec<Alert> {
        self.engine.active()
    }

    fn alerts(&self) -> Vec<Alert> {
        self.active_alerts()
    }

    fn local_control(request: ControlRequest) -> Result<()> {
        use vitals_core::provider::Priority as P;
        let outcome = match request {
            ControlRequest::Terminate { key } => vitals_macos::terminate(key),
            ControlRequest::Suspend { key } => vitals_macos::suspend(key),
            ControlRequest::Resume { key } => vitals_macos::resume(key),
            ControlRequest::SetPriority { key, priority } => {
                let level = match priority.as_str() {
                    "idle" => P::Idle,
                    "below-normal" => P::BelowNormal,
                    "normal" => P::Normal,
                    "above-normal" => P::AboveNormal,
                    "high" => P::High,
                    "realtime" => P::Realtime,
                    other => anyhow::bail!("unknown priority {other:?}"),
                };
                vitals_macos::set_priority(key, level)
            }
            ControlRequest::SetEfficiencyMode { .. } => {
                anyhow::bail!("efficiency mode is a Windows feature")
            }
        };
        outcome.map_err(|error| match error {
            vitals_core::Error::Refused(reason) => anyhow::anyhow!("refused: {reason}"),
            vitals_core::Error::AccessDenied { .. } => anyhow::anyhow!(
                "access denied: the process belongs to another user; try again with sudo"
            ),
            other => other.into(),
        })
    }

    #[allow(clippy::unnecessary_wraps)]
    fn efficiency_mode(_key: ProcessKey) -> Result<Option<bool>> {
        Ok(None)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub struct Direct {
    view: View,
}

#[cfg(not(any(windows, target_os = "macos")))]
impl Direct {
    /// # Errors
    /// Always: there is no sampler on this platform yet.
    pub fn new() -> Result<Self> {
        anyhow::bail!(
            "direct sampling is only available on Windows for now; \
             point the CLI at a running Vitals with --attach http://host:port"
        )
    }

    /// # Errors
    /// Unreachable: `new` never returns a value on this platform.
    // Same signature as the Windows sampler so callers compile unchanged.
    #[allow(clippy::unused_self)]
    pub fn tick(&mut self) -> Result<Frame> {
        anyhow::bail!("no sampler on this platform")
    }

    #[allow(clippy::unused_self)]
    fn snapshot(&mut self) -> Result<View> {
        anyhow::bail!("no sampler on this platform")
    }

    #[allow(clippy::unused_self)]
    fn host(&self) -> Option<HostInfo> {
        None
    }

    fn local_host() -> Option<HostInfo> {
        None
    }

    #[allow(clippy::unused_self)]
    fn alerts(&self) -> Vec<Alert> {
        Vec::new()
    }

    #[allow(clippy::needless_pass_by_value)]
    fn local_control(_request: ControlRequest) -> Result<()> {
        anyhow::bail!("no process backend on this platform")
    }

    #[allow(clippy::unnecessary_wraps)]
    fn efficiency_mode(_key: ProcessKey) -> Result<Option<bool>> {
        Ok(None)
    }
}

impl std::fmt::Debug for Direct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Direct")
            .field("primed", &self.view.is_primed())
            .finish_non_exhaustive()
    }
}

/// Whether the note above may carry ANSI styling.
///
/// Pure so the decision is testable without a terminal and without touching a
/// process-wide environment variable: `NO_COLOR` set to anything at all turns
/// the styling off (the convention <https://no-color.org> defines), and so
/// does an `stderr` that is not a terminal — a redirected run must not write
/// `\x1b[2m` into a log file.
fn style_enabled(no_color: bool, stderr_is_terminal: bool) -> bool {
    !no_color && stderr_is_terminal
}

fn note_is_styled() -> bool {
    style_enabled(
        std::env::var_os("NO_COLOR").is_some(),
        std::io::stderr().is_terminal(),
    )
}

/// The note itself, its escape bytes present only when styling is allowed.
///
/// Split from [`note_is_styled`] so the exact bytes a redirected run would
/// have written can be asserted: `style_enabled` decides, and this shows what
/// that decision produces.
const fn sampling_note(styled: bool) -> &'static str {
    if styled {
        "\x1b[2m(sampling directly — Vitals is not running)\x1b[0m"
    } else {
        "(sampling directly — Vitals is not running)"
    }
}

/// How long a local sensor read is reused.
///
/// One read is a WMI round trip plus a walk of the sensor service's pipe —
/// 48 ms measured on the machine this was written on — so an agent that calls
/// the `sensors` tool twice inside a second must not pay for two. The desktop
/// caches its own LAN reads for the same reason and for the same five
/// seconds, so a phone and a CLI polling at once cost one read between them.
#[cfg(windows)]
const SENSOR_TTL: Duration = Duration::from_secs(5);

/// Every sensor reading this machine can make now.
///
/// The same call the desktop's `/api/v1/sensors` makes, through the same
/// platform module, so the two surfaces cannot disagree about the hardware.
///
/// # Errors
/// This platform has no sensor backend yet.
#[allow(clippy::unnecessary_wraps)]
// One shape for two platforms: the
// non-Windows branch below must fail, so this branch returns
// the same type and the caller keeps a single call site.
#[cfg(windows)]
fn local_sensor_lines() -> Result<Vec<SensorLine>> {
    static CACHE: std::sync::Mutex<Option<(Instant, Vec<SensorLine>)>> =
        std::sync::Mutex::new(None);
    Ok(cached_read(Instant::now(), SENSOR_TTL, &CACHE, || {
        vitals_win::sensors::read_all()
            .readings
            .iter()
            .map(vitals_win::sensors::SensorReading::to_line)
            .collect()
    }))
}

/// # Errors
/// Always: no sensor backend exists below Windows in this CLI.
#[cfg(not(windows))]
fn local_sensor_lines() -> Result<Vec<SensorLine>> {
    anyhow::bail!("sensor readings are only available on Windows")
}

/// A reading reused for `ttl`, taken again otherwise.
///
/// Split out from the read so the caching rule can be asserted without a
/// machine: a caller inside the window must not read, and one outside it
/// must. An earlier `now` than the stored instant counts as outside — a
/// monitor must never serve a reading its caller's clock says is in the
/// future, and `Instant` subtraction would panic rather than say so.
#[cfg(windows)]
fn cached_read<T: Clone>(
    now: Instant,
    ttl: Duration,
    cache: &std::sync::Mutex<Option<(Instant, T)>>,
    read: impl FnOnce() -> T,
) -> T {
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, value)) = guard.as_ref()
        && now
            .checked_duration_since(*at)
            .is_some_and(|elapsed| elapsed < ttl)
    {
        return value.clone();
    }
    let value = read();
    *guard = Some((now, value.clone()));
    value
}

#[cfg(all(test, windows))]
mod cache_tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use super::{Duration, SENSOR_TTL, cached_read};

    #[test]
    fn a_reading_taken_inside_the_window_is_reused_and_a_later_one_is_not() {
        let cache: Mutex<Option<(Instant, u32)>> = Mutex::new(None);
        let reads = AtomicUsize::new(0);
        let start = Instant::now();
        let read = || {
            reads.fetch_add(1, Ordering::SeqCst);
            7
        };

        assert_eq!(cached_read(start, SENSOR_TTL, &cache, read), 7);
        assert_eq!(
            cached_read(start + Duration::from_secs(1), SENSOR_TTL, &cache, read),
            7
        );
        assert_eq!(
            reads.load(Ordering::SeqCst),
            1,
            "the second call must be served from the cache"
        );

        assert_eq!(
            cached_read(start + Duration::from_secs(5), SENSOR_TTL, &cache, read),
            7
        );
        assert_eq!(
            reads.load(Ordering::SeqCst),
            2,
            "a call at the end of the window is a new read"
        );
    }

    #[test]
    fn a_call_whose_clock_is_behind_the_cached_read_takes_a_fresh_one_instead_of_panicking() {
        // The tool can be called at any moment, and `Instant` subtraction
        // panics when the subtrahend is later: a panic inside a monitor is a
        // far worse answer than one extra WMI read.
        let cache: Mutex<Option<(Instant, u32)>> = Mutex::new(None);
        let start = Instant::now();
        assert_eq!(cached_read(start, SENSOR_TTL, &cache, || 1), 1);
        // checked_sub, not `-`: the point of the test is a clock that has gone
        // backwards, and expressing it with an operator that can panic would
        // make the test panic in the very case it exists to rule out.
        let behind = start
            .checked_sub(Duration::from_secs(30))
            .expect("thirty seconds before an Instant taken a moment ago exists on every platform");
        assert_eq!(cached_read(behind, SENSOR_TTL, &cache, || 2), 2);
    }
}

#[cfg(test)]
mod tests {
    use super::{sampling_note, style_enabled};

    #[test]
    fn the_direct_sampling_note_is_styled_only_on_a_terminal_without_no_color() {
        assert!(style_enabled(false, true));
        assert!(
            !style_enabled(true, true),
            "NO_COLOR must win even when stderr is a terminal"
        );
        assert!(
            !style_enabled(false, false),
            "a redirected stream must not collect escape bytes"
        );
    }

    #[test]
    fn the_note_carries_escape_bytes_exactly_when_styling_is_on() {
        // Case 1: a terminal with no NO_COLOR — styled, so the dim escape is
        // present and the reset closes it.
        assert!(style_enabled(false, true));
        assert!(sampling_note(true).starts_with("\x1b[2m"));
        assert!(sampling_note(true).ends_with("\x1b[0m"));
        // Case 2: NO_COLOR set, even with stderr a terminal — plain.
        assert!(!style_enabled(true, true));
        assert!(!sampling_note(false).contains('\u{1b}'));
        // Case 3: stderr a pipe — plain, so a redirected run writes no escapes.
        assert!(!style_enabled(false, false));
        assert!(!sampling_note(false).contains('\u{1b}'));
    }
}
