//! Proves the Turbo scan and the saved index against this machine.
//!
//! Three modes, because the three need different rights:
//!
//! `cargo run --release -p vitals-win --example prove_turbo -- compare D`
//!   Run **elevated**. Reads D:'s file table and walks D:, then prints both
//!   totals, their difference, both timings and the ten folders whose
//!   figures differ most. The walk is elevated too, so the difference is
//!   the method's, not access rights'.
//!
//! `cargo run --release -p vitals-win --example prove_turbo -- index D`
//!   Unelevated. Walks D:, saves an index to `.copilot-tmp/prove-index`,
//!   writes a probe file, rescans from the index, and prints the reused and
//!   re-read folder counts, both timings, and whether the new total equals
//!   a fresh walk's.
//!
//! `cargo run --release -p vitals-win --example prove_turbo -- table D`
//!   Elevated. Reads the table only, prints its timing and orphan count.

use std::path::Path;
use std::time::Instant;

use vitals_win::storage::{
    ScanControl, ScanOptions, ScanResult, index, journal, scan_directory, turbo,
};

fn gb(bytes: u64) -> String {
    format!("{:.3} GB", bytes as f64 / 1e9)
}

fn walk(root: &str) -> (ScanResult, f64) {
    let started = Instant::now();
    let result = scan_directory(Path::new(root), ScanOptions::default(), &mut ScanControl::default());
    (result, started.elapsed().as_secs_f64())
}

fn turbo_scan(letter: char) -> (ScanResult, f64) {
    let started = Instant::now();
    let (folders, cluster) =
        turbo::read_volume(letter, None, &mut |_, _| {}).expect("elevated, NTFS");
    println!("  orphans (unreachable from the root): {}", folders.orphans);
    let result = turbo::into_result(folders, letter, Some(cluster), started.elapsed());
    (result, started.elapsed().as_secs_f64())
}

fn compare(letter: char) {
    let root = format!("{letter}:\\");
    let (t, t_secs) = turbo_scan(letter);
    let (w, w_secs) = walk(&root);
    let (ta, wa) = (t.allocated().get(), w.allocated().get());
    println!("turbo: {} alloc, {} logical, {} files, {} dirs, {t_secs:.1} s", gb(ta), gb(t.logical().get()), t.files_scanned, t.tree.len());
    println!("walk : {} alloc, {} logical, {} files, {} dirs, {w_secs:.1} s, {} gaps", gb(wa), gb(w.logical().get()), w.files_scanned, w.tree.len(), w.gaps());
    let diff = ta.abs_diff(wa);
    println!("difference {} = {:.4} % of the walk; speed-up {:.1}x", gb(diff), diff as f64 * 100.0 / wa.max(1) as f64, w_secs / t_secs.max(0.001));

    // Folder by folder, by path, for the ten largest disagreements.
    let mut worst: Vec<(u64, String, u64, u64)> = Vec::new();
    for i in 0..w.tree.len() {
        let id = vitals_win::storage::NodeId(i as u32);
        let path = w.tree.path_of(id);
        let Some(node) = w.tree.node(id) else { continue };
        let own_w = node.own_allocated().get();
        let own_t = t
            .find_directory(&path)
            .and_then(|n| t.tree.node(n))
            .map_or(0, |n| n.own_allocated().get());
        let d = own_t.abs_diff(own_w);
        if d > 0 {
            worst.push((d, path, own_t, own_w));
        }
    }
    worst.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    println!("folders whose own bytes differ: {}", worst.len());
    for (d, path, own_t, own_w) in worst.iter().take(10) {
        println!("  {:>12} diff  turbo {:>14}  walk {:>14}  {path}", d, own_t, own_w);
    }
}

fn index_run(letter: char) {
    let root = format!("{letter}:\\");
    let dir = Path::new(".copilot-tmp").join("prove-index");
    let before = journal::checkpoint(&root).expect("journal");
    let (first, first_secs) = walk(&root);
    let bytes = index::save(&dir, &first, &before).expect("save");
    println!("walk 1: {} in {first_secs:.1} s; index {} MB", gb(first.allocated().get()), bytes / 1_000_000);

    let probe_dir = format!("{root}vitals-index-probe");
    std::fs::create_dir_all(&probe_dir).expect("probe dir");
    std::fs::write(format!("{probe_dir}\\probe.bin"), vec![7_u8; 3_000_000]).expect("probe file");

    let now = journal::checkpoint(&root).expect("journal");
    let saved = index::load(&dir, &root).expect("load");
    let started = Instant::now();
    let again = index::rescan(&saved, &now, ScanOptions::default(), &mut ScanControl::default())
        .expect("usable");
    let again_secs = started.elapsed().as_secs_f64();
    let (truth, truth_secs) = walk(&root);
    let _ = std::fs::remove_dir_all(&probe_dir);

    println!(
        "rescan: {} in {again_secs:.2} s; reused {:?}, re-read {:?}",
        gb(again.allocated().get()),
        again.reused_directories,
        again.relisted_directories
    );
    println!("walk 2: {} in {truth_secs:.1} s", gb(truth.allocated().get()));
    let found = again.find_directory(&probe_dir).is_some();
    println!("probe folder picked up: {found}");
    println!(
        "rescan vs walk 2: {} apart ({} files vs {}); speed-up {:.1}x",
        gb(again.allocated().get().abs_diff(truth.allocated().get())),
        again.files_scanned,
        truth.files_scanned,
        truth_secs / again_secs.max(0.001)
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    let letter = args.next().and_then(|s| s.chars().next()).unwrap_or('D').to_ascii_uppercase();
    match mode.as_str() {
        "compare" => compare(letter),
        "index" => index_run(letter),
        "table" => {
            let (t, secs) = turbo_scan(letter);
            println!("table: {} dirs, {} files, {} in {secs:.1} s", t.tree.len(), t.files_scanned, gb(t.allocated().get()));
        }
        _ => eprintln!("usage: prove_turbo compare|index|table <letter>"),
    }
}
