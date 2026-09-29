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

use std::time::Duration;

use anyhow::{Context, Result};
use vitals_core::alerts::Alert;
use vitals_core::ids::ProcessKey;
use vitals_core::provider::HostInfo;
use vitals_core::sample::Frame;
use vitals_ipc::attach::{AttachClient, FrameStream};
use vitals_server::ControlRequest;

use crate::client::Client;
use crate::discovery;
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
                eprintln!("\x1b[2m(sampling directly — Vitals is not running)\x1b[0m");
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
        match request {
            ControlRequest::SetEfficiencyMode { key, enabled } => {
                actions::set_efficiency_mode(key, enabled).map_err(Into::into)
            }
            other => anyhow::bail!("the CLI does not issue {other:?} directly"),
        }
    }

    fn efficiency_mode(key: ProcessKey) -> Result<Option<bool>> {
        vitals_win::actions::efficiency_mode(key).map_err(Into::into)
    }
}

#[cfg(not(windows))]
pub struct Direct {
    view: View,
}

#[cfg(not(windows))]
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
