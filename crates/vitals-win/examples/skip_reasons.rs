//! Counts why a scan skipped folders, and shows a sample of each reason.
//!
//! `cargo run -p vitals-win --release --example skip_reasons -- "C:\Users"`

use std::collections::BTreeMap;
use std::path::PathBuf;

use vitals_win::storage::{ScanControl, ScanOptions, scan_directory};

fn main() {
    let root = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("C:\\Users"), PathBuf::from);
    let mut control = ScanControl::default();
    let result = scan_directory(&root, ScanOptions::default(), &mut control);

    let mut by_reason: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for item in result.tree.skipped() {
        by_reason
            .entry(format!("{:?}", item.reason))
            .or_default()
            .push(&item.path);
    }
    println!(
        "{} files, {} skipped, {} ms",
        result.files_scanned,
        result.tree.skipped().len(),
        result.elapsed_ms
    );
    for (reason, paths) in &by_reason {
        println!("{reason}: {}", paths.len());
        for path in paths.iter().take(6) {
            println!("    {path}");
        }
    }

    let base = root.to_string_lossy().trim_end_matches('\\').len() + 1;
    let mut by_top: BTreeMap<String, usize> = BTreeMap::new();
    for item in result.tree.skipped() {
        let rest = item.path.get(base..).unwrap_or("");
        let top = rest.split('\\').next().unwrap_or("").to_owned();
        *by_top.entry(top).or_default() += 1;
    }
    let mut tops: Vec<_> = by_top.into_iter().collect();
    tops.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("by top-level folder:");
    for (top, n) in tops.iter().take(10) {
        println!("  {n:>8}  {top}");
    }
}
