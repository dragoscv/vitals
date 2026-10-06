//! Prints the readings the Performance screen shows as "not reported":
//! CPU clocks and temperature, GPU temperature, and per-disk temperature and
//! health — from the same sampler the app runs, after a warm-up tick.
//!
//! `cargo run -p vitals-win --example prove_readings`
//!
//! A screen of em dashes cannot say whether the machine refuses or the code
//! never asks. This says which field is `None` on which device.

#![allow(clippy::print_stdout, clippy::expect_used)]

use std::time::Duration;

use vitals_win::sampler::SystemSampler;

fn main() {
    let mut sampler = SystemSampler::new();
    sampler.sample().expect("first sample");
    // Starts the background thread; its first answer lands within a second
    // or two (drive IOCTLs + NVML init).
    let _ = vitals_win::slow_readings::latest();
    let started = std::time::Instant::now();
    let direct = vitals_win::slow_readings::read_now();
    println!(
        "slow readings, read directly in {:?}: {} drive(s), {} nvidia",
        started.elapsed(),
        direct.drives.len(),
        direct.nvidia.len()
    );
    // Clocks and some sensors need a second reading or a background warm-up.
    std::thread::sleep(Duration::from_millis(2500));
    sampler.sample().expect("second sample");
    std::thread::sleep(Duration::from_millis(1100));
    let s = sampler.sample().expect("third sample").system;

    println!(
        "cpu: total {:.1}% effective_clock {:?} max_clock {:?} temperature {:?} power {:?}",
        s.cpu.total.get(),
        s.cpu.effective_clock,
        s.cpu.max_clock,
        s.cpu.temperature,
        s.cpu.power
    );
    for g in &s.gpus {
        println!(
            "gpu {}: util {:?} temperature {:?} hotspot {:?} power {:?} core_clock {:?}",
            g.name, g.utilization, g.temperature, g.hotspot_temperature, g.power, g.core_clock
        );
    }
    for d in &s.disks {
        println!(
            "disk {} ({:?}, model {:?}): temperature {:?} health {:?}",
            d.name, d.kind, d.model, d.temperature, d.health
        );
    }
}
