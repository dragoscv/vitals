//! Attributes delta churn to the specific field that triggered it.
//!
//! Run with: `cargo run -p vitals-win --release --example delta_reasons`
//!
//! ~75 of 589 processes report changed every tick. Before tuning a
//! threshold, find out which field is responsible — guessing would mean
//! loosening the wrong one and either keeping the churn or hiding real
//! changes.

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use vitals_core::ids::ProcessKey;
use vitals_win::sampler::{SampledProcess, SystemSampler};

#[derive(Default, Debug)]
struct Reasons {
    cpu: usize,
    memory: usize,
    disk: usize,
    other: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sampler = SystemSampler::new();
    sampler.sample()?; // prime
    thread::sleep(Duration::from_secs(1));

    let first = sampler.sample()?;
    let baseline: HashMap<ProcessKey, SampledProcess> = first
        .processes
        .into_iter()
        .map(|p| (p.raw.key, p))
        .collect();

    thread::sleep(Duration::from_secs(1));
    let second = sampler.sample()?;

    let mut reasons = Reasons::default();
    let mut cpu_deltas: Vec<f32> = Vec::new();
    let total = second.processes.len();

    for current in &second.processes {
        let Some(previous) = baseline.get(&current.raw.key) else {
            continue;
        };

        let cpu_delta = (current.cpu.get() - previous.cpu.get()).abs();
        let memory_delta = current
            .raw
            .private_bytes
            .abs_diff(previous.raw.private_bytes);
        let disk_delta = current
            .disk_read
            .get()
            .abs_diff(previous.disk_read.get())
            .max(current.disk_write.get().abs_diff(previous.disk_write.get()));

        if cpu_delta >= 0.05 {
            reasons.cpu += 1;
            cpu_deltas.push(cpu_delta);
        } else if memory_delta >= 64 * 1024 {
            reasons.memory += 1;
        } else if disk_delta >= 4 * 1024 {
            reasons.disk += 1;
        } else if current.raw.thread_count != previous.raw.thread_count {
            reasons.other += 1;
        }
    }

    println!("{total} processes compared across one second\n");
    println!("triggered by cpu    : {}", reasons.cpu);
    println!("triggered by memory : {}", reasons.memory);
    println!("triggered by disk   : {}", reasons.disk);
    println!("triggered by other  : {}", reasons.other);

    if !cpu_deltas.is_empty() {
        cpu_deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = cpu_deltas[cpu_deltas.len() / 2];
        let p90 = cpu_deltas[cpu_deltas.len() * 9 / 10];
        println!(
            "\ncpu deltas that crossed the threshold: median {median:.3}%, p90 {p90:.3}%, max {:.3}%",
            cpu_deltas.last().copied().unwrap_or(0.0)
        );

        // How much churn would a slightly coarser threshold remove?
        for candidate in [0.1_f32, 0.25, 0.5] {
            let remaining = cpu_deltas.iter().filter(|d| **d >= candidate).count();
            println!(
                "  threshold {candidate:.2}% would report {remaining} instead of {}",
                cpu_deltas.len()
            );
        }
    }

    Ok(())
}
