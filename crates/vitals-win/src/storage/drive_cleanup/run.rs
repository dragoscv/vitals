//! Executes the items a user ticked, in the order that gives the most back:
//! file deletions, then Docker prunes, then cleans inside each running
//! distro, and last the one elevated pass that stops WSL, compacts and
//! starts everything again — compaction only returns what the steps before
//! it freed.
//!
//! Every action is re-checked at execution, minutes after the analysis: a
//! container that was restarted, a volume something mounted since, a
//! folder that is no longer a build folder, are skipped and the report says
//! why.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use super::super::devclean::{self as dc, Recheck, system32};
use super::super::managed::{self, ManagedTool};
use super::elevated::{self, Pass};
use super::parse::{self, removable_containers, unused_volumes};
use super::{Action, BUILD_CACHE_KEEP, Blocker, BlockerKind, DistroTask, FolderKind, JOURNAL_KEEP};
use super::{TEMP_MIN_AGE_DAYS, probe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Done,
    Skipped,
    Failed,
}

impl Status {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemResult {
    pub id: String,
    pub status: Status,
    /// Measured bytes given back; `None` when it could not be measured.
    pub freed: Option<u64>,
    pub message: Option<String>,
}

impl ItemResult {
    fn new(id: &str, status: Status, freed: Option<u64>, message: Option<String>) -> Self {
        Self {
            id: id.to_owned(),
            status,
            freed,
            message,
        }
    }

    fn skipped(id: &str, why: &str) -> Self {
        Self::new(id, Status::Skipped, None, Some(why.to_owned()))
    }
}

/// One progress report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunProgress {
    pub stage: &'static str,
    pub current: Option<String>,
    pub done: u64,
    pub total: u64,
    pub disk_index: Option<u64>,
    pub disk_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub items: Vec<ItemResult>,
    pub drive_freed: Option<u64>,
    pub restarted: Vec<Blocker>,
    pub restart_failed: Vec<Blocker>,
}

/// Runs `items` (id, action) for the drive `mount`. `scan` names the
/// progress file of the elevated pass.
pub fn run(
    mount: &str,
    scan: &str,
    items: &[(String, Action)],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(RunProgress),
) -> Report {
    let root = PathBuf::from(format!("{}\\", super::drive_role::normalise_mount(mount)));
    let free_before = managed::free_bytes(&root);
    let total = items.len() as u64;
    let mut done = 0_u64;
    let mut out: Vec<ItemResult> = Vec::new();
    let (elevated, plain): (Vec<&(String, Action)>, Vec<&(String, Action)>) =
        items.iter().partition(|(_, a)| a.elevated());
    let mut plain = plain;
    plain.sort_by_key(|(_, a)| match a {
        Action::DockerBuildCache
        | Action::DockerOldImages
        | Action::DockerRemoveContainer { .. }
        | Action::DockerAnonVolumes(_)
        | Action::DockerNamedVolume(_) => 1,
        Action::Distro { .. } => 2,
        _ => 0,
    });

    let rules = super::super::Rules::for_this_machine();
    let running = probe::running_distros();
    for (id, action) in plain {
        if cancel.load(Ordering::Relaxed) {
            out.push(ItemResult::skipped(id, "cancelled"));
            continue;
        }
        done += 1;
        progress(RunProgress {
            stage: "cleaning",
            current: Some(id.clone()),
            done,
            total,
            disk_index: None,
            disk_count: None,
        });
        out.push(run_plain(id, action, &rules, &running));
    }

    let mut restarted = Vec::new();
    let mut restart_failed = Vec::new();
    if !elevated.is_empty() {
        let ran = run_elevated(scan, &elevated, &running, total, &mut done, progress);
        out.extend(ran.items);
        restarted = ran.restarted;
        restart_failed = ran.restart_failed;
    }

    progress(RunProgress {
        stage: "remeasuring",
        current: None,
        done: total,
        total,
        disk_index: None,
        disk_count: None,
    });
    let drive_freed = free_before
        .zip(managed::free_bytes(&root))
        .map(|(b, a)| a.saturating_sub(b));
    Report {
        items: out,
        drive_freed,
        restarted,
        restart_failed,
    }
}

// ---------------------------------------------------------------------------
// Unelevated
// ---------------------------------------------------------------------------

fn run_plain(
    id: &str,
    action: &Action,
    rules: &super::super::Rules,
    running: &[String],
) -> ItemResult {
    match action {
        Action::Unavailable => ItemResult::skipped(id, "not possible on this machine"),
        Action::TempOld(dir) => temp_old(id, dir),
        Action::RecycleBin(root) => recycle_bin(id, root),
        Action::DeleteCache { kind, path } => {
            if !kind.locations().iter().any(|l| l == path) {
                return ItemResult::skipped(id, "no longer that cache");
            }
            removal(id, &dc::remove_folder(path, &AtomicBool::new(false)))
        }
        Action::PnpmPrune(store) => pnpm_prune(id, store),
        Action::Artefact { path, kind } => match dc::recheck_artefact(path, *kind, rules) {
            Recheck::Ok => removal(id, &dc::remove_folder(path, &AtomicBool::new(false))),
            Recheck::Changed(why) => ItemResult::skipped(id, why),
        },
        Action::DockerBuildCache => docker_prune(
            id,
            &[
                "builder",
                "prune",
                "--force",
                "--keep-storage",
                &BUILD_CACHE_KEEP.to_string(),
            ],
        ),
        Action::DockerOldImages => docker_prune(
            id,
            &[
                "image",
                "prune",
                "--all",
                "--force",
                "--filter",
                "until=24h",
            ],
        ),
        Action::DockerRemoveContainer { id: cid, name } => remove_container(id, cid, name),
        Action::DockerAnonVolumes(names) => remove_volumes(id, names, true),
        Action::DockerNamedVolume(name) => remove_volumes(id, std::slice::from_ref(name), false),
        Action::Distro { distro, task } => {
            if !running.iter().any(|r| r == distro) {
                return ItemResult::skipped(id, "the distro is not running");
            }
            distro_task(id, distro, task)
        }
        Action::Managed(_)
        | Action::HiberfilOrphan
        | Action::Compact { .. }
        | Action::Sparse { .. } => ItemResult::skipped(id, "needs the elevated pass"),
    }
}

fn removal(id: &str, r: &dc::Removal) -> ItemResult {
    let freed = r.before.zip(r.after).map(|(b, a)| b.saturating_sub(a));
    let status = match (r.after, freed) {
        (Some(0), _) => Status::Done,
        (_, Some(f)) if f > 0 => Status::Done,
        _ => Status::Failed,
    };
    ItemResult::new(id, status, freed, r.error.clone())
}

/// Size and count of files under `dir` last written more than `days` ago,
/// never following links. `None` when `dir` cannot be read at all.
#[must_use]
pub fn old_files(dir: &Path, days: u64) -> Option<(u64, u64)> {
    old_files_within(dir, days, None, &AtomicBool::new(false))
}

/// [`old_files`] that gives up at `deadline` or on `cancel` and then
/// answers `None`: a half-walked figure would be a number nobody measured,
/// and a `%TEMP%` of a few million files (34 GB on the machine this was
/// written on) held the analysis for minutes.
#[must_use]
pub fn old_files_within(
    dir: &Path,
    days: u64,
    deadline: Option<std::time::Instant>,
    cancel: &AtomicBool,
) -> Option<(u64, u64)> {
    let cutoff = SystemTime::now().checked_sub(Duration::from_secs(days * 86_400))?;
    let mut acc = (0_u64, 0_u64);
    std::fs::read_dir(dir).ok()?;
    let stop = || {
        cancel.load(Ordering::Relaxed) || deadline.is_some_and(|d| std::time::Instant::now() >= d)
    };
    let complete = walk_old(dir, cutoff, &stop, &mut |_, len| {
        acc.0 += len;
        acc.1 += 1;
    });
    complete.then_some(acc)
}

/// Returns false when `stop` cut the walk short.
fn walk_old(
    dir: &Path,
    cutoff: SystemTime,
    stop: &dyn Fn() -> bool,
    visit: &mut dyn FnMut(&Path, u64),
) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    for entry in entries.flatten() {
        if stop() {
            return false;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        if meta.is_dir() {
            if !walk_old(&path, cutoff, stop, visit) {
                return false;
            }
        } else if meta.modified().is_ok_and(|m| m < cutoff) {
            visit(&path, meta.len());
        }
    }
    true
}

fn temp_old(id: &str, dir: &Path) -> ItemResult {
    let current = std::env::var_os("TEMP").map(PathBuf::from);
    if current.as_deref() != Some(dir) {
        return ItemResult::skipped(id, "no longer the temp folder");
    }
    let Some(cutoff) =
        SystemTime::now().checked_sub(Duration::from_secs(TEMP_MIN_AGE_DAYS * 86_400))
    else {
        return ItemResult::skipped(id, "clock");
    };
    let (mut freed, mut failed) = (0_u64, 0_u64);
    // Runs to the end: the deletion is what the user asked for.
    let _ = walk_old(dir, cutoff, &|| false, &mut |path, len| {
        // A file still open is skipped by Windows; that is the point of
        // the age floor, not a failure worth reporting one by one.
        if std::fs::remove_file(path).is_ok() {
            freed += len;
        } else {
            failed += 1;
        }
    });
    let message = (failed > 0).then(|| format!("{failed} files were in use and stayed"));
    ItemResult::new(id, Status::Done, Some(freed), message)
}

fn recycle_bin(id: &str, root: &Path) -> ItemResult {
    use windows_sys::Win32::UI::Shell::{
        SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW,
    };
    let before = probe::recycle_bin_size(root).map(|(b, _)| b);
    let wide: Vec<u16> = root
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: `wide` is NUL-terminated; no window owns the call.
    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            wide.as_ptr(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    let after = probe::recycle_bin_size(root).map(|(b, _)| b);
    let freed = before.zip(after).map(|(b, a)| b.saturating_sub(a));
    if hr >= 0 {
        ItemResult::new(id, Status::Done, freed, None)
    } else {
        ItemResult::new(id, Status::Failed, freed, Some(format!("{hr:#x}")))
    }
}

fn pnpm_prune(id: &str, store: &Path) -> ItemResult {
    let Some(root) = store.parent().filter(|_| dc::is_pnpm_store(store)) else {
        return ItemResult::skipped(id, "no longer a pnpm store");
    };
    let Some(pnpm) = dc::tool("pnpm") else {
        return ItemResult::skipped(id, "pnpm is not installed");
    };
    let before = dc::measure(store, &AtomicBool::new(false)).map(|(b, _)| b);
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let root = root.display().to_string();
    let (ok, text) = dc::run_tool(
        &pnpm,
        &["store", "prune", "--store-dir", &root],
        home.as_deref(),
    );
    let after = dc::measure(store, &AtomicBool::new(false)).map(|(b, _)| b);
    ItemResult::new(
        id,
        if ok { Status::Done } else { Status::Failed },
        before.zip(after).map(|(b, a)| b.saturating_sub(a)),
        (!ok).then(|| text.trim().chars().take(400).collect()),
    )
}

fn docker_prune(id: &str, args: &[&str]) -> ItemResult {
    let Some(docker) = dc::tool("docker") else {
        return ItemResult::skipped(id, "docker is not installed");
    };
    let (ok, text) = dc::run_tool(&docker, args, None);
    ItemResult::new(
        id,
        if ok { Status::Done } else { Status::Failed },
        dc::parse_reclaimed(&text),
        (!ok).then(|| text.trim().chars().take(400).collect()),
    )
}

fn remove_container(id: &str, cid: &str, name: &str) -> ItemResult {
    let (Some(docker), Some(usage)) = (dc::tool("docker"), probe::docker_usage()) else {
        return ItemResult::skipped(id, "docker is not answering");
    };
    let containers = probe::docker_containers();
    if !removable_containers(&containers, &usage.volumes)
        .iter()
        .any(|c| c.id == cid)
    {
        return ItemResult::skipped(id, "it was started or changed since the analysis");
    }
    let size = usage
        .container_sizes
        .iter()
        .find(|(short, _)| cid.starts_with(short.as_str()))
        .and_then(|(_, s)| *s);
    let (ok, text) = dc::run_tool(&docker, &["rm", cid], None);
    ItemResult::new(
        id,
        if ok { Status::Done } else { Status::Failed },
        size.filter(|_| ok),
        (!ok).then(|| format!("{name}: {}", text.trim())),
    )
}

/// Removes volumes that are still unused right now; any that something
/// mounted since the analysis is left alone.
fn remove_volumes(id: &str, names: &[String], anonymous: bool) -> ItemResult {
    let (Some(docker), Some(usage)) = (dc::tool("docker"), probe::docker_usage()) else {
        return ItemResult::skipped(id, "docker is not answering");
    };
    let containers = probe::docker_containers();
    let still: Vec<&parse::DockerVolume> = unused_volumes(&usage.volumes, &containers)
        .into_iter()
        .filter(|v| v.anonymous == anonymous && names.contains(&v.name))
        .collect();
    if still.is_empty() {
        return ItemResult::skipped(id, "in use since the analysis");
    }
    let mut args = vec!["volume", "rm"];
    args.extend(still.iter().map(|v| v.name.as_str()));
    let (ok, text) = dc::run_tool(&docker, &args, None);
    let freed = still.iter().map(|v| v.size).sum::<Option<u64>>();
    let left = names.len() - still.len();
    let message = match (ok, left) {
        (false, _) => Some(text.trim().chars().take(400).collect()),
        (true, 0) => None,
        (true, n) => Some(format!("{n} were in use since the analysis and stayed")),
    };
    ItemResult::new(
        id,
        if ok { Status::Done } else { Status::Failed },
        freed.filter(|_| ok),
        message,
    )
}

/// A path the measurement script reported, accepted only when it is plain:
/// absolute, under a home folder, no `..`, no quote.
#[must_use]
pub fn is_plain_distro_path(path: &str, kind: FolderKind) -> bool {
    let plain = path.starts_with('/')
        && !path.contains("..")
        && !path.contains(['\'', '\n', '\0'])
        && (path.starts_with("/home/") || path.starts_with("/root/"));
    let depth = path.split('/').filter(|s| !s.is_empty()).count();
    plain
        && depth >= 3
        && match kind {
            FolderKind::UserCache => path.contains("/.cache/"),
            FolderKind::RustTarget | FolderKind::Build => true,
        }
}

/// The shell for one inside-distro task. Each prints `FREED <bytes>` last.
#[must_use]
pub fn distro_script(task: &DistroTask) -> Option<String> {
    const SZ: &str = "sz() { [ -e \"$1\" ] && du -sbx \"$1\" 2>/dev/null | cut -f1 || echo 0; }\n";
    let body = match task {
        DistroTask::Apt => "b=$(sz /var/cache/apt/archives); apt-get clean >/dev/null 2>&1; a=$(sz /var/cache/apt/archives); echo FREED $((b-a))".to_owned(),
        DistroTask::Dnf => "b=$(sz /var/cache/dnf); dnf clean all >/dev/null 2>&1; a=$(sz /var/cache/dnf); echo FREED $((b-a))".to_owned(),
        DistroTask::Journal => format!(
            "b=$(sz /var/log/journal); journalctl --vacuum-size={JOURNAL_KEEP} >/dev/null 2>&1; a=$(sz /var/log/journal); echo FREED $((b-a))"
        ),
        DistroTask::Tmp => "s=$(find /tmp -xdev -type f -mtime +2 -printf '%s\\n' 2>/dev/null | awk '{s+=$1} END {print s+0}'); find /tmp -xdev -type f -mtime +2 -delete 2>/dev/null; echo FREED $s".to_owned(),
        DistroTask::Folder { path, kind } => {
            if !is_plain_distro_path(path, *kind) {
                return None;
            }
            let check = match kind {
                FolderKind::UserCache => "[ -d \"$d\" ]".to_owned(),
                FolderKind::RustTarget => "[ -d \"$d\" ] && { [ -f \"$(dirname \"$d\")/Cargo.toml\" ] || [ -d \"$d/debug\" ] || [ -d \"$d/release\" ]; }".to_owned(),
                FolderKind::Build => "[ -d \"$d\" ] && for m in package.json build.gradle build.gradle.kts CMakeLists.txt Cargo.toml; do [ -f \"$(dirname \"$d\")/$m\" ] && break; false; done".to_owned(),
            };
            format!(
                "d='{path}'; [ -L \"$d\" ] && {{ echo 'a link'; exit 3; }}; {check} || {{ echo 'no longer a build folder'; exit 3; }}; b=$(sz \"$d\"); rm -rf --one-file-system -- \"$d\"; a=$(sz \"$d\"); echo FREED $((b-a))"
            )
        }
        DistroTask::Docker => format!(
            "command -v docker >/dev/null || exit 3; docker builder prune -f --keep-storage {BUILD_CACHE_KEEP} 2>&1; docker image prune -a -f --filter until=24h 2>&1"
        ),
    };
    Some(format!("{SZ}{body}"))
}

fn distro_task(id: &str, distro: &str, task: &DistroTask) -> ItemResult {
    let Some(script) = distro_script(task) else {
        return ItemResult::skipped(id, "not a plain folder path");
    };
    let Some(text) = probe::in_distro(distro, &script) else {
        return ItemResult::new(id, Status::Failed, None, Some("wsl did not run it".into()));
    };
    let freed = if matches!(task, DistroTask::Docker) {
        let sizes: Vec<u64> = text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("Total reclaimed space:"))
            .filter_map(|s| dc::parse_docker_size(s.trim()))
            .collect();
        (!sizes.is_empty()).then(|| sizes.iter().sum())
    } else {
        parse::parse_freed(&text)
    };
    if freed.is_none() && !matches!(task, DistroTask::Docker) {
        return ItemResult::new(
            id,
            Status::Skipped,
            None,
            Some(text.trim().chars().take(200).collect()),
        );
    }
    ItemResult::new(id, Status::Done, freed, None)
}

// ---------------------------------------------------------------------------
// Elevated
// ---------------------------------------------------------------------------

struct Elevated {
    items: Vec<ItemResult>,
    restarted: Vec<Blocker>,
    restart_failed: Vec<Blocker>,
}

/// What was running before WSL stopped, to start again afterwards.
#[derive(Debug, Default)]
struct Before {
    distros: Vec<String>,
    docker_desktop: Option<PathBuf>,
    containers: Vec<(String, String)>,
    runners: Vec<(String, String)>,
    tasks: Vec<parse::WslTask>,
}

fn record(running: &[String]) -> Before {
    let docker_desktop = dc::docker_desktop_exe();
    let containers = if docker_desktop.is_some() {
        probe::docker_containers()
            .into_iter()
            .filter(|c| c.running)
            .map(|c| (c.id, c.name))
            .collect()
    } else {
        Vec::new()
    };
    let distros: Vec<String> = running
        .iter()
        .filter(|d| !d.starts_with("docker-desktop"))
        .cloned()
        .collect();
    let runners = distros
        .iter()
        .flat_map(|d| {
            probe::in_distro(
                d,
                "systemctl list-units --type=service --state=running --no-legend --plain 'actions.runner*' 2>/dev/null",
            )
            .map(|t| parse::parse_runner_units(&t))
            .unwrap_or_default()
            .into_iter()
            .map(|u| (d.clone(), u))
        })
        .collect();
    Before {
        distros,
        docker_desktop,
        containers,
        runners,
        tasks: probe::wsl_tasks(),
    }
}

/// Maps the child's `result` lines onto the items it was given.
#[must_use]
pub fn match_results(lines: &[parse::ProgressLine], key: &str) -> Option<i64> {
    lines
        .iter()
        .rev()
        .find(|l| l.stage == "result" && l.current.as_deref() == Some(key))
        .and_then(|l| l.code)
}

fn result_key(action: &Action) -> Option<String> {
    match action {
        Action::Managed(tool) => Some(tool.as_arg()),
        Action::HiberfilOrphan => Some("hiberfil".to_owned()),
        Action::Compact { path, .. } => Some(path.to_string_lossy().into_owned()),
        Action::Sparse { distro, .. } => Some(format!("sparse:{distro}")),
        _ => None,
    }
}

fn pass_for(items: &[&(String, Action)]) -> Pass {
    let mut pass = Pass::default();
    for (_, action) in items {
        match action {
            Action::Managed(tool) => pass.tools.push(*tool),
            Action::HiberfilOrphan => pass.hiberfil = true,
            Action::Compact { path, .. } => pass.disks.push(path.clone()),
            Action::Sparse { distro, .. } => pass.sparse.push(distro.clone()),
            _ => {}
        }
    }
    pass
}

/// Trims inside each running distro whose disk is compacted, while it still
/// runs: compaction only returns trimmed or zeroed blocks.
fn trim(
    pass: &Pass,
    running: &[String],
    total: u64,
    done: u64,
    progress: &mut dyn FnMut(RunProgress),
) {
    for disk in dc::virtual_disks()
        .iter()
        .filter(|d| pass.disks.contains(&d.path))
    {
        let distro = match (&disk.distro, disk.docker) {
            (Some(n), _) => n.clone(),
            (None, true) => "docker-desktop".to_owned(),
            _ => continue,
        };
        if !running.contains(&distro) {
            continue;
        }
        progress(RunProgress {
            stage: "trimming",
            current: Some(distro.clone()),
            done,
            total,
            disk_index: None,
            disk_count: None,
        });
        let _ = dc::run_tool(
            &system32("wsl.exe"),
            &["-d", &distro, "-u", "root", "--", "fstrim", "-a"],
            None,
        );
    }
}

/// The drive a Windows tool frees space on.
fn managed_root(action: &Action) -> Option<PathBuf> {
    match action {
        Action::Managed(ManagedTool::ComponentCleanup | ManagedTool::DiskCleanup(_)) => {
            super::drive_role::windows_directory()
                .and_then(|w| w.ancestors().last().map(Path::to_path_buf))
        }
        _ => None,
    }
}

/// What an elevated item measures before the pass: its own size for a file
/// that shrinks, the drive's free bytes for a Windows tool.
fn measure_before(action: &Action) -> Option<u64> {
    match action {
        Action::Compact { path, .. } => std::fs::metadata(path).ok().map(|m| m.len()),
        Action::HiberfilOrphan => hiberfil_size(),
        _ => managed_root(action).and_then(|r| managed::free_bytes(&r)),
    }
}

fn freed_after(action: &Action, before: Option<u64>, lone: bool) -> Option<u64> {
    match action {
        Action::Compact { path, .. } => before
            .zip(std::fs::metadata(path).ok().map(|m| m.len()))
            .map(|(b, a)| b.saturating_sub(a)),
        Action::HiberfilOrphan => before
            .zip(hiberfil_size())
            .map(|(b, a)| b.saturating_sub(a)),
        // A Windows tool's own figure is the drive's free space change
        // across it; with several tools in one pass that is shared, so
        // only a lone tool claims it.
        Action::Managed(_) if lone => before
            .zip(managed_root(action).and_then(|r| managed::free_bytes(&r)))
            .map(|(b, a)| a.saturating_sub(b)),
        _ => None,
    }
}

/// One elevated item's outcome from the pass's exit code and its own
/// `result` line.
#[must_use]
pub fn elevated_result(
    id: &str,
    stops_wsl: bool,
    code: Option<u32>,
    item_code: Option<i64>,
    freed: Option<u64>,
) -> ItemResult {
    match (code, item_code) {
        (Some(elevated::exit::WSL_STILL_RUNNING), _) if stops_wsl => ItemResult::new(
            id,
            Status::Failed,
            None,
            Some("WSL did not stop, so no disk was touched".into()),
        ),
        (_, Some(0 | 3010)) => ItemResult::new(id, Status::Done, freed, None),
        (_, Some(c)) => ItemResult::new(
            id,
            Status::Failed,
            freed.filter(|f| *f > 0),
            Some(format!("code {c:#x}")),
        ),
        (None, None) => ItemResult::new(
            id,
            Status::Failed,
            None,
            Some("the elevated pass could not be waited on".into()),
        ),
        (Some(_), None) => ItemResult::skipped(id, "the elevated pass did not reach it"),
    }
}

fn run_elevated(
    scan: &str,
    items: &[&(String, Action)],
    running: &[String],
    total: u64,
    done: &mut u64,
    progress: &mut dyn FnMut(RunProgress),
) -> Elevated {
    let mut pass = pass_for(items);
    let fail_all = |why: &str| Elevated {
        items: items
            .iter()
            .map(|(id, _)| ItemResult::new(id, Status::Skipped, None, Some(why.to_owned())))
            .collect(),
        restarted: Vec::new(),
        restart_failed: Vec::new(),
    };
    let Some(file) = elevated::progress_path(scan) else {
        return fail_all("no folder for the progress file");
    };
    let _ = std::fs::remove_file(&file);
    if let Err(err) = elevated::create_progress_file(&file) {
        return fail_all(&format!("the progress file could not be created: {err}"));
    }
    pass.progress.clone_from(&file);

    let before = if pass.stops_wsl() {
        let before = record(running);
        trim(&pass, running, total, *done, progress);
        before
    } else {
        Before::default()
    };
    let measured: Vec<Option<u64>> = items.iter().map(|(_, a)| measure_before(a)).collect();

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return fail_all(&err.to_string()),
    };
    let child = crate::actions::taskmgr::spawn_program_elevated(
        &exe,
        &pass.to_args(),
        "nothing was changed",
    );
    let (code, lines) = match child {
        Ok(mut child) => tail(&mut child, &file, total, done, progress),
        Err(err) => {
            let _ = std::fs::remove_file(&file);
            return fail_all(&err.to_string());
        }
    };
    let _ = std::fs::remove_file(&file);

    let lone = items.len() == 1;
    let out = items
        .iter()
        .zip(measured)
        .map(|((id, action), before)| {
            let item_code = result_key(action).and_then(|k| match_results(&lines, &k));
            let freed = freed_after(action, before, lone);
            elevated_result(id, action.stops_wsl(), code, item_code, freed)
        })
        .collect();

    let (restarted, restart_failed) = if pass.stops_wsl() {
        restart(&before, total, *done, progress)
    } else {
        (Vec::new(), Vec::new())
    };
    Elevated {
        items: out,
        restarted,
        restart_failed,
    }
}

fn hiberfil_size() -> Option<u64> {
    let file = super::drive_role::windows_directory()?
        .ancestors()
        .last()?
        .join("hiberfil.sys");
    match std::fs::metadata(&file) {
        Ok(m) => Some(m.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(0),
        Err(_) => None,
    }
}

/// Waits for the child, turning each new progress line into a report.
/// Returns the exit code (`None` when the wait failed) and every line read.
fn tail(
    child: &mut crate::actions::taskmgr::ElevatedProcess,
    file: &Path,
    total: u64,
    done: &mut u64,
    progress: &mut dyn FnMut(RunProgress),
) -> (Option<u32>, Vec<parse::ProgressLine>) {
    let mut read = 0_usize;
    let mut pending = String::new();
    let mut lines = Vec::new();
    let mut drain =
        |pending: &mut String, read: &mut usize, lines: &mut Vec<parse::ProgressLine>| {
            let Ok(bytes) = std::fs::read(file) else {
                return;
            };
            if bytes.len() > *read {
                pending.push_str(&String::from_utf8_lossy(&bytes[*read..]));
                *read = bytes.len();
            }
            let (complete, rest) = parse::split_complete(pending);
            let parsed: Vec<parse::ProgressLine> = complete
                .iter()
                .filter_map(|l| parse::parse_progress_line(l))
                .collect();
            let rest = rest.to_owned();
            *pending = rest;
            for line in parsed {
                if line.stage == "result" {
                    *done += 1;
                } else {
                    progress(RunProgress {
                        stage: match line.stage.as_str() {
                            "stopping" => "stopping",
                            "compacting" => "compacting",
                            "restarting" => "restarting",
                            _ => "cleaning",
                        },
                        current: line.current.clone(),
                        done: (*done).min(total),
                        total,
                        disk_index: line.disk_index,
                        disk_count: line.disk_count,
                    });
                }
                lines.push(line);
            }
        };
    let code = loop {
        match child.wait(Some(Duration::from_millis(500))) {
            Ok(Some(code)) => break Some(code),
            Ok(None) => drain(&mut pending, &mut read, &mut lines),
            Err(_) => break None,
        }
    };
    drain(&mut pending, &mut read, &mut lines);
    (code, lines)
}

/// Starts again what was running: Docker Desktop (unelevated, from this
/// process, so it is the user's), its containers, the distros, and checks
/// that the runners and the paused tasks came back.
fn restart(
    before: &Before,
    total: u64,
    done: u64,
    progress: &mut dyn FnMut(RunProgress),
) -> (Vec<Blocker>, Vec<Blocker>) {
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    let mut report = |kind, name: String, success: bool| {
        let b = Blocker {
            kind,
            name,
            restart_after: true,
        };
        if success { ok.push(b) } else { failed.push(b) }
    };
    let say = |progress: &mut dyn FnMut(RunProgress), current: &str| {
        progress(RunProgress {
            stage: "restarting",
            current: Some(current.to_owned()),
            done,
            total,
            disk_index: None,
            disk_count: None,
        });
    };
    let wsl = system32("wsl.exe");
    for d in &before.distros {
        say(progress, d);
        let (started, _) = dc::run_tool(&wsl, &["-d", d, "--exec", "true"], None);
        report(BlockerKind::WslDistro, d.clone(), started);
    }
    if let Some(exe) = &before.docker_desktop {
        say(progress, "Docker Desktop");
        let launched = std::process::Command::new(exe).spawn().is_ok();
        report(
            BlockerKind::DockerDesktop,
            "Docker Desktop".to_owned(),
            launched,
        );
        if launched
            && !before.containers.is_empty()
            && let Some(docker) = dc::tool("docker")
        {
            let ids: Vec<String> = before.containers.iter().map(|(id, _)| id.clone()).collect();
            dc::restart_containers(&docker, &ids);
            let now: Vec<String> = probe::docker_containers()
                .into_iter()
                .filter(|c| c.running)
                .map(|c| c.id)
                .collect();
            for (id, name) in &before.containers {
                report(BlockerKind::Container, name.clone(), now.contains(id));
            }
        }
    }
    for (d, unit) in &before.runners {
        let active = probe::in_distro(d, &format!("systemctl is-active '{unit}'"))
            .is_some_and(|t| t.trim() == "active");
        report(BlockerKind::GithubRunner, format!("{d}: {unit}"), active);
    }
    let tasks_now = probe::wsl_tasks();
    for t in &before.tasks {
        let back = tasks_now
            .iter()
            .any(|n| n.path == t.path && n.name == t.name);
        report(
            BlockerKind::ScheduledTask,
            format!("{}{}", t.path, t.name),
            back,
        );
    }
    (ok, failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_distro_folder_is_removed_only_when_its_path_is_plain_and_under_a_home() {
        assert!(is_plain_distro_path(
            "/home/u/.cache/pip",
            FolderKind::UserCache
        ));
        assert!(is_plain_distro_path(
            "/home/u/scrin-target",
            FolderKind::RustTarget
        ));
        assert!(!is_plain_distro_path(
            "/home/u/../../etc",
            FolderKind::Build
        ));
        assert!(!is_plain_distro_path("/etc/x/build", FolderKind::Build));
        assert!(!is_plain_distro_path("/home/u", FolderKind::Build));
        assert!(!is_plain_distro_path(
            "/home/u/it's/build",
            FolderKind::Build
        ));
        assert!(!is_plain_distro_path(
            "/home/u/build",
            FolderKind::UserCache
        ));
        assert_eq!(
            distro_script(&DistroTask::Folder {
                path: "/var/lib/docker".into(),
                kind: FolderKind::Build
            }),
            None
        );
    }

    #[test]
    fn every_inside_distro_clean_measures_and_reports_what_it_freed() {
        for task in [
            DistroTask::Apt,
            DistroTask::Dnf,
            DistroTask::Journal,
            DistroTask::Tmp,
        ] {
            let s = distro_script(&task).expect("script");
            assert!(s.contains("FREED"), "{task:?}");
        }
        let s = distro_script(&DistroTask::Folder {
            path: "/home/u/scrin-target".into(),
            kind: FolderKind::RustTarget,
        })
        .expect("script");
        let check = s.find("Cargo.toml").expect("re-checks");
        let rm = s.find("rm -rf --one-file-system").expect("removes");
        assert!(check < rm && s.contains("[ -L \"$d\" ]"));
        assert!(
            distro_script(&DistroTask::Journal)
                .expect("j")
                .contains("--vacuum-size=104857600")
        );
    }

    #[test]
    fn results_are_matched_by_key_and_the_last_line_for_a_key_wins() {
        let lines = vec![
            parse::ProgressLine {
                stage: "compacting".into(),
                current: Some("C:\\a.vhdx".into()),
                disk_index: Some(1),
                disk_count: Some(1),
                code: None,
            },
            parse::ProgressLine {
                stage: "result".into(),
                current: Some("C:\\a.vhdx".into()),
                disk_index: Some(1),
                disk_count: Some(1),
                code: Some(1),
            },
            parse::ProgressLine {
                stage: "result".into(),
                current: Some("dism.components".into()),
                disk_index: None,
                disk_count: None,
                code: Some(3010),
            },
        ];
        assert_eq!(match_results(&lines, "C:\\a.vhdx"), Some(1));
        assert_eq!(match_results(&lines, "dism.components"), Some(3010));
        assert_eq!(match_results(&lines, "hiberfil"), None);
        assert_eq!(
            result_key(&Action::Sparse {
                distro: "Ubuntu".into(),
                path: PathBuf::from("C:\\x")
            }),
            Some("sparse:Ubuntu".into())
        );
    }

    #[test]
    fn old_files_counts_only_files_past_the_age_floor() {
        let dir = std::env::temp_dir().join(format!("vitals-old-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).expect("dir");
        std::fs::write(dir.join("sub").join("new.txt"), b"12345").expect("write");
        assert_eq!(old_files(&dir, 2), Some((0, 0)));
        assert_eq!(old_files(&dir, 0).map(|(_, n)| n), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(old_files(&dir, 2), None);
    }

    #[test]
    fn an_elevated_item_is_done_only_on_its_own_success_line_and_never_when_wsl_would_not_stop() {
        let wsl = Some(elevated::exit::WSL_STILL_RUNNING);
        let r = elevated_result("a", true, wsl, Some(0), Some(5));
        assert_eq!((r.status, r.freed), (Status::Failed, None));
        // A Windows tool in the same pass still ran before WSL was touched.
        assert_eq!(
            elevated_result("b", false, wsl, Some(3010), None).status,
            Status::Done
        );
        let r = elevated_result(
            "c",
            true,
            Some(elevated::exit::SOME_FAILED),
            Some(1),
            Some(0),
        );
        assert_eq!((r.status, r.freed), (Status::Failed, None));
        assert_eq!(
            elevated_result("d", true, Some(0), None, None).status,
            Status::Skipped
        );
        assert_eq!(
            elevated_result("e", true, None, None, None).status,
            Status::Failed
        );
        assert_eq!(
            elevated_result("f", true, Some(0), Some(0), Some(9)).freed,
            Some(9)
        );
    }

    #[test]
    fn a_temp_walk_cut_short_by_its_deadline_or_a_cancel_reports_unmeasured_not_a_partial_sum() {
        let dir = std::env::temp_dir().join(format!("vitals-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("a.txt"), b"1").expect("write");
        let past = std::time::Instant::now();
        assert_eq!(
            old_files_within(&dir, 0, Some(past), &AtomicBool::new(false)),
            None
        );
        assert_eq!(
            old_files_within(&dir, 0, None, &AtomicBool::new(true)),
            None
        );
        assert_eq!(
            old_files_within(&dir, 0, None, &AtomicBool::new(false)).map(|(_, n)| n),
            Some(1)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
