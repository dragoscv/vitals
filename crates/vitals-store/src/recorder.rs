//! The sampler-thread face of the store.
//!
//! [`Recorder`] is what the sampling loop talks to: hand it every frame and it
//! decides what to keep. Two independent switches:
//!
//! - **history** — the tiered machine-wide time series behind long-range
//!   charts. Off by default; the user turns it on in Settings.
//! - **flight recorder** — the last N raw frames, always on while the app
//!   runs, exported on request for a bug report. Its cost is bounded by N
//!   and its lifetime by the process; nothing about it is a "recording" in
//!   the privacy sense, which is why it needs no opt-in.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vitals_core::history::MachineSample;
use vitals_core::sample::{Frame, FramePayload};

use crate::db::{Result, Store};
use crate::retention::{Resolution, RetentionPolicy};

/// How many raw frames the flight recorder keeps. At 1 Hz this is two
/// minutes; at the fastest rate, one.
pub const FLIGHT_FRAMES: u64 = 120;

/// How often rollups and pruning run.
const MAINTAIN_EVERY: Duration = Duration::from_secs(60);

/// Owns the store on the sampler thread.
#[derive(Debug)]
pub struct Recorder {
    store: Store,
    path: PathBuf,
    history_enabled: bool,
    last_maintain: Instant,
}

impl Recorder {
    /// Opens the database at `path`. History starts **off**; call
    /// [`Self::set_history_enabled`] once the user's setting is known.
    pub fn open(path: &Path) -> Result<Self> {
        let store = Store::open(path, RetentionPolicy::default())?;
        // The flight recorder's lifetime is the process (see the module
        // docs); a new session starts it empty. See `clear_flight_frames`.
        store.clear_flight_frames()?;
        Ok(Self {
            store,
            path: path.to_path_buf(),
            history_enabled: false,
            last_maintain: Instant::now(),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn history_enabled(&self) -> bool {
        self.history_enabled
    }

    pub fn set_history_enabled(&mut self, enabled: bool) {
        self.history_enabled = enabled;
    }

    /// Applies the user's "keep data for N days" choice.
    ///
    /// The finest tiers are unchanged — an hour of seconds and a day of
    /// minutes are what make recent charts sharp regardless of how long the
    /// long-range data is kept. Only the two coarse tiers stretch or shrink.
    pub fn set_retention_days(&mut self, days: u32) {
        let secs = u64::from(days) * 86_400;
        let mut policy = self.store.policy().clone();
        policy.tiers = vec![
            (Resolution::Second, 3_600),
            (Resolution::Minute, 86_400.min(secs.max(3_600))),
            (Resolution::FiveMinutes, secs),
            (Resolution::Hour, secs),
        ];
        self.store.set_policy(policy);
    }

    /// Records one frame. Cheap when history is off: a single bounded
    /// `INSERT` into the flight table.
    pub fn observe(&mut self, frame: &Frame, encoded: &[u8]) -> Result<()> {
        let ts = i64::try_from(frame.timestamp_ms / 1_000).unwrap_or(0);

        self.store.record_frame(
            i64::try_from(frame.seq.0).unwrap_or(0),
            ts,
            encoded,
            FLIGHT_FRAMES,
        )?;

        if self.history_enabled {
            let system = match &frame.payload {
                FramePayload::Keyframe { system, .. } | FramePayload::Delta { system, .. } => {
                    system
                }
            };
            self.store
                .insert(&MachineSample::from_metrics(ts, system))?;

            if self.last_maintain.elapsed() >= MAINTAIN_EVERY {
                self.store.maintain(now_secs())?;
                self.last_maintain = Instant::now();
            }
        }
        Ok(())
    }

    /// Forces a maintenance pass. Called on shutdown so the last minute is
    /// rolled up rather than left for the next launch.
    pub fn flush(&mut self) -> Result<()> {
        if self.history_enabled {
            self.store.maintain(now_secs())?;
        }
        Ok(())
    }

    /// Deletes all recorded history and reclaims the file.
    pub fn clear(&self) -> Result<()> {
        self.store.clear()
    }

    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restart_records_the_new_session_not_the_old_one() {
        // Two "launches" on one file. Before the fix, session one's seq
        // 1..=200 outranked session two's 1..=3 and every new frame was
        // trimmed on insert; the export showed only the old session.
        let dir = std::env::temp_dir().join(format!("vitals-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.db");
        let _ = std::fs::remove_file(&path);

        {
            let r = Recorder::open(&path).unwrap();
            for seq in 1..=200 {
                r.store
                    .record_frame(seq, seq, b"old", FLIGHT_FRAMES)
                    .unwrap();
            }
        }
        let r = Recorder::open(&path).unwrap();
        for seq in 1..=3 {
            r.store
                .record_frame(seq, 1_000 + seq, b"new", FLIGHT_FRAMES)
                .unwrap();
        }
        let frames = r.store.flight_frames().unwrap();
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(frames.len(), 3, "only this session's frames");
        assert!(frames.iter().all(|(_, _, f)| f == b"new"));
    }

    #[test]
    fn retention_days_stretches_only_the_coarse_tiers() {
        let mut r = Recorder {
            store: Store::in_memory(RetentionPolicy::default()).unwrap(),
            path: PathBuf::new(),
            history_enabled: false,
            last_maintain: Instant::now(),
        };
        r.set_retention_days(90);
        let tiers = &r.store.policy().tiers;
        assert_eq!(tiers[0], (Resolution::Second, 3_600));
        assert_eq!(tiers[1], (Resolution::Minute, 86_400));
        assert_eq!(tiers[2], (Resolution::FiveMinutes, 90 * 86_400));

        // One day: the minute tier is capped to that day too.
        r.set_retention_days(1);
        assert_eq!(r.store.policy().tiers[1], (Resolution::Minute, 86_400));
        assert_eq!(r.store.policy().tiers[2], (Resolution::FiveMinutes, 86_400));
    }
}
