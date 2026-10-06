//! Scans a directory and reports where the space went.
//!
//! Deliberately bounded by default: scanning all of `C:` in a routine test
//! run costs minutes and tells nobody anything they did not know.
//!
//! Run with:
//! `cargo run -p vitals-win --example storage_probe`
//! `cargo run -p vitals-win --example storage_probe -- "C:\Program Files"`
//!
//! Cross-check the logical total against PowerShell — note `Length`, which is
//! the *logical* size, so it is the LOGICAL column that should match, never
//! the allocated one:
//! `Get-ChildItem -Recurse -File -Force "C:\Windows\System32" -EA SilentlyContinue |
//!  Measure-Object -Sum Length`

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use vitals_win::storage::{
    CellKind, Detail, ScanControl, ScanOptions, ScanProgress, ScanResult, ScanStrategy,
    find_cleanup_candidates, icicle, largest_directories, scan_directory, strategy_for, treemap,
};

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn print_strategy() {
    let fs = vitals_win::disk::enumerate_volumes()
        .into_iter()
        .find(|v| v.mount.starts_with('C'))
        .and_then(|v| v.file_system);
    match strategy_for(fs.as_deref()) {
        ScanStrategy::Turbo => {
            println!("strategy: NTFS, so a Turbo scan (elevated, prove_turbo) is available");
        }
        ScanStrategy::DirectoryWalk => println!("strategy: directory walk only"),
    }
}

fn run_scan(root: &Path, cancel: &AtomicBool) -> ScanResult {
    let mut on_progress = |p: ScanProgress| {
        if let Some(rate) = p.files_per_second() {
            print!(
                "\r  scanning… {} files, {} dirs, {} — {rate:.0} files/s   ",
                p.files_seen,
                p.directories_seen,
                human(p.bytes_seen)
            );
        }
    };

    let mut control = ScanControl {
        cancel: Some(cancel),
        progress: Some(&mut on_progress),
    };

    // VITALS_PROBE_LARGEST=0 turns the largest-files list off, for an A/B of
    // what keeping it costs.
    let largest_files = std::env::var("VITALS_PROBE_LARGEST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(ScanOptions::default().largest_files);
    scan_directory(
        root,
        ScanOptions {
            largest_files,
            ..ScanOptions::default()
        },
        &mut control,
    )
}

fn print_totals(result: &ScanResult) {
    println!("== totals ==");
    println!(
        "  allocated (on disk)   {:>14}  ({} bytes)",
        human(result.allocated().get()),
        result.allocated().get()
    );
    println!(
        "  logical (file length) {:>14}  ({} bytes)",
        human(result.logical().get()),
        result.logical().get()
    );
    println!(
        "  cluster size          {:>14}",
        result
            .cluster_bytes
            .map_or_else(|| "unknown".to_owned(), human)
    );
    // Printed in whichever direction it actually falls. A saturating
    // subtraction would show "0 B" whenever compression wins, hiding the more
    // interesting of the two cases behind a number that looks like agreement.
    let allocated = result.allocated().get();
    let logical = result.logical().get();
    if allocated >= logical {
        println!(
            "  cluster slack         {:>14}  — allocated exceeds logical: \
             every file is rounded up to a whole cluster",
            human(allocated - logical)
        );
    } else {
        println!(
            "  compression saving    {:>14}  — allocated is BELOW logical: \
             NTFS-compressed files occupy less than their length",
            human(logical - allocated)
        );
    }
    println!();
}

fn print_scan_stats(result: &ScanResult) {
    println!("== scan ==");
    println!("  files                 {:>14}", result.files_scanned);
    println!("  directories           {:>14}", result.directories_scanned);
    println!("  elapsed               {:>11} ms", result.elapsed_ms);
    println!("  threads               {:>14}", result.threads);
    match result.files_per_second() {
        Some(rate) => println!("  rate                  {rate:>11.0} files/s"),
        None => println!("  rate                       too fast to measure"),
    }
    println!(
        "  tree memory           {:>14}  ({} nodes, {} distinct names)",
        human(result.tree.memory_bytes() as u64),
        result.tree.len(),
        result.tree.distinct_names()
    );
    println!(
        "  hard links suppressed {:>14}  saving {}",
        result.hard_link_duplicates,
        human(result.hard_link_bytes_saved)
    );
    println!(
        "  complete              {:>14}",
        if result.is_complete() { "yes" } else { "NO" }
    );
    println!();
}

fn print_largest(result: &ScanResult) {
    println!("== top 20 directories by allocated size ==");
    for (i, entry) in largest_directories(result, 20).iter().enumerate() {
        println!(
            "  {:>2}. {:>10}  {:>7} files  {}{}",
            i + 1,
            human(entry.allocated.get()),
            entry.files,
            entry.path,
            entry
                .incomplete
                .map_or_else(String::new, |r| format!("  [{}]", r.as_str()))
        );
    }
    println!();
}

fn print_skipped(result: &ScanResult) {
    let skipped = result.tree.skipped();
    println!(
        "== skipped ({} directories, excluded from totals) ==",
        skipped.len()
    );
    let mut by_reason: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for item in skipped {
        *by_reason.entry(item.reason.as_str()).or_default() += 1;
    }
    for (reason, count) in &by_reason {
        println!("  {count:>8}  {reason}");
    }
    for item in skipped.iter().take(15) {
        println!("  {:<28}  {}", item.reason.as_str(), item.path);
    }
    if skipped.len() > 15 {
        println!("  … and {} more", skipped.len() - 15);
    }
    println!();
}

fn print_largest_files(result: &ScanResult) {
    println!(
        "== top 10 of the {} largest files kept ==",
        result.largest_files.len()
    );
    for (i, file) in result.largest_files.iter().take(10).enumerate() {
        println!(
            "  {:>2}. {:>10}  {}",
            i + 1,
            human(file.allocated.get()),
            result.path_of_file(file)
        );
    }
    println!();
}

/// What the navigator costs: layout time and cell count at the root, and at
/// the largest child, for both shapes. The UI asks for exactly these.
fn print_maps(result: &ScanResult) {
    println!("== map layouts (what the navigator is sent) ==");
    let tree = &result.tree;
    let mut foci = vec![tree.root()];
    if let Some(first) = tree.children_by_size(tree.root()).first() {
        foci.push(*first);
    }
    for focus in foci {
        for shape in ["icicle", "treemap"] {
            let started = std::time::Instant::now();
            let cells = if shape == "icicle" {
                icicle(tree, focus, Detail::default())
            } else {
                treemap(
                    tree,
                    focus,
                    1.8,
                    Detail {
                        max_depth: 4,
                        ..Detail::default()
                    },
                )
            };
            let elapsed = started.elapsed();
            let smaller = cells.iter().filter(|c| c.kind == CellKind::Smaller).count();
            println!(
                "  {:<8} {:>5} cells ({smaller} folded) in {:>6.2} ms  at {}",
                shape,
                cells.len(),
                elapsed.as_secs_f64() * 1000.0,
                tree.path_of(focus)
            );
        }
    }
    println!();
}

fn print_cleanup(cancel: &AtomicBool) {
    println!("== cleanup candidates (report only — nothing is deleted) ==");
    let candidates = find_cleanup_candidates(Some(cancel));
    if candidates.is_empty() {
        println!("  none found");
    }
    for candidate in &candidates {
        let size = candidate
            .size
            .map_or_else(|| "unknown".to_owned(), |b| human(b.get()));
        println!(
            "  {:>10}  {:<7}  {}{}",
            size,
            candidate.safety.as_str(),
            candidate.path.display(),
            if candidate.needs_elevation {
                "  [needs elevation]"
            } else {
                ""
            }
        );
        println!("              {}", candidate.kind.reason());
    }
    println!();
}

fn main() {
    let root = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("C:\\Windows\\System32"), PathBuf::from);

    println!("Vitals storage probe");
    println!("root: {}", root.display());
    print_strategy();
    println!();

    let cancel = AtomicBool::new(false);
    let result = run_scan(&root, &cancel);
    println!("\r{:70}\r", "");

    print_totals(&result);
    print_scan_stats(&result);
    print_largest(&result);
    print_skipped(&result);
    print_largest_files(&result);
    print_maps(&result);
    // Off with VITALS_PROBE_NO_CLEANUP=1, so a timing run measures the scan
    // and not seconds of cache sizing after it.
    if std::env::var_os("VITALS_PROBE_NO_CLEANUP").is_none() {
        print_cleanup(&cancel);
    }

    println!("Cross-check the LOGICAL total (not the allocated one) with:");
    println!(
        "  Get-ChildItem -Recurse -File -Force \"{}\" -EA SilentlyContinue | \
         Measure-Object -Sum Length",
        root.display()
    );
}
