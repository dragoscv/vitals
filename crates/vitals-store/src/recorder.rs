//! The sampler-thread face of the store.
//!
//! [`Recorder`] is what the sampling loop talks to: hand it every frame and it
//! decides what to keep. Two independent switches:
//!
//! - **history** — the tiered machine-wide time series behind long-range
//!   charts. Off by default; the user turns it on in Settings.
//! - **flight recorder** — the last N raw frames, always on while the app
//!   runs, exported on request for a bug report. Held in memory
//!   ([`FlightRecorder`]): its cost is bounded by N, its lifetime is the
//!   process, and with history off nothing at all is written to disk.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vitals_core::history::MachineSample;
use vitals_core::sample::{Frame, FramePayload};

use crate::db::{Result, Store, StoreError};
use crate::flight::FlightRecorder;
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
    flight: FlightRecorder,
}

impl Recorder {
    /// Opens the database at `path`. History starts **off**; call
    /// [`Self::set_history_enabled`] once the user's setting is known.
    ///
    /// A file SQLite reports as corrupt or not a database at all is moved
    /// aside as `history.corrupt-<unix seconds>.db` and a fresh one created,
    /// once. Without this one damaged file — a power cut mid-checkpoint, a
    /// sync tool writing over it — disabled history for every launch after,
    /// silently, until the user found and deleted it by hand. Moved rather
    /// than deleted so the data is still there if anyone wants it back.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_flight(path, FlightRecorder::default())
    }

    /// As [`Self::open`], recording frames into `flight` — a handle the
    /// caller keeps so the export can read the ring from another thread.
    pub fn open_with_flight(path: &Path, flight: FlightRecorder) -> Result<Self> {
        let store = match Self::open_store(path) {
            Err(error) if is_corruption(&error) => {
                match quarantine(path) {
                    Ok(aside) => tracing::warn!(
                        %error,
                        moved_to = %aside.display(),
                        "history store is corrupt; starting a new one"
                    ),
                    // Retried anyway: if the rename failed the open fails
                    // again and history is disabled, as it was before.
                    Err(io) => {
                        tracing::warn!(%error, %io, "history store is corrupt and could not be moved aside");
                    }
                }
                Self::open_store(path)?
            }
            other => other?,
        };
        store.clear_flight_frames()?;
        Ok(Self {
            store,
            path: path.to_path_buf(),
            history_enabled: false,
            last_maintain: Instant::now(),
            flight,
        })
    }

    fn open_store(path: &Path) -> Result<Store> {
        let store = Store::open(path, RetentionPolicy::default())?;
        store.convert_if_small()?;
        Ok(store)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A handle to the flight recorder, for the export.
    #[must_use]
    pub fn flight(&self) -> FlightRecorder {
        self.flight.clone()
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

    /// Records one frame. With history off this touches no disk at all:
    /// the frame goes into the in-memory flight ring and nowhere else.
    ///
    /// Serialises the frame itself, with identifiers removed (see the module
    /// docs). The frame is borrowed mutably only for the length of that
    /// serialisation and is returned exactly as it came in.
    pub fn observe(&mut self, frame: &mut Frame) -> Result<()> {
        let ts = i64::try_from(frame.timestamp_ms / 1_000).unwrap_or(0);
        self.flight.record(frame)?;

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

    /// Forces a maintenance pass and folds the write-ahead log back into
    /// the file. Called on shutdown so the last minute is rolled up rather
    /// than left for the next launch, and so the `-wal` file does not sit
    /// at its high-water size until then.
    pub fn flush(&mut self) -> Result<()> {
        if self.history_enabled {
            self.store.maintain(now_secs())?;
        }
        self.store.checkpoint()
    }

    /// Deletes all recorded history — the time series and the flight
    /// recording — and reclaims the file.
    pub fn clear(&self) -> Result<()> {
        self.flight.clear();
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

/// Whether an open failed because the file itself is damaged, as opposed
/// to unreadable (permissions, a locked file, a full disk), where moving it
/// aside would destroy a good database to work around a passing problem.
fn is_corruption(error: &StoreError) -> bool {
    matches!(
        error,
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _))
            if matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            )
    )
}

/// Moves a damaged database (and its WAL sidecars) out of the way.
///
/// The sidecars go too: a `-wal` left behind would be replayed into the
/// fresh file, or make SQLite reject it, and the damage would come back.
fn quarantine(path: &Path) -> std::io::Result<PathBuf> {
    let aside = path.with_file_name(format!("history.corrupt-{}.db", now_secs()));
    std::fs::rename(path, &aside)?;
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        let mut moved = aside.as_os_str().to_owned();
        moved.push(suffix);
        let _ = std::fs::rename(&sidecar, &moved);
    }
    Ok(aside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vitals-rec-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("history.sqlite")
    }

    fn in_memory() -> Recorder {
        Recorder {
            store: Store::in_memory(RetentionPolicy::default()).unwrap(),
            path: PathBuf::new(),
            history_enabled: false,
            last_maintain: Instant::now(),
            flight: FlightRecorder::default(),
        }
    }

    #[test]
    fn with_history_off_observing_frames_writes_nothing_to_the_database() {
        // The flight recorder used to INSERT, trim and commit every tick
        // for users who had asked for nothing to be recorded.
        let path = temp_path("nowrite");
        let mut r = Recorder::open(&path).unwrap();
        let wal = path.with_extension("sqlite-wal");
        let before = std::fs::metadata(&wal).map_or(0, |m| m.len());
        for seq in 1..=50 {
            let mut frame = vitals_core::fixtures::keyframe(seq, Vec::new());
            r.observe(&mut frame).unwrap();
        }
        assert_eq!(std::fs::metadata(&wal).map_or(0, |m| m.len()), before);
        assert_eq!(r.store.legacy_flight_rows().unwrap(), 0);
        assert_eq!(r.flight().len(), 50, "but the frames were recorded");
    }

    #[test]
    fn a_restart_starts_an_empty_recording_and_clears_what_an_older_build_left() {
        let path = temp_path("restart");
        drop(Recorder::open(&path).unwrap());
        // What a pre-memory build wrote into the shared file.
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "INSERT INTO flight_frames (seq, ts, frame) VALUES (999, 1, x'7b7d')",
                [],
            )
            .unwrap();
        let r = Recorder::open(&path).unwrap();
        assert_eq!(r.store.legacy_flight_rows().unwrap(), 0);
        assert!(r.flight().is_empty());
    }

    #[test]
    fn the_recording_keeps_only_the_last_flight_frames() {
        let mut r = in_memory();
        for seq in 1..=(FLIGHT_FRAMES + 30) {
            let mut frame = vitals_core::fixtures::keyframe(seq, Vec::new());
            r.observe(&mut frame).unwrap();
        }
        let frames = r.flight().frames();
        assert_eq!(frames.len(), usize::try_from(FLIGHT_FRAMES).unwrap());
        assert_eq!(frames.first().map(|f| f.0), Some(31));
        assert_eq!(
            frames.last().map(|f| f.0),
            Some(i64::try_from(FLIGHT_FRAMES + 30).unwrap())
        );
    }

    #[test]
    fn clearing_history_also_forgets_the_flight_recording() {
        let mut r = in_memory();
        let mut frame = vitals_core::fixtures::keyframe(1, Vec::new());
        r.observe(&mut frame).unwrap();
        r.clear().unwrap();
        assert!(r.flight().is_empty());
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_and_a_fresh_store_opened_in_its_place() {
        let path = temp_path("corrupt");
        std::fs::write(&path, vec![0x5a_u8; 8_192]).unwrap();

        let r = Recorder::open(&path).expect("recovers rather than disabling history");
        r.store
            .insert(&MachineSample::from_metrics(
                1,
                &vitals_core::fixtures::system(),
            ))
            .unwrap();

        let aside: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|ext| ext == "db")
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("history.corrupt-"))
            })
            .collect();
        assert_eq!(aside.len(), 1, "the damaged file is kept, not deleted");
    }

    #[test]
    fn a_file_that_cannot_be_opened_for_another_reason_is_left_alone() {
        // A directory where the file should be: an I/O failure, not damage.
        // Moving it aside would destroy good data to dodge a passing fault.
        let path = temp_path("notcorrupt");
        std::fs::create_dir_all(&path).unwrap();
        assert!(Recorder::open(&path).is_err());
        assert!(path.is_dir(), "left where it was");
    }

    #[test]
    fn retention_days_stretches_only_the_coarse_tiers() {
        let mut r = in_memory();
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

    #[test]
    fn the_flight_recorder_never_stores_who_was_signed_in_or_a_mac_address() {
        let mut r = in_memory();
        let mut frame = vitals_core::fixtures::keyframe(
            1,
            vec![vitals_core::fixtures::process("notepad.exe", 42, 1.0)],
        );
        if let FramePayload::Keyframe { system, .. } = &mut frame.payload {
            system.networks[0].mac = Some("AA-BB-CC-DD-EE-FF".into());
        }

        r.observe(&mut frame).unwrap();

        let stored = r.flight().frames();
        let text = String::from_utf8(stored[0].2.clone()).unwrap();
        assert!(
            text.contains("notepad.exe"),
            "the frame itself is kept: {text}"
        );
        assert!(!text.contains("VITALS\\\\dev"), "owner leaked: {text}");
        assert!(!text.contains("AA-BB-CC-DD-EE-FF"), "MAC leaked: {text}");

        // And the live frame the UI and the LAN receive is untouched.
        let FramePayload::Keyframe { system, processes } = &frame.payload else {
            unreachable!()
        };
        assert_eq!(processes[0].user.as_deref(), Some("VITALS\\dev"));
        assert_eq!(system.networks[0].mac.as_deref(), Some("AA-BB-CC-DD-EE-FF"));
    }
}
