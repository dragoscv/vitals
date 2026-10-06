//! Extracts the icon of every running process and prints what came back.
//!
//! The icon pipeline has two links that fail for different reasons — the
//! path (access denied for protected and other users' processes) and the
//! extraction itself — and a screen of generic glyphs cannot say which one
//! broke. This prints the count at each link and a sample of each outcome.
//!
//! `cargo run -p vitals-win --example prove_icons`

#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::time::Instant;

use vitals_win::actions::executable_path;
use vitals_win::process::ProcessEnumerator;
use vitals_win::process::icon::icon_data_url;

fn main() {
    let processes = match ProcessEnumerator::new().enumerate() {
        Ok(p) => p,
        Err(e) => {
            println!("enumerate failed: {e}");
            return;
        }
    };
    let started = Instant::now();
    let mut no_path = Vec::new();
    let mut no_icon = Vec::new();
    let mut sizes = BTreeMap::new();
    let mut with_icon = 0;
    for p in &processes {
        let name = p.name.clone().unwrap_or_default();
        let Some(path) = executable_path(p.key)
            .ok()
            .flatten()
            .or_else(|| vitals_win::process::image_path::image_path_by_pid(p.key.pid.get()))
        else {
            no_path.push(name);
            continue;
        };
        match icon_data_url(&path) {
            Some(url) => {
                with_icon += 1;
                *sizes.entry(url.len() / 1024).or_insert(0) += 1;
            }
            None => no_icon.push(path),
        }
    }
    println!(
        "{} processes: {with_icon} with an icon, {} without a readable path, {} with a path but no icon ({:.0} ms)",
        processes.len(),
        no_path.len(),
        no_icon.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    println!("data URL sizes (KiB -> count): {sizes:?}");
    no_path.sort();
    no_path.dedup();
    println!(
        "no path (sample): {:?}",
        no_path.iter().take(15).collect::<Vec<_>>()
    );
    no_icon.sort();
    no_icon.dedup();
    println!(
        "no icon (sample): {:?}",
        no_icon.iter().take(15).collect::<Vec<_>>()
    );
}
