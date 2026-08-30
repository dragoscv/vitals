//! A bounded, overwriting ring buffer for sample frames.
//!
//! ## Why overwriting rather than blocking
//!
//! The sampler must never be slowed down by a slow consumer. If the webview
//! is busy laying out a 5000-row table, the correct behaviour is to discard
//! the frames it missed and show it the newest one — not to queue them up and
//! then replay a burst of stale data, and certainly not to block the sampler
//! and corrupt the timing of every subsequent measurement.
//!
//! A dropped frame is invisible to a user watching a live graph. A stalled
//! sampler is not: it produces a visible freeze and, worse, wrong rate
//! calculations on the frame after the stall.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::{Condvar, Mutex};

use vitals_core::sample::{Frame, FrameSeq};

/// Shared state behind [`FrameWriter`] and [`FrameReader`].
#[derive(Debug)]
struct Shared {
    slots: Mutex<Inner>,
    /// Signals a waiting reader. Avoids a polling loop, which would defeat
    /// the point of the adaptive sample rate.
    available: Condvar,
}

#[derive(Debug)]
struct Inner {
    frames: VecDeque<Frame>,
    capacity: usize,
    /// Frames discarded because the reader fell behind.
    ///
    /// Surfaced in diagnostics: a nonzero value under normal load means the
    /// UI has a rendering performance problem, and we would rather know.
    dropped: u64,
    /// Sequence of the most recently written frame.
    latest: FrameSeq,
    closed: bool,
}

/// Creates a connected writer/reader pair.
///
/// # Panics
///
/// Panics if `capacity` is zero — a zero-capacity buffer would silently
/// discard every frame, which is never what the caller meant.
#[must_use]
pub fn channel(capacity: usize) -> (FrameWriter, FrameReader) {
    assert!(capacity > 0, "frame buffer capacity must be non-zero");

    let shared = Arc::new(Shared {
        slots: Mutex::new(Inner {
            frames: VecDeque::with_capacity(capacity),
            capacity,
            dropped: 0,
            latest: FrameSeq(0),
            closed: false,
        }),
        available: Condvar::new(),
    });

    (
        FrameWriter {
            shared: Arc::clone(&shared),
        },
        FrameReader { shared },
    )
}

/// Alias kept for the common case of constructing both ends together.
pub type FrameBuffer = (FrameWriter, FrameReader);

/// The producing end, held by the sampler thread.
#[derive(Debug, Clone)]
pub struct FrameWriter {
    shared: Arc<Shared>,
}

impl FrameWriter {
    /// Publishes a frame, evicting the oldest if the buffer is full.
    ///
    /// Never blocks.
    pub fn push(&self, frame: Frame) {
        let mut inner = self.shared.slots.lock();
        if inner.closed {
            return;
        }

        if inner.frames.len() == inner.capacity {
            inner.frames.pop_front();
            inner.dropped += 1;
        }

        inner.latest = frame.seq;
        inner.frames.push_back(frame);
        drop(inner);

        self.shared.available.notify_one();
    }

    /// Signals that no more frames will arrive, waking any blocked reader.
    pub fn close(&self) {
        self.shared.slots.lock().closed = true;
        self.shared.available.notify_all();
    }
}

/// The consuming end, held by the UI bridge.
#[derive(Debug)]
pub struct FrameReader {
    shared: Arc<Shared>,
}

impl FrameReader {
    /// Takes the oldest buffered frame, or `None` if empty.
    pub fn try_recv(&self) -> Option<Frame> {
        self.shared.slots.lock().frames.pop_front()
    }

    /// Discards everything buffered and returns only the newest frame.
    ///
    /// This is what a render loop should call. Drawing intermediate frames
    /// that will be overwritten in the same paint is wasted work, and on a
    /// 144 Hz display with a 1 Hz sampler it is *all* wasted work.
    pub fn recv_latest(&self) -> Option<Frame> {
        let mut inner = self.shared.slots.lock();
        let skipped = inner.frames.len().saturating_sub(1);
        if skipped > 0 {
            inner.dropped += skipped as u64;
            for _ in 0..skipped {
                inner.frames.pop_front();
            }
        }
        inner.frames.pop_front()
    }

    /// Blocks until a frame is available or the writer closes.
    pub fn recv(&self) -> Option<Frame> {
        let mut inner = self.shared.slots.lock();
        loop {
            if let Some(frame) = inner.frames.pop_front() {
                return Some(frame);
            }
            if inner.closed {
                return None;
            }
            self.shared.available.wait(&mut inner);
        }
    }

    /// Number of frames dropped because this reader fell behind.
    pub fn dropped(&self) -> u64 {
        self.shared.slots.lock().dropped
    }

    /// Sequence number of the most recent frame written.
    pub fn latest_seq(&self) -> FrameSeq {
        self.shared.slots.lock().latest
    }

    pub fn len(&self) -> usize {
        self.shared.slots.lock().frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
// Tests assert on values that must be present; `unwrap` failing there is the
// test failing, which is the intent.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use vitals_core::metrics::SystemMetrics;
    use vitals_core::sample::FramePayload;

    fn frame(seq: u64) -> Frame {
        Frame {
            seq: FrameSeq(seq),
            timestamp_ms: seq * 1000,
            elapsed_ms: 1000,
            payload: FramePayload::Delta {
                system: SystemMetrics::default(),
                changed: vec![],
                exited: vec![],
            },
        }
    }

    #[test]
    fn delivers_frames_in_order() {
        let (w, r) = channel(4);
        w.push(frame(1));
        w.push(frame(2));
        assert_eq!(r.try_recv().unwrap().seq, FrameSeq(1));
        assert_eq!(r.try_recv().unwrap().seq, FrameSeq(2));
        assert!(r.try_recv().is_none());
    }

    #[test]
    fn overwrites_oldest_when_full_instead_of_blocking() {
        let (w, r) = channel(2);
        w.push(frame(1));
        w.push(frame(2));
        w.push(frame(3));

        assert_eq!(r.dropped(), 1, "the oldest frame should have been evicted");
        assert_eq!(
            r.try_recv().unwrap().seq,
            FrameSeq(2),
            "frame 1 was dropped, so 2 is now oldest"
        );
    }

    #[test]
    fn recv_latest_skips_intermediate_frames() {
        let (w, r) = channel(8);
        for i in 1..=5 {
            w.push(frame(i));
        }

        let got = r.recv_latest().expect("a frame should be available");
        assert_eq!(
            got.seq,
            FrameSeq(5),
            "render loops want the newest frame only"
        );
        assert!(
            r.is_empty(),
            "intermediate frames should have been discarded"
        );
        assert_eq!(r.dropped(), 4);
    }

    #[test]
    fn recv_latest_on_empty_buffer_is_none() {
        let (_w, r) = channel(4);
        assert!(r.recv_latest().is_none());
    }

    #[test]
    fn close_wakes_a_blocked_reader() {
        let (w, r) = channel(4);
        let handle = std::thread::spawn(move || r.recv());
        // Give the reader a moment to block, then close.
        std::thread::sleep(std::time::Duration::from_millis(50));
        w.close();
        assert!(
            handle.join().unwrap().is_none(),
            "closing must unblock, not hang"
        );
    }

    #[test]
    fn push_after_close_is_ignored() {
        let (w, r) = channel(4);
        w.close();
        w.push(frame(1));
        assert!(r.is_empty());
    }

    #[test]
    #[should_panic(expected = "capacity must be non-zero")]
    fn zero_capacity_is_rejected() {
        let _ = channel(0);
    }
}
