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
    #[error("frame did not serialise: {0}")]
    Encode(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Bumped when a migration is added to [`Store::migrate`].
const SCHEMA_VERSION: i64 = 1;

/// Free pages handed back to the filesystem per maintenance pass.
///
/// Bounded so a pass after a large prune never stalls the sampler thread
/// for one long vacuum; 512 pages of 4 KiB is 2 MiB a minute, which drains
/// any realistic backlog within the hour.
const RECLAIM_PAGES: u32 = 512;

/// Below this size an existing file is converted to incremental
/// auto-vacuum on open.
///
/// The conversion is a full `VACUUM`: it rewrites the file, which is
/// instant for the few megabytes the default tiers produce and is exactly
/// what must not happen at launch to a file that is somehow hundreds of
/// megabytes. A larger file is converted on the next Clear instead.
const CONVERT_ON_OPEN_BELOW: u64 = 16 * 1024 * 1024;

/// Upper bound on eviction rounds in one maintenance pass, so a budget
/// that cannot be met (smaller than an empty schema) cannot spin.
const MAX_EVICTION_ROUNDS: u32 = 64;

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
        // Incremental auto-vacuum lets `maintain` hand pruned pages back to
        // the filesystem a few at a time. Without it a file only ever grows:
        // deleted rows become free pages that SQLite reuses but never
        // returns, so the 512 MB cap would bound the data, not the file.
        // The pragma only takes effect before the first table exists, so it
        // must precede both the WAL switch and `migrate`; on an existing
        // file it is a no-op until a `VACUUM` (see `convert_if_small`).
        conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
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

    /// Converts a pre-existing, non-incremental file while that is cheap.
    ///
    /// Called by the writer ([`crate::Recorder::open`]) only. Readers open
    /// their own connection per command, and a `VACUUM` from one of them
    /// would contend with the sampler's writes for the whole rewrite.
    pub fn convert_if_small(&self) -> Result<()> {
        if self.auto_vacuum_mode()? == AUTO_VACUUM_INCREMENTAL {
            return Ok(());
        }
        if self.size_bytes()? < CONVERT_ON_OPEN_BELOW {
            self.conn.execute_batch("VACUUM;")?;
        }
        Ok(())
    }

    fn auto_vacuum_mode(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA auto_vacuum", [], |r| r.get(0))?)
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
    ///
    /// Then enforces the byte budget ([`RetentionPolicy::max_bytes`]) and
    /// returns a bounded number of free pages to the filesystem.
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

        self.enforce_budget()?;
        self.reclaim()?;
        Ok(())
    }

    /// Evicts the oldest data until the live pages fit the byte budget.
    ///
    /// Measured on live pages (`page_count - freelist_count`), not the file
    /// size: free pages are reused before the file grows, so counting them
    /// would evict real data to pay for space that is already empty — and
    /// on a file not yet converted to incremental auto-vacuum the file never
    /// shrinks, so the loop would delete everything.
    ///
    /// Oldest first across all tiers, as the policy promises: each round
    /// takes the tier holding the single oldest row and drops the oldest
    /// quarter of it. The defaults produce a few megabytes, so this is a
    /// guard against the unforeseen rather than a routine path.
    fn enforce_budget(&self) -> Result<()> {
        let budget = self.policy.max_bytes;
        for _ in 0..MAX_EVICTION_ROUNDS {
            if self.live_bytes()? <= budget {
                return Ok(());
            }
            let Some(table) = self.tier_with_oldest_row()? else {
                return Ok(());
            };
            let rows: i64 =
                self.conn
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
            let evict = (rows / 4).max(1);
            self.conn.execute(
                &format!(
                    "DELETE FROM {table} WHERE ts IN (
                        SELECT ts FROM {table} ORDER BY ts LIMIT ?1)"
                ),
                params![evict],
            )?;
        }
        Ok(())
    }

    fn tier_with_oldest_row(&self) -> Result<Option<&'static str>> {
        let mut oldest: Option<(i64, &'static str)> = None;
        for (resolution, _) in TIERS {
            let table = table_for(resolution);
            let min: Option<i64> =
                self.conn
                    .query_row(&format!("SELECT MIN(ts) FROM {table}"), [], |r| r.get(0))?;
            if let Some(ts) = min
                && oldest.is_none_or(|(best, _)| ts < best)
            {
                oldest = Some((ts, table));
            }
        }
        Ok(oldest.map(|(_, table)| table))
    }

    /// Bytes held by pages that contain data.
    pub fn live_bytes(&self) -> Result<u64> {
        let free: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        let page_size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let total = self.size_bytes()?;
        let free_bytes = u64::try_from(free.saturating_mul(page_size)).unwrap_or(0);
        Ok(total.saturating_sub(free_bytes))
    }

    /// Returns up to [`RECLAIM_PAGES`] free pages to the filesystem.
    ///
    /// A no-op on a file not yet converted to incremental auto-vacuum,
    /// which SQLite handles by ignoring the pragma.
    ///
    /// Stepped to completion rather than run with `execute_batch`: the
    /// pragma frees one page per `sqlite3_step`, and a batch steps each
    /// statement once — so it returned exactly one page and the file never
    /// shrank (caught by the budget test, not by reading the docs).
    fn reclaim(&self) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA incremental_vacuum({RECLAIM_PAGES})"))?;
        let mut rows = stmt.query([])?;
        while rows.next()?.is_some() {}
        Ok(())
    }

    /// Folds the write-ahead log into the database and truncates it.
    ///
    /// Called on shutdown. Left alone, the `-wal` file keeps its high-water
    /// size between launches, which for a store written once a second is
    /// several megabytes of duplicate pages sitting next to the real file.
    /// A reader holding a snapshot makes the checkpoint partial rather than
    /// failing, which is fine: the next launch finishes it.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
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
    ///
    /// Also the moment a file too large to convert on open is switched to
    /// incremental auto-vacuum: the `VACUUM` here rewrites the file anyway.
    pub fn clear(&self) -> Result<()> {
        for (_, suffix) in TIERS {
            self.conn
                .execute(&format!("DELETE FROM machine_{suffix}"), [])?;
        }
        self.conn.execute("DELETE FROM rollup_cursor", [])?;
        self.conn.execute("DELETE FROM flight_frames", [])?;
        self.conn
            .execute_batch("PRAGMA auto_vacuum = INCREMENTAL; VACUUM;")?;
        Ok(())
    }

    // ── Flight recorder ────────────────────────────────────────────────

    /// Empties the legacy `flight_frames` table.
    ///
    /// The flight recorder lives in memory now ([`crate::FlightRecorder`]):
    /// writing every frame here cost an `INSERT`, a trimming `DELETE` and a
    /// WAL commit a second, all day, for a user who never asked for history.
    /// The table stays so older and newer builds can share one file without
    /// a migration; this clears whatever an older build left in it, because
    /// those frames describe a previous session and PRIVACY.md promises the
    /// recording is cleared at each launch.
    pub fn clear_flight_frames(&self) -> Result<()> {
        self.conn.execute("DELETE FROM flight_frames", [])?;
        Ok(())
    }

    /// Rows left in the legacy `flight_frames` table. For tests.
    pub fn legacy_flight_rows(&self) -> Result<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM flight_frames", [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    }
}

/// `PRAGMA auto_vacuum` value for incremental mode.
const AUTO_VACUUM_INCREMENTAL: i64 = 2;

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
    fn clear_empties_every_table() {
        let s = store();
        s.insert(&sample(1, 1.0)).unwrap();
        s.conn
            .execute(
                "INSERT INTO flight_frames (seq, ts, frame) VALUES (1, 1, x'7b7d')",
                [],
            )
            .unwrap();
        s.clear().unwrap();
        assert_eq!(s.count(Resolution::Second).unwrap(), 0);
        assert_eq!(s.legacy_flight_rows().unwrap(), 0);
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("vitals-db-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("history.sqlite")
    }

    #[test]
    fn a_new_file_is_created_with_incremental_auto_vacuum_so_pruned_space_can_be_returned() {
        let path = temp_path("autovac");
        let s = Store::open(&path, RetentionPolicy::default()).unwrap();
        assert_eq!(s.auto_vacuum_mode().unwrap(), AUTO_VACUUM_INCREMENTAL);
    }

    #[test]
    fn a_small_legacy_file_is_converted_to_incremental_auto_vacuum_by_the_writer() {
        let path = temp_path("convert");
        {
            // What an older build produced: no auto-vacuum at all.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA auto_vacuum = NONE; CREATE TABLE t (x INTEGER);")
                .unwrap();
        }
        let s = Store::open(&path, RetentionPolicy::default()).unwrap();
        // A reader's open must not rewrite the file under the sampler.
        assert_eq!(s.auto_vacuum_mode().unwrap(), 0);
        s.convert_if_small().unwrap();
        assert_eq!(s.auto_vacuum_mode().unwrap(), AUTO_VACUUM_INCREMENTAL);
    }

    #[test]
    fn maintain_evicts_the_oldest_rows_until_the_store_fits_its_byte_budget() {
        let path = temp_path("budget");
        let mut s = Store::open(&path, RetentionPolicy::default()).unwrap();
        // Two days of seconds: far more than a 256 KiB budget holds, and all
        // inside a retention window long enough that pruning alone keeps it.
        s.set_policy(RetentionPolicy {
            enabled: true,
            tiers: vec![(Resolution::Second, 10 * 86_400)],
            max_bytes: u64::MAX,
        });
        let tx = s.conn.unchecked_transaction().unwrap();
        for ts in 0..20_000 {
            Store::insert_into(&tx, "machine_1s", &sample(ts, 1.0)).unwrap();
        }
        tx.commit().unwrap();
        let before = s.live_bytes().unwrap();
        let budget = 256 * 1024;
        assert!(before > budget, "fixture too small: {before}");

        s.set_policy(RetentionPolicy {
            max_bytes: budget,
            ..s.policy().clone()
        });
        s.maintain(20_000).unwrap();

        assert!(s.live_bytes().unwrap() <= budget, "still over budget");
        let kept = s.query_at(Resolution::Second, 0, 30_000).unwrap();
        assert!(!kept.is_empty(), "evicted everything");
        // Oldest first: what survives is the newest end, contiguous to it.
        assert_eq!(kept.last().map(|x| x.ts), Some(19_999));
        assert!(kept[0].ts > 0, "the oldest rows were not the ones evicted");
        // And the freed pages went back to the filesystem, not just the
        // freelist: the file itself shrank.
        assert!(s.size_bytes().unwrap() < before, "file did not shrink");
    }

    #[test]
    fn a_store_within_its_budget_loses_nothing_to_eviction() {
        let s = store();
        for ts in 0..100 {
            s.insert(&sample(ts, 1.0)).unwrap();
        }
        s.maintain(100).unwrap();
        assert_eq!(s.count(Resolution::Second).unwrap(), 100);
    }

    #[test]
    fn a_checkpoint_truncates_the_write_ahead_log() {
        let path = temp_path("ckpt");
        let s = Store::open(&path, RetentionPolicy::default()).unwrap();
        for ts in 0..500 {
            s.insert(&sample(ts, 1.0)).unwrap();
        }
        let wal = path.with_extension("sqlite-wal");
        assert!(std::fs::metadata(&wal).unwrap().len() > 0);
        s.checkpoint().unwrap();
        assert_eq!(std::fs::metadata(&wal).unwrap().len(), 0);
    }

    #[test]
    fn refuses_a_database_from_the_future() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 99;").unwrap();
        let err = Store::configure(conn, RetentionPolicy::default()).unwrap_err();
        assert!(matches!(err, StoreError::SchemaTooNew { found: 99, .. }));
    }
}
