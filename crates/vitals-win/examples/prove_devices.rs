//! Proves disk throughput, drive kinds and GPU names through the real sampler.
//!
//! Two ticks a second apart through `SystemSampler`, so what is printed is
//! exactly what the dashboard receives — not the output of a helper that the
//! sampler might not call.
//!
//! Run with: `cargo run -p vitals-win --example prove_devices`

use std::thread::sleep;
use std::time::Duration;

use vitals_win::sampler::SystemSampler;

fn main() {
    let mut sampler = SystemSampler::new();
    if let Err(error) = sampler.sample() {
        eprintln!("priming tick failed: {error}");
        return;
    }
    // Some load so a zero is distinguishable from "never measured".
    let _ = std::fs::read_dir(std::env::temp_dir()).map(Iterator::count);
    sleep(Duration::from_secs(1));

    let sample = match sampler.sample() {
        Ok(sample) => sample,
        Err(error) => {
            eprintln!("second tick failed: {error}");
            return;
        }
    };

    println!(
        "{:<5} {:<14} {:<10} {:>12} {:>12} {:>7} {:>9}",
        "MOUNT", "NAME", "KIND", "READ/s", "WRITE/s", "ACTIVE", "RESP ms"
    );
    for disk in &sample.system.disks {
        println!(
            "{:<5} {:<14} {:<10} {:>12} {:>12} {:>6.1}% {:>9}",
            disk.mount.as_deref().unwrap_or("-"),
            disk.name,
            format!("{:?}", disk.kind),
            disk.read.get(),
            disk.write.get(),
            disk.active_time.get(),
            disk.response_ms
                .map_or_else(|| "-".to_owned(), |ms| format!("{ms:.2}")),
        );
    }

    println!();
    for gpu in &sample.system.gpus {
        println!(
            "GPU {:<44} util {:>5.1}%  vram {}",
            gpu.name,
            gpu.utilization
                .map_or(f32::NAN, vitals_core::units::Percent::get),
            gpu.memory_total.map_or_else(
                || "-".to_owned(),
                |b| format!("{:.1} GB", b.get() as f64 / 1e9)
            ),
        );
    }
}
