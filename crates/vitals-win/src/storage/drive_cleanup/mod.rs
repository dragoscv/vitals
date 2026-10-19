//! Per-drive clean-up: what can be freed on one drive, and freeing it.
//!
//! The analysis reuses the scanners that already exist — the Windows
//! cleanup locations, the developer clean-up's artefacts, caches, Docker and
//! WSL disks — and keeps only what lives on the drive asked about. Its result
//! is a [`Plan`] of [`PlannedItem`]s, each carrying the [`Action`] that frees
//! it; the desktop keeps the actions and hands the webview ids, so the
//! webview can choose among what was found and never name a path itself.
//!
//! Measured or `None`: an item whose size could not be read says so. A
//! zero there would tell the user a 300 GB disk has nothing to give.

pub mod elevated;
pub mod history;
pub mod parse;
pub mod probe;
pub mod run;
pub mod schedule;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use vitals_core::metrics::DiskKind;

use super::cleanup::{CleanupKind, candidate_locations};
use super::devclean::{self as dc, ArtefactKind, CacheKind};
use super::drive_role::{self, DriveRole, is_on};
use super::managed::{Handler, ManagedTool};
use parse::{Hiberfil, WslVersion, removable_containers, unused_volumes};

/// Artefacts smaller than this are not worth a row in a per-drive plan;
/// the developer clean-up lists them all.
const MIN_ARTEFACT: u64 = 100 * 1024 * 1024;
/// At most this many artefact rows, largest first.
const MAX_ARTEFACTS: usize = 400;
/// What the build cache prune keeps, so the next build stays warm.
pub const BUILD_CACHE_KEEP: u64 = 20 * 1_000_000_000;
/// Images younger than this are kept: a day's builds are usually still wanted.
pub const IMAGE_MIN_AGE_HOURS: u64 = 24;
/// `%TEMP%` files younger than this are kept: an installer may still need them.
pub const TEMP_MIN_AGE_DAYS: u64 = 2;
/// The journal is vacuumed down to this, not emptied.
pub const JOURNAL_KEEP: u64 = 100 * 1024 * 1024;
/// The first WSL with `wsl --manage <distro> --set-sparse`
/// (<https://github.com/microsoft/WSL/releases/tag/2.0.0>).
pub const SPARSE_FIRST_WSL: WslVersion = WslVersion([2, 0, 0, 0]);
/// From this WSL on, `--set-sparse true` is refused ("Sparse VHD support is
/// currently disabled due to potential data corruption") unless
/// `--allow-unsafe` is added, which Vitals never does: the flag exists
/// because sparse disks were corrupting distros (microsoft/WSL#10609).
/// <https://github.com/microsoft/WSL/releases/tag/2.5.6>
pub const SPARSE_DISABLED_WSL: WslVersion = WslVersion([2, 5, 6, 0]);

/// The wizard's groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Windows,
    Temp,
    RecycleBin,
    Caches,
    DevArtefacts,
    Docker,
    Wsl,
    Compaction,
    Sparse,
}

impl Group {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Temp => "temp",
            Self::RecycleBin => "recycleBin",
            Self::Caches => "caches",
            Self::DevArtefacts => "devArtefacts",
            Self::Docker => "docker",
            Self::Wsl => "wsl",
            Self::Compaction => "compaction",
            Self::Sparse => "sparse",
        }
    }
}

/// How careful the user should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    Safe,
    Review,
    Danger,
}

impl Safety {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Review => "review",
            Self::Danger => "danger",
        }
    }
}

/// Why an item is shown for its size but cannot be done from here. The
/// webview names each one in both languages; `detail` stays the technical
/// line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// Windows refuses `powercfg /h on` (Guarded Host / virtualisation-based
    /// security), so a leftover `hiberfil.sys` cannot be removed that way.
    HibernationRefused,
    /// This WSL refuses sparse disks without `--allow-unsafe`.
    SparseDisabledByWsl,
    /// This WSL predates `--set-sparse`.
    SparseNeedsNewerWsl,
    /// `wsl --version` could not be read, so whether the step would be
    /// refused is unknown — and a step that may fail is not offered.
    WslVersionUnknown,
}

impl UnavailableReason {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::HibernationRefused => "hibernationRefused",
            Self::SparseDisabledByWsl => "sparseDisabledByWsl",
            Self::SparseNeedsNewerWsl => "sparseNeedsNewerWsl",
            Self::WslVersionUnknown => "wslVersionUnknown",
        }
    }
}

/// Whether this WSL would refuse `--set-sparse true`, and why. An unread
/// version is not taken as "allowed": the step would then fail mid-pass,
/// after WSL and Docker had already been stopped for it.
#[must_use]
pub fn sparse_refusal(version: Option<WslVersion>) -> Option<UnavailableReason> {
    match version {
        None => Some(UnavailableReason::WslVersionUnknown),
        Some(v) if v >= SPARSE_DISABLED_WSL => Some(UnavailableReason::SparseDisabledByWsl),
        Some(v) if v < SPARSE_FIRST_WSL => Some(UnavailableReason::SparseNeedsNewerWsl),
        Some(_) => None,
    }
}

/// What actually runs for an item. Kept by the desktop behind an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Files older than [`TEMP_MIN_AGE_DAYS`] under the user's temp folder.
    TempOld(PathBuf),
    /// A Windows tool in the elevated pass.
    Managed(ManagedTool),
    /// `powercfg /h on` then `/h off`, which makes Windows delete a
    /// `hiberfil.sys` left behind while hibernation was already off.
    HiberfilOrphan,
    /// Shown, never run: the reason is in the item's detail.
    Unavailable,
    /// This drive's Recycle Bin, emptied through the shell.
    RecycleBin(PathBuf),
    /// A pure download cache folder, deleted.
    DeleteCache {
        kind: CacheFolder,
        path: PathBuf,
    },
    /// `pnpm store prune --store-dir`, which keeps every linked package.
    PnpmPrune(PathBuf),
    /// A build artefact folder (re-checked before removal).
    Artefact {
        path: PathBuf,
        kind: ArtefactKind,
    },
    DockerBuildCache,
    DockerOldImages,
    DockerRemoveContainer {
        id: String,
        name: String,
    },
    /// Anonymous volumes nothing mounts, by name.
    DockerAnonVolumes(Vec<String>),
    DockerNamedVolume(String),
    /// A clean inside a WSL distro, run as root while the distro runs.
    Distro {
        distro: String,
        task: DistroTask,
    },
    /// Compacting one `.vhdx` in the elevated pass.
    Compact {
        path: PathBuf,
        docker: bool,
    },
    /// `wsl --manage <distro> --set-sparse true` while WSL is stopped.
    Sparse {
        distro: String,
        path: PathBuf,
    },
}

impl Action {
    /// Whether the elevated pass does it.
    #[must_use]
    pub const fn elevated(&self) -> bool {
        matches!(
            self,
            Self::Managed(_) | Self::HiberfilOrphan | Self::Compact { .. } | Self::Sparse { .. }
        )
    }

    /// Whether WSL is stopped for it.
    #[must_use]
    pub const fn stops_wsl(&self) -> bool {
        matches!(self, Self::Compact { .. } | Self::Sparse { .. })
    }
}

/// A cache folder deleted as a whole: only pure download caches whose
/// owner fetches them again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheFolder {
    Npm,
    PnpmMetadata,
    NugetHttp,
    NugetPackages,
    CargoRegistry,
    Gradle,
}

impl CacheFolder {
    pub const ALL: [Self; 6] = [
        Self::Npm,
        Self::PnpmMetadata,
        Self::NugetHttp,
        Self::NugetPackages,
        Self::CargoRegistry,
        Self::Gradle,
    ];

    /// The folders it lives in, recomputed from the environment each time,
    /// so a kept action can be checked against them at execution.
    #[must_use]
    pub fn locations(self) -> Vec<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
        let at = |base: &Option<PathBuf>, rel: &str| base.as_ref().map(|b| b.join(rel));
        match self {
            Self::Npm => vec![at(&local, "npm-cache")],
            Self::PnpmMetadata => vec![at(&local, "pnpm-cache")],
            Self::NugetHttp => vec![at(&local, "NuGet\\v3-cache")],
            Self::NugetPackages => vec![at(&home, ".nuget\\packages")],
            Self::CargoRegistry => CacheKind::Cargo.locations().into_iter().map(Some).collect(),
            Self::Gradle => CacheKind::Gradle
                .locations()
                .into_iter()
                .map(Some)
                .collect(),
        }
        .into_iter()
        .flatten()
        .collect()
    }

    const fn kind(self) -> &'static str {
        match self {
            Self::Npm => "npmCache",
            Self::PnpmMetadata => "pnpmCache",
            Self::NugetHttp | Self::NugetPackages => "nugetCache",
            Self::CargoRegistry => "cargoRegistry",
            Self::Gradle => "gradleCaches",
        }
    }

    /// Caches that refill themselves on the next command and hold nothing
    /// a project needs offline. The rest cost a long re-download.
    const fn self_restoring(self) -> bool {
        matches!(self, Self::Npm | Self::PnpmMetadata | Self::NugetHttp)
    }
}

/// One clean inside a distro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistroTask {
    Apt,
    Dnf,
    Journal,
    Tmp,
    /// `rm -rf` of a folder the measurement found (`~/.cache/x`, a Rust
    /// target, a `build`), re-checked inside the distro first.
    Folder {
        path: String,
        kind: FolderKind,
    },
    /// The distro's own Docker engine (not Docker Desktop's).
    Docker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderKind {
    UserCache,
    RustTarget,
    Build,
}

/// One row of the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
// Each flag is a separate fact the UI shows; they are not exclusive states.
pub struct PlannedItem {
    pub group: Group,
    /// i18n key suffix (`tempOld`, `compactVhd`...).
    pub kind: &'static str,
    pub label: Option<String>,
    pub size: Option<u64>,
    pub safety: Safety,
    pub default_selected: bool,
    pub irreversible: bool,
    pub needs_elevation: bool,
    pub stops_wsl: bool,
    pub stops_docker: bool,
    pub detail: Option<String>,
    /// False when it is shown for its size but cannot be done here.
    pub available: bool,
    /// Why it cannot, when it cannot and the reason is known.
    pub unavailable_reason: Option<UnavailableReason>,
    pub action: Action,
}

impl PlannedItem {
    fn new(group: Group, kind: &'static str, action: Action) -> Self {
        Self {
            group,
            kind,
            label: None,
            size: None,
            safety: Safety::Review,
            default_selected: false,
            irreversible: true,
            needs_elevation: action.elevated(),
            stops_wsl: action.stops_wsl(),
            stops_docker: matches!(action, Action::Compact { docker: true, .. })
                || action.stops_wsl(),
            detail: None,
            available: !matches!(action, Action::Unavailable),
            unavailable_reason: None,
            action,
        }
    }

    /// Shown, never offered, with the reason the webview names.
    fn because(mut self, reason: UnavailableReason) -> Self {
        self.available = false;
        self.default_selected = false;
        self.unavailable_reason = Some(reason);
        self
    }

    fn safe_default(mut self) -> Self {
        self.safety = Safety::Safe;
        self.default_selected = true;
        self
    }

    fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    fn size(mut self, size: Option<u64>) -> Self {
        self.size = size;
        self
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Something that is stopped while WSL is down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockerKind {
    WslDistro,
    DockerDesktop,
    Container,
    ScheduledTask,
    GithubRunner,
    HyperVVm,
    VscodeServer,
}

impl BlockerKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::WslDistro => "wslDistro",
            Self::DockerDesktop => "dockerDesktop",
            Self::Container => "container",
            Self::ScheduledTask => "scheduledTask",
            Self::GithubRunner => "githubRunner",
            Self::HyperVVm => "hyperVVm",
            Self::VscodeServer => "vscodeServer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    pub kind: BlockerKind,
    pub name: String,
    pub restart_after: bool,
}

/// The analysis of one drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub mount: String,
    pub role: DriveRole,
    pub role_reasons: Vec<&'static str>,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub items: Vec<PlannedItem>,
    pub blockers: Vec<Blocker>,
}

impl Plan {
    /// Items grouped in the wizard's order, groups without items dropped.
    #[must_use]
    pub fn groups(&self) -> Vec<(Group, Vec<&PlannedItem>)> {
        let mut out: Vec<(Group, Vec<&PlannedItem>)> = Vec::new();
        for item in &self.items {
            match out.iter_mut().find(|(g, _)| *g == item.group) {
                Some((_, list)) => list.push(item),
                None => out.push((item.group, vec![item])),
            }
        }
        out.sort_by_key(|(g, _)| *g);
        out
    }
}

/// Reports where the analysis has got to: (stage, done, total).
pub type Progress<'a> = &'a mut dyn FnMut(&'static str, u64, u64);

/// The stages, in order, for the progress bar.
pub const STAGES: [&str; 9] = [
    "role",
    "windows",
    "temp",
    "recycleBin",
    "caches",
    "artefacts",
    "docker",
    "wsl",
    "blockers",
];

/// Analyses `mount` (`C:` or `C:\`). Read-only: nothing is started,
/// stopped or deleted — a stopped distro is left stopped, so what is inside
/// it is not measured (its compaction is still offered).
#[must_use]
pub fn analyse(mount: &str, cancel: &AtomicBool, progress: Progress<'_>) -> Option<Plan> {
    let mount = drive_role::normalise_mount(mount).to_ascii_uppercase();
    let volume = crate::disk::enumerate_volumes()
        .into_iter()
        .find(|v| v.mount.eq_ignore_ascii_case(&mount))?;
    let total = STAGES.len() as u64;
    let mut step = 0_u64;
    let mut stage = |name: &'static str| {
        progress(name, step, total);
        step += 1;
    };

    stage("role");
    let disks = dc::virtual_disks();
    let vhdx: Vec<(PathBuf, bool)> = disks.iter().map(|d| (d.path.clone(), d.docker)).collect();
    let roots = probe::project_roots();
    let facts = drive_role::facts(&mount, volume.kind, &roots, &vhdx);
    let (role, role_reasons) = drive_role::classify(&facts);
    let root = PathBuf::from(format!("{mount}\\"));
    let free_bytes = super::managed::free_bytes(&root).unwrap_or(volume.available.0);
    let mut plan = Plan {
        mount: mount.clone(),
        role,
        role_reasons,
        free_bytes,
        total_bytes: volume.total.0,
        items: Vec::new(),
        blockers: Vec::new(),
    };
    if !role.offers_anything() || matches!(volume.kind, DiskKind::Network | DiskKind::Optical) {
        return Some(plan);
    }

    stage("windows");
    if role == DriveRole::System {
        plan.items.extend(windows_items());
    }
    stage("temp");
    plan.items.extend(temp_items(&mount, cancel));
    stage("recycleBin");
    plan.items.extend(recycle_items(&root));
    stage("caches");
    plan.items.extend(cache_items(&mount, cancel));
    stage("artefacts");
    if role != DriveRole::External {
        plan.items.extend(artefact_items(&mount, &roots, cancel));
    }
    let on_drive: Vec<&dc::VirtualDisk> = disks.iter().filter(|d| is_on(&d.path, &mount)).collect();
    // WSL and Docker are never stopped for a removable drive's sake: a
    // `.vhdx` there is unusual, and a USB disk pulled mid-compaction is a
    // corrupted distro.
    let offer_wsl = role != DriveRole::External && !on_drive.is_empty();
    let running = if offer_wsl {
        probe::running_distros()
    } else {
        Vec::new()
    };
    stage("docker");
    let docker_on_drive = on_drive.iter().any(|d| d.docker);
    if offer_wsl && docker_on_drive && !cancel.load(Ordering::Relaxed) {
        plan.items.extend(docker_items());
    }
    stage("wsl");
    if offer_wsl && !cancel.load(Ordering::Relaxed) {
        plan.items.extend(wsl_items(&on_drive, &running));
    }
    stage("blockers");
    if plan.items.iter().any(|i| i.stops_wsl) {
        let compacted: Vec<PathBuf> = on_drive.iter().map(|d| d.path.clone()).collect();
        plan.blockers = probe::blockers(&running, &compacted);
    }
    Some(plan)
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

fn windows_items() -> Vec<PlannedItem> {
    let mut out = vec![
        PlannedItem::new(
            Group::Windows,
            "componentStore",
            Action::Managed(ManagedTool::ComponentCleanup),
        )
        .safe_default()
        // WinSxS is mostly hard links into System32; a walk would report
        // the links as its own. Only the drive's free space tells.
        .size(None)
        .detail("dism /Online /Cleanup-Image /StartComponentCleanup"),
    ];
    for (path, kind) in candidate_locations() {
        let (handler, key) = match kind {
            CleanupKind::WindowsUpdateCache => (Handler::UpdateCleanup, "windowsUpdate"),
            CleanupKind::DeliveryOptimisation => {
                (Handler::DeliveryOptimization, "deliveryOptimization")
            }
            CleanupKind::ThumbnailCache => (Handler::ThumbnailCache, "thumbnailCache"),
            CleanupKind::CrashDump if path.ends_with("Minidump") => {
                (Handler::Minidumps, "minidumps")
            }
            CleanupKind::CrashDump if path.ends_with("MEMORY.DMP") => {
                (Handler::MemoryDump, "memoryDump")
            }
            _ => continue,
        };
        if !path.exists() || out.iter().any(|i| i.kind == key) {
            continue;
        }
        let mut item = PlannedItem::new(
            Group::Windows,
            key,
            Action::Managed(ManagedTool::DiskCleanup(handler)),
        )
        .size(super::managed::measure_location(&path))
        .detail(path.display().to_string());
        if kind.safety() == super::Safety::Safe {
            item = item.safe_default();
        }
        out.push(item);
    }
    out.extend(hibernation_item());
    out
}

fn hibernation_item() -> Option<PlannedItem> {
    let file = drive_role::windows_directory()?
        .ancestors()
        .last()?
        .join("hiberfil.sys");
    // `hiberfil.sys` is a protected system file: metadata answers even
    // where a read would be refused.
    let meta = std::fs::metadata(&file).ok();
    let size = meta.as_ref().map(std::fs::Metadata::len);
    let state = parse::assess_hiberfil(
        meta.is_some(),
        size,
        probe::hibernate_enabled(),
        probe::hibernation_supported(),
    );
    Some(match state {
        Hiberfil::Absent => return None,
        Hiberfil::InUse { size } => PlannedItem::new(
            Group::Windows,
            "hibernateOff",
            Action::Managed(ManagedTool::HibernateOff),
        )
        .size(size)
        .detail("powercfg /hibernate off — also turns off fast startup"),
        Hiberfil::Orphan {
            size,
            removable: true,
        } => PlannedItem::new(Group::Windows, "hiberfilOrphan", Action::HiberfilOrphan)
            .size(size)
            .detail(
                "Hibernation is off but the file stayed. Windows deletes it when \
                 hibernation is turned on and off again (powercfg /h on, then /h off).",
            ),
        Hiberfil::Orphan {
            size,
            removable: false,
        } => PlannedItem::new(Group::Windows, "hiberfilOrphan", Action::Unavailable)
            .size(size)
            .because(UnavailableReason::HibernationRefused)
            .detail(
                "Hibernation is off but the file stayed, and Windows refuses to turn \
                 hibernation on here (Guarded Host / virtualisation-based security), \
                 so `powercfg /h on` then `/h off` cannot remove it. It is removed by \
                 the next restart that runs without that protection.",
            ),
    })
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// How long the `%TEMP%` measurement may take before it is reported as
/// unmeasured rather than holding the whole analysis.
const TEMP_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

fn temp_items(mount: &str, cancel: &AtomicBool) -> Vec<PlannedItem> {
    let Some(temp) = std::env::var_os("TEMP").map(PathBuf::from) else {
        return Vec::new();
    };
    if !is_on(&temp, mount) || !temp.is_dir() {
        return Vec::new();
    }
    let deadline = std::time::Instant::now() + TEMP_BUDGET;
    let size = run::old_files_within(&temp, TEMP_MIN_AGE_DAYS, Some(deadline), cancel)
        .map(|(bytes, _)| bytes);
    vec![
        PlannedItem::new(Group::Temp, "tempOld", Action::TempOld(temp.clone()))
            .safe_default()
            .size(size)
            .detail(temp.display().to_string()),
    ]
}

fn recycle_items(root: &Path) -> Vec<PlannedItem> {
    match probe::recycle_bin_size(root) {
        Some((0, _)) => Vec::new(),
        size => vec![
            PlannedItem::new(
                Group::RecycleBin,
                "recycleBin",
                Action::RecycleBin(root.to_path_buf()),
            )
            .size(size.map(|(b, _)| b))
            .label(root.display().to_string()),
        ],
    }
}

fn cache_items(mount: &str, cancel: &AtomicBool) -> Vec<PlannedItem> {
    let mut out = Vec::new();
    for cache in CacheFolder::ALL {
        for path in cache.locations() {
            if !is_on(&path, mount) || !path.is_dir() {
                continue;
            }
            let mut item = PlannedItem::new(
                Group::Caches,
                cache.kind(),
                Action::DeleteCache {
                    kind: cache,
                    path: path.clone(),
                },
            )
            .size(dc::measure(&path, cancel).map(|(b, _)| b))
            .label(path.display().to_string());
            if cache.self_restoring() {
                item = item.safe_default();
            }
            out.push(item);
        }
    }
    for store in dc::pnpm_stores().into_iter().filter(|s| is_on(s, mount)) {
        out.push(
            PlannedItem::new(Group::Caches, "pnpmStore", Action::PnpmPrune(store.clone()))
                // What prune removes is only the unlinked part; the whole
                // store's size would promise far more than it gives.
                .size(None)
                .label(store.display().to_string())
                .detail("pnpm store prune — keeps every package a project links to"),
        );
    }
    out
}

fn artefact_items(mount: &str, roots: &[PathBuf], cancel: &AtomicBool) -> Vec<PlannedItem> {
    let mut found = Vec::new();
    for root in roots.iter().filter(|r| is_on(r, mount)) {
        dc::find_artefacts(root, 8, cancel, &mut found);
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup_by(|a, b| a.path == b.path);
    let mut sized: Vec<(Option<u64>, dc::Found)> = found
        .into_iter()
        .map(|f| (dc::measure(&f.path, cancel).map(|(b, _)| b), f))
        .filter(|(size, _)| size.is_none_or(|s| s >= MIN_ARTEFACT))
        .collect();
    sized.sort_by_key(|s| std::cmp::Reverse(s.0));
    sized.truncate(MAX_ARTEFACTS);
    sized
        .into_iter()
        .map(|(size, f)| {
            let kind = match f.kind {
                ArtefactKind::CargoTarget => "rustTarget",
                ArtefactKind::NodeModules => "nodeModules",
                _ => "buildDir",
            };
            PlannedItem::new(
                Group::DevArtefacts,
                kind,
                Action::Artefact {
                    path: f.path.clone(),
                    kind: f.kind,
                },
            )
            .size(size)
            .label(f.path.display().to_string())
            .detail(format!("restore: {}", f.kind.restore()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Docker Desktop
// ---------------------------------------------------------------------------

fn docker_items() -> Vec<PlannedItem> {
    let Some(usage) = probe::docker_usage() else {
        return Vec::new();
    };
    let containers = probe::docker_containers();
    let mut out = vec![
        PlannedItem::new(Group::Docker, "dockerBuildCache", Action::DockerBuildCache)
            .safe_default()
            .size(
                usage
                    .build_cache
                    .map(|b| b.saturating_sub(BUILD_CACHE_KEEP)),
            )
            .detail("docker builder prune --keep-storage 20GB"),
    ];
    let old: Vec<&parse::DockerImage> = usage
        .images
        .iter()
        .filter(|i| i.containers == Some(0))
        .filter(|i| i.age_hours.is_some_and(|h| h >= IMAGE_MIN_AGE_HOURS))
        .collect();
    if !old.is_empty() {
        let size = old.iter().map(|i| i.unique).sum::<Option<u64>>();
        out.push(
            PlannedItem::new(Group::Docker, "dockerOldImages", Action::DockerOldImages)
                .safe_default()
                .size(size)
                .label(format!("{}", old.len()))
                .detail("docker image prune --all --filter until=24h"),
        );
    }
    for c in removable_containers(&containers, &usage.volumes) {
        let size = usage
            .container_sizes
            .iter()
            .find(|(id, _)| c.id.starts_with(id.as_str()) || id.starts_with(c.id.as_str()))
            .and_then(|(_, s)| *s);
        out.push(
            PlannedItem::new(
                Group::Docker,
                "dockerStoppedContainer",
                Action::DockerRemoveContainer {
                    id: c.id.clone(),
                    name: c.name.clone(),
                },
            )
            .size(size)
            .label(c.name.clone())
            .detail(format!("docker rm {}", c.name)),
        );
    }
    let unused = unused_volumes(&usage.volumes, &containers);
    let anon: Vec<&&parse::DockerVolume> = unused.iter().filter(|v| v.anonymous).collect();
    if !anon.is_empty() {
        out.push(
            PlannedItem::new(
                Group::Docker,
                "dockerAnonVolumes",
                Action::DockerAnonVolumes(anon.iter().map(|v| v.name.clone()).collect()),
            )
            .size(anon.iter().map(|v| v.size).sum::<Option<u64>>())
            .label(format!("{}", anon.len()))
            .detail("anonymous volumes no container mounts"),
        );
    }
    for v in unused.iter().filter(|v| !v.anonymous) {
        let mut item = PlannedItem::new(
            Group::Docker,
            "dockerNamedVolume",
            Action::DockerNamedVolume(v.name.clone()),
        )
        .size(v.size)
        .label(v.name.clone())
        .detail(format!(
            "named volume {} — may hold a database; no container uses it now",
            v.name
        ));
        item.safety = Safety::Danger;
        out.push(item);
    }
    out
}

// ---------------------------------------------------------------------------
// WSL
// ---------------------------------------------------------------------------

fn wsl_items(on_drive: &[&dc::VirtualDisk], running: &[String]) -> Vec<PlannedItem> {
    let mut out = Vec::new();
    let mut compactions = Vec::new();
    // Asked once, and only when a sparse item is about to be planned.
    let refusal = std::cell::OnceCell::new();
    for disk in on_drive {
        let file = std::fs::metadata(&disk.path).ok().map(|m| m.len());
        let distro = disk.distro.clone();
        let is_running = |d: &str| running.iter().any(|r| r.eq_ignore_ascii_case(d));
        let used = if disk.docker {
            is_running("docker-desktop")
                .then(|| probe::df_used("docker-desktop", "/mnt/docker-desktop-disk"))
                .flatten()
        } else if let Some(d) = distro.as_deref().filter(|d| is_running(d)) {
            let measure = probe::measure_distro(d);
            out.extend(distro_items(d, &measure));
            measure.root_used
        } else {
            None
        };
        let label = distro
            .clone()
            .unwrap_or_else(|| "docker-desktop".to_owned());
        let mut compact = PlannedItem::new(
            Group::Compaction,
            "compactVhd",
            Action::Compact {
                path: disk.path.clone(),
                docker: disk.docker,
            },
        )
        .size(file.zip(used).map(|(f, u)| f.saturating_sub(u)))
        .label(label)
        .detail(disk.path.display().to_string());
        // Compaction rewrites the disk in place; nothing is deleted.
        compact.irreversible = false;
        compactions.push(compact);
        // Docker Desktop's own distros are left to Docker Desktop, which has
        // its own disk settings and recreates them on update.
        if let Some(d) = distro
            .filter(|d| !d.starts_with("docker-desktop"))
            .filter(|_| !probe::is_sparse(&disk.path))
        {
            let refused = *refusal.get_or_init(|| sparse_refusal(probe::wsl_version()));
            out.push(sparse_item(d, &disk.path, refused));
        }
    }
    out.extend(compactions);
    out
}

/// The sparse row for one distro: offered, or shown with why it is not.
fn sparse_item(distro: String, path: &Path, refused: Option<UnavailableReason>) -> PlannedItem {
    let item = match refused {
        None => PlannedItem::new(
            Group::Sparse,
            "sparseVhd",
            Action::Sparse {
                distro: distro.clone(),
                path: path.to_path_buf(),
            },
        ),
        Some(reason) => {
            PlannedItem::new(Group::Sparse, "sparseVhd", Action::Unavailable).because(reason)
        }
    };
    let mut item = item.label(distro).detail(format!(
        "wsl --manage <distro> --set-sparse true — {}",
        path.display()
    ));
    item.irreversible = false;
    item
}

fn distro_items(distro: &str, m: &parse::DistroMeasure) -> Vec<PlannedItem> {
    let task = |task: DistroTask| Action::Distro {
        distro: distro.to_owned(),
        task,
    };
    let mut out = Vec::new();
    let mut push = |item: PlannedItem| out.push(item.label(distro.to_owned()));
    if let Some(size) = m.apt.filter(|s| *s > 0) {
        push(
            PlannedItem::new(Group::Wsl, "wslAptCache", task(DistroTask::Apt))
                .safe_default()
                .size(Some(size))
                .detail("apt-get clean"),
        );
    }
    if let Some(size) = m.dnf.filter(|s| *s > 0) {
        push(
            PlannedItem::new(Group::Wsl, "wslAptCache", task(DistroTask::Dnf))
                .safe_default()
                .size(Some(size))
                .detail("dnf clean all"),
        );
    }
    if let Some(journal) = m.journal.filter(|j| *j > JOURNAL_KEEP) {
        push(
            PlannedItem::new(Group::Wsl, "wslJournal", task(DistroTask::Journal))
                .safe_default()
                .size(Some(journal - JOURNAL_KEEP))
                .detail("journalctl --vacuum-size=100M"),
        );
    }
    if let Some(size) = m.tmp_old.filter(|s| *s > 0) {
        push(
            PlannedItem::new(Group::Wsl, "wslTmp", task(DistroTask::Tmp))
                .safe_default()
                .size(Some(size))
                .detail("/tmp files older than 2 days"),
        );
    }
    let folders = [
        (&m.caches, FolderKind::UserCache, "wslUserCache"),
        (&m.targets, FolderKind::RustTarget, "wslRustTarget"),
        (&m.builds, FolderKind::Build, "wslBuildDir"),
    ];
    for (list, kind, key) in folders {
        for (size, path) in list {
            push(
                PlannedItem::new(
                    Group::Wsl,
                    key,
                    task(DistroTask::Folder {
                        path: path.clone(),
                        kind,
                    }),
                )
                .size(Some(*size))
                .detail(path.clone()),
            );
        }
    }
    if let Some(size) = m.docker {
        push(
            PlannedItem::new(Group::Wsl, "wslDockerPrune", task(DistroTask::Docker))
                .safe_default()
                .size(Some(size))
                .detail("docker builder prune --keep-storage 20GB; docker image prune -a --filter until=24h"),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_come_out_in_the_wizard_order_and_empty_groups_are_dropped() {
        let plan = Plan {
            mount: "C:".into(),
            role: DriveRole::System,
            role_reasons: Vec::new(),
            free_bytes: 0,
            total_bytes: 0,
            items: vec![
                PlannedItem::new(Group::Compaction, "compactVhd", Action::Unavailable),
                PlannedItem::new(
                    Group::Temp,
                    "tempOld",
                    Action::TempOld(PathBuf::from("C:\\t")),
                ),
                PlannedItem::new(
                    Group::Temp,
                    "tempOld",
                    Action::TempOld(PathBuf::from("C:\\u")),
                ),
            ],
            blockers: Vec::new(),
        };
        let groups: Vec<(&str, usize)> = plan
            .groups()
            .iter()
            .map(|(g, items)| (g.key(), items.len()))
            .collect();
        assert_eq!(groups, [("temp", 2), ("compaction", 1)]);
    }

    #[test]
    fn compaction_and_sparse_stop_wsl_and_docker_and_need_elevation_but_a_prune_does_not() {
        let compact = PlannedItem::new(
            Group::Compaction,
            "compactVhd",
            Action::Compact {
                path: PathBuf::from("C:\\x.vhdx"),
                docker: false,
            },
        );
        assert!(compact.stops_wsl && compact.stops_docker && compact.needs_elevation);
        let prune = PlannedItem::new(Group::Docker, "dockerBuildCache", Action::DockerBuildCache);
        assert!(!prune.stops_wsl && !prune.stops_docker && !prune.needs_elevation);
    }

    #[test]
    fn an_unavailable_item_is_shown_but_marked_unavailable_and_never_preselected() {
        let item = PlannedItem::new(Group::Windows, "hiberfilOrphan", Action::Unavailable)
            .size(Some(76_700_000_000));
        assert!(!item.available);
        assert!(!item.default_selected);
        assert_eq!(item.size, Some(76_700_000_000));
    }

    #[test]
    fn a_wsl_from_2_5_6_on_gets_the_sparse_item_shown_but_never_offered() {
        let path = Path::new(r"C:\wsl\ext4.vhdx");
        let plan =
            |v: [u32; 4]| sparse_item("Ubuntu".into(), path, sparse_refusal(Some(WslVersion(v))));
        for refusing in [[2, 5, 6, 0], [2, 5, 7, 0], [2, 7, 13, 0], [3, 0, 0, 0]] {
            let item = plan(refusing);
            assert!(!item.available, "{refusing:?}");
            assert_eq!(
                item.unavailable_reason,
                Some(UnavailableReason::SparseDisabledByWsl)
            );
            assert_eq!(item.action, Action::Unavailable);
            assert!(!item.stops_wsl && !item.needs_elevation && !item.default_selected);
        }
        for allowing in [[2, 0, 0, 0], [2, 4, 13, 0], [2, 5, 5, 0]] {
            let item = plan(allowing);
            assert!(item.available, "{allowing:?}");
            assert_eq!(item.unavailable_reason, None);
            assert!(matches!(item.action, Action::Sparse { .. }));
            assert!(item.stops_wsl && item.needs_elevation);
        }
    }

    #[test]
    fn an_unreadable_or_too_old_wsl_version_is_never_taken_as_allowing_sparse() {
        assert_eq!(
            sparse_refusal(None),
            Some(UnavailableReason::WslVersionUnknown)
        );
        assert_eq!(
            sparse_refusal(Some(WslVersion([1, 2, 5, 0]))),
            Some(UnavailableReason::SparseNeedsNewerWsl)
        );
        let item = sparse_item(
            "Ubuntu".into(),
            Path::new(r"C:\x.vhdx"),
            sparse_refusal(None),
        );
        assert!(!item.available);
    }

    #[test]
    fn distro_items_keep_the_journal_floor_and_label_every_row_with_the_distro() {
        let m = parse::DistroMeasure {
            journal: Some(JOURNAL_KEEP + 5),
            apt: Some(0),
            targets: vec![(2_000_000_000, "/home/u/scrin-target".into())],
            ..parse::DistroMeasure::default()
        };
        let items = distro_items("Ubuntu-24.04", &m);
        let kinds: Vec<&str> = items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["wslJournal", "wslRustTarget"]);
        assert_eq!(items[0].size, Some(5));
        assert!(
            items
                .iter()
                .all(|i| i.label.as_deref() == Some("Ubuntu-24.04"))
        );
        assert_eq!(items[1].safety, Safety::Review);
        assert!(!items[1].default_selected);
    }

    #[test]
    fn only_pure_download_caches_are_selected_by_default() {
        let defaults: Vec<CacheFolder> = CacheFolder::ALL
            .into_iter()
            .filter(|c| c.self_restoring())
            .collect();
        assert_eq!(
            defaults,
            [
                CacheFolder::Npm,
                CacheFolder::PnpmMetadata,
                CacheFolder::NugetHttp
            ]
        );
    }
}
