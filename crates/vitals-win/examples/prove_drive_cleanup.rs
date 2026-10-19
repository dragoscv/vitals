//! Prints the per-drive clean-up plan for one drive of this machine.
//!
//! Read-only: the analysis starts, stops and deletes nothing. It shows what
//! the wizard would offer, with the sizes it measured (`?` = could not be
//! measured, never shown as zero) and what would be stopped.
//!
//! ```text
//! cargo run -p vitals-win --example prove_drive_cleanup -- C:
//! ```

#[cfg(windows)]
fn main() {
    use std::sync::atomic::AtomicBool;
    use vitals_win::storage::drive_cleanup::analyse;

    let mount = std::env::args().nth(1).unwrap_or_else(|| "C:".to_owned());
    let started = std::time::Instant::now();
    let mut progress = |stage: &'static str, done: u64, total: u64| {
        eprintln!("  [{done}/{total}] {stage}");
    };
    let Some(plan) = analyse(&mount, &AtomicBool::new(false), &mut progress) else {
        println!("{mount}: not a mounted drive");
        return;
    };
    let gb =
        |b: Option<u64>| b.map_or_else(|| "?".to_owned(), |b| format!("{:.2} GB", b as f64 / 1e9));
    println!(
        "{}  role={}  reasons={:?}  free={}  total={}",
        plan.mount,
        plan.role.key(),
        plan.role_reasons,
        gb(Some(plan.free_bytes)),
        gb(Some(plan.total_bytes))
    );
    let mut sum = 0_u64;
    let mut unmeasured = 0_usize;
    for (group, items) in plan.groups() {
        println!("\n== {}", group.key());
        for i in items {
            match i.size {
                Some(s) => sum += s,
                None => unmeasured += 1,
            }
            let flags = [
                (i.default_selected, "default"),
                (!i.available, "UNAVAILABLE"),
                (i.needs_elevation, "admin"),
                (i.stops_wsl, "stopsWsl"),
                (i.stops_docker, "stopsDocker"),
                (i.irreversible, "irreversible"),
            ]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| *name)
            .collect::<Vec<_>>()
            .join(",");
            println!(
                "  {:<24} {:>10}  {:<6} [{}] {}{}",
                i.kind,
                gb(i.size),
                i.safety.key(),
                flags,
                i.label.as_deref().unwrap_or(""),
                i.detail
                    .as_deref()
                    .map(|d| format!("\n{:>38}{d}", ""))
                    .unwrap_or_default()
            );
        }
    }
    println!("\n== blockers");
    for b in &plan.blockers {
        println!(
            "  {:<14} {}  restartAfter={}",
            b.kind.key(),
            b.name,
            b.restart_after
        );
    }
    println!(
        "\nmeasured total {} ({unmeasured} unmeasured), analysed in {:.1} s",
        gb(Some(sum)),
        started.elapsed().as_secs_f64()
    );
}

#[cfg(not(windows))]
fn main() {
    eprintln!("prove_drive_cleanup inspects Windows drives");
}
