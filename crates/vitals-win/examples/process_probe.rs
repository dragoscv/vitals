//! Lists the top processes by CPU, for comparison against Task Manager.
//!
//! Run with: `cargo run -p vitals-win --example process_probe`

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use vitals_win::cpu::{logical_core_count, process_cpu_percent};
use vitals_win::process::ProcessEnumerator;

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cores = logical_core_count();
    let mut enumerator = ProcessEnumerator::new();

    // Baseline: CPU is a rate, so it needs two samples separated by real time.
    let first = enumerator.enumerate()?;
    let baseline: HashMap<_, _> = first.iter().map(|p| (p.key, p.cpu_time())).collect();

    let start = Instant::now();
    thread::sleep(Duration::from_secs(2));
    let elapsed = start.elapsed();
    // Wall clock in the same 100ns units the kernel reports.
    let elapsed_100ns = elapsed.as_nanos() as u64 / 100;

    let second = enumerator.enumerate()?;

    let mut rows: Vec<_> = second
        .iter()
        .filter(|p| !p.is_idle_process())
        .map(|p| {
            let delta = baseline
                .get(&p.key)
                .map_or(0, |prev| p.cpu_time().saturating_sub(*prev));
            (p, process_cpu_percent(delta, elapsed_100ns, cores as u32))
        })
        .collect();

    rows.sort_by(|a, b| {
        b.1.get()
            .partial_cmp(&a.1.get())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total: f32 = rows.iter().map(|(_, cpu)| cpu.get()).sum();

    println!(
        "{} processes · {} logical cores · sampled over {:.2}s\n",
        second.len(),
        cores,
        elapsed.as_secs_f64()
    );
    println!(
        "{:>7}  {:>6}  {:>10}  {:>10}  {:>4}  NAME",
        "PID", "CPU%", "PRIVATE", "WORKSET", "THR"
    );
    println!("{}", "─".repeat(72));

    for (p, cpu) in rows.iter().take(20) {
        println!(
            "{:>7}  {:>5.1}%  {:>7.1} MB  {:>7.1} MB  {:>4}  {}",
            p.key.pid.get(),
            cpu.get(),
            mib(p.private_bytes),
            mib(p.working_set),
            p.thread_count,
            p.name.as_deref().unwrap_or("<unknown>")
        );
    }

    println!("\nsum of all process CPU: {total:.1}%");
    println!("(should be close to Task Manager's total CPU figure)");

    Ok(())
}
