//! Lists GPU adapters, for comparison against Device Manager.
//!
//! Run with: `cargo run -p vitals-win --example gpu_probe`

use vitals_win::gpu::adapters::GpuSampler;

fn main() {
    let mut sampler = GpuSampler::new();
    let adapters = sampler.sample(0);

    if adapters.is_empty() {
        println!("no GPU adapters reported (headless, container, or no WDDM driver)");
        return;
    }

    println!("{:<44}  {:>18}  {:>7}", "ADAPTER", "LUID", "ENGINES");
    println!("{}", "-".repeat(74));

    for adapter in &adapters {
        let mut name = adapter.name.clone();
        name.truncate(44);
        println!(
            "{:<44}  {:>18}  {:>7}",
            name,
            format!("{:#x}", adapter.luid),
            adapter.engines.len()
        );
    }

    println!("\n{} adapter(s)", adapters.len());

    let with_engines = adapters.iter().filter(|a| !a.engines.is_empty()).count();
    if with_engines == 0 {
        println!("engine utilisation: unavailable (D3DKMTQueryStatistics not yet wired)");
    }
}
