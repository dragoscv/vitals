//! Lists the top processes by private bytes, for direct comparison with
//! `Get-Process | Sort PrivateMemorySize64`.
//!
//! Run with: `cargo run -p vitals-win --example process_mem_probe`

use vitals_win::process::ProcessEnumerator;

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut enumerator = ProcessEnumerator::new();
    let mut processes = enumerator.enumerate()?;

    processes.sort_by_key(|p| std::cmp::Reverse(p.private_bytes));

    println!("vitals, top 10 by private bytes");
    println!(
        "{:>7}  {:>10}  {:>10}  {:>4}  NAME",
        "PID", "PRIVATE", "WORKSET", "THR"
    );
    println!("{}", "-".repeat(62));

    for p in processes.iter().take(10) {
        println!(
            "{:>7}  {:>7.1} MB  {:>7.1} MB  {:>4}  {}",
            p.key.pid.get(),
            mib(p.private_bytes),
            mib(p.working_set),
            p.thread_count,
            p.name.as_deref().unwrap_or("<unknown>")
        );
    }

    println!("{} processes total", processes.len());
    Ok(())
}
