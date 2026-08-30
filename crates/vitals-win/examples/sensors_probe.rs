//! Prints every sensor this machine will report, and what it will not.
//!
//! Cross-check against:
//! ```text
//! Get-CimInstance -Namespace root/WMI -ClassName MSAcpi_ThermalZoneTemperature
//! Get-CimInstance Win32_Battery
//! powercfg /getactivescheme
//! ```
//!
//! Run with: `cargo run -p vitals-win --example sensors_probe`

use std::time::Instant;

use vitals_win::sensors::{
    ChargeState, DRIVER_GAPS, LineStatus, SensorReader, SensorSample, ThermalAvailability,
    capabilities,
};

fn main() {
    // Timed separately from the steady state: the first WMI call in a
    // process pays for CoInitializeEx and ConnectServer, which is a
    // one-off cost of a completely different magnitude. Reporting only the
    // average would understate the stall the first sample causes.
    let cold_start = Instant::now();
    let mut reader = SensorReader::new();
    let sample = reader.refresh().clone();
    let cold = cold_start.elapsed();

    println!("== timing ==");
    println!("  cold sample     {:>8.2} ms", cold.as_secs_f64() * 1000.0);

    let mut warm_total = 0.0_f64;
    let runs = 5;
    for _ in 0..runs {
        let started = Instant::now();
        let _ = reader.refresh();
        warm_total += started.elapsed().as_secs_f64() * 1000.0;
    }
    println!(
        "  warm sample     {:>8.2} ms  (mean of {runs})",
        warm_total / f64::from(runs)
    );
    println!("  cache hit       {:>8.2} ms", {
        let started = Instant::now();
        let _ = reader.sample();
        started.elapsed().as_secs_f64() * 1000.0
    });
    println!(
        "  cadence hint    {:>8.2} s",
        reader.sample_interval_hint().as_secs_f64()
    );

    show_thermal(&sample);
    show_batteries(&sample);
    show_power(&sample);
    show_readings(&sample);

    println!("\n== capabilities ==");
    let caps = capabilities(&sample);
    for cap in &caps.available {
        println!("  available    {cap:?}");
    }
    for (cap, why) in &caps.unavailable {
        println!("  unavailable  {cap:?} — {why:?}");
    }

    println!("\n== what needs a driver or vendor SDK ==");
    for gap in DRIVER_GAPS {
        println!("  {}", gap.label);
        for line in wrap(gap.requirement, 74) {
            println!("      {line}");
        }
    }
}

fn show_thermal(sample: &SensorSample) {
    println!("\n== thermal zones ==");
    match sample.thermal.availability {
        ThermalAvailability::Available => {
            for zone in &sample.thermal.zones {
                let critical = zone
                    .critical
                    .map_or_else(|| "—".to_owned(), |c| format!("{:.1} °C", c.0));
                println!(
                    "  {:<40} {:>8.2} °C   critical {critical}",
                    zone.instance, zone.temperature.0
                );
            }
        }
        ThermalAvailability::AccessDenied => {
            println!("  UNAVAILABLE — root\\WMI refused the query (needs elevation)");
        }
        ThermalAvailability::NoZonesPresent => {
            println!("  UNAVAILABLE — this firmware exposes no ACPI thermal zones");
        }
        ThermalAvailability::ProviderMissing => {
            println!("  UNAVAILABLE — the ACPI WMI provider did not respond");
        }
    }
}

fn show_batteries(sample: &SensorSample) {
    println!("\n== batteries ==");
    if sample.batteries.is_empty() {
        println!("  none (desktop, or no system battery present)");
    }
    for (index, pack) in sample.batteries.iter().enumerate() {
        println!("  battery {index}  [{}]", pack.chemistry);
        show("charge", pack.charge.map(|p| format!("{:.1} %", p.get())));
        show("health", pack.health.map(|p| format!("{:.1} %", p.get())));
        show("rate", pack.rate.map(|w| format!("{:.2} W", w.0)));
        show("voltage", pack.voltage.map(|v| format!("{:.3} V", v.0)));
        show(
            "design",
            pack.design_capacity_mwh.map(|c| format!("{c} mWh")),
        );
        show(
            "full charge",
            pack.full_charge_capacity_mwh.map(|c| format!("{c} mWh")),
        );
        show("cycles", pack.cycle_count.map(|c| c.to_string()));
        println!("    {:<14} {:?}", "state", pack.state);
        if pack.state == ChargeState::Discharging {
            show(
                "time left",
                pack.seconds_to_empty.map(|s| format!("{} min", s / 60)),
            );
        }
    }
}

fn show_power(sample: &SensorSample) {
    println!("\n== power ==");
    println!(
        "  line            {}",
        match sample.power.line {
            LineStatus::Ac => "AC",
            LineStatus::Battery => "battery",
            LineStatus::Unknown => "unknown",
        }
    );
    println!("  mode            {:?}", sample.power.mode);
    println!(
        "  scheme GUID     {}",
        sample
            .power
            .scheme_guid
            .map_or_else(|| "unavailable".to_owned(), format_guid)
    );
    println!("  has battery     {}", sample.power.has_battery);
    println!("  power saver     {}", sample.power.power_saver);
}

fn show_readings(sample: &SensorSample) {
    println!("\n== readings ==");
    if sample.readings.is_empty() {
        println!("  none measured");
    }
    for reading in &sample.readings {
        println!(
            "  {:<28} {:>10.2} {:<5} {:<20} {:?}",
            reading.label,
            reading.value.magnitude(),
            reading.value.unit(),
            reading.source.label(),
            reading.quality,
        );
    }
}

fn show(label: &str, value: Option<String>) {
    println!(
        "    {label:<14} {}",
        value.unwrap_or_else(|| "unavailable".to_owned())
    );
}

/// Formats a GUID in the canonical printed order.
///
/// Data1..3 are little-endian in memory and Data4 is not, which is why this
/// cannot be a straight hex dump of the bytes.
fn format_guid(g: [u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut tail = String::with_capacity(12);
    for byte in &g[10..] {
        let _ = write!(tail, "{byte:02x}");
    }

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{tail}",
        g[3], g[2], g[1], g[0], g[5], g[4], g[7], g[6], g[8], g[9],
    )
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}
