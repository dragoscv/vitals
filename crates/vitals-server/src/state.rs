//! What the router is handed by its host.

use std::sync::Arc;

use parking_lot_shim::RwLock;
use tokio::sync::broadcast;
use vitals_core::provider::HostInfo;
use vitals_core::sample::Frame;

use crate::auth::{Scope, TokenSet};
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
    /// Complete current state, rebuilt as frames arrive.
    ///
    /// A client that connects mid-stream cannot use a delta: it describes
    /// changes against a previous frame it never received, so the phone would
    /// render an empty process list until the next keyframe up to thirty
    /// ticks later. Verified against a live sampler — connecting after 2.5 s
    /// returned `processes=0`.
    ///
    /// Serving the last *keyframe* instead is not enough either: it can be
    /// thirty seconds old, and the very first one always reports 0% CPU
    /// because rates need two samples. So the server keeps a materialised
    /// view — keyframe, then every delta applied on top — which is the same
    /// thing the desktop's `metrics.ts` maintains. New clients get one frame
    /// that is both complete and current.
    current: Arc<RwLock<Option<Materialised>>>,
    tx: broadcast::Sender<Arc<Frame>>,
}

/// The reconstructed present.
#[derive(Debug, Clone)]
struct Materialised {
    seq: vitals_core::sample::FrameSeq,
    timestamp_ms: u64,
    elapsed_ms: u32,
    system: vitals_core::metrics::SystemMetrics,
    /// Keyed by PID so a delta can replace an entry in place.
    processes: std::collections::HashMap<vitals_core::ids::Pid, vitals_core::process::Process>,
}

impl Materialised {
    /// Renders the view as a keyframe a client can consume directly.
    fn to_frame(&self) -> Frame {
        let mut processes: Vec<_> = self.processes.values().cloned().collect();
        // Stable order so two consecutive snapshots do not appear to shuffle
        // the list; a HashMap's iteration order is deliberately arbitrary.
        processes.sort_by_key(|p| p.key.pid.0);
        Frame {
            seq: self.seq,
            timestamp_ms: self.timestamp_ms,
            elapsed_ms: self.elapsed_ms,
            payload: vitals_core::sample::FramePayload::Keyframe {
                system: self.system.clone(),
                processes,
            },
        }
    }
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
            current: Arc::new(RwLock::new(None)),
            tx,
        }
    }

    /// Publishes a frame. Cheap and non-blocking, even with no subscribers.
    pub fn publish(&self, frame: Arc<Frame>) {
        self.apply(&frame);
        *self.latest.write() = Some(Arc::clone(&frame));
        // `Err` means nobody is listening, which is the normal case.
        let _ = self.tx.send(frame);
    }

    /// Folds a frame into the materialised view.
    ///
    /// Exits are applied **before** changes, matching the desktop's rule: a
    /// PID that exited and was immediately reused within one tick appears in
    /// both lists, and removing afterwards would delete the new owner.
    fn apply(&self, frame: &Frame) {
        use vitals_core::sample::FramePayload as P;

        let mut guard = self.current.write();
        match &frame.payload {
            P::Keyframe { system, processes } => {
                *guard = Some(Materialised {
                    seq: frame.seq,
                    timestamp_ms: frame.timestamp_ms,
                    elapsed_ms: frame.elapsed_ms,
                    system: system.clone(),
                    processes: processes.iter().map(|p| (p.key.pid, p.clone())).collect(),
                });
            }
            P::Delta {
                system,
                changed,
                exited,
            } => {
                // A delta before any keyframe cannot be reconstructed. Drop
                // it rather than inventing a partial machine.
                let Some(view) = guard.as_mut() else { return };
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
            }
        }
    }

    /// Complete, current state as a keyframe. What a new connection is sent.
    #[must_use]
    pub fn initial(&self) -> Option<Arc<Frame>> {
        self.current.read().as_ref().map(|v| Arc::new(v.to_frame()))
    }

    /// The most recent frame of any kind. For `/metrics`, which is scraped
    /// on its own schedule and re-reads everything each time.
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
    /// The alert engine's current list. A closure like `host` so the server
    /// crate does not own the engine; the desktop runs it on the sampler
    /// thread and the server only reads.
    pub alerts: Arc<dyn Fn() -> Vec<vitals_core::alerts::Alert> + Send + Sync>,
    /// Shown at `/api/v1/health` so a client can tell which build it is
    /// talking to before trusting the shape of anything else.
    pub version: String,
    /// Scope granted to a caller on the loopback interface **without a
    /// token**. `None` — the LAN server's setting — means loopback callers
    /// are treated like anyone else.
    ///
    /// A process on the same machine, running as the same user, already
    /// owns Vitals: it can read the token file, or simply kill the app. A
    /// token therefore buys nothing against it, and demanding one makes
    /// `vitals ps` unusable until the user has done a pairing dance with
    /// their own computer. The desktop's `127.0.0.1` listener sets this to
    /// `Control`; the `0.0.0.0` listener never does, and the check is made
    /// against the connecting peer, not the listener, so a LAN request can
    /// never qualify.
    pub loopback_scope: Option<Scope>,
}

impl std::fmt::Debug for ApiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiState")
            .field("version", &self.version)
            .field("has_assets", &self.assets.is_some())
            .field("loopback_scope", &self.loopback_scope)
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

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::fixtures;
    use vitals_core::ids::Pid;
    use vitals_core::sample::{FramePayload, FrameSeq};

    fn delta(seq: u64, changed: Vec<vitals_core::process::Process>, exited: Vec<Pid>) -> Frame {
        Frame {
            seq: FrameSeq(seq),
            timestamp_ms: 0,
            elapsed_ms: 1_000,
            payload: FramePayload::Delta {
                system: fixtures::system(),
                changed,
                exited,
            },
        }
    }

    fn pids(frame: &Frame) -> Vec<u32> {
        match &frame.payload {
            FramePayload::Keyframe { processes, .. } => {
                processes.iter().map(|p| p.key.pid.0).collect()
            }
            FramePayload::Delta { .. } => panic!("initial() must always be a keyframe"),
        }
    }

    #[test]
    fn nothing_before_the_first_frame() {
        assert!(FrameSource::new().initial().is_none());
    }

    #[test]
    fn a_delta_before_any_keyframe_is_dropped_rather_than_invented() {
        let src = FrameSource::new();
        src.publish(Arc::new(delta(
            1,
            vec![fixtures::process("a.exe", 1, 1.0)],
            vec![],
        )));
        assert!(src.initial().is_none(), "half a machine is not a snapshot");
    }

    #[test]
    fn deltas_are_folded_into_the_view() {
        let src = FrameSource::new();
        src.publish(Arc::new(fixtures::keyframe(
            1,
            vec![
                fixtures::process("a.exe", 1, 1.0),
                fixtures::process("b.exe", 2, 1.0),
            ],
        )));
        src.publish(Arc::new(delta(
            2,
            vec![fixtures::process("c.exe", 3, 1.0)],
            vec![Pid(1)],
        )));

        let view = src.initial().unwrap();
        assert_eq!(view.seq, FrameSeq(2));
        assert_eq!(pids(&view), vec![2, 3]);
    }

    #[test]
    fn exits_apply_before_changes_so_a_recycled_pid_survives() {
        // PID 1 exits and is immediately reused within one tick: it is in
        // both lists. Removing after inserting would delete the new owner.
        let src = FrameSource::new();
        src.publish(Arc::new(fixtures::keyframe(
            1,
            vec![fixtures::process("old.exe", 1, 1.0)],
        )));
        src.publish(Arc::new(delta(
            2,
            vec![fixtures::process("new.exe", 1, 1.0)],
            vec![Pid(1)],
        )));

        let view = src.initial().unwrap();
        let FramePayload::Keyframe { processes, .. } = &view.payload else {
            unreachable!()
        };
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].name, "new.exe");
    }

    #[test]
    fn a_new_keyframe_replaces_the_view_entirely() {
        let src = FrameSource::new();
        src.publish(Arc::new(fixtures::keyframe(
            1,
            vec![fixtures::process("a.exe", 1, 1.0)],
        )));
        src.publish(Arc::new(fixtures::keyframe(
            2,
            vec![fixtures::process("z.exe", 9, 1.0)],
        )));
        assert_eq!(pids(&src.initial().unwrap()), vec![9]);
    }
}
