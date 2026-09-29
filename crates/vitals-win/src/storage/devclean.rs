//! Developer cleanup: regenerable build output, stray git worktrees, package
//! manager caches, Docker, and the virtual disks WSL and Docker grow.
//!
//! # What may be removed, and how
//!
//! Only things that come back by themselves or with one command:
//!
//! - **Build artefacts** (`node_modules`, Rust `target`, `.next`, `.turbo`,
//!   `dist`, `build`, `.gradle`, `__pycache__`, `.venv`, ...) — but only when
//!   the folder's *parent* is recognisably a project that produces it (a
//!   `package.json` beside `node_modules`, a `Cargo.toml` beside `target`, a
//!   `next.config.*` beside `.next`). A folder called `build` under
//!   `Documents` is not a build artefact, and guessing that it is would be
//!   how a cleaner deletes someone's work.
//! - **Git worktrees** only through `git worktree remove` (never a raw
//!   delete: git also holds metadata for them), and only when clean, on a
//!   remote ref and idle. A worktree whose folder is already gone is
//!   `git worktree prune`d.
//! - **Package caches** through their own tool (`pnpm store prune`, `npm
//!   cache clean --force`, `cargo cache`...) wherever one exists, so the tool
//!   keeps its own bookkeeping consistent.
//! - **Docker** through the Docker CLI: dangling and unused images, build
//!   cache, stopped containers. **Volumes are never offered** — they hold
//!   databases.
//! - **Virtual disks** by compaction (`Optimize-VHD`, elevated once), which
//!   gives space Docker and WSL already freed back to Windows. Nothing
//!   inside the disk is deleted by it.
//!
//! # The webview names ids, never paths
//!
//! A scan mints an id per actionable item and the desktop keeps the scan.
//! [`Plan::resolve`] maps ids back to items of *that* scan, and every item
//! is re-checked at execution time ([`recheck`]), because minutes can pass
//! between the scan and the click.
//!
//! # Freed is measured
//!
//! Every folder is sized before it is removed and checked after; the report
//! carries what actually went. A pnpm `node_modules` is mostly hard links
//! into the store, so its bytes are only freed when the last link goes —
//! the scan says so rather than promising the whole figure.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use super::protect::Rules;
use super::{ScanControl, ScanOptions, scan_directory};

/// A regenerable folder kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtefactKind {
    NodeModules,
    CargoTarget,
    Next,
    Turbo,
    Dist,
    Build,
    Gradle,
    Pycache,
    Venv,
    PytestCache,
    ParcelCache,
    SvelteKit,
    Nuxt,
    Coverage,
    Expo,
}

impl ArtefactKind {
    /// Translation key, matching `ArtefactKindKey` in the UI.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::NodeModules => "nodeModules",
            Self::CargoTarget => "cargoTarget",
            Self::Next => "next",
            Self::Turbo => "turbo",
            Self::Dist => "dist",
            Self::Build => "build",
            Self::Gradle => "gradle",
            Self::Pycache => "pycache",
            Self::Venv => "venv",
            Self::PytestCache => "pytestCache",
            Self::ParcelCache => "parcelCache",
            Self::SvelteKit => "svelteKit",
            Self::Nuxt => "nuxt",
            Self::Coverage => "coverage",
            Self::Expo => "expo",
        }
    }

    /// The command that brings it back.
    #[must_use]
    pub const fn restore(self) -> &'static str {
        match self {
            Self::NodeModules => "pnpm install",
            Self::CargoTarget => "cargo build",
            Self::Next => "next build",
            Self::Turbo => "turbo run build",
            Self::Dist | Self::Build | Self::Coverage => "the project's build",
            Self::Gradle => "gradlew build",
            Self::Pycache | Self::PytestCache => "regenerated when Python runs",
            Self::Venv => "python -m venv .venv",
            Self::ParcelCache => "parcel build",
            Self::SvelteKit => "vite dev",
            Self::Nuxt => "nuxt build",
            Self::Expo => "expo start",
        }
    }
}

/// Recognises an artefact folder by its name **and** the project around it.
///
/// `parent_has` answers "does the parent folder contain an entry with this
/// name". Pure, so the rule table is testable without a disk.
#[must_use]
pub fn classify(name: &str, parent_has: &dyn Fn(&str) -> bool) -> Option<ArtefactKind> {
    let any = |names: &[&str]| names.iter().any(|n| parent_has(n));
    let node = || any(&["package.json"]);
    let kind = match name {
        "node_modules" if node() => ArtefactKind::NodeModules,
        "target" if any(&["Cargo.toml"]) => ArtefactKind::CargoTarget,
        ".next" if node() => ArtefactKind::Next,
        ".turbo" if node() || any(&["turbo.json"]) => ArtefactKind::Turbo,
        ".svelte-kit" if node() => ArtefactKind::SvelteKit,
        ".nuxt" if node() => ArtefactKind::Nuxt,
        ".parcel-cache" if node() => ArtefactKind::ParcelCache,
        ".expo" if node() => ArtefactKind::Expo,
        // `dist`/`build`/`coverage` are generic names: only beside a
        // manifest that produces them, and never a Gradle `build` that sits
        // next to source it would be mistaken for.
        "dist" if node() || any(&["pyproject.toml", "setup.py"]) => ArtefactKind::Dist,
        "build"
            if node()
                || any(&[
                    "build.gradle",
                    "build.gradle.kts",
                    "settings.gradle",
                    "settings.gradle.kts",
                    "CMakeLists.txt",
                ]) =>
        {
            ArtefactKind::Build
        }
        "coverage" if node() => ArtefactKind::Coverage,
        ".gradle"
            if any(&[
                "build.gradle",
                "build.gradle.kts",
                "settings.gradle",
                "settings.gradle.kts",
                "gradlew",
            ]) =>
        {
            ArtefactKind::Gradle
        }
        "__pycache__" => ArtefactKind::Pycache,
        ".pytest_cache" => ArtefactKind::PytestCache,
        ".venv" | "venv"
            if any(&[
                "pyproject.toml",
                "requirements.txt",
                "setup.py",
                "Pipfile",
                "uv.lock",
            ]) =>
        {
            // A venv must also look like one; `venv` is otherwise just a name.
            ArtefactKind::Venv
        }
        _ => return None,
    };
    Some(kind)
}

/// Folders never descended into while looking for projects.
fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "$RECYCLE.BIN" | "System Volume Information" | ".wt-backup"
    ) || name.eq_ignore_ascii_case("windows")
}

/// One artefact found by [`find_artefacts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub kind: ArtefactKind,
    /// The folder the artefact belongs to (its parent).
    pub owner: PathBuf,
}

/// Walks `root` for artefact folders without descending into them.
///
/// Reparse points (junctions, symlinks) are never followed: pnpm and
/// worktrees link folders into each other, and following one would find
/// the same `node_modules` twice or walk into another drive.
pub fn find_artefacts(root: &Path, max_depth: usize, cancel: &AtomicBool, out: &mut Vec<Found>) {
    walk(root, 0, max_depth, cancel, out);
}

fn walk(dir: &Path, depth: usize, max_depth: usize, cancel: &AtomicBool, out: &mut Vec<Found>) {
    if depth > max_depth || cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let entries: Vec<_> = entries.flatten().collect();
    let names: BTreeSet<String> = entries
        .iter()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let parent_has = |n: &str| {
        names.contains(n)
            || names
                .iter()
                .any(|x| x.starts_with("next.config.") && n == "next.config")
    };
    for entry in &entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // `file_type` on Windows reports a junction as a symlink.
        if !kind.is_dir() || kind.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if skip_dir(&name) {
            continue;
        }
        let path = entry.path();
        if let Some(kind) = classify(&name, &parent_has) {
            out.push(Found {
                path,
                kind,
                owner: dir.to_path_buf(),
            });
        } else {
            walk(&path, depth + 1, max_depth, cancel, out);
        }
    }
}

/// On-disk size of a folder, and its file count; `None` when unreadable.
#[must_use]
pub fn measure(path: &Path, cancel: &AtomicBool) -> Option<(u64, u64)> {
    let mut control = ScanControl {
        cancel: Some(cancel),
        progress: None,
    };
    let result = scan_directory(
        path,
        ScanOptions {
            // Hard links on: a pnpm `node_modules` is mostly links, and
            // counting each once is the honest per-folder figure.
            detect_hard_links: true,
            ..ScanOptions::default()
        },
        &mut control,
    );
    if result.cancelled {
        return None;
    }
    let root = result.tree.node(result.tree.root())?;
    if root.skipped().is_some() {
        return None;
    }
    Some((result.allocated().0, root.file_count()))
}

/// Whether a `node_modules` is pnpm's layout (a `.pnpm` folder inside):
/// its files are hard links into the global store.
#[must_use]
pub fn is_pnpm_layout(node_modules: &Path) -> bool {
    node_modules.join(".pnpm").is_dir()
}

/// Days since anything happened in a project: the later of its last commit
/// or reflog entry and the newest modification among its top-level entries
/// (excluding the artefacts themselves, which builds touch constantly).
#[must_use]
pub fn idle_days(project: &Path, artefacts: &[PathBuf], git: Option<SystemTime>) -> Option<u64> {
    let mut newest = git;
    if let Ok(entries) = std::fs::read_dir(project) {
        for entry in entries.flatten() {
            let path = entry.path();
            if artefacts.iter().any(|a| a == &path) || entry.file_name() == ".git" {
                continue;
            }
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                newest = Some(newest.map_or(modified, |n: SystemTime| n.max(modified)));
            }
        }
    }
    let age = SystemTime::now()
        .duration_since(newest?)
        .unwrap_or(Duration::ZERO);
    Some(age.as_secs() / 86_400)
}

/// Days of inactivity from which a project's artefacts are pre-ticked.
pub const STALE_DAYS: u64 = 30;

/// The git repository a folder belongs to, if any (walking up to `stop`).
#[must_use]
pub fn git_root(start: &Path, stop: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        if d == stop {
            return None;
        }
        dir = d.parent();
    }
    None
}

/// Newest of the last commit and the last reflog entry, from git itself.
#[must_use]
pub fn git_last_activity(repo: &Path) -> Option<SystemTime> {
    let out = git(repo, &["log", "-g", "-1", "--format=%ct"])
        .or_else(|| git(repo, &["log", "-1", "--format=%ct"]))?;
    let secs: u64 = out.trim().parse().ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

/// Runs git with no optional locks, returning stdout on success.
fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = hidden(tool("git")?)
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A command that never flashes a console window.
fn hidden(program: PathBuf) -> std::process::Command {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Resolves a tool on `PATH` to an absolute path, or `None`.
///
/// Absolute so the command that runs is the one we found, and never a file
/// of the same name in the project folder (Windows searches the current
/// directory first for `CreateProcess` without a path).
#[must_use]
pub fn tool(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
        .split(';')
        .map(str::to_ascii_lowercase)
        .collect();
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Worktrees
// ---------------------------------------------------------------------------

/// Why a worktree is or is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeState {
    /// Clean, HEAD on a remote ref, idle: `git worktree remove`.
    Removable,
    /// Its folder is gone; only git's record is left: `git worktree prune`.
    Prunable,
    Dirty,
    Unpushed,
    Active,
    Locked,
}

impl WorktreeState {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Removable => "removable",
            Self::Prunable => "prunable",
            Self::Dirty => "dirty",
            Self::Unpushed => "unpushed",
            Self::Active => "active",
            Self::Locked => "locked",
        }
    }

    #[must_use]
    pub const fn offered(self) -> bool {
        matches!(self, Self::Removable | Self::Prunable)
    }
}

/// Hours below which a worktree counts as in use.
pub const WORKTREE_IDLE_HOURS: u64 = 12;

/// Decides a worktree's state from the facts git gives. Pure.
#[must_use]
pub fn worktree_state(
    exists: bool,
    locked: bool,
    changes: usize,
    on_remote: bool,
    idle_hours: Option<u64>,
) -> WorktreeState {
    if !exists {
        return WorktreeState::Prunable;
    }
    if locked {
        return WorktreeState::Locked;
    }
    if changes > 0 {
        return WorktreeState::Dirty;
    }
    if !on_remote {
        return WorktreeState::Unpushed;
    }
    match idle_hours {
        Some(h) if h >= WORKTREE_IDLE_HOURS => WorktreeState::Removable,
        // Unknown idleness is treated as active: offering a worktree we
        // cannot date is offering one someone may be using.
        _ => WorktreeState::Active,
    }
}

/// A linked worktree as git lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub locked: bool,
    /// Git itself says the folder is gone.
    pub prunable: bool,
}

/// Parses `git worktree list --porcelain`, skipping the main working tree
/// (the first record).
#[must_use]
pub fn parse_worktrees(porcelain: &str) -> Vec<WorktreeEntry> {
    let mut out = Vec::new();
    for (index, block) in porcelain.split("\n\n").enumerate() {
        let mut entry = WorktreeEntry {
            path: PathBuf::new(),
            branch: None,
            head: None,
            locked: false,
            prunable: false,
        };
        let mut bare = false;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                entry.path = PathBuf::from(p.replace('/', "\\"));
            } else if let Some(b) = line.strip_prefix("branch ") {
                entry.branch = Some(b.trim_start_matches("refs/heads/").to_owned());
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                entry.head = Some(h.to_owned());
            } else if line.starts_with("locked") {
                entry.locked = true;
            } else if line.starts_with("prunable") {
                entry.prunable = true;
            } else if line == "bare" {
                bare = true;
            }
        }
        if index == 0 || bare || entry.path.as_os_str().is_empty() {
            continue;
        }
        out.push(entry);
    }
    out
}

/// A worktree with the facts that decide whether it may go.
#[derive(Debug, Clone)]
pub struct Worktree {
    pub repo: PathBuf,
    pub entry: WorktreeEntry,
    pub state: WorktreeState,
    pub changes: Option<usize>,
    pub idle_hours: Option<u64>,
}

/// Inspects every linked worktree of `repo`.
#[must_use]
pub fn worktrees_of(repo: &Path) -> Vec<Worktree> {
    let Some(list) = git(repo, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    parse_worktrees(&list.replace("\r\n", "\n"))
        .into_iter()
        .map(|entry| inspect_worktree(repo, entry))
        .collect()
}

fn inspect_worktree(repo: &Path, entry: WorktreeEntry) -> Worktree {
    let exists = !entry.prunable && entry.path.is_dir();
    let (changes, on_remote, idle) = if exists {
        let changes = git(
            &entry.path,
            &["status", "--porcelain", "--untracked-files=normal"],
        )
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count());
        let on_remote = git(&entry.path, &["branch", "-r", "--contains", "HEAD"])
            .is_some_and(|s| !s.trim().is_empty());
        let idle = git(&entry.path, &["log", "-g", "-1", "--format=%ct"])
            .and_then(|s| s.trim().parse::<u64>().ok())
            .and_then(|secs| {
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
                    .ok()
            })
            .map(|d| d.as_secs() / 3600);
        (changes, on_remote, idle)
    } else {
        (Some(0), true, None)
    };
    // An unreadable status is treated as dirty: never offer what we could
    // not prove clean.
    let state = worktree_state(
        exists,
        entry.locked,
        changes.unwrap_or(usize::MAX),
        on_remote,
        idle,
    );
    Worktree {
        repo: repo.to_path_buf(),
        entry,
        state,
        changes,
        idle_hours: idle,
    }
}

// ---------------------------------------------------------------------------
// Package caches
// ---------------------------------------------------------------------------

/// A package manager cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    Pnpm,
    Npm,
    Yarn,
    Cargo,
    Gradle,
    Nuget,
    Pip,
    Uv,
    Go,
    Playwright,
    Electron,
}

/// How a cache is emptied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheMethod {
    /// The tool's own command, with its arguments.
    Command {
        program: &'static str,
        args: &'static [&'static str],
    },
    /// No tool: the folder is deleted (only for pure download caches).
    Delete,
}

impl CacheKind {
    pub const ALL: [Self; 11] = [
        Self::Pnpm,
        Self::Npm,
        Self::Yarn,
        Self::Cargo,
        Self::Gradle,
        Self::Nuget,
        Self::Pip,
        Self::Uv,
        Self::Go,
        Self::Playwright,
        Self::Electron,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Pnpm => "pnpm",
            Self::Npm => "npm",
            Self::Yarn => "yarn",
            Self::Cargo => "cargo",
            Self::Gradle => "gradle",
            Self::Nuget => "nuget",
            Self::Pip => "pip",
            Self::Uv => "uv",
            Self::Go => "go",
            Self::Playwright => "playwright",
            Self::Electron => "electron",
        }
    }

    /// The tool's own clean command where one exists.
    ///
    /// `pnpm store prune` removes only packages no project links to, so it
    /// never breaks an installed `node_modules`. Cargo has no built-in
    /// clean for the registry, and Gradle's caches are rebuilt on demand, so
    /// those two are plain deletes of pure download caches.
    #[must_use]
    pub const fn method(self) -> CacheMethod {
        match self {
            Self::Pnpm => CacheMethod::Command {
                program: "pnpm",
                args: &["store", "prune"],
            },
            Self::Npm => CacheMethod::Command {
                program: "npm",
                args: &["cache", "clean", "--force"],
            },
            Self::Yarn => CacheMethod::Command {
                program: "yarn",
                args: &["cache", "clean"],
            },
            Self::Nuget => CacheMethod::Command {
                program: "dotnet",
                args: &["nuget", "locals", "all", "--clear"],
            },
            Self::Pip => CacheMethod::Command {
                program: "pip",
                args: &["cache", "purge"],
            },
            Self::Uv => CacheMethod::Command {
                program: "uv",
                args: &["cache", "clean"],
            },
            Self::Go => CacheMethod::Command {
                program: "go",
                args: &["clean", "-cache"],
            },
            Self::Cargo | Self::Gradle | Self::Playwright | Self::Electron => CacheMethod::Delete,
        }
    }

    #[must_use]
    pub const fn restore(self) -> &'static str {
        match self {
            Self::Pnpm => "re-downloaded by the next pnpm install",
            Self::Npm | Self::Yarn | Self::Pip | Self::Uv => "re-downloaded by the next install",
            Self::Cargo => "re-downloaded by the next cargo build",
            Self::Gradle => "re-downloaded by the next Gradle build",
            Self::Nuget => "re-downloaded by the next dotnet restore",
            Self::Go => "rebuilt by the next go build",
            Self::Playwright => "pnpm exec playwright install",
            Self::Electron => "re-downloaded by the next Electron install",
        }
    }

    /// Where the cache lives, from the environment. `None` when the
    /// variable is missing. For pnpm, the store path comes from pnpm
    /// itself when it is installed.
    #[must_use]
    pub fn locations(self) -> Vec<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
        let at = |base: &Option<PathBuf>, rel: &str| base.as_ref().map(|b| b.join(rel));
        let v: Vec<Option<PathBuf>> = match self {
            Self::Pnpm => vec![at(&local, "pnpm\\store")],
            Self::Npm => vec![at(&local, "npm-cache")],
            Self::Yarn => vec![at(&local, "Yarn\\Cache")],
            Self::Cargo => vec![
                at(&home, ".cargo\\registry\\cache"),
                at(&home, ".cargo\\registry\\src"),
            ],
            Self::Gradle => vec![at(&home, ".gradle\\caches")],
            Self::Nuget => vec![at(&home, ".nuget\\packages")],
            Self::Pip => vec![at(&local, "pip\\Cache")],
            Self::Uv => vec![at(&local, "uv\\cache")],
            Self::Go => vec![at(&local, "go-build")],
            Self::Playwright => vec![at(&local, "ms-playwright")],
            Self::Electron => vec![at(&local, "electron\\Cache")],
        };
        v.into_iter().flatten().collect()
    }
}

/// Every pnpm store on this machine that the installed pnpm uses, as the
/// versioned folder it works in (`...\v11`). Prune it with
/// `--store-dir <parent>`.
///
/// pnpm keeps one store per drive, because hard links cannot cross drives:
/// a project on E: uses `E:\.pnpm-store`, one on C: the store under
/// `%LOCALAPPDATA%`. `pnpm store path` answers for the current directory
/// only, so asking it once found one store and `pnpm store prune` run from
/// the profile pruned a different one — reported as done, 0 B given back,
/// with 9 GB still on E: (2026-09-29). Each store is therefore found on its
/// own and pruned with `--store-dir`. Older version folders (`v3`, `v10`)
/// are left alone: this pnpm cannot prune them, and projects installed by an
/// older pnpm still link into them.
#[must_use]
pub fn pnpm_stores() -> Vec<PathBuf> {
    let Some(version) = pnpm_store_version() else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = CacheKind::Pnpm.locations();
    for drive in fixed_drive_roots() {
        roots.push(drive.join(".pnpm-store"));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        let path = root.join(&version);
        if path.is_dir() && !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// The store layout version the installed pnpm uses (`v11`), from the last
/// part of `pnpm store path`.
fn pnpm_store_version() -> Option<String> {
    let out = hidden(tool("pnpm")?)
        .args(["store", "path"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() {
        return None;
    }
    let name = Path::new(&text).file_name()?.to_str()?.to_owned();
    is_store_version(&name).then_some(name)
}

/// `v3`, `v10`, `v11`...
fn is_store_version(name: &str) -> bool {
    name.strip_prefix('v')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The roots of every fixed drive (`C:\`, `E:\`...).
fn fixed_drive_roots() -> Vec<PathBuf> {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::core::PCWSTR;
    const DRIVE_FIXED: u32 = 3;
    (b'A'..=b'Z')
        .map(|l| format!("{}:\\", char::from(l)))
        .filter(|root| {
            let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            // SAFETY: `wide` is a live NUL-terminated UTF-16 string for the call.
            unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) == DRIVE_FIXED }
        })
        .map(PathBuf::from)
        .collect()
}

/// Whether `path` is one of [`pnpm_stores`] — the only folders the prune
/// is pointed at.
#[must_use]
pub fn is_pnpm_store(path: &Path) -> bool {
    pnpm_stores()
        .iter()
        .any(|s| s.as_os_str().eq_ignore_ascii_case(path.as_os_str()))
}

// ---------------------------------------------------------------------------
// Docker
// ---------------------------------------------------------------------------

/// Something Docker can reclaim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerKind {
    DanglingImages,
    UnusedImages,
    BuildCache,
    StoppedContainers,
    Volumes,
}

impl DockerKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::DanglingImages => "danglingImages",
            Self::UnusedImages => "unusedImages",
            Self::BuildCache => "buildCache",
            Self::StoppedContainers => "stoppedContainers",
            Self::Volumes => "volumes",
        }
    }

    /// The prune command. `None` for volumes: never run by Vitals.
    #[must_use]
    pub const fn args(self) -> Option<&'static [&'static str]> {
        match self {
            Self::DanglingImages => Some(&["image", "prune", "--force"]),
            Self::UnusedImages => Some(&["image", "prune", "--all", "--force"]),
            Self::BuildCache => Some(&["builder", "prune", "--force"]),
            Self::StoppedContainers => Some(&["container", "prune", "--force"]),
            Self::Volumes => None,
        }
    }
}

/// One row of `docker system df --format json`.
#[derive(Debug, Clone, PartialEq)]
pub struct DockerDf {
    pub kind: String,
    pub total: Option<u64>,
    pub reclaimable: Option<u64>,
}

/// Parses Docker's size strings (`46.89GB (92%)`, `175.7MB`, `0B`).
#[must_use]
pub fn parse_docker_size(text: &str) -> Option<u64> {
    let number = text.split_whitespace().next()?.trim();
    let split = number.find(|c: char| c.is_ascii_alphabetic())?;
    let (value, unit) = number.split_at(split);
    let value: f64 = value.parse().ok()?;
    let scale = match unit.to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" | "KIB" => 1e3,
        "MB" | "MIB" => 1e6,
        "GB" | "GIB" => 1e9,
        "TB" | "TIB" => 1e12,
        _ => return None,
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // A size is non-negative and far below u64::MAX; rounding is the intent.
    Some((value * scale).round() as u64)
}

/// Parses `docker system df --format "{{json .}}"` (one JSON object a line).
#[must_use]
pub fn parse_docker_df(text: &str) -> Vec<DockerDf> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .map(|v| DockerDf {
            kind: v["Type"].as_str().unwrap_or_default().to_owned(),
            total: v["TotalCount"].as_str().and_then(|s| s.parse().ok()),
            reclaimable: v["Reclaimable"].as_str().and_then(parse_docker_size),
        })
        .collect()
}

/// Docker's own "Total reclaimed space: 1.2GB" line.
#[must_use]
pub fn parse_reclaimed(output: &str) -> Option<u64> {
    output
        .lines()
        .find_map(|l| l.trim().strip_prefix("Total reclaimed space:"))
        .and_then(|s| parse_docker_size(s.trim()))
}

/// Docker's state as the CLI sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum DockerState {
    NotInstalled,
    NotRunning,
    Ok(Vec<DockerDf>),
}

/// Asks the Docker CLI what it could reclaim.
#[must_use]
pub fn docker_df() -> DockerState {
    let Some(docker) = tool("docker") else {
        return DockerState::NotInstalled;
    };
    let Ok(out) = hidden(docker)
        .args(["system", "df", "--format", "{{json .}}"])
        .output()
    else {
        return DockerState::NotRunning;
    };
    if !out.status.success() {
        return DockerState::NotRunning;
    }
    DockerState::Ok(parse_docker_df(&String::from_utf8_lossy(&out.stdout)))
}

/// Dangling images (`<none>`) count and size, from `docker images`.
#[must_use]
pub fn docker_dangling() -> Option<(u64, u64)> {
    let out = hidden(tool("docker")?)
        .args([
            "images",
            "--filter",
            "dangling=true",
            "--format",
            "{{.Size}}",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let sizes: Vec<u64> = text.lines().filter_map(parse_docker_size).collect();
    Some((sizes.len() as u64, sizes.iter().sum()))
}

// ---------------------------------------------------------------------------
// Virtual disks
// ---------------------------------------------------------------------------

/// A virtual disk WSL or Docker Desktop grows and never shrinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualDisk {
    pub path: PathBuf,
    pub docker: bool,
    pub distro: Option<String>,
    pub size: u64,
}

/// Finds Docker Desktop's and each WSL distro's `.vhdx`.
#[must_use]
pub fn virtual_disks() -> Vec<VirtualDisk> {
    let mut out = Vec::new();
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return out;
    };
    let docker = local.join("Docker\\wsl");
    for sub in ["disk\\docker_data.vhdx", "data\\ext4.vhdx"] {
        push_disk(&mut out, docker.join(sub), true, None);
    }
    // Registered distros: HKCU\...\Lxss\{guid}\BasePath + DistributionName.
    for (name, base) in wsl_distros() {
        push_disk(&mut out, base.join("ext4.vhdx"), false, Some(name));
    }
    out
}

fn push_disk(out: &mut Vec<VirtualDisk>, path: PathBuf, docker: bool, distro: Option<String>) {
    if let Ok(meta) = std::fs::metadata(&path)
        && meta.is_file()
        && !out.iter().any(|d| d.path == path)
    {
        out.push(VirtualDisk {
            path,
            docker,
            distro,
            size: meta.len(),
        });
    }
}

fn wsl_distros() -> Vec<(String, PathBuf)> {
    // `reg query` keeps this free of another windows-sys feature and is
    // read-only; the output is stable across Windows 10 and 11.
    let Ok(out) = hidden(system32("reg.exe"))
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss",
            "/s",
        ])
        .output()
    else {
        return Vec::new();
    };
    parse_lxss(&String::from_utf8_lossy(&out.stdout))
}

/// Parses `reg query ...\Lxss /s` into (distro name, base path).
#[must_use]
pub fn parse_lxss(text: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let (mut name, mut base) = (None::<String>, None::<PathBuf>);
    let flush = |name: &mut Option<String>,
                 base: &mut Option<PathBuf>,
                 out: &mut Vec<(String, PathBuf)>| {
        if let (Some(n), Some(b)) = (name.take(), base.take()) {
            out.push((n, b));
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("HKEY_") {
            flush(&mut name, &mut base, &mut out);
            name = None;
            base = None;
            continue;
        }
        let mut parts = line.splitn(3, "    ").map(str::trim);
        let (Some(key), Some(_kind), Some(value)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        match key {
            "DistributionName" => name = Some(value.to_owned()),
            "BasePath" => base = Some(PathBuf::from(value.trim_start_matches(r"\\?\"))),
            _ => {}
        }
    }
    flush(&mut name, &mut base, &mut out);
    out
}

fn system32(name: &str) -> PathBuf {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join("System32").join(name)
}

// ---------------------------------------------------------------------------
// Removal
// ---------------------------------------------------------------------------

/// Why an item was not acted on at execution time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recheck {
    Ok,
    /// Something changed since the scan: it no longer qualifies.
    Changed(&'static str),
}

/// Re-checks an artefact just before removal: the folder still exists, is
/// not a link, still has its name, its project still produces it, and it
/// is not under a protected path.
#[must_use]
pub fn recheck_artefact(path: &Path, kind: ArtefactKind, rules: &Rules) -> Recheck {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Recheck::Changed("gone");
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Recheck::Changed("not a folder");
    }
    let (Some(name), Some(parent)) = (path.file_name(), path.parent()) else {
        return Recheck::Changed("no parent");
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Recheck::Changed("parent unreadable");
    };
    let names: BTreeSet<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let has = |n: &str| names.contains(n);
    if classify(&name.to_string_lossy(), &has) != Some(kind) {
        return Recheck::Changed("no longer a build folder");
    }
    // The artefact itself sits inside a project, never at a protected spot
    // (a drive root's `node_modules`, `C:\Windows\...\build`).
    if let Some(parent) = path.parent().and_then(|p| p.to_str())
        && let Some(why) = rules.check(parent)
        && !matches!(why, super::Protection::UserFolder)
    {
        return Recheck::Changed("protected location");
    }
    Recheck::Ok
}

/// What happened to one folder removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// Measured before removal; `None` when unreadable.
    pub before: Option<u64>,
    /// What is left (0 when the folder is gone).
    pub after: Option<u64>,
    /// First error, if any file could not be removed.
    pub error: Option<String>,
}

/// Removes a folder permanently, never following links: a junction inside
/// is removed as a link, its target untouched.
///
/// `std::fs::remove_dir_all` on Windows removes a directory symlink or
/// junction without traversing it (Rust 1.58+ fixed CVE-2022-21658), which
/// is exactly the behaviour a pnpm `node_modules` full of junctions needs.
#[must_use]
pub fn remove_folder(path: &Path, cancel: &AtomicBool) -> Removal {
    let before = measure(path, cancel).map(|(b, _)| b);
    let error = clear_readonly(path)
        .and_then(|()| std::fs::remove_dir_all(path))
        .err()
        .map(|e| e.to_string());
    let after = if path.exists() {
        measure(path, cancel).map(|(b, _)| b)
    } else {
        Some(0)
    };
    Removal {
        before,
        after,
        error,
    }
}

/// Git marks its pack files read-only, and Windows refuses to delete a
/// read-only file. Clears the attribute below `path`, not following links.
fn clear_readonly(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    let mut perms = meta.permissions();
    if perms.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        // Windows-only: clears FILE_ATTRIBUTE_READONLY; there is no Unix
        // world-writable consequence here.
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(path, perms);
    }
    if meta.is_dir() {
        for entry in std::fs::read_dir(path)?.flatten() {
            let _ = clear_readonly(&entry.path());
        }
    }
    Ok(())
}

/// Runs a tool and returns (success, combined output).
#[must_use]
pub fn run_tool(program: &Path, args: &[&str], cwd: Option<&Path>) -> (bool, String) {
    let mut command = hidden(program.to_path_buf());
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    match command.output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), text)
        }
        Err(err) => (false, err.to_string()),
    }
}

/// `git worktree remove <path>` from the main repository — without
/// `--force`, so git itself refuses a worktree that became dirty since the
/// scan.
#[must_use]
pub fn remove_worktree(repo: &Path, path: &Path) -> (bool, String) {
    let Some(git) = tool("git") else {
        return (false, "git is not installed".into());
    };
    let path = path.to_string_lossy();
    run_tool(
        &git,
        &["-C", &repo.to_string_lossy(), "worktree", "remove", &path],
        None,
    )
}

/// `git worktree prune` for a repository whose worktree folder is gone.
#[must_use]
pub fn prune_worktrees(repo: &Path) -> (bool, String) {
    let Some(git) = tool("git") else {
        return (false, "git is not installed".into());
    };
    run_tool(
        &git,
        &["-C", &repo.to_string_lossy(), "worktree", "prune"],
        None,
    )
}

/// Free bytes on the volume holding `path`.
#[must_use]
pub fn drive_free(path: &Path) -> Option<(String, u64)> {
    let root = match path.components().next()? {
        std::path::Component::Prefix(p) => format!("{}\\", p.as_os_str().to_string_lossy()),
        _ => return None,
    };
    let free = super::managed::free_bytes(Path::new(&root))?;
    Some((root, free))
}

// ---------------------------------------------------------------------------
// Virtual disk compaction (elevated child)
// ---------------------------------------------------------------------------

/// The argument the elevated compaction instance is started with.
pub const COMPACT_VHD_ARG: &str = "--elevated-compact-vhd";

/// Exit codes of the compaction child.
mod exit {
    pub const OK: u32 = 0;
    /// The script's own code when the WSL VM never stopped; no disk was touched.
    pub const SCRIPT_WSL_RUNNING: i32 = 32;
    pub const BAD_ARGS: u32 = 0xE5C1_0001;
    pub const NOT_A_DISK: u32 = 0xE5C1_0002;
    pub const WSL_STILL_RUNNING: u32 = 0xE5C1_0003;
    pub const COMPACT_FAILED: u32 = 0xE5C1_0004;
}

/// How many disks one elevated pass accepts; there are rarely more than four.
const COMPACT_MAX: usize = 8;

/// Whether `path` is one of the disks [`virtual_disks`] finds — the only
/// files the elevated child will compact.
#[must_use]
pub fn is_known_disk(path: &Path) -> bool {
    virtual_disks()
        .iter()
        .any(|d| d.path.as_os_str().eq_ignore_ascii_case(path.as_os_str()))
}

/// The compaction child's whole job: stop WSL so that nothing can restart
/// it, compact every disk, put back what it paused, exit with a code.
/// Returns the code so it is testable.
#[must_use]
pub fn perform_compact<I, S>(rest: I) -> u32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let paths: Vec<PathBuf> = rest
        .into_iter()
        .map(|s| PathBuf::from(s.as_ref()))
        .collect();
    if paths.is_empty() || paths.len() > COMPACT_MAX {
        return exit::BAD_ARGS;
    }
    // Checked here too: the elevated pass must not compact (or attach) an
    // arbitrary file it was handed.
    if paths.iter().any(|p| {
        p.extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("vhdx"))
            || !is_known_disk(p)
    }) {
        return exit::NOT_A_DISK;
    }
    let status = hidden(system32("WindowsPowerShell\\v1.0\\powershell.exe"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &compact_script(&paths),
        ])
        .status();
    match status.map(|s| s.code()) {
        Ok(Some(0)) => exit::OK,
        Ok(Some(exit::SCRIPT_WSL_RUNNING)) => exit::WSL_STILL_RUNNING,
        Ok(Some(n @ 1..=8)) => n.cast_unsigned(),
        _ => exit::COMPACT_FAILED,
    }
}

/// The elevated script. Why each step is there:
///
/// - `wsl --shutdown` alone is undone within seconds by anything holding a
///   WSL client open. On the machine this was built on a scheduled task
///   starts `wsl --exec sleep infinity` every 15 s, and Docker Desktop
///   restarts its own distro; either one makes `Optimize-VHD` fail with
///   0x80070020 "in use". So every non-Microsoft scheduled task whose
///   action mentions WSL is disabled and stopped first, and Docker Desktop
///   is closed.
/// - The VM (`vmmemWSL`) lets go of the disks a few seconds after the last
///   distro stops, so the script waits for that process rather than for
///   the `wsl` command to return. Its absence is the only check that does
///   not depend on `wsl`'s localised output.
/// - If WSL will not stop, no disk is touched (exit 32).
/// - Paused tasks are put back in a `finally`, so a failed compaction
///   never leaves them disabled.
fn compact_script(paths: &[PathBuf]) -> String {
    let list = paths
        .iter()
        .map(|p| format!("'{}'", p.to_string_lossy().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"$ErrorActionPreference = 'Continue'
$disks = @({list})
$paused = @()
$fail = 0
try {{
    foreach ($t in @(Get-ScheduledTask -ErrorAction SilentlyContinue)) {{
        if ($t.State -eq 'Disabled' -or $t.TaskPath -like '\Microsoft\*') {{ continue }}
        $a = ($t.Actions | ForEach-Object {{ "$($_.Execute) $($_.Arguments)" }}) -join ' '
        if ($a -notmatch '(?i)wsl') {{ continue }}
        $wasRunning = $t.State -eq 'Running'
        try {{
            Disable-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -ErrorAction Stop | Out-Null
            $paused += [pscustomobject]@{{ Name = $t.TaskName; Path = $t.TaskPath; Running = $wasRunning }}
            if ($wasRunning) {{ Stop-ScheduledTask -TaskName $t.TaskName -TaskPath $t.TaskPath -ErrorAction SilentlyContinue }}
        }} catch {{ }}
    }}
    Get-Process 'Docker Desktop', 'com.docker.backend', 'com.docker.build', 'com.docker.proxy' -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
    $down = $false
    for ($i = 0; $i -lt 8 -and -not $down; $i++) {{
        & wsl.exe --shutdown 2>$null | Out-Null
        for ($w = 0; $w -lt 10 -and (Get-Process vmmemWSL -ErrorAction SilentlyContinue); $w++) {{ Start-Sleep -Seconds 2 }}
        $down = -not (Get-Process vmmemWSL -ErrorAction SilentlyContinue)
        if (-not $down) {{
            Get-Process wsl, wslhost -ErrorAction SilentlyContinue | Where-Object SessionId -ne 0 |
                Stop-Process -Force -ErrorAction SilentlyContinue
        }}
    }}
    if (-not $down) {{ exit 32 }}
    Start-Sleep -Seconds 3
    $hyperv = [bool](Get-Command Optimize-VHD -ErrorAction SilentlyContinue)
    foreach ($p in $disks) {{
        $ok = $false
        if ($hyperv) {{
            try {{ Optimize-VHD -Path $p -Mode Full -ErrorAction Stop; $ok = $true }} catch {{ }}
        }}
        if (-not $ok) {{
            $f = Join-Path $env:TEMP 'vitals-compact.txt'
            Set-Content -Path $f -Encoding ascii -Value @("select vdisk file=`"$p`"", 'attach vdisk readonly', 'compact vdisk', 'detach vdisk')
            diskpart /s $f | Out-Null
            $ok = $LASTEXITCODE -eq 0
            Remove-Item $f -ErrorAction SilentlyContinue
        }}
        if (-not $ok) {{ $fail++ }}
    }}
}} finally {{
    foreach ($t in $paused) {{
        Enable-ScheduledTask -TaskName $t.Name -TaskPath $t.Path -ErrorAction SilentlyContinue | Out-Null
        if ($t.Running) {{ Start-ScheduledTask -TaskName $t.Name -TaskPath $t.Path -ErrorAction SilentlyContinue }}
    }}
}}
exit $fail
"#
    )
}

/// Maps the compaction child's exit code to a result for the report.
///
/// # Errors
///
/// A message naming what went wrong.
pub fn compact_result(code: u32) -> Result<(), String> {
    match code {
        exit::OK => Ok(()),
        exit::NOT_A_DISK => Err("not one of the WSL or Docker disks Vitals found".into()),
        exit::WSL_STILL_RUNNING => {
            Err("WSL was still using the disk; close Docker Desktop and try again".into())
        }
        exit::BAD_ARGS => Err("the compaction did not understand its arguments".into()),
        // The script exits with the number of disks it could not compact.
        n @ 1..=8 => Err(format!(
            "{n} of the disks could not be compacted; the others were"
        )),
        other => Err(format!(
            "Windows could not compact the disk (code {other:#x})"
        )),
    }
}

/// Compacts `disks` in one elevated pass (one UAC prompt) and waits.
///
/// Around it, unelevated: trims each running distro so the blocks the
/// prunes freed are actually released to the `.vhdx` (compaction only
/// returns zeroed or trimmed blocks), and relaunches Docker Desktop
/// afterwards if it was open — from this process, so it comes back as the
/// user and not as administrator (an elevated Docker Desktop shows no
/// window and cannot be closed from a normal session).
///
/// # Errors
///
/// A declined UAC prompt is `Err` with a message saying nothing changed.
pub fn compact_disks(disks: &[PathBuf]) -> Result<(), String> {
    let known = virtual_disks();
    let docker_exe = docker_desktop_exe();
    let wsl = system32("wsl.exe");
    for disk in &known {
        if !disks
            .iter()
            .any(|d| d.as_os_str().eq_ignore_ascii_case(disk.path.as_os_str()))
        {
            continue;
        }
        let distro = match (&disk.distro, disk.docker) {
            (Some(name), _) => name.clone(),
            (None, true) if docker_exe.is_some() => "docker-desktop".to_owned(),
            _ => continue,
        };
        // Best effort: a distro without fstrim still compacts, just less.
        let _ = run_tool(
            &wsl,
            &["-d", &distro, "-u", "root", "--", "fstrim", "-a"],
            None,
        );
    }
    // Containers without a restart policy do not come back when WSL does
    // (a hand-started `docker run` on 2026-09-29 stayed exited), so the
    // running set is remembered and started again afterwards.
    let docker = tool("docker");
    let running = docker
        .as_deref()
        .map(running_containers)
        .unwrap_or_default();
    let args = disks
        .iter()
        .fold(String::from(COMPACT_VHD_ARG), |mut a, d| {
            use std::fmt::Write as _;
            let _ = write!(a, " \"{}\"", d.to_string_lossy());
            a
        });
    let result = match crate::actions::taskmgr::run_elevated(&args, "no disk was compacted") {
        Ok(code) => compact_result(code),
        Err(err) => Err(err.to_string()),
    };
    if let Some(exe) = docker_exe {
        let _ = std::process::Command::new(exe).spawn();
    }
    if let Some(docker) = docker.filter(|_| !running.is_empty()) {
        restart_containers(&docker, &running);
    }
    result
}

/// IDs of the running containers, or none when Docker is not answering.
fn running_containers(docker: &Path) -> Vec<String> {
    let (ok, text) = run_tool(docker, &["ps", "--quiet", "--no-trunc"], None);
    if !ok {
        return Vec::new();
    }
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned)
        .collect()
}

/// Waits up to three minutes for Docker to answer again, then starts every
/// container in `ids`. Starting one its restart policy already brought back
/// is a no-op.
fn restart_containers(docker: &Path, ids: &[String]) {
    for _ in 0..60 {
        if run_tool(docker, &["info", "--format", "{{.ServerVersion}}"], None).0 {
            let mut args = vec!["start"];
            args.extend(ids.iter().map(String::as_str));
            let _ = run_tool(docker, &args, None);
            return;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// The running Docker Desktop's executable, if it is open.
fn docker_desktop_exe() -> Option<PathBuf> {
    let (ok, text) = run_tool(
        &system32("WindowsPowerShell\\v1.0\\powershell.exe"),
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-Process 'Docker Desktop' -ErrorAction SilentlyContinue | Select-Object -First 1).Path",
        ],
        None,
    );
    let path = PathBuf::from(text.trim());
    (ok && path.is_absolute() && path.is_file()).then_some(path)
}

/// True if `err` came from a declined UAC prompt.
#[must_use]
pub fn is_declined(message: &str) -> bool {
    message.contains("declined")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has<'a>(names: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
        move |n| names.contains(&n)
    }

    #[test]
    fn a_build_folder_counts_only_beside_the_project_that_produces_it() {
        assert_eq!(
            classify("node_modules", &has(&["package.json"])),
            Some(ArtefactKind::NodeModules)
        );
        assert_eq!(
            classify("target", &has(&["Cargo.toml"])),
            Some(ArtefactKind::CargoTarget)
        );
        assert_eq!(
            classify("build", &has(&["build.gradle.kts"])),
            Some(ArtefactKind::Build)
        );
        // The same names with no manifest beside them are somebody's folders.
        for name in [
            "node_modules",
            "target",
            "build",
            "dist",
            ".next",
            "coverage",
            "venv",
        ] {
            assert_eq!(classify(name, &has(&["notes.txt"])), None, "{name}");
        }
        assert_eq!(classify("Documents", &has(&["package.json"])), None);
    }

    #[test]
    fn a_folder_named_venv_is_only_a_venv_in_a_python_project() {
        assert_eq!(
            classify(".venv", &has(&["pyproject.toml"])),
            Some(ArtefactKind::Venv)
        );
        assert_eq!(classify("venv", &has(&["package.json"])), None);
    }

    #[test]
    fn the_walk_finds_artefacts_does_not_enter_them_and_never_follows_links() {
        let root = std::env::temp_dir().join(format!("vitals-devclean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = root.join("app");
        std::fs::create_dir_all(app.join("node_modules\\left-pad\\node_modules")).expect("mk");
        std::fs::write(app.join("package.json"), b"{}").expect("pkg");
        std::fs::write(app.join("node_modules\\left-pad\\package.json"), b"{}").expect("pkg");
        let docs = root.join("Documents\\build");
        std::fs::create_dir_all(&docs).expect("docs");
        // A junction to the app from elsewhere must not produce a duplicate.
        let link = root.join("linked");
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&app)
            .output()
            .is_ok_and(|o| o.status.success());

        let mut found = Vec::new();
        find_artefacts(&root, 8, &AtomicBool::new(false), &mut found);
        assert_eq!(
            found.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            vec![app.join("node_modules")],
            "one node_modules, not the nested one, not Documents\\build, not via the junction (junction made: {made})"
        );
        let _ = std::process::Command::new("cmd")
            .args(["/c", "rmdir"])
            .arg(&link)
            .output();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_a_node_modules_with_a_junction_inside_leaves_the_target_untouched() {
        let root = std::env::temp_dir().join(format!("vitals-devclean-rm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let keep = root.join("store\\pkg");
        std::fs::create_dir_all(&keep).expect("store");
        std::fs::write(keep.join("index.js"), b"keep me").expect("store file");
        let app = root.join("app");
        let nm = app.join("node_modules");
        std::fs::create_dir_all(&nm).expect("nm");
        std::fs::write(app.join("package.json"), b"{}").expect("pkg");
        std::fs::write(nm.join("ro.txt"), b"x").expect("ro");
        let mut perms = std::fs::metadata(nm.join("ro.txt"))
            .expect("m")
            .permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(nm.join("ro.txt"), perms).expect("ro");
        let linked = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(nm.join("pkg"))
            .arg(&keep)
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(linked, "junction");

        assert_eq!(
            recheck_artefact(&nm, ArtefactKind::NodeModules, &Rules::for_this_machine()),
            Recheck::Ok
        );
        let removal = remove_folder(&nm, &AtomicBool::new(false));
        assert_eq!(removal.error, None);
        assert_eq!(removal.after, Some(0));
        assert!(!nm.exists());
        assert_eq!(
            std::fs::read(keep.join("index.js")).expect("still there"),
            b"keep me",
            "the junction's target must survive"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_artefact_that_changed_since_the_scan_is_not_removed() {
        let root = std::env::temp_dir().join(format!("vitals-devclean-rc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let nm = root.join("app\\node_modules");
        std::fs::create_dir_all(&nm).expect("nm");
        // package.json was deleted after the scan: no longer provably build output.
        assert_eq!(
            recheck_artefact(&nm, ArtefactKind::NodeModules, &Rules::for_this_machine()),
            Recheck::Changed("no longer a build folder")
        );
        assert!(matches!(
            recheck_artefact(
                &root.join("gone"),
                ArtefactKind::NodeModules,
                &Rules::for_this_machine()
            ),
            Recheck::Changed(_)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_worktree_is_offered_only_when_clean_pushed_and_idle() {
        use WorktreeState as S;
        assert_eq!(worktree_state(true, false, 0, true, Some(13)), S::Removable);
        assert_eq!(worktree_state(false, false, 0, true, None), S::Prunable);
        assert_eq!(worktree_state(true, true, 0, true, Some(99)), S::Locked);
        assert_eq!(worktree_state(true, false, 3, true, Some(99)), S::Dirty);
        assert_eq!(worktree_state(true, false, 0, false, Some(99)), S::Unpushed);
        assert_eq!(worktree_state(true, false, 0, true, Some(2)), S::Active);
        assert_eq!(
            worktree_state(true, false, 0, true, None),
            S::Active,
            "a worktree we cannot date is treated as in use"
        );
        for s in [S::Dirty, S::Unpushed, S::Active, S::Locked] {
            assert!(!s.offered(), "{s:?}");
        }
    }

    #[test]
    fn the_porcelain_worktree_list_skips_the_main_tree_and_reads_flags() {
        let text = "worktree E:/gh/codai\nHEAD aaa\nbranch refs/heads/main\n\n\
                    worktree E:/gh/.wt/codai/x\nHEAD bbb\nbranch refs/heads/feat/x\nlocked reason\n\n\
                    worktree E:/gh/.wt/codai/gone\nHEAD ccc\ndetached\nprunable gitdir file points to non-existent location\n";
        let list = parse_worktrees(text);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].path, PathBuf::from(r"E:\gh\.wt\codai\x"));
        assert_eq!(list[0].branch.as_deref(), Some("feat/x"));
        assert!(list[0].locked);
        assert!(list[1].prunable && list[1].branch.is_none());
    }

    #[test]
    fn docker_sizes_and_reports_parse_as_docker_prints_them() {
        assert_eq!(parse_docker_size("46.89GB (92%)"), Some(46_890_000_000));
        assert_eq!(parse_docker_size("175.7MB"), Some(175_700_000));
        assert_eq!(parse_docker_size("0B"), Some(0));
        assert_eq!(parse_docker_size("n/a"), None);
        let df = parse_docker_df(
            "{\"Type\":\"Images\",\"TotalCount\":\"86\",\"Reclaimable\":\"46.89GB (92%)\"}\n\
             {\"Type\":\"Build Cache\",\"TotalCount\":\"322\",\"Reclaimable\":\"16.03GB\"}",
        );
        assert_eq!(df.len(), 2);
        assert_eq!(df[1].reclaimable, Some(16_030_000_000));
        assert_eq!(
            parse_reclaimed("Deleted: x\nTotal reclaimed space: 1.5GB\n"),
            Some(1_500_000_000)
        );
    }

    #[test]
    fn docker_volumes_have_no_command_and_are_never_offered() {
        assert_eq!(DockerKind::Volumes.args(), None);
        for kind in [
            DockerKind::DanglingImages,
            DockerKind::UnusedImages,
            DockerKind::BuildCache,
            DockerKind::StoppedContainers,
        ] {
            let args = kind.args().expect("a prune command");
            assert!(!args.iter().any(|a| a.contains("volume")), "{kind:?}");
        }
    }

    #[test]
    fn pnpm_is_pruned_by_pnpm_never_deleted() {
        assert_eq!(
            CacheKind::Pnpm.method(),
            CacheMethod::Command {
                program: "pnpm",
                args: &["store", "prune"]
            }
        );
    }

    #[test]
    fn the_lxss_registry_dump_yields_each_distro_and_its_disk_folder() {
        let text = "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss\n    DefaultDistribution    REG_SZ    {a}\n\n\
                    HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss\\{a}\n    BasePath    REG_SZ    \\\\?\\C:\\Users\\x\\Ubuntu\n    DistributionName    REG_SZ    Ubuntu-24.04\n";
        assert_eq!(
            parse_lxss(text),
            vec![(
                "Ubuntu-24.04".to_owned(),
                PathBuf::from(r"C:\Users\x\Ubuntu")
            )]
        );
    }

    #[test]
    fn the_compaction_child_refuses_anything_but_a_disk_it_found_itself() {
        assert_eq!(perform_compact(Vec::<String>::new()), exit::BAD_ARGS);
        assert_eq!(perform_compact(["a.vhdx"; COMPACT_MAX + 1]), exit::BAD_ARGS);
        // One unknown disk among several refuses the whole pass.
        assert_eq!(perform_compact(["a.vhdx", "b.vhdx"]), exit::NOT_A_DISK);
        assert_eq!(
            perform_compact([r"C:\Windows\System32\config\SAM"]),
            exit::NOT_A_DISK
        );
        assert_eq!(perform_compact([r"C:\evil\made-up.vhdx"]), exit::NOT_A_DISK);
    }

    #[test]
    fn only_pnpm_store_version_folders_count_as_stores() {
        for ok in ["v3", "v10", "v11"] {
            assert!(is_store_version(ok), "{ok}");
        }
        for no in ["v", "vx", "files", "v11a", "11", ""] {
            assert!(!is_store_version(no), "{no}");
        }
    }

    #[test]
    fn compaction_script_pauses_what_restarts_wsl_and_always_puts_it_back() {
        let s = compact_script(&[PathBuf::from(r"C:\it's\ext4.vhdx")]);
        // The path is quoted for PowerShell, not spliced raw.
        assert!(s.contains(r"'C:\it''s\ext4.vhdx'"));
        let pause = s.find("Disable-ScheduledTask").expect("pauses tasks");
        let shutdown = s.find("wsl.exe --shutdown").expect("stops wsl");
        let compact = s.find("Optimize-VHD -Path").expect("compacts");
        let finally = s.find("} finally {").expect("restores in finally");
        let restore = s.find("Enable-ScheduledTask").expect("re-enables");
        assert!(pause < shutdown && shutdown < compact && compact < finally && finally < restore);
        // No disk is touched unless the VM has actually gone.
        let refuse = s.find("exit 32").expect("refuses while running");
        assert!(s.contains("vmmemWSL") && refuse < compact);
        assert!(s.contains("Stop-Process") && s.contains("Docker Desktop"));
    }

    #[test]
    fn a_declined_compaction_says_nothing_changed() {
        assert!(is_declined(
            "refused: administrator approval was declined, so the disk was not compacted"
        ));
        assert!(compact_result(exit::NOT_A_DISK).is_err());
        assert!(compact_result(exit::OK).is_ok());
    }

    #[test]
    fn tools_resolve_to_absolute_paths() {
        if let Some(git) = tool("git") {
            assert!(git.is_absolute());
        }
        assert_eq!(tool("vitals-no-such-tool-7f3a"), None);
    }
}
