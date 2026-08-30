//! Reads GPU engine utilisation, and reports what it cost.
//!
//! Two things worth checking on any given machine: whether the WDDM counter
//! set exists at all, and whether reading it fits the sampler's ~30 ms tick
//! budget. The counter set has ~1000 instances — one per (process, adapter,
//! engine) triple — which sounds far too expensive until it is measured.
//!
//! It is worth saying how wrong the easy measurement is. A PowerShell
//! `Get-Counter '\GPU Engine(*)\Utilization Percentage'` reports ~6 seconds,
//! which would rule the approach out entirely. That number is almost all
//! `Get-Counter`'s own one-second sample interval: the same command against
//! `\Processor(_Total)\% Processor Time`, a counter with one instance, takes
//! just as long. Measured through PDH directly it is about a millisecond.
//!
//! Run with: `cargo run --release -p vitals-win --example gpu_engine_probe`

use std::time::{Duration, Instant};

use vitals_win::gpu::counters::{EngineCounters, total_by_kind, total_by_process};

fn main() {
    let t = Instant::now();
    let Some(mut counters) = EngineCounters::open() else {
        println!("no GPU Engine counters on this machine.");
        println!("normal for a server, a container, or a VM with a basic display driver.");
        return;
    };
    println!("open + add counter   {:>8.1} ms  (once, at startup)", ms(t));

    // The counter is a rate, so the first collection only sets a baseline.
    counters.sample();
    std::thread::sleep(Duration::from_secs(1));

    println!();
    println!("steady state — what one tick pays:");

    let mut timings = Vec::new();
    let mut last = Vec::new();

    for round in 1..=5 {
        let t = Instant::now();
        let samples = counters.sample();
        let elapsed = ms(t);
        timings.push(elapsed);

        println!(
            "  round {round}: {elapsed:>6.1} ms   {} busy instances",
            samples.len()
        );
        last = samples;

        std::thread::sleep(Duration::from_millis(300));
    }

    timings.sort_by(f64::total_cmp);
    println!();
    println!(
        "median {:.1} ms against a ~30 ms tick budget",
        timings[timings.len() / 2]
    );

    println!();
    println!("by engine:");
    for (kind, total) in total_by_kind(&last) {
        println!("  {:>7.2}%  {}", total, kind.slug());
    }

    println!();
    println!("by process (primary engines only):");
    for (pid, total) in total_by_process(&last).into_iter().take(10) {
        println!("  {:>7.2}%  pid {}", total, pid.0);
    }
}

fn ms(from: Instant) -> f64 {
    from.elapsed().as_secs_f64() * 1000.0
}
