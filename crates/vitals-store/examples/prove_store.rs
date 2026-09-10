//! Proves the store works against real sampled data, not fixtures.
//!
//! Exists because "the tests pass" and "the feature works" have come apart in
//! this repo before: a complete, tested backend whose caller never fed it.
//! This drives the exact path the sampler drives — open, observe frames,
//! maintain, read back — and prints what a user would see.
//!
//! Run with: `cargo run -p vitals-store --example prove_store`

#![allow(clippy::expect_used, clippy::print_stdout)]

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vitals_store::{Recorder, Resolution, Store};

fn main() {
    let dir = std::env::temp_dir().join("vitals-prove-store");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("history.sqlite");

    let mut rec = Recorder::open(&path).expect("open");
    rec.set_history_enabled(true);
    rec.set_retention_days(7);

    #[cfg(windows)]
    {
        let mut sampler = vitals_win::SystemSampler::new();
        let mut frames = vitals_win::FrameBuilder::new();

        println!("sampling 5 real frames...");
        let started = Instant::now();
        for i in 0..5 {
            let sample = sampler.sample().expect("sample");
            let frame = frames.build(sample);
            let bytes = serde_json::to_vec(&frame).expect("encode");
            rec.observe(&frame, &bytes).expect("observe");
            println!("  frame {i}: seq={} bytes={}", frame.seq.0, bytes.len());
            std::thread::sleep(Duration::from_millis(200));
        }
        println!("  {} ms total", started.elapsed().as_millis());
    }

    #[cfg(not(windows))]
    println!("no sampler on this platform; exercising the store only");

    rec.flush().expect("flush");
    drop(rec);

    // Read back through a fresh connection, as the Tauri command does.
    let store = Store::open(&path, vitals_store::RetentionPolicy::default()).expect("reopen");
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_secs(),
    )
    .expect("fits");

    let rows = store.query(now - 3_600, now, now).expect("query");
    println!("\nread back {} machine samples", rows.len());
    for r in rows.iter().take(3) {
        println!(
            "  ts={} cpu={:.1}% mem={} MB disk_r={} B/s gpu={:?}",
            r.ts,
            r.cpu_percent,
            r.memory_used / 1_048_576,
            r.disk_read_bps,
            r.gpu_percent
        );
    }

    let frames = store.flight_frames().expect("frames");
    println!("flight recorder holds {} frames", frames.len());
    if let Some((seq, ts, bytes)) = frames.first() {
        let parsed: serde_json::Value = serde_json::from_slice(bytes).expect("valid json");
        // The whole point of storing raw bytes: what comes out must be a
        // frame the UI could have rendered, field for field.
        println!(
            "  first: seq={seq} ts={ts} payload_kind={:?} has_system={}",
            parsed
                .get("payload")
                .and_then(|p| p.get("kind"))
                .and_then(serde_json::Value::as_str),
            parsed
                .get("payload")
                .and_then(|p| p.get("system"))
                .is_some()
        );
    }

    println!(
        "\ndatabase: {} bytes, {} fine rows",
        store.size_bytes().expect("size"),
        store.count(Resolution::Second).expect("count")
    );
    println!("path: {}", path.display());
}
