//! Cross-checks the condition probe against a known workload.
//!
//! Exists because `background_load` is the one field in [`RunConditions`] that
//! is computed rather than read, and a wrong subtraction there is invisible:
//! it just makes every result say `tainted`, which looks like a busy machine
//! rather than a bug. Three cases with known answers make the difference
//! obvious — an idle interval should report roughly the machine's real
//! background, and a benchmark should not report itself.
//!
//! Run with `cargo run --release -p vitals-bench --example conditions_probe`.

use std::time::{Duration, Instant};

use vitals_bench::ConditionsProbe;

fn measure(label: &str, work: impl FnOnce()) {
    let probe = ConditionsProbe::start();
    let started = Instant::now();
    work();
    let elapsed = started.elapsed();
    let c = probe.finish(elapsed);

    println!(
        "{label:<28} background={:>6.2}%  plan={:?} onBattery={} tainted={}",
        c.background_load,
        c.power_plan,
        c.on_battery,
        c.is_tainted()
    );
}

fn main() {
    println!(
        "logical processors: {}",
        std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get)
    );

    measure("sleeping 500 ms", || {
        std::thread::sleep(Duration::from_millis(500));
    });

    measure("one busy thread", || {
        std::hint::black_box(vitals_bench::workloads::cpu_single_thread());
    });

    measure("every core busy", || {
        std::hint::black_box(vitals_bench::workloads::cpu_multi_thread());
    });
}
