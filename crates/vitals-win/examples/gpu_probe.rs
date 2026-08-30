//! Lists GPU adapters, for comparison against Device Manager.
//!
//! Run with: `cargo run -p vitals-win --example gpu_probe`

use vitals_win::gpu::adapters::GpuSampler;

fn main() {
    let mut sampler = GpuSampler::new();

    // Utilisation is a rate, so the first sample only establishes a baseline
    // and reports no engines. Sampling twice is what the real sampler does
    // across two ticks; doing it here keeps the probe honest rather than
    // showing zero engines on a machine that has them.
    let _ = sampler.sample(0);
    std::thread::sleep(std::time::Duration::from_secs(1));

    let adapters = sampler.sample(10_000_000);

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
        println!("engine utilisation: no engine was busy during the sample");
        println!("(idle GPU, or no WDDM GPU Engine counters on this machine)");
        return;
    }

    println!();
    for adapter in adapters.iter().filter(|a| !a.engines.is_empty()) {
        println!("{}", adapter.name);
        for engine in &adapter.engines {
            println!(
                "  {:>7.2}%  {}",
                engine.utilisation.get(),
                engine.kind.slug()
            );
        }
    }
}
