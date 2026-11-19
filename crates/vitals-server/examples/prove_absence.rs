//! Proves an unmeasurable reading reaches the exporters as absence, not zero.
//!
//! `prove_lan` shows a phone what a real machine *does* send. This one shows
//! what a machine *cannot* send: it takes a real sample (or, off the platforms
//! we sample, a fixture), builds the same frame the way macOS would describe
//! it — no process count, no thread count, no disk busy-time, no Windows-only
//! memory figures — and prints exactly which Prometheus series and which Home
//! Assistant entities survive. The point is the ones that do not appear.
//!
//! The exporters are an external boundary: a fixture cannot show that absence
//! survives a whole scrape or an MQTT publish, and a fabricated zero here is
//! invisible on a dashboard because it looks like a real minimum.
//!
//! Run with: `cargo run -p vitals-server --example prove_absence`

#![allow(clippy::print_stdout)]

use vitals_core::metrics::SystemMetrics;

/// The Prometheus series this proves are omitted when unmeasured.
const PROM_SERIES: [&str; 3] = [
    "vitals_process_count",
    "vitals_thread_count",
    "vitals_disk_active_percent",
];

/// The Home Assistant entity that must be omitted when unmeasured.
const HA_ENTITY: &str = "processes";

fn main() {
    let measured = real_system();
    let mac = mac_shaped(&measured);

    for (label, system) in [
        ("this machine, as measured", &measured),
        ("the same machine as macOS describes it", &mac),
    ] {
        println!("\n=== {label} ===");
        report_prometheus(system);
        report_home_assistant(system);
    }

    println!(
        "\nThe three series above must be present for the measured machine and\n\
         absent for the macOS one. Absent, not 0: Prometheus has `absent()` for\n\
         exactly this, and a zero poisons every average on a dashboard."
    );
}

fn report_prometheus(system: &SystemMetrics) {
    let text = vitals_server::prometheus::render(system, &[], 0);
    for name in PROM_SERIES {
        // Sample lines only: `# HELP`/`# TYPE` would count even with no series.
        let samples = text
            .lines()
            .filter(|line| line.starts_with(name) && !line.starts_with('#'))
            .count();
        if samples == 0 {
            println!("  prometheus  {name:<28} omitted (absent, not 0)");
        } else {
            println!("  prometheus  {name:<28} {samples} series");
        }
    }
}

fn report_home_assistant(system: &SystemMetrics) {
    let readings = vitals_mqtt::feed::readings(system, 0);
    let published = readings
        .iter()
        .any(|reading| reading.spec.object == HA_ENTITY);
    if published {
        println!("  HA entity   {HA_ENTITY:<28} published");
    } else {
        println!("  HA entity   {HA_ENTITY:<28} omitted (absent, not 0)");
    }
}

/// How many processes and threads a machine can report, and a disk's busy time,
/// all blanked out the way macOS and the App Store sandbox leave them.
fn mac_shaped(measured: &SystemMetrics) -> SystemMetrics {
    let mut mac = measured.clone();
    // The App Store build is sandboxed and cannot enumerate processes, so the
    // count is unknown; off the store the list is real and so is the count.
    mac.cpu.process_count = None;
    mac.cpu.thread_count = None;
    // macOS publishes no disk busy-time counter.
    for disk in &mut mac.disks {
        disk.active_time = None;
    }
    // Windows-only memory figures.
    mac.memory.paged_pool = None;
    mac.memory.non_paged_pool = None;
    mac.memory.committed = None;
    mac.memory.commit_limit = None;
    mac.memory.hardware_reserved = None;
    mac
}

/// A real sample from this machine, or a fixture where nothing can sample.
fn real_system() -> SystemMetrics {
    #[cfg(windows)]
    {
        let mut sampler = vitals_win::SystemSampler::new();
        let mut builder = vitals_win::FrameBuilder::new();
        if let Ok(sample) = sampler.sample() {
            println!("sampled this machine for real");
            return builder.build(sample).system().clone();
        }
    }
    println!("using the core fixture (no sampler on this platform)");
    vitals_core::fixtures::system()
}
