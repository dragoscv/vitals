//! Prints live CPU utilisation so the numbers can be eyeballed against Task
//! Manager.
//!
//! Run with: `cargo run -p vitals-win --example cpu_probe`
//!
//! Exists because "the value is between 0 and 100" is a weak assertion. The
//! only way to know the arithmetic is right is to load one core and watch
//! exactly one number move.

use std::thread;
use std::time::Duration;

use vitals_win::cpu::{CpuSampler, logical_core_count};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cores = logical_core_count();
    println!("logical processors: {cores}\n");

    let mut sampler = CpuSampler::new(cores);
    sampler.sample()?; // prime the baseline

    for tick in 1..=10 {
        thread::sleep(Duration::from_secs(1));
        let usage = sampler.sample()?;

        let total: f32 = usage.iter().map(|u| u.total.get()).sum::<f32>() / cores as f32;
        let kernel: f32 = usage.iter().map(|u| u.kernel.get()).sum::<f32>() / cores as f32;
        let user: f32 = usage.iter().map(|u| u.user.get()).sum::<f32>() / cores as f32;

        let bars: String = usage
            .iter()
            .map(|u| match u.total.get() as u32 {
                0..=5 => '▁',
                6..=20 => '▂',
                21..=40 => '▃',
                41..=60 => '▅',
                61..=80 => '▆',
                _ => '█',
            })
            .collect();

        println!("[{tick:2}] total {total:5.1}%  kernel {kernel:5.1}%  user {user:5.1}%  {bars}");
    }

    Ok(())
}
