//! Costs a full sample in CPU time rather than wall time.
//!
//! The 30 ms budget in `tests/overhead.rs` is wall-clock, which on a loaded
//! machine measures the scheduler, not the sampler. Process CPU time
//! (`QueryProcessCycleTime`) count only what this process ran, so they answer
//! the budget's real question — what share of a core does Vitals cost at 1 Hz
//! — when the machine cannot be made idle. Not `GetProcessTimes`: its clock
//! ticks every 15.6 ms, which cannot resolve a 30 ms budget. Cycles are
//! converted at the nominal frequency from the registry (`~MHz`), so the
//! result is a slight overestimate on a chip that boosts above it.
//!
//! Run with: `cargo run --release -p vitals-win --example cpu_cost`

use std::time::{Duration, Instant};

use vitals_win::sampler::SystemSampler;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Threading::GetCurrentProcess;

#[link(name = "kernel32")]
unsafe extern "system" {
    // Declared here rather than enabling `Win32_System_WindowsProgramming`
    // for the whole crate: one example does not justify a feature.
    fn QueryProcessCycleTime(process: HANDLE, cycle_time: *mut u64) -> i32;
}

fn process_cycles() -> u64 {
    let mut cycles = 0_u64;
    // SAFETY: the pseudo-handle is always valid; the out-pointer is live.
    unsafe { QueryProcessCycleTime(GetCurrentProcess(), &raw mut cycles) };
    cycles
}

/// Nominal core frequency in MHz, from the same key Task Manager reads.
fn nominal_mhz() -> f64 {
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "/v",
            "~MHz",
        ])
        .output()
        .ok();
    out.and_then(|o| {
        let text = String::from_utf8_lossy(&o.stdout);
        let hex = text.split_whitespace().last()?;
        u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
    })
    .map_or(3000.0, |mhz| mhz as f64)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let mhz = nominal_mhz();
    let mut sampler = SystemSampler::new();
    if let Err(error) = sampler.sample() {
        eprintln!("priming tick failed: {error}");
        return;
    }

    let mut cpu_ms = Vec::with_capacity(15);
    let mut wall_ms = Vec::with_capacity(15);
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(200));
        let cycles_before = process_cycles();
        let wall = Instant::now();
        if let Err(error) = sampler.sample() {
            eprintln!("sample failed: {error}");
            return;
        }
        wall_ms.push(wall.elapsed().as_secs_f64() * 1000.0);
        cpu_ms.push((process_cycles() - cycles_before) as f64 / (mhz * 1000.0));
    }

    println!(
        "full sample @ {mhz:.0} MHz nominal: CPU median {:.2} ms (max {:.2}), wall median {:.2} ms (max {:.2}), budget 30 ms",
        median(cpu_ms.clone()),
        cpu_ms.iter().copied().fold(0.0, f64::max),
        median(wall_ms.clone()),
        wall_ms.iter().copied().fold(0.0, f64::max),
    );
}
