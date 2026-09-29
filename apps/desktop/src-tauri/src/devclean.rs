//! Tauri commands for the developer cleanup (`vitals_win::storage::devclean`).
//!
//! The scan is kept here, behind a `scan_id`, and every actionable item gets
//! an id minted from it. `run_dev_cleanup` accepts ids only: the webview can
//! choose among what the scan found, never name a folder of its own. Each
//! item is re-checked at execution time, because minutes pass between the
//! scan and the click.
//!
//! Desktop-only, like recycling: a paired phone must not delete files.

#![cfg(windows)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use serde::Serialize;
use tauri::Emitter as _;
use vitals_win::storage::devclean::{
    self as dc, ArtefactKind, CacheKind, CacheMethod, DockerKind, DockerState, Recheck,
    WorktreeState,
};

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

const SCAN_PROGRESS: &str = "vitals://storage/dev-scan-progress";
const CLEAN_PROGRESS: &str = "vitals://storage/dev-clean-progress";

/// How deep below a root the project search goes. `E:\gh\.wt\<repo>\<name>`
/// is four levels; monorepo packages add three more.
const MAX_DEPTH: usize = 8;

static CANCEL: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
static NEXT_SCAN: AtomicU64 = AtomicU64::new(1);

/// What an id resolves to.
#[derive(Debug, Clone)]
enum Item {
    Artefact { path: PathBuf, kind: ArtefactKind },
    Worktree { repo: PathBuf, path: PathBuf },
    Prune { repo: PathBuf, path: PathBuf },
    Cache { kind: CacheKind, path: PathBuf },
    Docker(DockerKind),
    Disk(PathBuf),
}

#[derive(Debug)]
struct Kept {
    id: u64,
    items: HashMap<String, Item>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

/// Claims the one-at-a-time slot for a scan or a run.
struct Busy;

impl Busy {
    fn claim(what: &str) -> CommandResult<Self> {
        if BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(CommandError::Refused {
                message: format!("{what}: another developer cleanup is already running"),
            });
        }
        CANCEL.store(false, Ordering::Release);
        Ok(Self)
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// DTOs — shapes match `devclean/model.ts`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtefactDto {
    id: String,
    path: String,
    kind: &'static str,
    size: Option<u64>,
    files: Option<u64>,
    shared_with_pnpm_store: bool,
    restore: &'static str,
    preselected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    path: String,
    name: String,
    is_git: bool,
    idle_days: Option<u64>,
    artefacts: Vec<ArtefactDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeDto {
    id: Option<String>,
    path: String,
    repo: String,
    branch: Option<String>,
    size: Option<u64>,
    state: &'static str,
    changes: Option<u64>,
    idle_hours: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheDto {
    id: Option<String>,
    kind: &'static str,
    path: String,
    size: Option<u64>,
    method: &'static str,
    command: String,
    restore: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerItemDto {
    id: Option<String>,
    kind: &'static str,
    count: Option<u64>,
    reclaimable: Option<u64>,
    command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerDto {
    state: &'static str,
    items: Vec<DockerItemDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskDto {
    id: String,
    path: String,
    kind: &'static str,
    distro: Option<String>,
    size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevScanDto {
    scan_id: u64,
    roots: Vec<String>,
    projects: Vec<ProjectDto>,
    worktrees: Vec<WorktreeDto>,
    caches: Vec<CacheDto>,
    docker: DockerDto,
    vdisks: Vec<DiskDto>,
    elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgressDto {
    phase: &'static str,
    found: u64,
    current_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanProgressDto {
    id: String,
    index: u64,
    total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderDto {
    pid: u32,
    name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCleanItemDto {
    id: String,
    path: String,
    /// `done` | `partial` | `failed` | `refused` | `unavailable`.
    outcome: &'static str,
    freed: Option<u64>,
    tool_freed: Option<u64>,
    message: Option<String>,
    holders: Option<Vec<HolderDto>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveDto {
    drive: String,
    freed: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCleanReportDto {
    items: Vec<DevCleanItemDto>,
    drives: Vec<DriveDto>,
}

// ---------------------------------------------------------------------------
// Roots
// ---------------------------------------------------------------------------

/// Folder names that hold projects on a developer's machine.
const PROJECT_DIRS: &[&str] = &[
    "gh",
    "GitHub",
    "git",
    "src",
    "code",
    "dev",
    "projects",
    "Projects",
    "repos",
    "source\\repos",
    "work",
];

/// The usual project roots on every fixed drive and in the profile, that exist.
#[must_use]
pub fn default_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_dir()
            && !out
                .iter()
                .any(|o: &PathBuf| o.as_os_str().eq_ignore_ascii_case(p.as_os_str()))
        {
            out.push(p);
        }
    };
    for volume in vitals_win::disk::enumerate_volumes() {
        if !matches!(
            volume.kind,
            vitals_core::metrics::DiskKind::Hdd
                | vitals_core::metrics::DiskKind::Ssd
                | vitals_core::metrics::DiskKind::Nvme
                | vitals_core::metrics::DiskKind::Unknown
        ) {
            continue;
        }
        for dir in PROJECT_DIRS {
            push(PathBuf::from(format!("{}\\{dir}", volume.mount)));
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for dir in PROJECT_DIRS {
            push(home.join(dir));
        }
    }
    out
}

#[tauri::command]
pub fn default_dev_roots() -> Vec<String> {
    default_roots()
        .iter()
        .map(|p| p.display().to_string())
        .collect()
}

#[tauri::command]
pub fn cancel_dev_cleanup_scan() {
    CANCEL.store(true, Ordering::Release);
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

/// Validates caller-supplied roots: absolute, existing folders, not a drive
/// root of the system drive's Windows folder or other protected locations.
fn vet_roots(roots: Option<Vec<String>>) -> CommandResult<Vec<PathBuf>> {
    let Some(roots) = roots else {
        return Ok(default_roots());
    };
    if roots.len() > 32 {
        return Err(CommandError::Refused {
            message: "at most 32 folders can be scanned at once".into(),
        });
    }
    let mut out = Vec::new();
    for raw in roots {
        let path = PathBuf::from(raw.trim());
        if !path.is_absolute() || !path.is_dir() {
            return Err(CommandError::NotFound {
                message: format!("{} is not a folder", path.display()),
            });
        }
        out.push(path);
    }
    Ok(out)
}

#[tauri::command]
pub async fn scan_dev_cleanup(
    app: tauri::AppHandle,
    roots: Option<Vec<String>>,
) -> CommandResult<DevScanDto> {
    let roots = vet_roots(roots)?;
    let busy = Busy::claim("scan")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = busy;
        scan(&app, &roots)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the scan thread did not finish: {err}"),
    })
}

fn emit(app: &tauri::AppHandle, phase: &'static str, found: u64, path: Option<&Path>) {
    let _ = app.emit(
        SCAN_PROGRESS,
        ScanProgressDto {
            phase,
            found,
            current_path: path.map(|p| p.display().to_string()),
        },
    );
}

/// Mints ids for one scan and remembers what each resolves to.
struct Minter {
    scan_id: u64,
    items: HashMap<String, Item>,
}

impl Minter {
    fn mint(&mut self, item: Item) -> String {
        let id = format!("{}-{}", self.scan_id, self.items.len());
        self.items.insert(id.clone(), item);
        id
    }
}

fn scan(app: &tauri::AppHandle, roots: &[PathBuf]) -> DevScanDto {
    let started = Instant::now();
    let mut minter = Minter {
        scan_id: NEXT_SCAN.fetch_add(1, Ordering::Relaxed),
        items: HashMap::new(),
    };
    let projects = scan_projects(app, roots, &mut minter);
    let worktrees = scan_worktrees(app, roots, &projects, &mut minter);
    let caches = scan_caches(app, &mut minter);
    let docker = scan_docker(app, &mut minter);
    emit(app, "vdisks", 0, None);
    let vdisks = dc::virtual_disks()
        .into_iter()
        .map(|d| DiskDto {
            id: minter.mint(Item::Disk(d.path.clone())),
            path: d.path.display().to_string(),
            kind: if d.docker { "docker" } else { "wsl" },
            distro: d.distro,
            size: d.size,
        })
        .collect();

    let scan_id = minter.scan_id;
    if let Ok(mut kept) = KEPT.lock() {
        *kept = Some(Kept {
            id: scan_id,
            items: minter.items,
        });
    }
    DevScanDto {
        scan_id,
        roots: roots.iter().map(|p| p.display().to_string()).collect(),
        projects,
        worktrees,
        caches,
        docker,
        vdisks,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }
}

/// Finds artefacts under every root, groups them by project (the git
/// repository, else the artefact's owner), sizes each and dates each project.
fn scan_projects(
    app: &tauri::AppHandle,
    roots: &[PathBuf],
    minter: &mut Minter,
) -> Vec<ProjectDto> {
    let mut found = Vec::new();
    for root in roots {
        emit(app, "projects", found.len() as u64, Some(root));
        dc::find_artefacts(root, MAX_DEPTH, &CANCEL, &mut found);
    }
    // One artefact per path even when roots overlap.
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup_by(|a, b| a.path == b.path);

    let mut groups: BTreeMap<PathBuf, (bool, Vec<dc::Found>)> = BTreeMap::new();
    for f in found {
        let stop = roots
            .iter()
            .find(|r| f.path.starts_with(r))
            .cloned()
            .unwrap_or_else(|| f.owner.clone());
        let (project, is_git) =
            dc::git_root(&f.owner, &stop).map_or((f.owner.clone(), false), |r| (r, true));
        groups
            .entry(project)
            .or_insert_with(|| (is_git, Vec::new()))
            .1
            .push(f);
    }

    let total = groups.values().map(|g| g.1.len()).sum::<usize>() as u64;
    let mut done = 0_u64;
    let mut projects = Vec::new();
    for (project, (is_git, list)) in groups {
        let paths: Vec<PathBuf> = list.iter().map(|f| f.path.clone()).collect();
        let git_time = if is_git {
            dc::git_last_activity(&project)
        } else {
            None
        };
        let idle = dc::idle_days(&project, &paths, git_time);
        let stale = idle.is_some_and(|d| d >= dc::STALE_DAYS);
        let mut artefacts = Vec::new();
        for f in list {
            if CANCEL.load(Ordering::Relaxed) {
                break;
            }
            done += 1;
            emit(app, "sizing", done.min(total), Some(&f.path));
            artefacts.push(artefact_dto(&f, stale, minter));
        }
        let name = project.file_name().map_or_else(
            || project.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        projects.push(ProjectDto {
            path: project.display().to_string(),
            name,
            is_git,
            idle_days: idle,
            artefacts,
        });
    }
    projects
}

fn artefact_dto(f: &dc::Found, stale: bool, minter: &mut Minter) -> ArtefactDto {
    let measured = dc::measure(&f.path, &CANCEL);
    let pnpm = f.kind == ArtefactKind::NodeModules && dc::is_pnpm_layout(&f.path);
    ArtefactDto {
        id: minter.mint(Item::Artefact {
            path: f.path.clone(),
            kind: f.kind,
        }),
        path: f.path.display().to_string(),
        kind: f.kind.key(),
        size: measured.map(|(b, _)| b),
        files: measured.map(|(_, n)| n),
        shared_with_pnpm_store: pnpm,
        restore: f.kind.restore(),
        preselected: stale,
    }
}

/// Worktrees of every git repository directly under a root, and of every
/// project repository found.
fn scan_worktrees(
    app: &tauri::AppHandle,
    roots: &[PathBuf],
    projects: &[ProjectDto],
    minter: &mut Minter,
) -> Vec<WorktreeDto> {
    emit(app, "worktrees", 0, None);
    let mut repos: Vec<PathBuf> = projects
        .iter()
        .filter(|p| p.is_git)
        .map(|p| PathBuf::from(&p.path))
        .collect();
    for root in roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            repos.extend(
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.join(".git").is_dir()),
            );
        }
    }
    repos.sort();
    repos.dedup();
    let mut worktrees = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for repo in repos {
        // Only main working trees list their worktrees; a linked one has
        // a `.git` *file*.
        if !repo.join(".git").is_dir() || CANCEL.load(Ordering::Relaxed) {
            continue;
        }
        for wt in dc::worktrees_of(&repo) {
            if !seen.insert(wt.entry.path.clone()) {
                continue;
            }
            emit(
                app,
                "worktrees",
                worktrees.len() as u64,
                Some(&wt.entry.path),
            );
            let size = if wt.state == WorktreeState::Removable {
                dc::measure(&wt.entry.path, &CANCEL).map(|(b, _)| b)
            } else {
                None
            };
            let id = match wt.state {
                WorktreeState::Removable => Some(minter.mint(Item::Worktree {
                    repo: wt.repo.clone(),
                    path: wt.entry.path.clone(),
                })),
                WorktreeState::Prunable => Some(minter.mint(Item::Prune {
                    repo: wt.repo.clone(),
                    path: wt.entry.path.clone(),
                })),
                _ => None,
            };
            worktrees.push(WorktreeDto {
                id,
                path: wt.entry.path.display().to_string(),
                repo: wt.repo.display().to_string(),
                branch: wt.entry.branch.clone(),
                size,
                state: wt.state.key(),
                changes: wt.changes.map(|c| c as u64),
                idle_hours: wt.idle_hours,
            });
        }
    }
    worktrees
}

fn scan_caches(app: &tauri::AppHandle, minter: &mut Minter) -> Vec<CacheDto> {
    emit(app, "caches", 0, None);
    let mut caches: Vec<CacheDto> = Vec::new();
    for kind in CacheKind::ALL {
        // pnpm has one store per drive, each its own row and its own prune.
        let mut locations = if kind == CacheKind::Pnpm {
            dc::pnpm_stores()
        } else {
            kind.locations()
        };
        locations.dedup();
        for path in locations.into_iter().filter(|p| p.is_dir()) {
            if CANCEL.load(Ordering::Relaxed) {
                break;
            }
            let (method, command, available) = match kind.method() {
                CacheMethod::Command { program, args } => (
                    "command",
                    format!("{program} {}", args.join(" ")),
                    dc::tool(program).is_some(),
                ),
                CacheMethod::Delete => ("delete", format!("delete {}", path.display()), true),
            };
            // A command-driven cache is one row per tool, not per folder —
            // except pnpm, whose command is pointed at each store.
            if method == "command"
                && kind != CacheKind::Pnpm
                && caches.iter().any(|c| c.kind == kind.key())
            {
                continue;
            }
            emit(app, "caches", caches.len() as u64, Some(&path));
            let size = dc::measure(&path, &CANCEL).map(|(b, _)| b);
            let id = available.then(|| {
                minter.mint(Item::Cache {
                    kind,
                    path: path.clone(),
                })
            });
            caches.push(CacheDto {
                id,
                kind: kind.key(),
                path: path.display().to_string(),
                size,
                method,
                command,
                restore: kind.restore(),
            });
        }
    }
    caches
}

fn scan_docker(app: &tauri::AppHandle, minter: &mut Minter) -> DockerDto {
    emit(app, "docker", 0, None);
    let rows = match dc::docker_df() {
        DockerState::NotInstalled => {
            return DockerDto {
                state: "notInstalled",
                items: Vec::new(),
            };
        }
        DockerState::NotRunning => {
            return DockerDto {
                state: "notRunning",
                items: Vec::new(),
            };
        }
        DockerState::Ok(rows) => rows,
    };
    let row = |t: &str| rows.iter().find(|r| r.kind == t);
    let dangling = dc::docker_dangling();
    let counts = [
        (
            DockerKind::DanglingImages,
            dangling.map(|d| d.0),
            dangling.map(|d| d.1),
        ),
        (
            DockerKind::UnusedImages,
            row("Images").and_then(|r| r.total),
            row("Images").and_then(|r| r.reclaimable),
        ),
        (
            DockerKind::BuildCache,
            row("Build Cache").and_then(|r| r.total),
            row("Build Cache").and_then(|r| r.reclaimable),
        ),
        (
            DockerKind::StoppedContainers,
            row("Containers").and_then(|r| r.total),
            row("Containers").and_then(|r| r.reclaimable),
        ),
        (
            DockerKind::Volumes,
            row("Local Volumes").and_then(|r| r.total),
            row("Local Volumes").and_then(|r| r.reclaimable),
        ),
    ];
    let items = counts
        .into_iter()
        .map(|(kind, count, reclaimable)| DockerItemDto {
            // Volumes have no command, so no id: never offered.
            id: kind.args().map(|_| minter.mint(Item::Docker(kind))),
            kind: kind.key(),
            count,
            reclaimable,
            command: kind.args().map_or_else(
                || "never removed by Vitals".to_owned(),
                |a| format!("docker {}", a.join(" ")),
            ),
        })
        .collect();
    DockerDto { state: "ok", items }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

/// At most this many items per confirmation.
const RUN_MAX: usize = 2_000;

/// Resolves ids against the kept scan. Unknown ids are refused as a whole:
/// an id that is not from this scan is a bug or a forgery, and neither
/// should run half a request.
fn resolve(scan_id: u64, ids: &[String]) -> CommandResult<Vec<(String, Item)>> {
    let kept = KEPT.lock().map_err(|_| CommandError::Internal {
        message: "the kept scan is unavailable".into(),
    })?;
    let Some(kept) = kept.as_ref().filter(|k| k.id == scan_id) else {
        return Err(CommandError::NotFound {
            message: "that scan is no longer current; scan again".into(),
        });
    };
    ids.iter()
        .map(|id| {
            kept.items
                .get(id)
                .map(|item| (id.clone(), item.clone()))
                .ok_or_else(|| CommandError::Refused {
                    message: format!("{id} is not an item of this scan"),
                })
        })
        .collect()
}

#[tauri::command]
pub async fn run_dev_cleanup(
    app: tauri::AppHandle,
    scan_id: u64,
    ids: Vec<String>,
    confirmed: bool,
) -> CommandResult<DevCleanReportDto> {
    if !confirmed {
        return Err(CommandError::Refused {
            message: "this removes files permanently and was not confirmed, so nothing ran".into(),
        });
    }
    if ids.is_empty() || ids.len() > RUN_MAX {
        return Err(CommandError::Refused {
            message: format!("choose between 1 and {RUN_MAX} items"),
        });
    }
    let items = resolve(scan_id, &ids)?;
    let busy = Busy::claim("clean-up")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = busy;
        run(&app, &items)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the clean-up thread did not finish: {err}"),
    })
}

fn item_path(item: &Item) -> PathBuf {
    match item {
        Item::Artefact { path, .. }
        | Item::Worktree { path, .. }
        | Item::Prune { path, .. }
        | Item::Cache { path, .. }
        | Item::Disk(path) => path.clone(),
        // Docker's space is inside its disk; the drive that disk is on.
        Item::Docker(_) => dc::virtual_disks()
            .into_iter()
            .find(|d| d.docker)
            .map_or_else(|| PathBuf::from("C:\\"), |d| d.path),
    }
}

fn run(app: &tauri::AppHandle, items: &[(String, Item)]) -> DevCleanReportDto {
    let rules = vitals_win::storage::Rules::for_this_machine();
    // Free space per drive before anything runs.
    let mut before: BTreeMap<String, u64> = BTreeMap::new();
    for (_, item) in items {
        if let Some((drive, free)) = dc::drive_free(&item_path(item)) {
            before.entry(drive).or_insert(free);
        }
    }

    let total = items.len() as u64;
    // Docker kinds run in a fixed order and once each; disks go last,
    // because compaction only returns what the prunes freed.
    let mut order: Vec<&(String, Item)> = items.iter().collect();
    order.sort_by_key(|(_, i)| match i {
        Item::Disk(_) => 2,
        Item::Docker(_) => 1,
        _ => 0,
    });

    let mut out = Vec::new();
    let mut disks: Vec<(&String, &PathBuf)> = Vec::new();
    for (index, (id, item)) in order.into_iter().enumerate() {
        let _ = app.emit(
            CLEAN_PROGRESS,
            CleanProgressDto {
                id: id.clone(),
                index: index as u64 + 1,
                total,
            },
        );
        match item {
            // One elevated pass for every disk: one UAC prompt, and WSL is
            // stopped (and its restarters paused) once rather than per disk.
            Item::Disk(path) => disks.push((id, path)),
            _ => out.push(run_one(id, item, &rules)),
        }
    }
    out.extend(run_disks(&disks));

    let drives = before
        .into_iter()
        .filter_map(|(drive, was)| {
            let now = vitals_win::storage::managed::free_bytes(Path::new(&drive))?;
            Some(DriveDto {
                drive,
                freed: now.saturating_sub(was),
            })
        })
        .collect();
    DevCleanReportDto { items: out, drives }
}

fn report(id: &str, path: &Path, outcome: &'static str) -> DevCleanItemDto {
    DevCleanItemDto {
        id: id.to_owned(),
        path: path.display().to_string(),
        outcome,
        freed: None,
        tool_freed: None,
        message: None,
        holders: None,
    }
}

fn holders(path: &Path) -> Option<Vec<HolderDto>> {
    vitals_win::storage::holders_of(path).ok().map(|list| {
        list.into_iter()
            .map(|h| HolderDto {
                pid: h.pid,
                name: h.name,
            })
            .collect()
    })
}

/// Outcome of a folder removal: done when nothing is left, partial when
/// some of it went, failed when none did.
fn folder_outcome(id: &str, path: &Path, removal: &dc::Removal) -> DevCleanItemDto {
    let freed = removal
        .before
        .zip(removal.after)
        .map(|(b, a)| b.saturating_sub(a));
    let outcome = match (removal.after, freed) {
        (Some(0), _) => "done",
        (_, Some(f)) if f > 0 => "partial",
        _ => "failed",
    };
    DevCleanItemDto {
        freed,
        message: removal.error.clone(),
        holders: (outcome != "done").then(|| holders(path)).flatten(),
        ..report(id, path, outcome)
    }
}

fn run_one(id: &str, item: &Item, rules: &vitals_win::storage::Rules) -> DevCleanItemDto {
    match item {
        Item::Artefact { path, kind } => match dc::recheck_artefact(path, *kind, rules) {
            Recheck::Ok => folder_outcome(id, path, &dc::remove_folder(path, &CANCEL)),
            Recheck::Changed(why) => DevCleanItemDto {
                message: Some(why.to_owned()),
                ..report(id, path, "refused")
            },
        },
        Item::Worktree { repo, path } => {
            // Re-inspect: it must still be removable right now.
            let now = dc::worktrees_of(repo).into_iter().find(|w| {
                w.entry
                    .path
                    .as_os_str()
                    .eq_ignore_ascii_case(path.as_os_str())
            });
            if !now
                .as_ref()
                .is_some_and(|w| w.state == WorktreeState::Removable)
            {
                return DevCleanItemDto {
                    message: Some(now.map_or("gone", |w| w.state.key()).to_owned()),
                    ..report(id, path, "refused")
                };
            }
            let before = dc::measure(path, &CANCEL).map(|(b, _)| b);
            let (ok, text) = dc::remove_worktree(repo, path);
            let freed = if path.exists() { None } else { before };
            DevCleanItemDto {
                freed,
                message: (!ok).then_some(text.trim().to_owned()),
                holders: (!ok).then(|| holders(path)).flatten(),
                ..report(
                    id,
                    path,
                    if ok && !path.exists() {
                        "done"
                    } else {
                        "failed"
                    },
                )
            }
        }
        Item::Prune { repo, path } => {
            if path.exists() {
                return report(id, path, "refused");
            }
            let (ok, text) = dc::prune_worktrees(repo);
            DevCleanItemDto {
                freed: Some(0),
                message: (!ok).then_some(text.trim().to_owned()),
                ..report(id, path, if ok { "done" } else { "failed" })
            }
        }
        Item::Cache { kind, path } => run_cache(id, *kind, path),
        Item::Docker(kind) => {
            let Some(args) = kind.args() else {
                return report(id, Path::new("docker"), "refused");
            };
            let Some(docker) = dc::tool("docker") else {
                return report(id, Path::new("docker"), "unavailable");
            };
            let (ok, text) = dc::run_tool(&docker, args, None);
            DevCleanItemDto {
                tool_freed: dc::parse_reclaimed(&text),
                message: (!ok).then_some(text.trim().to_owned()),
                ..report(
                    id,
                    Path::new(&format!("docker {}", args.join(" "))),
                    if ok { "done" } else { "failed" },
                )
            }
        }
        Item::Disk(path) => run_disks(&[(&id.to_owned(), path)]).remove(0),
    }
}

fn run_disks(disks: &[(&String, &PathBuf)]) -> Vec<DevCleanItemDto> {
    if disks.is_empty() {
        return Vec::new();
    }
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    let before: Vec<Option<u64>> = disks.iter().map(|(_, p)| size(p)).collect();
    let paths: Vec<PathBuf> = disks.iter().map(|(_, p)| (*p).clone()).collect();
    let result = dc::compact_disks(&paths);
    disks
        .iter()
        .zip(before)
        .map(|((id, path), was)| {
            let freed = was.zip(size(path)).map(|(b, a)| b.saturating_sub(a));
            match &result {
                Ok(()) => DevCleanItemDto {
                    freed,
                    ..report(id, path, "done")
                },
                // Some disks may have shrunk even when another failed.
                Err(message) => DevCleanItemDto {
                    freed: freed.filter(|f| *f > 0),
                    message: Some(message.clone()),
                    ..report(
                        id,
                        path,
                        if dc::is_declined(message) {
                            "refused"
                        } else if freed.is_some_and(|f| f > 0) {
                            "partial"
                        } else {
                            "failed"
                        },
                    )
                },
            }
        })
        .collect()
}

fn run_cache(id: &str, kind: CacheKind, path: &Path) -> DevCleanItemDto {
    match kind.method() {
        CacheMethod::Command { program, args } => {
            let Some(exe) = dc::tool(program) else {
                return report(id, path, "unavailable");
            };
            let mut args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
            if kind == CacheKind::Pnpm {
                // Re-vetted now, and pointed at exactly the store that was
                // measured: without --store-dir pnpm prunes whichever store
                // the working directory's drive uses.
                let Some(root) = path.parent().filter(|_| dc::is_pnpm_store(path)) else {
                    return report(id, path, "refused");
                };
                args.push("--store-dir".to_owned());
                args.push(root.display().to_string());
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let before = dc::measure(path, &CANCEL).map(|(b, _)| b);
            // Run from the user's profile, never a project: `pnpm store
            // prune` inside a workspace would read that workspace's config.
            let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
            let (ok, text) = dc::run_tool(&exe, &args, home.as_deref());
            let after = if path.exists() {
                dc::measure(path, &CANCEL).map(|(b, _)| b)
            } else {
                Some(0)
            };
            DevCleanItemDto {
                freed: before.zip(after).map(|(b, a)| b.saturating_sub(a)),
                message: (!ok).then_some(text.trim().chars().take(400).collect()),
                ..report(id, path, if ok { "done" } else { "failed" })
            }
        }
        CacheMethod::Delete => {
            // Only the known cache folders, recomputed now: the id cannot
            // point this at anything else.
            if !kind.locations().iter().any(|l| l == path) {
                return report(id, path, "refused");
            }
            folder_outcome(id, path, &dc::remove_folder(path, &CANCEL))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_from_another_scan_or_a_made_up_one_is_refused_before_anything_runs() {
        *KEPT.lock().expect("lock") = Some(Kept {
            id: 7,
            items: HashMap::from([(
                "7-0".to_owned(),
                Item::Artefact {
                    path: PathBuf::from(r"C:\nowhere\node_modules"),
                    kind: ArtefactKind::NodeModules,
                },
            )]),
        });
        assert!(resolve(7, &["7-0".into()]).is_ok());
        assert!(matches!(
            resolve(6, &["7-0".into()]),
            Err(CommandError::NotFound { .. })
        ));
        assert!(matches!(
            resolve(7, &["7-0".into(), r"C:\Users".into()]),
            Err(CommandError::Refused { .. })
        ));
    }

    #[test]
    fn caller_roots_must_be_existing_absolute_folders() {
        assert!(vet_roots(Some(vec!["relative\\path".into()])).is_err());
        assert!(vet_roots(Some(vec![r"C:\vitals-no-such-folder-3c1".into()])).is_err());
        assert!(vet_roots(Some(vec!["C:\\".into(); 40])).is_err());
        assert!(vet_roots(Some(vec![std::env::temp_dir().display().to_string()])).is_ok());
    }

    #[test]
    fn a_folder_that_went_is_done_one_that_shrank_is_partial_and_one_that_stayed_failed() {
        let p = Path::new(r"C:\x");
        let done = folder_outcome(
            "a",
            p,
            &dc::Removal {
                before: Some(10),
                after: Some(0),
                error: None,
            },
        );
        assert_eq!((done.outcome, done.freed), ("done", Some(10)));
        let part = folder_outcome(
            "b",
            p,
            &dc::Removal {
                before: Some(10),
                after: Some(4),
                error: Some("in use".into()),
            },
        );
        assert_eq!((part.outcome, part.freed), ("partial", Some(6)));
        let none = folder_outcome(
            "c",
            p,
            &dc::Removal {
                before: Some(10),
                after: Some(10),
                error: Some("denied".into()),
            },
        );
        assert_eq!((none.outcome, none.freed), ("failed", Some(0)));
        let unknown = folder_outcome(
            "d",
            p,
            &dc::Removal {
                before: None,
                after: None,
                error: Some("x".into()),
            },
        );
        assert_eq!(
            (unknown.outcome, unknown.freed),
            ("failed", None),
            "unmeasured is None, never 0"
        );
    }

    #[test]
    fn a_cache_delete_is_refused_for_a_folder_that_is_not_that_cache() {
        let item = run_cache("x", CacheKind::Gradle, Path::new(r"C:\Users"));
        assert_eq!(item.outcome, "refused");
        assert!(Path::new(r"C:\Users").exists());
    }
}
