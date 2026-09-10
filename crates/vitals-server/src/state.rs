//! What the router is handed by its host.

use std::sync::Arc;

use parking_lot_shim::RwLock;
use tokio::sync::broadcast;
use vitals_core::provider::HostInfo;
use vitals_core::sample::Frame;

use crate::auth::TokenSet;
use crate::control::Controller;

/// Where frames come from.
///
/// The host pushes into [`FrameSource::publish`] from wherever it samples; the
/// server broadcasts to SSE and WebSocket subscribers and keeps the last one
/// so a client that has just connected gets an answer immediately rather than
/// waiting up to two seconds for the next tick.
#[derive(Debug, Clone)]
pub struct FrameSource {
    latest: Arc<RwLock<Option<Arc<Frame>>>>,
    tx: broadcast::Sender<Arc<Frame>>,
}

impl Default for FrameSource {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameSource {
    #[must_use]
    pub fn new() -> Self {
        // A slow client must never stall the sampler. `broadcast` drops the
        // oldest for a lagging receiver and tells it how many it missed,
        // which is exactly the right trade for live metrics: a phone that
        // fell behind wants the current numbers, not a backlog.
        let (tx, _) = broadcast::channel(16);
        Self {
            latest: Arc::new(RwLock::new(None)),
            tx,
        }
    }

    /// Publishes a frame. Cheap and non-blocking, even with no subscribers.
    pub fn publish(&self, frame: Arc<Frame>) {
        *self.latest.write() = Some(Arc::clone(&frame));
        // `Err` means nobody is listening, which is the normal case.
        let _ = self.tx.send(frame);
    }

    #[must_use]
    pub fn latest(&self) -> Option<Arc<Frame>> {
        self.latest.read().clone()
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.tx.subscribe()
    }
}

/// The static files served to a phone.
///
/// A closure rather than a directory path because the desktop app already has
/// the built assets embedded in the binary (Tauri's `frontendDist`), and
/// serving them from disk would mean shipping a second copy. The CLI passes a
/// closure that reads from a directory.
pub type StaticAssets = Arc<dyn Fn(&str) -> Option<(Vec<u8>, &'static str)> + Send + Sync>;

/// Everything a request handler can reach.
#[derive(Clone)]
pub struct ApiState {
    pub frames: FrameSource,
    pub tokens: Arc<RwLock<TokenSet>>,
    pub controller: Arc<dyn Controller>,
    pub assets: Option<StaticAssets>,
    pub host: Arc<dyn Fn() -> Option<HostInfo> + Send + Sync>,
    /// Shown at `/api/v1/health` so a client can tell which build it is
    /// talking to before trusting the shape of anything else.
    pub version: String,
}

impl std::fmt::Debug for ApiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiState")
            .field("version", &self.version)
            .field("has_assets", &self.assets.is_some())
            .finish_non_exhaustive()
    }
}

/// `parking_lot` is not a dependency of this crate; `std` is enough for locks
/// held for the length of a clone.
mod parking_lot_shim {
    pub use std::sync::RwLock as StdRwLock;

    /// A thin wrapper that ignores poisoning.
    ///
    /// A panic while holding one of these locks cannot corrupt an invariant —
    /// the contents are a token list and an `Option<Arc<Frame>>` — and a
    /// poisoned lock would take the LAN server down for the rest of the
    /// session over an unrelated failure.
    #[derive(Debug, Default)]
    pub struct RwLock<T>(StdRwLock<T>);

    impl<T> RwLock<T> {
        pub const fn new(value: T) -> Self {
            Self(StdRwLock::new(value))
        }

        pub fn read(&self) -> std::sync::RwLockReadGuard<'_, T> {
            self.0
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }

        pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, T> {
            self.0
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }
}

pub use parking_lot_shim::RwLock as ServerLock;
