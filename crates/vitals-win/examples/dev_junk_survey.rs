//! Read-only survey of developer junk under a root: every `node_modules`,
//! `target`, `.next`, `.turbo`, `dist`, `build`, `.gradle`, `__pycache__`,
//! `.venv` folder, plus git worktrees and how stale each project is.
//!
//! **Deletes nothing.** Used to size the advanced cleanup before building it.
//!
//! `cargo run --release -p vitals-win --example dev_junk_survey -- E:\gh`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use vitals_win::storage::{ScanControl, ScanOptions, scan_directory};

const ARTEFACTS: &[&str] = &[
    "node_modules",
    "target",
    ".next",
    ".turbo",
    "dist",
    "build",
    ".gradle",
    "__pycache__",
    ".venv",
    "venv",
    ".pytest_cache",
    ".parcel-cache",
    ".svelte-kit",
    ".nuxt",
    "coverage",
    ".expo",
];

fn human(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let gb = bytes as f64 / 1_073_741_824.0;
    format!("{gb:8.2} GB")
}

fn size(path: &Path) -> u64 {
    let mut control = ScanControl {
        cancel: None,
        progress: None,
    };
    scan_directory(path, ScanOptions::default(), &mut control)
        .allocated()
        .0
}

/// Finds artefact folders without descending into them.
fn find(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, String)>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() || kind.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if ARTEFACTS.contains(&name.as_str()) {
            out.push((path, name));
        } else if name != ".git" {
            find(&path, depth + 1, out);
        }
    }
}

fn last_commit_age(repo: &Path) -> Option<Duration> {
    let out = std::process::Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["log", "-1", "--format=%ct"])
        .output()
        .ok()?;
    let secs: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        .ok()
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| r"E:\gh".into()));
    let mut found = Vec::new();
    find(&root, 0, &mut found);

    let mut by_kind: BTreeMap<String, (u64, usize)> = BTreeMap::new();
    let mut by_project: BTreeMap<PathBuf, u64> = BTreeMap::new();
    for (path, kind) in &found {
        let bytes = size(path);
        let entry = by_kind.entry(kind.clone()).or_default();
        entry.0 += bytes;
        entry.1 += 1;
        let project = path
            .strip_prefix(&root)
            .ok()
            .and_then(|rel| rel.components().next())
            .map_or_else(|| root.clone(), |c| root.join(c));
        *by_project.entry(project).or_default() += bytes;
    }

    println!("== by kind (root {})", root.display());
    let mut total = 0;
    for (kind, (bytes, count)) in &by_kind {
        total += bytes;
        println!("{} {count:6} x {kind}", human(*bytes));
    }
    println!("{} total", human(total));

    println!("\n== by top-level project (with last commit age)");
    let mut projects: Vec<_> = by_project.into_iter().collect();
    projects.sort_by_key(|(_, b)| std::cmp::Reverse(*b));
    for (project, bytes) in projects.iter().take(60) {
        let age = last_commit_age(project).map_or_else(
            || "   no git".to_owned(),
            |d| format!("{:5} days", d.as_secs() / 86_400),
        );
        println!("{} {age}  {}", human(*bytes), project.display());
    }
}
