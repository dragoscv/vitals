//! Runs the whole available suite and prints what was measured.
//!
//! Exists so the figures can be sanity-checked against the machine's actual
//! specification before anyone trusts the UI that displays them. A memory
//! bandwidth an order of magnitude above the DIMM's rated transfer rate, or a
//! latency under 10 ns, means the working set is being served from cache and
//! the number is fiction — and that is far easier to spot in a console than
//! behind a chart.
//!
//! Run with `cargo run --release -p vitals-bench --example bench_probe`. The
//! release profile matters: the debug profile does not optimise this crate,
//! so the CPU figures are several times lower and are not comparable.

fn main() {
    let mut runner = vitals_bench::Runner::new();

    println!("catalogue");
    for info in vitals_bench::CATALOGUE {
        match info.unavailable {
            None => println!(
                "  {:<20} available    ~{}s  ({})",
                info.kind.id(),
                info.estimated_seconds,
                vitals_bench::unit_for(info.kind)
            ),
            Some(reason) => println!("  {:<20} unavailable  {}", info.kind.id(), reason.key()),
        }
    }

    println!("\nrunning {} runs each", vitals_bench::RUNS_PER_BENCHMARK);
    let suite = runner.run_all_available();

    for result in &suite.results {
        let variability = result
            .variability()
            .map_or_else(|| "n/a".to_owned(), |cv| format!("{cv:.2}%"));

        println!(
            "\n{}\n  score        {:.2} {}\n  runs         {}\n  variability  {}\n  \
             trustworthy  {}\n  duration     {} ms",
            result.kind.id(),
            result.score,
            result.unit,
            result
                .runs
                .iter()
                .map(|r| format!("{r:.2}"))
                .collect::<Vec<_>>()
                .join(", "),
            variability,
            result.is_trustworthy(),
            result.duration_ms,
        );

        let c = &result.conditions;
        println!(
            "  conditions   plan={:?} onBattery={} throttled={} background={:.2}% \
             ambient={:?}..{:?} tainted={}",
            c.power_plan,
            c.on_battery,
            c.throttled,
            c.background_load,
            c.ambient_start_temp,
            c.ambient_end_temp,
            c.is_tainted(),
        );
    }

    println!("\ntotal {} ms", suite.total_duration.as_millis());
}
