//! Breaks a full sample down by subsystem.
//!
//! Run with: `cargo run -p vitals-win --release --example profile_sample`
//!
//! Exists because "the sample is too slow" is not actionable. Optimising
//! before measuring is how you spend an afternoon speeding up the part that
//! was already fast.

// A probe: a failure to sample IS the finding, so panicking with the
// subsystem name is the right report.
#![allow(clippy::expect_used)]

use std::time::{Duration, Instant};

use vitals_win::cpu::{CpuSampler, logical_core_count};
use vitals_win::disk::enumerate_volumes;
use vitals_win::memory::MemorySampler;
use vitals_win::network::enumerate_adapters;
use vitals_win::process::ProcessEnumerator;
use vitals_win::sampler::SystemSampler;

const ITERATIONS: usize = 30;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort_unstable();
    v[v.len() / 2]
}

fn measure(label: &str, mut f: impl FnMut()) -> Duration {
    f(); // warm up: first call primes caches and buffers
    let mut timings = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        f();
        timings.push(start.elapsed());
    }
    let mid = median(timings);
    println!("{label:<28} {mid:>10.3?}");
    mid
}

fn main() {
    println!("{:<28} {:>10}", "SUBSYSTEM", "MEDIAN");
    println!("{}", "-".repeat(40));

    let mut cpu = CpuSampler::new(logical_core_count());
    let t_cpu = measure("cpu", || {
        cpu.sample().expect("cpu");
    });

    let mut memory = MemorySampler::new();
    let t_mem = measure("memory", || {
        memory.sample(1000).expect("memory");
    });

    let mut processes = ProcessEnumerator::new();
    let t_proc = measure("processes", || {
        processes.enumerate().expect("processes");
    });

    let t_disk = measure("volumes", || {
        let _ = enumerate_volumes();
    });

    let t_net = measure("adapters", || {
        let _ = enumerate_adapters();
    });

    let total = t_cpu + t_mem + t_proc + t_disk + t_net;
    println!("{}", "-".repeat(40));
    println!("{:<28} {:>10.3?}", "sum", total);

    // The parts summed above are measured in isolation. If a full sample
    // costs materially more, the difference is our own glue -- and that is
    // the number worth chasing.
    let mut sampler = SystemSampler::new();
    let t_full = measure("FULL SAMPLE", || {
        sampler.sample().expect("sample");
    });

    let overhead = t_full.saturating_sub(total);
    println!(
        "{:<28} {:>10.3?}  ({:.0}% of the full sample)",
        "glue overhead",
        overhead,
        overhead.as_secs_f64() / t_full.as_secs_f64() * 100.0
    );

    println!("\nshare of total:");
    for (name, d) in [
        ("cpu", t_cpu),
        ("memory", t_mem),
        ("processes", t_proc),
        ("volumes", t_disk),
        ("adapters", t_net),
    ] {
        let pct = d.as_secs_f64() / total.as_secs_f64() * 100.0;
        let bar = "█".repeat((pct / 2.0).round() as usize);
        println!("  {name:<12} {pct:>5.1}%  {bar}");
    }
}
