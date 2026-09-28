//! Lists network adapters and live throughput.
//!
//! Run with: `cargo run -p vitals-win --example network_probe`

use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use vitals_win::network::{compute_rates, enumerate_adapters};

fn rate(bytes_per_sec: u64) -> String {
    match bytes_per_sec {
        0 => "        —".into(),
        b if b < 1024 => format!("{b:>6} B/s"),
        b if b < 1024 * 1024 => format!("{:>6.1} KB/s", b as f64 / 1024.0),
        b => format!("{:>6.1} MB/s", b as f64 / (1024.0 * 1024.0)),
    }
}

fn main() {
    let first = enumerate_adapters();
    let baseline: HashMap<_, _> = first.iter().map(|a| (a.id, a.counters)).collect();

    let start = Instant::now();
    thread::sleep(Duration::from_secs(2));
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let second = enumerate_adapters();

    println!(
        "{:<38}  {:<9}  {:<5}  {:<4}  {:>11}  {:>11}  {:>6}",
        "ADAPTER", "KIND", "UP", "HW", "DOWN", "UP", "ERR/s"
    );
    println!("{}", "-".repeat(96));

    for a in &second {
        let previous = baseline.get(&a.id).copied().unwrap_or_default();
        let rates = compute_rates(previous, a.counters, elapsed_ms);

        let mut name = a.alias.clone();
        name.truncate(38);

        println!(
            "{:<38}  {:<9}  {:<5}  {:<4}  {}  {}  {:>6}",
            name,
            format!("{:?}", a.kind),
            if a.connected { "yes" } else { "no" },
            if a.hardware { "yes" } else { "no" },
            rate(rates.rx.get()),
            rate(rates.tx.get()),
            rates.errors_per_sec
        );
    }

    println!("\n{} adapter(s), sampled over {elapsed_ms}ms", second.len());

    println!("\nconnected adapters with a MAC:");
    for a in second.iter().filter(|a| a.connected && a.mac.is_some()) {
        println!(
            "  {:<38}  {}  {}",
            a.alias,
            a.mac.as_deref().unwrap_or("—"),
            a.link_speed
                .map_or_else(|| "— ".to_string(), |s| format!("{} Mb/s", s / 1_000_000))
        );
    }
}
