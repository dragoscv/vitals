//! Per-volume history: how full each drive was, what each clean-up
//! analysis measured, and what each clean-up run gave back.
//!
//! # Its own file, always on
//!
//! `history.sqlite` is off by default because it records what the machine
//! *does*. This records three numbers a drive reports to anyone who asks
//! (used, free, when) once an hour, plus the sizes a clean-up analysis the
//! user started already measured — so "why is this drive growing?" has an
//! answer the first time it is asked, without the user having had to turn
//! history on weeks earlier. PRIVACY.md lists the file and what is in it.
//!
//! # Measured or absent
//!
//! A volume that could not be read is not written: an hour with no row is
//! an hour nobody measured, and a row of zeroes would be a drive that
//! emptied itself and filled up again. An item whose size the analysis
//! could not read is stored as `NULL` and never takes part in a delta.
//!
//! # Retention
//!
//! Hourly rows for [`HOURLY_DAYS`] days, then one row per volume per day
//! (the last of that day) up to [`KEEP_DAYS`] days. Analyses and runs are
//! kept [`KEEP_DAYS`] days. At one row an hour per volume the file stays in
//! the tens of kilobytes per drive.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{Result, StoreError};

/// Bumped when a migration is added to [`DriveHistory::migrate`].
const SCHEMA_VERSION: i64 = 1;
/// Hourly rows are kept this long before being thinned to one a day.
pub const HOURLY_DAYS: i64 = 30;
/// Nothing older than this is kept.
pub const KEEP_DAYS: i64 = 180;
const DAY: i64 = 86_400;
/// Errors kept per run, and characters per error: a history entry, not a log.
const MAX_ERRORS: usize = 5;
const MAX_ERROR_CHARS: usize = 300;

/// One reading of a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumePoint {
    pub ts: i64,
    pub used: u64,
    pub free: u64,
}

/// One item's size in an analysis. `label` tells apart several items of
/// one kind (two distros' journals, many build folders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSize {
    pub kind: String,
    pub label: Option<String>,
    /// `None` when the analysis could not measure it.
    pub size: Option<u64>,
}

/// Everything one analysis measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub ts: i64,
    pub items: Vec<ItemSize>,
}

/// One clean-up run, as the history shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub ts: i64,
    /// Started by the weekly schedule rather than by the user.
    pub scheduled: bool,
    /// What the drive gained; `None` when it could not be measured.
    pub freed: Option<u64>,
    /// Items attempted.
    pub items: u32,
    pub failed: u32,
    pub errors: Vec<String>,
}

/// How fast a volume's used space changes, in bytes per day, over three
/// windows. `None` where the readings do not cover enough of the window to
/// say: a "per week" rate from two hours of data would be a guess.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Growth {
    pub day: Option<i64>,
    pub week: Option<i64>,
    pub month: Option<i64>,
}

/// One item that changed between two analyses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemChange {
    pub kind: String,
    pub label: Option<String>,
    pub then: u64,
    pub now: u64,
}

impl ItemChange {
    /// Signed change in bytes.
    #[must_use]
    pub fn delta(&self) -> i64 {
        i64::try_from(i128::from(self.now) - i128::from(self.then)).unwrap_or(i64::MAX)
    }
}

/// The per-volume history file. One connection per use: the sampler-side
/// writer and the commands that read it are on different threads, and
/// WAL lets them share the file.
#[derive(Debug)]
pub struct DriveHistory {
    conn: Connection,
}

impl DriveHistory {
    /// Opens (creating if needed) the file at `path` and migrates it.
    ///
    /// # Errors
    ///
    /// SQLite failures, or a file written by a newer build.
    pub fn open(path: &Path) -> Result<Self> {
        Self::configure(Connection::open(path)?)
    }

    /// An in-memory history, for tests.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn in_memory() -> Result<Self> {
        Self::configure(Connection::open_in_memory()?)
    }

    fn configure(conn: Connection) -> Result<Self> {
        // Two writers can meet here (the hourly sampler and an analysis the
        // user started); a short wait beats a dropped reading.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        let mut history = Self { conn };
        history.migrate()?;
        Ok(history)
    }

    fn migrate(&mut self) -> Result<()> {
        let found: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if found > SCHEMA_VERSION {
            return Err(StoreError::SchemaTooNew {
                found,
                supported: SCHEMA_VERSION,
            });
        }
        if found < 1 {
            let tx = self.conn.transaction()?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS volume_samples (
                    volume TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    used INTEGER NOT NULL,
                    free INTEGER NOT NULL,
                    PRIMARY KEY (volume, ts)
                ) WITHOUT ROWID;
                CREATE TABLE IF NOT EXISTS analysis_items (
                    volume TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    label TEXT NOT NULL,
                    size INTEGER,
                    PRIMARY KEY (volume, ts, kind, label)
                ) WITHOUT ROWID;
                CREATE TABLE IF NOT EXISTS cleanup_runs (
                    volume TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    scheduled INTEGER NOT NULL,
                    freed INTEGER,
                    items INTEGER NOT NULL,
                    failed INTEGER NOT NULL,
                    errors TEXT NOT NULL,
                    PRIMARY KEY (volume, ts, scheduled)
                ) WITHOUT ROWID;
                PRAGMA user_version = 1;",
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Records one reading of `volume`.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn record_volume(&self, volume: &str, point: VolumePoint) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO volume_samples (volume, ts, used, free) VALUES (?1, ?2, ?3, ?4)",
            params![volume, point.ts, to_i64(point.used), to_i64(point.free)],
        )?;
        Ok(())
    }

    /// Readings of `volume` at or after `from`, oldest first.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn volume_series(&self, volume: &str, from: i64) -> Result<Vec<VolumePoint>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT ts, used, free FROM volume_samples WHERE volume = ?1 AND ts >= ?2 ORDER BY ts",
        )?;
        let rows = stmt.query_map(params![volume, from], |r| {
            Ok(VolumePoint {
                ts: r.get(0)?,
                used: get_u64(r, 1)?,
                free: get_u64(r, 2)?,
            })
        })?;
        rows.map(|r| r.map_err(StoreError::from)).collect()
    }

    /// Records what one analysis measured.
    ///
    /// # Errors
    ///
    /// SQLite failures; nothing is written when one row fails.
    pub fn record_analysis(&self, volume: &str, ts: i64, items: &[ItemSize]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO analysis_items (volume, ts, kind, label, size)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for item in items {
                stmt.execute(params![
                    volume,
                    ts,
                    item.kind,
                    item.label.as_deref().unwrap_or(""),
                    item.size.map(to_i64),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The latest analysis of `volume` and the one before it.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn latest_analyses(&self, volume: &str) -> Result<(Option<Analysis>, Option<Analysis>)> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT ts FROM analysis_items WHERE volume = ?1 ORDER BY ts DESC LIMIT 2",
        )?;
        let stamps: Vec<i64> = stmt
            .query_map(params![volume], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let load = |ts: Option<&i64>| -> Result<Option<Analysis>> {
            ts.map(|ts| self.analysis(volume, *ts)).transpose()
        };
        Ok((load(stamps.first())?, load(stamps.get(1))?))
    }

    fn analysis(&self, volume: &str, ts: i64) -> Result<Analysis> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT kind, label, size FROM analysis_items WHERE volume = ?1 AND ts = ?2",
        )?;
        let items = stmt
            .query_map(params![volume, ts], |r| {
                let label: String = r.get(1)?;
                let size: Option<i64> = r.get(2)?;
                Ok(ItemSize {
                    kind: r.get(0)?,
                    label: (!label.is_empty()).then_some(label),
                    size: size.map(|s| u64::try_from(s).unwrap_or(0)),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Analysis { ts, items })
    }

    /// Records one clean-up run.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn record_run(&self, volume: &str, run: &RunRecord) -> Result<()> {
        let errors: Vec<String> = run
            .errors
            .iter()
            .take(MAX_ERRORS)
            .map(|e| e.chars().take(MAX_ERROR_CHARS).collect())
            .collect();
        self.conn.execute(
            "INSERT OR REPLACE INTO cleanup_runs (volume, ts, scheduled, freed, items, failed, errors)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                volume,
                run.ts,
                run.scheduled,
                run.freed.map(to_i64),
                run.items,
                run.failed,
                serde_json::to_string(&errors)?,
            ],
        )?;
        Ok(())
    }

    /// The newest `limit` runs of `volume`, newest first.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn runs(&self, volume: &str, limit: u32) -> Result<Vec<RunRecord>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT ts, scheduled, freed, items, failed, errors FROM cleanup_runs
             WHERE volume = ?1 ORDER BY ts DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![volume, limit], |r| {
            let freed: Option<i64> = r.get(2)?;
            let errors: String = r.get(5)?;
            Ok(RunRecord {
                ts: r.get(0)?,
                scheduled: r.get(1)?,
                freed: freed.map(|f| u64::try_from(f).unwrap_or(0)),
                items: r.get(3)?,
                failed: r.get(4)?,
                errors: serde_json::from_str(&errors).unwrap_or_default(),
            })
        })?;
        rows.map(|r| r.map_err(StoreError::from)).collect()
    }

    /// When the weekly schedule last ran on `volume`, nothing to do included.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn last_scheduled_run(&self, volume: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT MAX(ts) FROM cleanup_runs WHERE volume = ?1 AND scheduled = 1",
                params![volume],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Thins hourly readings older than [`HOURLY_DAYS`] to the last one of
    /// each day, and drops everything older than [`KEEP_DAYS`].
    ///
    /// # Errors
    ///
    /// SQLite failures; nothing is deleted when one statement fails.
    pub fn prune(&self, now: i64) -> Result<()> {
        let keep = now.saturating_sub(KEEP_DAYS * DAY);
        let hourly = now.saturating_sub(HOURLY_DAYS * DAY);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM volume_samples WHERE ts < ?1", params![keep])?;
        // A range on the key rather than `t.ts / 86400 = …`, so each lookup
        // is an index seek and not a scan of the volume's rows.
        tx.execute(
            "DELETE FROM volume_samples WHERE ts < ?1 AND ts <> (
                SELECT MAX(t.ts) FROM volume_samples t
                WHERE t.volume = volume_samples.volume
                  AND t.ts >= (volume_samples.ts / 86400) * 86400
                  AND t.ts < (volume_samples.ts / 86400 + 1) * 86400)",
            params![hourly],
        )?;
        tx.execute("DELETE FROM analysis_items WHERE ts < ?1", params![keep])?;
        tx.execute("DELETE FROM cleanup_runs WHERE ts < ?1", params![keep])?;
        tx.commit()?;
        let mut stmt = self.conn.prepare("PRAGMA incremental_vacuum(64)")?;
        let mut rows = stmt.query([])?;
        while rows.next()?.is_some() {}
        Ok(())
    }

    /// Number of readings of `volume`. For tests and the prover.
    ///
    /// # Errors
    ///
    /// SQLite failures.
    pub fn count(&self, volume: &str) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM volume_samples WHERE volume = ?1",
            params![volume],
            |r| r.get(0),
        )?;
        Ok(u64::try_from(n).unwrap_or(0))
    }
}

/// Bytes per day over the last `window` seconds before `now`.
///
/// From the oldest reading inside the window to the newest, and only when
/// those two span at least half the window — otherwise the rate would be
/// an extrapolation from a few hours.
#[must_use]
pub fn rate(points: &[VolumePoint], now: i64, window: i64) -> Option<i64> {
    let latest = points.iter().rfind(|p| p.ts <= now)?;
    let earliest = points
        .iter()
        .find(|p| p.ts >= now - window && p.ts <= now)?;
    let span = latest.ts - earliest.ts;
    if span <= 0 || span * 2 < window {
        return None;
    }
    let delta = i128::from(latest.used) - i128::from(earliest.used);
    i64::try_from(delta * i128::from(DAY) / i128::from(span)).ok()
}

/// Growth over a day, a week and a month.
#[must_use]
pub fn growth(points: &[VolumePoint], now: i64) -> Growth {
    Growth {
        day: rate(points, now, DAY),
        week: rate(points, now, 7 * DAY),
        month: rate(points, now, 30 * DAY),
    }
}

/// At most `max` points, evenly spread, the newest always kept: enough for
/// a sparkline, small enough for the IPC.
#[must_use]
pub fn downsample(points: &[VolumePoint], max: usize) -> Vec<VolumePoint> {
    if max == 0 || points.len() <= max {
        return points.to_vec();
    }
    let step = points.len().div_ceil(max);
    let mut out: Vec<VolumePoint> = points.iter().step_by(step).copied().collect();
    if let (Some(last), Some(kept)) = (points.last(), out.last())
        && last != kept
    {
        if out.len() >= max {
            out.pop();
        }
        out.push(*last);
    }
    out
}

/// What changed between two analyses, largest growth first.
///
/// Matched by kind and label; an item measured in only one of them is left
/// out rather than compared with a zero nobody measured.
#[must_use]
pub fn changes(previous: &[ItemSize], current: &[ItemSize]) -> Vec<ItemChange> {
    let mut out: Vec<ItemChange> = current
        .iter()
        .filter_map(|now| {
            let then = previous
                .iter()
                .find(|p| p.kind == now.kind && p.label == now.label)?;
            let (then_size, now_size) = (then.size?, now.size?);
            (then_size != now_size).then(|| ItemChange {
                kind: now.kind.clone(),
                label: now.label.clone(),
                then: then_size,
                now: now_size,
            })
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.delta()));
    out
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn get_u64(r: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<u64> {
    let v: i64 = r.get(idx)?;
    Ok(u64::try_from(v).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn point(ts: i64, used: u64) -> VolumePoint {
        VolumePoint {
            ts,
            used,
            free: 500 * GB - used,
        }
    }

    fn item(kind: &str, label: Option<&str>, size: Option<u64>) -> ItemSize {
        ItemSize {
            kind: kind.into(),
            label: label.map(Into::into),
            size,
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vitals-drivehist-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("drive-history.sqlite")
    }

    #[test]
    fn everything_written_survives_closing_and_reopening_the_file() {
        let path = temp_path("roundtrip");
        let run = RunRecord {
            ts: 5_000,
            scheduled: true,
            freed: Some(3 * GB + 17),
            items: 4,
            failed: 1,
            errors: vec!["docker: the engine is not running".into()],
        };
        {
            let h = DriveHistory::open(&path).unwrap();
            h.record_volume("vol-a", point(1_000, 120 * GB + 3))
                .unwrap();
            h.record_volume("vol-a", point(4_600, 121 * GB + 7))
                .unwrap();
            h.record_volume("vol-b", point(1_000, 7)).unwrap();
            h.record_analysis(
                "vol-a",
                2_000,
                &[
                    item("tempOld", None, Some(4 * GB + 1)),
                    item("compactVhd", Some("Ubuntu"), None),
                ],
            )
            .unwrap();
            h.record_run("vol-a", &run).unwrap();
        }
        let h = DriveHistory::open(&path).unwrap();
        assert_eq!(
            h.volume_series("vol-a", 0).unwrap(),
            [point(1_000, 120 * GB + 3), point(4_600, 121 * GB + 7)]
        );
        let (latest, previous) = h.latest_analyses("vol-a").unwrap();
        let mut items = latest.unwrap().items;
        items.sort_by(|a, b| a.kind.cmp(&b.kind));
        assert_eq!(
            items,
            [
                item("compactVhd", Some("Ubuntu"), None),
                item("tempOld", None, Some(4 * GB + 1)),
            ],
            "an unmeasured size comes back unmeasured, not as 0"
        );
        assert!(previous.is_none());
        assert_eq!(h.runs("vol-a", 10).unwrap(), [run]);
        assert_eq!(h.last_scheduled_run("vol-a").unwrap(), Some(5_000));
        assert_eq!(h.last_scheduled_run("vol-b").unwrap(), None);
    }

    #[test]
    fn a_file_from_a_newer_build_is_refused_rather_than_misread() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 7;").unwrap();
        let err = DriveHistory::configure(conn).unwrap_err();
        assert!(matches!(err, StoreError::SchemaTooNew { found: 7, .. }));
    }

    #[test]
    fn opening_an_existing_file_again_does_not_migrate_it_twice() {
        let path = temp_path("remigrate");
        drop(DriveHistory::open(&path).unwrap());
        let h = DriveHistory::open(&path).unwrap();
        let version: i64 = h
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn a_manual_run_never_counts_as_the_weekly_one() {
        let h = DriveHistory::in_memory().unwrap();
        h.record_run(
            "v",
            &RunRecord {
                ts: 10,
                scheduled: false,
                freed: None,
                items: 1,
                failed: 0,
                errors: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(h.last_scheduled_run("v").unwrap(), None);
    }

    #[test]
    fn old_hourly_readings_thin_to_one_a_day_and_the_oldest_go_entirely() {
        let h = DriveHistory::in_memory().unwrap();
        let now = 400 * DAY;
        // Hourly for the whole 200 days.
        for hour in 0..(200 * 24) {
            let ts = now - hour * 3_600;
            h.record_volume("v", point(ts, GB + u64::try_from(hour).unwrap()))
                .unwrap();
        }
        h.prune(now).unwrap();
        let kept = h.volume_series("v", 0).unwrap();
        assert!(kept.iter().all(|p| p.ts >= now - KEEP_DAYS * DAY));
        let recent = kept
            .iter()
            .filter(|p| p.ts >= now - HOURLY_DAYS * DAY)
            .count();
        assert!(recent >= usize::try_from(HOURLY_DAYS * 24).unwrap());
        let old: Vec<&VolumePoint> = kept
            .iter()
            .filter(|p| p.ts < now - HOURLY_DAYS * DAY)
            .collect();
        let days: std::collections::HashSet<i64> = old.iter().map(|p| p.ts / DAY).collect();
        assert_eq!(old.len(), days.len(), "one reading per day once thinned");
        // And every one of those days keeps it: thinning must not empty a day.
        let first = (now - KEEP_DAYS * DAY) / DAY;
        let last = (now - HOURLY_DAYS * DAY - 1) / DAY;
        for day in first..=last {
            assert!(days.contains(&day), "day {day} lost its reading");
        }
        // The last reading of each day is the one kept.
        assert!(old.iter().all(|p| (p.ts % DAY) >= DAY - 3_600));
    }

    #[test]
    fn growth_is_bytes_per_day_from_the_oldest_to_the_newest_reading_in_the_window() {
        let now = 100 * DAY;
        // +2 GB a day for ten days.
        let points: Vec<VolumePoint> = (0..=10)
            .map(|d| {
                point(
                    now - (10 - d) * DAY,
                    100 * GB + 2 * GB * u64::try_from(d).unwrap(),
                )
            })
            .collect();
        let g = growth(&points, now);
        assert_eq!(g.day, Some(i64::try_from(2 * GB).unwrap()));
        assert_eq!(g.week, Some(i64::try_from(2 * GB).unwrap()));
        // Ten days of a thirty-day window is too little to call a month.
        assert_eq!(g.month, None);
    }

    #[test]
    fn a_shrinking_drive_has_negative_growth_and_too_few_readings_have_none() {
        let now = 10 * DAY;
        let points = [point(now - DAY, 50 * GB), point(now, 49 * GB)];
        assert_eq!(rate(&points, now, DAY), Some(-i64::try_from(GB).unwrap()));
        assert_eq!(rate(&points[..1], now, DAY), None);
        assert_eq!(rate(&[], now, DAY), None);
        // Two hours of a day: not enough to say.
        let short = [point(now - 7_200, 1), point(now, 2)];
        assert_eq!(rate(&short, now, DAY), None);
    }

    #[test]
    fn a_sparkline_gets_at_most_the_points_asked_for_and_always_the_newest() {
        let points: Vec<VolumePoint> = (0..1_000).map(|i| point(i, 1)).collect();
        let small = downsample(&points, 120);
        assert!(small.len() <= 120, "{}", small.len());
        assert_eq!(small.last(), points.last());
        assert_eq!(downsample(&points[..5], 120).len(), 5);
    }

    #[test]
    fn changes_compare_only_items_measured_both_times_largest_growth_first() {
        let previous = [
            item("dockerVhd", None, Some(100 * GB)),
            item("tempOld", None, Some(4 * GB)),
            item("npmCache", None, None),
            item("rustTarget", Some("E:\\a\\target"), Some(10 * GB)),
        ];
        let current = [
            item("dockerVhd", None, Some(142 * GB)),
            item("tempOld", None, Some(GB)),
            item("npmCache", None, Some(3 * GB)),
            item("rustTarget", Some("E:\\a\\target"), Some(10 * GB)),
            item("rustTarget", Some("E:\\b\\target"), Some(9 * GB)),
        ];
        let got = changes(&previous, &current);
        let summary: Vec<(&str, i64)> = got.iter().map(|c| (c.kind.as_str(), c.delta())).collect();
        assert_eq!(
            summary,
            [
                ("dockerVhd", i64::try_from(42 * GB).unwrap()),
                ("tempOld", -i64::try_from(3 * GB).unwrap()),
            ]
        );
    }
}
