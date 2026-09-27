//! Costs the two per-tick additions of 2026-09-27 in isolation.
//!
//! Run with: `cargo run --release -p vitals-win --example cost_probe`

use std::time::Instant;

use vitals_win::disk::{enumerate_volumes, volume_counters};
use vitals_win::process::{OwnerCache, ProcessEnumerator};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let volumes = enumerate_volumes();
    let letters: Vec<char> = volumes
        .iter()
        .filter_map(|v| v.mount.chars().next())
        .collect();

    let mut samples = Vec::new();
    for _ in 0..50 {
        let start = Instant::now();
        for &l in &letters {
            let _ = volume_counters(l);
        }
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "disk counters x{} volumes: median {:.3} ms",
        letters.len(),
        median(samples)
    );

    let mut enumerator = ProcessEnumerator::new();
    let processes = enumerator.enumerate().expect("enumerate");
    let mut cache = OwnerCache::new();

    let start = Instant::now();
    for p in &processes {
        let _ = cache.owner(p.key);
    }
    let cold = start.elapsed().as_secs_f64() * 1000.0;

    let mut warm = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        for p in &processes {
            let _ = cache.owner(p.key);
        }
        warm.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "owners x{} processes: cold {:.3} ms, warm median {:.3} ms",
        processes.len(),
        cold,
        median(warm)
    );
}
