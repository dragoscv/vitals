//! Lists every WSL distro and Docker Desktop's disk on this machine, where
//! each lives, and which drives could take it — without moving anything.
//!
//! Read-only: no distro is stopped or started, nothing is written. A real
//! move belongs on a test machine (see ADR-0040); this proves the planning
//! half against the real registry, volumes and Docker settings.
//!
//! ```text
//! cargo run -p vitals-win --example prove_drive_move -- --dry-run [C:]
//! ```

#[cfg(windows)]
mod prove {
    use vitals_win::storage::drive_move::{
        self as dm, DiskOwner, DockerDisk, MovePlan, Registration, required_bytes,
    };
    use vitals_win::storage::drive_role::drive_of;

    pub fn gb(b: Option<u64>) -> String {
        b.map_or_else(|| "?".to_owned(), |b| format!("{:.1} GB", b as f64 / 1e9))
    }

    pub fn header() {
        let version = vitals_win::storage::drive_cleanup::probe::wsl_version();
        println!(
            "WSL {}  (move needs >= 2.3.11: {})",
            version.map_or_else(|| "unknown".to_owned(), |v| format!("{:?}", v.0)),
            dm::move_refusal(version).map_or("yes", |r| r.key())
        );
    }

    pub fn registrations() -> Vec<Registration> {
        println!("\n== registrations");
        let regs = dm::registrations();
        for r in &regs {
            let size = std::fs::metadata(r.vhd_path()).ok().map(|m| m.len());
            println!(
                "  {:<16} wsl2={} size={:<9} {}",
                r.name,
                r.wsl2,
                gb(size),
                r.vhd_path().display()
            );
        }
        regs
    }

    pub fn docker() -> DockerDisk {
        let docker = dm::docker_disk();
        println!(
            "\n== Docker Desktop  installed={} version={:?} crossDriveOk={:?}\n  location={:?}\n  disk={:?} size={}",
            docker.installed,
            docker.version,
            docker.cross_drive_ok(),
            docker.location,
            docker.path,
            gb(docker.size)
        );
        docker
    }

    pub fn volumes() {
        println!("\n== volumes");
        for f in dm::target_facts() {
            println!(
                "  {} {:<9} {:<6} free={:<9} label={:?}",
                f.mount,
                format!("{:?}", f.kind),
                f.file_system.as_deref().unwrap_or("?"),
                gb(Some(f.free)),
                f.label
            );
        }
    }

    /// The drives to plan: the one asked for, else every drive a disk is on.
    pub fn mounts(args: &[String], regs: &[Registration], docker: &DockerDisk) -> Vec<String> {
        if let Some(m) = args.iter().find(|a| a.ends_with(':')) {
            return vec![m.clone()];
        }
        let mut m: Vec<String> = regs
            .iter()
            .filter_map(|r| drive_of(&r.vhd_path()))
            .chain(docker.path.iter().filter_map(|p| drive_of(p)))
            .collect();
        m.sort();
        m.dedup();
        m
    }

    pub fn plan(plan: &MovePlan, elapsed_ms: u128) {
        println!("\n== plan for {} ({elapsed_ms} ms)", plan.mount);
        for d in &plan.disks {
            println!(
                "  {} [{}] size={} running={} needs={} unavailable={}",
                d.name,
                d.owner.key(),
                gb(d.size),
                d.running,
                gb(d.size.map(required_bytes)),
                d.unavailable.map_or("-", |u| u.key())
            );
            for t in &d.targets {
                println!(
                    "      -> {} {}",
                    t.facts.mount,
                    t.refusal.map_or("ELIGIBLE", |r| r.key())
                );
            }
            for b in &d.blockers {
                println!(
                    "      stops {} {} (restart={})",
                    b.kind.key(),
                    b.name,
                    b.restart_after
                );
            }
            let folder = (d.unavailable.is_none() && d.owner == DiskOwner::Distro)
                .then(|| d.targets.iter().find(|t| t.refusal.is_none()))
                .flatten()
                .and_then(|t| dm::wsl_target_folder(&t.facts.mount, &d.name));
            if let Some(folder) = folder {
                println!(
                    "      would run: {}",
                    dm::display_command(&dm::move_args(&d.name, &folder))
                );
            }
        }
    }
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.iter().any(|a| a == "--dry-run") {
        eprintln!("refusing: this prover only plans. Pass --dry-run.");
        std::process::exit(2);
    }
    prove::header();
    let regs = prove::registrations();
    let docker = prove::docker();
    prove::volumes();
    for mount in prove::mounts(&args, &regs, &docker) {
        let started = std::time::Instant::now();
        match vitals_win::storage::drive_move::plan(&mount) {
            Some(plan) => prove::plan(&plan, started.elapsed().as_millis()),
            None => println!("\n{mount}: not a mounted drive"),
        }
    }
    println!("\nNothing was stopped, started or moved.");
}

#[cfg(not(windows))]
fn main() {
    eprintln!("prove_drive_move is Windows-only");
}
