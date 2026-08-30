//! Prints memory state for comparison against Task Manager.
//!
//! Run with: `cargo run -p vitals-win --example memory_probe`

use vitals_win::memory::MemorySampler;
use vitals_win::memory::pressure::{PressureInputs, classify_pressure};

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sampler = MemorySampler::new();
    let m = sampler.sample(0)?;

    println!("physical");
    println!("  total       {:8.2} GiB", gib(m.total.get()));
    println!(
        "  used        {:8.2} GiB  ({:.1}%)",
        gib(m.used.get()),
        m.used_percent().get()
    );
    println!("  available   {:8.2} GiB", gib(m.available.get()));
    println!("  cached      {:8.2} GiB", gib(m.cached.get()));
    println!();
    println!("commit");
    println!("  charged     {:8.2} GiB", gib(m.committed.get()));
    println!("  limit       {:8.2} GiB", gib(m.commit_limit.get()));
    println!();
    println!("kernel pools");
    println!("  paged       {:8.2} GiB", gib(m.paged_pool.get()));
    println!("  non-paged   {:8.2} GiB", gib(m.non_paged_pool.get()));
    println!();
    println!("page file");
    println!("  size        {:8.2} GiB", gib(m.swap_total.get()));
    match m.swap_used {
        Some(used) => println!("  in use      {:8.2} GiB", gib(used.get())),
        // Distinct from "0.00 GiB": we have not measured it, and saying so
        // is the whole point of the Option.
        None => println!("  in use      {:>8}", "unknown"),
    }
    println!();

    let pressure = classify_pressure(PressureInputs {
        physical_total: m.total,
        physical_available: m.available,
        commit_used: m.committed,
        commit_limit: m.commit_limit,
        hard_faults_per_sec: m.page_faults_per_sec,
    });
    println!("pressure: {pressure:?}");

    Ok(())
}
