//! The SQLite store.
//!
//! One file, WAL mode, one writer (the sampler thread) and any number of
//! readers. Every tier is its own table with the same shape, so a read at any
//! resolution is the same indexed range scan; rollups are written by
//! [`Store::maintain`], which the caller runs on a timer rather than on every
//! insert so the hot path stays a single `INSERT`.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use vitals_core::history::MachineSample;

use crate::retention::{Resolution, RetentionPolicy};

/// Errors the store can produce.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("schema version {found} is newer than this build understands ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Bumped when a migration is added to [`Store::migrate`].
const SCHEMA_VERSION: i64 = 1;

/// The tiers, finest first. Each is a table named `machine_<suffix>`.
const TIERS: [(Resolution, &str); 4] = [
    (Resolution::Second, "1s"),
    (Resolution::Minute, "1m"),
    (Resolution::FiveMinutes, "5m"),
    (Resolution::Hour, "1h"),
];

fn table_for(resolution: Resolution) -> &'static str {
    match resolution {
        Resolution::Second => "machine_1s",
        Resolution::Minute => "machine_1m",
        Resolution::FiveMinutes => "machine_5m",
        Resolution::Hour | Resolution::Day => "machine_1h",
    }
}

/// A handle to the database. Not `Sync`: SQLite connections are not, and the
/// store is owned by one thread anyway. Open a second [`Store`] on the same
/// path for a reader.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    policy: RetentionPolicy,
}

impl Store {
    /// Opens (creating if needed) the database at `path` and migrates it.
    pub fn open(path: &Path, policy: RetentionPolicy) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::configure(conn, policy)
    }

    /// An in-memory store, for tests and for the flight recorder's staging.
    pub fn in_memory(policy: RetentionPolicy) -> Result<Self> {
        Self::configure(Connection::open_in_memory()?, policy)
    }

    fn configure(conn: Connection, policy: RetentionPolicy) -> Result<Self> {
        // WAL lets the UI read while the sampler writes; `synchronous=NORMAL`
        // is safe under WAL and avoids an fsync per insert. Losing the last
        // second of a usage chart in a power cut is acceptable; a 1 Hz fsync
        // on a laptop is not.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;",
        )?;
        let mut store = Self { conn, policy };
        store.migrate()?;
        Ok(store)
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
            for (_, suffix) in TIERS {
                tx.execute_batch(&format!(
                    "CREATE TABLE IF NOT EXISTS machine_{suffix} (
                        ts INTEGER PRIMARY KEY,
                        cpu REAL NOT NULL,
                        cpu_kernel REAL NOT NULL,
                        mem_used INTEGER NOT NULL,
                        mem_total INTEGER NOT NULL,
                        disk_r INTEGER NOT NULL,
                        disk_w INTEGER NOT NULL,
                        net_rx INTEGER NOT NULL,
                        net_tx INTEGER NOT NULL,
                        gpu REAL,
                        cpu_temp REAL,
                        power REAL
                    ) WITHOUT ROWID;"
                ))?;
            }
            // Where each rollup left off, so maintenance is incremental.
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS rollup_cursor (
                    tier TEXT PRIMARY KEY,
                    up_to INTEGER NOT NULL
                );
                -- Flight recorder: raw frames as the wire sends them, capped by
                -- the caller. Kept separate because it is a debugging artefact
                -- with a lifetime of minutes, not a time series.
                CREATE TABLE IF NOT EXISTS flight_frames (
                    seq INTEGER PRIMARY KEY,
                    ts INTEGER NOT NULL,
                    frame BLOB NOT NULL
                );
                PRAGMA user_version = 1;",
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    #[must_use]
    pub fn policy(&self) -> &RetentionPolicy {
        &self.policy
    }

    pub fn set_policy(&mut self, policy: RetentionPolicy) {
        self.policy = policy;
    }

    /// Records one finest-tier sample. The hot path: one statement.
    ///
    /// `ts` is the primary key at one-second resolution, so at the "fast"
    /// 500 ms rate two frames share a second and the later one wins. That is
    /// the tier's resolution rather than a loss: a second-resolution series
    /// holds one value per second by definition, and the finer detail is
    /// already on screen in the live charts.
    pub fn insert(&self, sample: &MachineSample) -> Result<()> {
        Self::insert_into(&self.conn, "machine_1s", sample)
    }

    fn insert_into(conn: &Connection, table: &str, s: &MachineSample) -> Result<()> {
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO {table}
                 (ts, cpu, cpu_kernel, mem_used, mem_total, disk_r, disk_w, net_rx, net_tx, gpu, cpu_temp, power)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
            ),
            params![
                s.ts,
                s.cpu_percent,
                s.cpu_kernel_percent,
                to_i64(s.memory_used),
                to_i64(s.memory_total),
                to_i64(s.disk_read_bps),
                to_i64(s.disk_write_bps),
                to_i64(s.net_rx_bps),
                to_i64(s.net_tx_bps),
                s.gpu_percent,
                s.cpu_temp_c,
                s.power_draw_w,
            ],
        )?;
        Ok(())
    }

    /// Reads samples in `[from, to]` at the resolution the policy assigns to
    /// that age, given `now`.
    pub fn query(&self, from: i64, to: i64, now: i64) -> Result<Vec<MachineSample>> {
        let age = u64::try_from(now.saturating_sub(from)).unwrap_or(0);
        let resolution = self.policy.resolution_for_age(age);
        self.query_at(resolution, from, to)
    }

    /// Reads samples from one tier explicitly.
    pub fn query_at(
        &self,
        resolution: Resolution,
        from: i64,
        to: i64,
    ) -> Result<Vec<MachineSample>> {
        let table = table_for(resolution);
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT ts, cpu, cpu_kernel, mem_used, mem_total, disk_r, disk_w, net_rx, net_tx, gpu, cpu_temp, power
             FROM {table} WHERE ts BETWEEN ?1 AND ?2 ORDER BY ts"
        ))?;
        let rows = stmt.query_map(params![from, to], row_to_sample)?;
        rows.map(|r| r.map_err(StoreError::from)).collect()
    }

    /// Rolls finer tiers into coarser ones and prunes past retention.
    ///
    /// Idempotent and incremental: each tier remembers the last bucket it
    /// completed, so calling this every minute costs a handful of small
    /// queries. Buckets still open at `now` are left alone — a five-minute
    /// average of ninety seconds would be a lie about the other 210.
    pub fn maintain(&self, now: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        for window in TIERS.windows(2) {
            let (fine, _) = window[0];
            let (coarse, coarse_suffix) = window[1];
            let bucket = i64::try_from(coarse.seconds()).unwrap_or(i64::MAX);

            let cursor: i64 = tx
                .query_row(
                    "SELECT up_to FROM rollup_cursor WHERE tier = ?1",
                    params![coarse_suffix],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(0);

            // Only buckets that have fully elapsed.
            let last_complete = (now / bucket) * bucket;
            let mut start = if cursor == 0 {
                // First run: begin at the oldest fine row, bucket-aligned.
                let oldest: Option<i64> = tx
                    .query_row(
                        &format!("SELECT MIN(ts) FROM {}", table_for(fine)),
                        [],
                        |r| r.get(0),
                    )
                    .optional()?
                    .flatten();
                match oldest {
                    Some(ts) => (ts / bucket) * bucket,
                    None => continue,
                }
            } else {
                cursor
            };

            while start + bucket <= last_complete {
                let end = start + bucket - 1;
                let fine_rows = Self::query_at_tx(&tx, fine, start, end)?;
                if let Some(rolled) = MachineSample::rollup(start, &fine_rows) {
                    Self::insert_into(&tx, table_for(coarse), &rolled)?;
                }
                start += bucket;
            }

            tx.execute(
                "INSERT OR REPLACE INTO rollup_cursor (tier, up_to) VALUES (?1, ?2)",
                params![coarse_suffix, start],
            )?;
        }

        // Prune each tier to its retention window.
        for (resolution, retain_secs) in &self.policy.tiers {
            let cutoff = now.saturating_sub(i64::try_from(*retain_secs).unwrap_or(i64::MAX));
            tx.execute(
                &format!("DELETE FROM {} WHERE ts < ?1", table_for(*resolution)),
                params![cutoff],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    fn query_at_tx(
        tx: &rusqlite::Transaction<'_>,
        resolution: Resolution,
        from: i64,
        to: i64,
    ) -> Result<Vec<MachineSample>> {
        let mut stmt = tx.prepare_cached(&format!(
            "SELECT ts, cpu, cpu_kernel, mem_used, mem_total, disk_r, disk_w, net_rx, net_tx, gpu, cpu_temp, power
             FROM {} WHERE ts BETWEEN ?1 AND ?2 ORDER BY ts",
            table_for(resolution)
        ))?;
        let rows = stmt.query_map(params![from, to], row_to_sample)?;
        rows.map(|r| r.map_err(StoreError::from)).collect()
    }

    /// Number of rows in a tier. For the settings screen's size estimate and
    /// for tests.
    pub fn count(&self, resolution: Resolution) -> Result<u64> {
        let n: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", table_for(resolution)),
            [],
            |r| r.get(0),
        )?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// Bytes the database occupies on disk, as SQLite reports them.
    pub fn size_bytes(&self) -> Result<u64> {
        let pages: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok(u64::try_from(pages.saturating_mul(page_size)).unwrap_or(0))
    }

    /// Deletes everything and reclaims the space.
    pub fn clear(&self) -> Result<()> {
        for (_, suffix) in TIERS {
            self.conn
                .execute(&format!("DELETE FROM machine_{suffix}"), [])?;
        }
        self.conn.execute("DELETE FROM rollup_cursor", [])?;
        self.conn.execute("DELETE FROM flight_frames", [])?;
        self.conn.execute_batch("VACUUM;")?;
        Ok(())
    }

    // ── Flight recorder ────────────────────────────────────────────────

    /// Appends a raw frame, keeping at most `keep` of them.
    ///
    /// The frame is stored as the bytes the caller hands over (JSON on the
    /// wire today) so the export is exactly what the UI saw, not a
    /// re-serialisation that might disagree with it.
    pub fn record_frame(&self, seq: i64, ts: i64, frame: &[u8], keep: u64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO flight_frames (seq, ts, frame) VALUES (?1, ?2, ?3)",
            params![seq, ts, frame],
        )?;
        tx.execute(
            "DELETE FROM flight_frames WHERE seq NOT IN (
                SELECT seq FROM flight_frames ORDER BY seq DESC LIMIT ?1)",
            params![i64::try_from(keep).unwrap_or(i64::MAX)],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Every recorded frame, oldest first.
    pub fn flight_frames(&self) -> Result<Vec<(i64, i64, Vec<u8>)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT seq, ts, frame FROM flight_frames ORDER BY seq")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.map(|r| r.map_err(StoreError::from)).collect()
    }
}

/// SQLite has one integer type, signed 64-bit. Byte counts above 2^63 do not
/// occur on hardware that exists; saturating is fine and never wraps.
fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn get_u64(r: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<u64> {
    let v: i64 = r.get(idx)?;
    Ok(u64::try_from(v).unwrap_or(0))
}

fn row_to_sample(r: &rusqlite::Row<'_>) -> rusqlite::Result<MachineSample> {
    Ok(MachineSample {
        ts: r.get(0)?,
        cpu_percent: r.get(1)?,
        cpu_kernel_percent: r.get(2)?,
        memory_used: get_u64(r, 3)?,
        memory_total: get_u64(r, 4)?,
        disk_read_bps: get_u64(r, 5)?,
        disk_write_bps: get_u64(r, 6)?,
        net_rx_bps: get_u64(r, 7)?,
        net_tx_bps: get_u64(r, 8)?,
        gpu_percent: r.get(9)?,
        cpu_temp_c: r.get(10)?,
        power_draw_w: r.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ts: i64, cpu: f32) -> MachineSample {
        MachineSample {
            ts,
            cpu_percent: cpu,
            cpu_kernel_percent: 1.0,
            memory_used: 10,
            memory_total: 100,
            disk_read_bps: 1,
            disk_write_bps: 1,
            net_rx_bps: 1,
            net_tx_bps: 1,
            gpu_percent: None,
            cpu_temp_c: Some(50.0),
            power_draw_w: None,
        }
    }

    fn store() -> Store {
        Store::in_memory(RetentionPolicy {
            enabled: true,
            ..RetentionPolicy::default()
        })
        .unwrap()
    }

    #[test]
    fn insert_then_read_back_round_trips_every_field() {
        let s = store();
        let want = sample(1_000, 42.5);
        s.insert(&want).unwrap();
        let got = s.query_at(Resolution::Second, 0, 2_000).unwrap();
        assert_eq!(got, vec![want]);
    }

    #[test]
    fn maintain_rolls_complete_minutes_and_leaves_the_open_one() {
        let s = store();
        // 90 seconds of 1 Hz data starting at t=0; `now` is 90.
        for ts in 0..90 {
            s.insert(&sample(ts, if ts < 60 { 10.0 } else { 90.0 }))
                .unwrap();
        }
        s.maintain(90).unwrap();

        let minutes = s.query_at(Resolution::Minute, 0, 1_000).unwrap();
        // Only [0,60) is complete. The 30 s of the second minute must NOT be
        // presented as a minute average.
        assert_eq!(minutes.len(), 1);
        assert_eq!(minutes[0].ts, 0);
        assert!((minutes[0].cpu_percent - 10.0).abs() < 0.01);
    }

    #[test]
    fn maintain_is_incremental_and_idempotent() {
        let s = store();
        for ts in 0..120 {
            s.insert(&sample(ts, 1.0)).unwrap();
        }
        s.maintain(120).unwrap();
        s.maintain(120).unwrap();
        assert_eq!(s.count(Resolution::Minute).unwrap(), 2);

        for ts in 120..180 {
            s.insert(&sample(ts, 1.0)).unwrap();
        }
        s.maintain(180).unwrap();
        assert_eq!(s.count(Resolution::Minute).unwrap(), 3);
    }

    #[test]
    fn prune_respects_each_tier_retention() {
        let mut s = store();
        s.set_policy(RetentionPolicy {
            enabled: true,
            tiers: vec![(Resolution::Second, 10), (Resolution::Minute, 1_000)],
            max_bytes: u64::MAX,
        });
        for ts in 0..100 {
            s.insert(&sample(ts, 1.0)).unwrap();
        }
        s.maintain(100).unwrap();
        // Second tier keeps 10 s → ts >= 90.
        let fine = s.query_at(Resolution::Second, 0, 1_000).unwrap();
        assert!(fine.iter().all(|x| x.ts >= 90), "{fine:?}");
        assert_eq!(fine.len(), 10);
    }

    #[test]
    fn query_picks_the_tier_by_age() {
        let s = store();
        for ts in 0..120 {
            s.insert(&sample(ts, 1.0)).unwrap();
        }
        s.maintain(120).unwrap();
        // Asking for the last 30 s → seconds; asking back 2 hours → minutes.
        assert_eq!(s.query(90, 120, 120).unwrap().len(), 30);
        let now = 7_200;
        let coarse = s.query(0, now, now).unwrap();
        assert!(coarse.iter().all(|x| x.ts % 60 == 0));
    }

    #[test]
    fn flight_recorder_keeps_only_the_last_n() {
        let s = store();
        for seq in 0..10 {
            s.record_frame(seq, seq, b"{}", 4).unwrap();
        }
        let frames = s.flight_frames().unwrap();
        assert_eq!(
            frames.iter().map(|f| f.0).collect::<Vec<_>>(),
            vec![6, 7, 8, 9]
        );
    }

    #[test]
    fn clear_empties_every_table() {
        let s = store();
        s.insert(&sample(1, 1.0)).unwrap();
        s.record_frame(1, 1, b"{}", 10).unwrap();
        s.clear().unwrap();
        assert_eq!(s.count(Resolution::Second).unwrap(), 0);
        assert!(s.flight_frames().unwrap().is_empty());
    }

    #[test]
    fn refuses_a_database_from_the_future() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 99;").unwrap();
        let err = Store::configure(conn, RetentionPolicy::default()).unwrap_err();
        assert!(matches!(err, StoreError::SchemaTooNew { found: 99, .. }));
    }
}
