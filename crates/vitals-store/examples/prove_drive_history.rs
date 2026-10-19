//! Proves the drive history against this machine's real volumes.
//!
//! Reads every local volume the way the hourly sampler does, writes the
//! readings (plus a synthetic hour-earlier reading so a rate exists) to a
//! fresh file, reopens it, and prints what the wizard would show.
//!
//! Run with: `cargo run -p vitals-store --example prove_drive_history`

#![allow(clippy::expect_used, clippy::print_stdout)]

use std::time::{SystemTime, UNIX_EPOCH};

use vitals_store::drive_history::{DriveHistory, ItemSize, VolumePoint, changes, growth};

fn main() {
    let dir = std::env::temp_dir().join("vitals-prove-drive-history");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("drive-history.sqlite");
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_secs(),
    )
    .expect("i64");

    let readings = volumes();
    {
        let history = DriveHistory::open(&path).expect("open");
        for (key, mount, used, free) in &readings {
            // Yesterday, 1 GiB emptier, so the day rate has two ends.
            let earlier = VolumePoint {
                ts: now - 86_400,
                used: used.saturating_sub(1 << 30),
                free: free + (1 << 30),
            };
            history.record_volume(key, earlier).expect("write earlier");
            history
                .record_volume(
                    key,
                    VolumePoint {
                        ts: now,
                        used: *used,
                        free: *free,
                    },
                )
                .expect("write");
            println!("wrote {mount} as {key}: used {used} free {free}");
        }
        history
            .record_analysis(
                "prove",
                now - 3_600,
                &[ItemSize {
                    kind: "dockerDisk".into(),
                    label: None,
                    size: Some(100 << 30),
                }],
            )
            .expect("analysis 1");
        history
            .record_analysis(
                "prove",
                now,
                &[ItemSize {
                    kind: "dockerDisk".into(),
                    label: None,
                    size: Some(142 << 30),
                }],
            )
            .expect("analysis 2");
    }

    let history = DriveHistory::open(&path).expect("reopen");
    for (key, mount, used, _) in &readings {
        let series = history.volume_series(key, 0).expect("read");
        assert_eq!(
            series.last().map(|p| p.used),
            Some(*used),
            "read back what was written"
        );
        let g = growth(&series, now);
        println!(
            "read {mount}: {} readings, growth/day {:?} bytes (expected ~{})",
            series.len(),
            g.day,
            1_u64 << 30
        );
    }
    let (latest, previous) = history.latest_analyses("prove").expect("analyses");
    let delta = changes(
        &previous.expect("previous").items,
        &latest.expect("latest").items,
    );
    println!(
        "analysis delta: {:?}",
        delta
            .iter()
            .map(|c| (&c.kind, c.delta()))
            .collect::<Vec<_>>()
    );
    let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
    println!("file: {} ({bytes} bytes)", path.display());
}

#[cfg(windows)]
fn volumes() -> Vec<(String, String, u64, u64)> {
    use vitals_win::storage::drive_cleanup::history::volume_key;
    vitals_win::disk::volumes::enumerate_volumes_cached()
        .into_iter()
        .filter(|v| {
            !matches!(
                v.kind,
                vitals_core::metrics::DiskKind::Network | vitals_core::metrics::DiskKind::Optical
            )
        })
        .map(|v| {
            (
                volume_key(&v.mount),
                v.mount.clone(),
                v.used().0,
                v.available.0,
            )
        })
        .collect()
}

#[cfg(not(windows))]
fn volumes() -> Vec<(String, String, u64, u64)> {
    vec![("letter:/".into(), "/".into(), 7 << 30, 9 << 30)]
}
