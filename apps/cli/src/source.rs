//! Where the numbers come from: the running desktop app, or our own sampler.
//!
//! Attaching is preferred when the app is up. It is already paying for the
//! sampler, its rates have a baseline (a fresh sampler's first tick reads 0 %
//! for everything), and it owns the alert state — a second engine in the CLI
//! would disagree with the one on screen. Direct sampling is the fallback for
//! a machine where the app is not running, and the only mode for `serve`.

use std::time::Duration;

use anyhow::{Context, Result};
use vitals_core::alerts::Alert;
use vitals_core::ids::ProcessKey;
use vitals_core::provider::HostInfo;
use vitals_core::sample::Frame;
use vitals_server::ControlRequest;

use crate::client::Client;
use crate::discovery;
use crate::fold::View;
use crate::sse::Events;

/// How the caller asked to connect.
#[derive(Debug, Clone, Default)]
pub enum Preference {
    /// Attach if the desktop is found, else sample directly.
    #[default]
    Auto,
    /// Attach to exactly this base URL, or fail.
    Attach(String),
    /// Never attach.
    Direct,
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
    Attached(Box<Attached>),
    Direct(Box<Direct>),
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
    /// An explicit `--attach` that does not answer `/health`; or direct mode
    /// on a platform with no sampler.
    pub fn discover(preference: &Preference) -> Result<Self> {
        match preference {
            Preference::Attach(base) => Self::attach(&Client::new(base))
                .with_context(|| format!("no Vitals local API at {base}")),
            Preference::Direct => Direct::new().map(|d| Self::Direct(Box::new(d))),
            Preference::Auto => {
                let found = discovery::read(&discovery::discovery_path())
                    .map(|d| Client::new(&format!("http://127.0.0.1:{}", d.port)))
                    .and_then(|client| Self::attach(&client).ok());
                match found {
                    Some(source) => Ok(source),
                    None => Direct::new().map(|d| Self::Direct(Box::new(d))),
                }
            }
        }
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
            Self::Attached(_) => "attached",
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
            Self::Attached(a) => a.client.host(),
            Self::Direct(direct) => Ok(direct.host()),
        }
    }

    /// # Errors
    /// Connection failure.
    pub fn alerts(&mut self) -> Result<Vec<Alert>> {
        match self {
            Self::Attached(a) => a.client.alerts(),
            Self::Direct(direct) => Ok(direct.alerts()),
        }
    }

    /// Acts on a process: through the desktop's controller when attached, so
    /// the app's own risk checks apply; through the platform actions directly
    /// otherwise.
    ///
    /// # Errors
    /// The host refused (read-only scope, protected process, PID recycled),
    /// or this platform has no process backend.
    pub fn control(&mut self, request: ControlRequest) -> Result<()> {
        match self {
            Self::Attached(a) => a.client.control(&request),
            Self::Direct(direct) => direct.control(request),
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
            Self::Direct(_) => true,
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

    #[allow(clippy::unused_self)]
    fn control(&self, request: ControlRequest) -> Result<()> {
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
    pub fn tick(&mut self) -> Result<Frame> {
        anyhow::bail!("no sampler on this platform")
    }

    fn snapshot(&mut self) -> Result<View> {
        anyhow::bail!("no sampler on this platform")
    }

    #[allow(clippy::unused_self)]
    fn host(&self) -> Option<HostInfo> {
        None
    }

    #[allow(clippy::unused_self)]
    fn alerts(&self) -> Vec<Alert> {
        Vec::new()
    }

    #[allow(clippy::unused_self, clippy::needless_pass_by_value)]
    fn control(&self, _request: ControlRequest) -> Result<()> {
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
