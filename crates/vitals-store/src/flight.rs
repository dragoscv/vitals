//! The flight recorder: the last N frames, in memory.
//!
//! It used to be a SQLite table, written every tick — an `INSERT`, a
//! trimming `DELETE` and a WAL commit a second, all day, on the machine of a
//! user who had left history off and so had asked for nothing to be written
//! at all. Its contents are only ever read when the user exports a bug
//! report, and its lifetime was already the process (each launch cleared
//! it), so memory is the honest home: the same 120 frames, no disk I/O, and
//! nothing to clean up after a crash.
//!
//! Shared rather than owned: the sampler thread writes it and the export
//! command reads it from the blocking pool, so the handle is a cheap clone
//! of one `Arc`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use vitals_core::sample::Frame;

use crate::db::{Result, StoreError};

/// One recorded frame: `(seq, unix seconds, frame JSON)`.
pub type FlightEntry = (i64, i64, Vec<u8>);

/// A bounded ring of the most recent frames, identifiers removed.
#[derive(Debug, Clone)]
pub struct FlightRecorder {
    ring: Arc<Mutex<VecDeque<FlightEntry>>>,
    capacity: usize,
}

impl Default for FlightRecorder {
    fn default() -> Self {
        Self::new(usize::try_from(crate::FLIGHT_FRAMES).unwrap_or(120))
    }
}

impl FlightRecorder {
    /// A recorder holding at most `capacity` frames.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            ring: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    /// Records one frame, without the owning user account or MAC addresses.
    ///
    /// Those identify a person and a device, a bug report does not need
    /// them, and this is recorded without the user asking. The frame is
    /// borrowed mutably only for that serialisation and comes back exactly
    /// as it went in.
    pub fn record(&self, frame: &mut Frame) -> Result<()> {
        let ts = i64::try_from(frame.timestamp_ms / 1_000).unwrap_or(0);
        let seq = i64::try_from(frame.seq.0).unwrap_or(0);
        let encoded = frame
            .with_identifiers_removed(serde_json::to_vec)
            .map_err(StoreError::from)?;
        self.push((seq, ts, encoded));
        Ok(())
    }

    fn push(&self, entry: FlightEntry) {
        if self.capacity == 0 {
            return;
        }
        let mut ring = self.lock();
        while ring.len() >= self.capacity {
            ring.pop_front();
        }
        ring.push_back(entry);
    }

    /// Every recorded frame, oldest first — the shape the export writes.
    #[must_use]
    pub fn frames(&self) -> Vec<FlightEntry> {
        self.lock().iter().cloned().collect()
    }

    /// How many frames are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Forgets every frame. Part of "Clear history".
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// A poisoned lock means a panic mid-push; the ring is still a valid
    /// `VecDeque`, and a bug report is exactly when it must stay readable.
    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<FlightEntry>> {
        self.ring.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_only_the_newest_frames_in_order() {
        let flight = FlightRecorder::new(4);
        for seq in 0..10 {
            flight.push((seq, seq, b"{}".to_vec()));
        }
        assert_eq!(
            flight.frames().iter().map(|f| f.0).collect::<Vec<_>>(),
            vec![6, 7, 8, 9]
        );
    }

    #[test]
    fn a_clone_is_the_same_recording_so_the_export_sees_what_the_sampler_wrote() {
        let sampler_side = FlightRecorder::new(8);
        let export_side = sampler_side.clone();
        sampler_side.push((1, 1, b"a".to_vec()));
        assert_eq!(export_side.len(), 1);
        export_side.clear();
        assert!(sampler_side.is_empty());
    }

    #[test]
    fn a_zero_capacity_recorder_records_nothing_rather_than_panicking() {
        let flight = FlightRecorder::new(0);
        flight.push((1, 1, b"a".to_vec()));
        assert!(flight.is_empty());
    }
}
