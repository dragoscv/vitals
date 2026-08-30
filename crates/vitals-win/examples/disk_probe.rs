//! Lists mounted volumes, for comparison against Explorer or `Get-Volume`.
//!
//! Run with: `cargo run -p vitals-win --example disk_probe`

use vitals_win::disk::enumerate_volumes;

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

fn main() {
    let volumes = enumerate_volumes();

    println!(
        "{:<5}  {:<16}  {:<7}  {:>10}  {:>10}  {:>10}  {:>6}",
        "MOUNT", "LABEL", "FS", "TOTAL", "USED", "FREE", "USED%"
    );
    println!("{}", "-".repeat(78));

    for v in &volumes {
        let used_pct = if v.total.get() == 0 {
            0.0
        } else {
            v.used().get() as f64 / v.total.get() as f64 * 100.0
        };

        println!(
            "{:<5}  {:<16}  {:<7}  {:>7.1} GB  {:>7.1} GB  {:>7.1} GB  {:>5.1}%",
            v.mount,
            v.label.as_deref().unwrap_or("—"),
            v.file_system.as_deref().unwrap_or("—"),
            gib(v.total.get()),
            gib(v.used().get()),
            gib(v.available.get()),
            used_pct
        );
    }

    println!("\n{} volume(s)", volumes.len());
}
