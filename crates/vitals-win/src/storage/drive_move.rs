//! Guided move of a WSL distro's disk, or Docker Desktop's, to another drive.
//!
//! The clean-up wizard can shrink a virtual disk, but a drive that is nearly
//! full because a 200 GB distro lives on it needs the disk somewhere else.
//! Two very different paths, chosen in ADR-0040:
//!
//! - **A WSL distro** moves with `wsl --manage <distro> --move <folder>`
//!   (WSL 2.3.11 and later). WSL itself copies the `.vhdx`, rewrites the
//!   registration's `BasePath` and rolls both back if anything fails, and it
//!   keeps the registration — default user, Terminal profile, shortcuts — which
//!   an export/unregister/import would lose. It runs as the user: the service
//!   impersonates the caller, so no elevation is needed.
//! - **Docker Desktop's data disk** is moved by Docker Desktop (Settings >
//!   Resources > Advanced > Disk image location). Docker documents no CLI or
//!   settings key for it, and moving its distros behind its back is the
//!   failure its own release notes keep fixing, so Vitals guides the user to
//!   that setting and checks the result afterwards. Nothing here writes to
//!   Docker's files.
//!
//! Measured or `None`: a target whose free space or a disk whose size could
//! not be read is refused with that reason, never assumed to fit.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vitals_core::metrics::DiskKind;

use super::devclean::{self as dc, hidden, system32};
use super::drive_cleanup::parse::{self as dcp, WslVersion};
use super::drive_cleanup::{Blocker, BlockerKind, probe};
use super::drive_role::{self, drive_of, is_on};
use super::managed;

/// The first WSL with `wsl --manage <distro> --move`
/// (<https://github.com/microsoft/WSL/releases/tag/2.3.11>).
pub const MOVE_FIRST_WSL: WslVersion = WslVersion([2, 3, 11, 0]);
/// The first Docker Desktop whose disk image move to another drive works:
/// earlier releases always failed it (robocopy's exit code 1 was read as a
/// failure, docker/desktop-feedback#548, fixed in 4.92.0).
pub const DOCKER_CROSS_DRIVE_FIRST: [u32; 3] = [4, 92, 0];
/// Head room on top of the disk itself: NTFS needs space for its own
/// metadata, and a target filled to the last byte is the next full drive.
pub const MOVE_MARGIN: u64 = 2 * 1024 * 1024 * 1024;
/// Where a moved distro goes on the target drive, one folder per distro.
pub const WSL_FOLDER: &str = r"Vitals\WSL";
/// The folder suggested for Docker Desktop's disk image location.
pub const DOCKER_FOLDER: &str = r"Vitals\Docker";

const LXSS_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss";

// ---------------------------------------------------------------------------
// Registrations
// ---------------------------------------------------------------------------

/// One distro registration under `HKCU\...\Lxss\{guid}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub guid: String,
    pub name: String,
    /// `BasePath`, with any `\\?\` prefix removed.
    pub base_path: PathBuf,
    /// `VhdFileName`; `ext4.vhdx` when absent (older registrations).
    pub vhd_file: String,
    /// `Version` is the filesystem layout, `Flags & 0x8` is WSL 2.
    pub wsl2: bool,
}

impl Registration {
    #[must_use]
    pub fn vhd_path(&self) -> PathBuf {
        self.base_path.join(&self.vhd_file)
    }
}

/// `LXSS_DISTRO_FLAGS_VM_MODE`: the distro runs in the WSL 2 VM.
const FLAG_VM_MODE: u32 = 0x8;

/// Parses `reg query ...\Lxss /s` into registrations. A key without a name
/// or a base path is skipped rather than guessed.
#[must_use]
pub fn parse_registrations(text: &str) -> Vec<Registration> {
    #[derive(Default)]
    struct Partial {
        guid: Option<String>,
        name: Option<String>,
        base: Option<PathBuf>,
        vhd: Option<String>,
        flags: Option<u32>,
    }
    let mut out = Vec::new();
    let mut cur = Partial::default();
    let flush = |cur: &mut Partial, out: &mut Vec<Registration>| {
        let p = std::mem::take(cur);
        if let (Some(guid), Some(name), Some(base_path)) = (p.guid, p.name, p.base) {
            out.push(Registration {
                guid,
                name,
                base_path,
                vhd_file: p.vhd.unwrap_or_else(|| "ext4.vhdx".to_owned()),
                wsl2: p.flags.is_some_and(|f| f & FLAG_VM_MODE != 0),
            });
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("HKEY_") {
            flush(&mut cur, &mut out);
            cur.guid = line
                .rsplit('\\')
                .next()
                .filter(|g| g.starts_with('{') && g.ends_with('}'))
                .map(str::to_owned);
            continue;
        }
        let mut parts = line.splitn(3, "    ").map(str::trim);
        let (Some(key), Some(_kind), Some(value)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        match key {
            "DistributionName" => cur.name = Some(value.to_owned()),
            "BasePath" => cur.base = Some(strip_verbatim(value)),
            "VhdFileName" => cur.vhd = Some(value.to_owned()),
            "Flags" => {
                cur.flags = u32::from_str_radix(value.trim_start_matches("0x"), 16).ok();
            }
            _ => {}
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn strip_verbatim(value: &str) -> PathBuf {
    PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(value))
}

/// The current registrations, read with `reg query` (read-only).
#[must_use]
pub fn registrations() -> Vec<Registration> {
    let Ok(out) = hidden(system32("reg.exe"))
        .args(["query", LXSS_KEY, "/s"])
        .output()
    else {
        return Vec::new();
    };
    parse_registrations(&String::from_utf8_lossy(&out.stdout))
}

/// Two paths name the same folder, ignoring case, a `\\?\` prefix and a
/// trailing separator (WSL writes `BasePath` without one).
#[must_use]
pub fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy();
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
        s.trim_end_matches(['\\', '/']).to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

// ---------------------------------------------------------------------------
// Targets
// ---------------------------------------------------------------------------

/// Why a drive cannot take the disk. The webview names each in both
/// languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRefusal {
    /// The disk is already on this drive.
    SameDrive,
    /// Removable, network or optical: a USB disk pulled out mid-session is a
    /// corrupted distro, and WSL cannot attach a disk on a network share.
    NotFixed,
    /// Not NTFS. WSL documents its disks on NTFS only; FAT32 cannot hold a
    /// file over 4 GB at all, and a Dev Drive (`ReFS`) is untested by WSL.
    NotNtfs,
    /// Less free space than the disk plus 10 % and [`MOVE_MARGIN`].
    TooLittleSpace,
    /// The disk's size could not be read, so whether it fits is unknown.
    SizeUnknown,
    /// The folder the disk would land in is NTFS-compressed or encrypted;
    /// WSL refuses such disks ("must be uncompressed and unencrypted").
    CompressedOrEncrypted,
    /// The target folder already holds a disk file of that name.
    Occupied,
}

impl TargetRefusal {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::SameDrive => "sameDrive",
            Self::NotFixed => "notFixed",
            Self::NotNtfs => "notNtfs",
            Self::TooLittleSpace => "tooLittleSpace",
            Self::SizeUnknown => "sizeUnknown",
            Self::CompressedOrEncrypted => "compressedOrEncrypted",
            Self::Occupied => "occupied",
        }
    }
}

/// What is known about a candidate drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetFacts {
    /// `E:`.
    pub mount: String,
    pub label: Option<String>,
    pub kind: DiskKind,
    pub file_system: Option<String>,
    pub free: u64,
    pub total: u64,
}

/// Bytes a drive must have free to take a disk of `size`.
#[must_use]
pub const fn required_bytes(size: u64) -> u64 {
    size.saturating_add(size / 10).saturating_add(MOVE_MARGIN)
}

/// Whether `target` can take a disk of `size` that lives on `source_mount`.
/// Checked in the order a user would want to hear it: the drive itself
/// first, then whether the disk fits.
#[must_use]
pub fn assess_target(
    source_mount: &str,
    size: Option<u64>,
    target: &TargetFacts,
) -> Option<TargetRefusal> {
    let source = drive_role::normalise_mount(source_mount);
    if target.mount.eq_ignore_ascii_case(source) {
        return Some(TargetRefusal::SameDrive);
    }
    if matches!(
        target.kind,
        DiskKind::Removable | DiskKind::Network | DiskKind::Optical
    ) {
        return Some(TargetRefusal::NotFixed);
    }
    if !target
        .file_system
        .as_deref()
        .is_some_and(|fs| fs.eq_ignore_ascii_case("NTFS"))
    {
        return Some(TargetRefusal::NotNtfs);
    }
    let Some(size) = size else {
        return Some(TargetRefusal::SizeUnknown);
    };
    (target.free < required_bytes(size)).then_some(TargetRefusal::TooLittleSpace)
}

/// A candidate drive with its verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub facts: TargetFacts,
    pub refusal: Option<TargetRefusal>,
}

/// Every mounted volume as a target fact. Network shares are listed too, so
/// the picker can say why they are not offered.
#[must_use]
pub fn target_facts() -> Vec<TargetFacts> {
    crate::disk::enumerate_volumes()
        .into_iter()
        .map(|v| {
            let root = PathBuf::from(format!("{}\\", v.mount));
            TargetFacts {
                free: managed::free_bytes(&root).unwrap_or(v.available.0),
                mount: v.mount,
                label: v.label,
                kind: v.kind,
                file_system: v.file_system,
                total: v.total.0,
            }
        })
        .collect()
}

/// Distro names WSL accepts (`^[a-zA-Z0-9._-]{1,64}$`); anything else is
/// refused before it becomes part of a path.
#[must_use]
pub fn is_safe_distro_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && name != "."
        && name != ".."
}

/// `E:\Vitals\WSL\Ubuntu`.
#[must_use]
pub fn wsl_target_folder(mount: &str, distro: &str) -> Option<PathBuf> {
    let mount = drive_role::normalise_mount(mount);
    (is_safe_distro_name(distro) && drive_of(Path::new(mount)).is_some())
        .then(|| PathBuf::from(format!("{mount}\\{WSL_FOLDER}\\{distro}")))
}

/// `E:\Vitals\Docker`.
#[must_use]
pub fn docker_target_folder(mount: &str) -> Option<PathBuf> {
    let mount = drive_role::normalise_mount(mount);
    drive_of(Path::new(mount)).map(|_| PathBuf::from(format!("{mount}\\{DOCKER_FOLDER}")))
}

/// The arguments for `wsl.exe`. Each path stays one argument: `Command`
/// quotes it for `CreateProcess`, so a folder with spaces is never split.
#[must_use]
pub fn move_args(distro: &str, folder: &Path) -> Vec<OsString> {
    vec![
        "--manage".into(),
        distro.into(),
        "--move".into(),
        folder.as_os_str().to_owned(),
    ]
}

/// The command as a person would type it, for the report and the UI.
#[must_use]
pub fn display_command(args: &[OsString]) -> String {
    std::iter::once("wsl".to_owned())
        .chain(args.iter().map(|a| {
            let a = a.to_string_lossy();
            if a.contains([' ', '\t']) {
                format!("\"{a}\"")
            } else {
                a.into_owned()
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether the nearest existing folder on the way to `folder` is compressed
/// or encrypted (new files inherit both).
fn compressed_or_encrypted(folder: &Path) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const COMPRESSED: u32 = 0x800;
    const ENCRYPTED: u32 = 0x4000;
    folder
        .ancestors()
        .find_map(|p| std::fs::metadata(p).ok())
        .is_some_and(|m| m.file_attributes() & (COMPRESSED | ENCRYPTED) != 0)
}

/// The full verdict for one target folder: the drive, then the folder.
fn assess_folder(
    source_mount: &str,
    size: Option<u64>,
    facts: &TargetFacts,
    folder: Option<&Path>,
    vhd_file: &str,
) -> Option<TargetRefusal> {
    assess_target(source_mount, size, facts).or_else(|| {
        let folder = folder?;
        if folder.join(vhd_file).exists() {
            Some(TargetRefusal::Occupied)
        } else {
            compressed_or_encrypted(folder).then_some(TargetRefusal::CompressedOrEncrypted)
        }
    })
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// Why a disk is shown but cannot be moved from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveUnavailable {
    /// This WSL predates `--manage --move`.
    WslTooOld,
    /// `wsl --version` could not be read; a step that may be refused is not
    /// offered.
    WslVersionUnknown,
    /// A WSL 1 distro has no `.vhdx` to move.
    Wsl1,
    /// The name does not match WSL's own rule, so it never becomes a path.
    UnsafeName,
}

impl MoveUnavailable {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::WslTooOld => "wslTooOld",
            Self::WslVersionUnknown => "wslVersionUnknown",
            Self::Wsl1 => "wsl1",
            Self::UnsafeName => "unsafeName",
        }
    }
}

/// Whether this WSL can move a distro.
#[must_use]
pub fn move_refusal(version: Option<WslVersion>) -> Option<MoveUnavailable> {
    match version {
        None => Some(MoveUnavailable::WslVersionUnknown),
        Some(v) if v < MOVE_FIRST_WSL => Some(MoveUnavailable::WslTooOld),
        Some(_) => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskOwner {
    Distro,
    DockerDesktop,
}

impl DiskOwner {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Distro => "distro",
            Self::DockerDesktop => "dockerDesktop",
        }
    }
}

/// One disk on the drive, with where it could go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovableDisk {
    pub owner: DiskOwner,
    /// The distro name, or `docker-desktop`.
    pub name: String,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub running: bool,
    pub unavailable: Option<MoveUnavailable>,
    pub targets: Vec<Target>,
    /// What stops while it moves (a distro only; Docker stops itself).
    pub blockers: Vec<Blocker>,
}

/// Docker Desktop's disk, as far as can be read without asking Docker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerDisk {
    pub path: Option<PathBuf>,
    pub size: Option<u64>,
    /// `CustomWslDistroDir` from `settings-store.json`: the folder Docker
    /// Desktop's "Disk image location" setting points at.
    pub location: Option<PathBuf>,
    pub version: Option<[u32; 3]>,
    pub installed: bool,
}

impl DockerDisk {
    /// `Some(false)` when the installed release fails cross-drive moves.
    #[must_use]
    pub fn cross_drive_ok(&self) -> Option<bool> {
        self.version.map(|v| v >= DOCKER_CROSS_DRIVE_FIRST)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePlan {
    pub mount: String,
    pub wsl_version: Option<WslVersion>,
    pub disks: Vec<MovableDisk>,
    pub docker: DockerDisk,
}

/// What would stop while `distro` moves: only that distro (WSL refuses to
/// move a running one, and nothing else needs to stop), the runners inside
/// it, and a VS Code server that will reconnect by itself.
#[must_use]
pub fn distro_blockers(distro: &str, running: bool) -> Vec<Blocker> {
    if !running {
        return Vec::new();
    }
    let mut out = vec![Blocker {
        kind: BlockerKind::WslDistro,
        name: distro.to_owned(),
        restart_after: true,
    }];
    if let Some(units) = probe::in_distro(distro, RUNNER_QUERY) {
        out.extend(
            dcp::parse_runner_units(&units)
                .into_iter()
                .map(|unit| Blocker {
                    kind: BlockerKind::GithubRunner,
                    name: format!("{distro}: {unit}"),
                    restart_after: true,
                }),
        );
    }
    if probe::in_distro(distro, "pgrep -f vscode-server >/dev/null && echo yes")
        .is_some_and(|t| t.trim() == "yes")
    {
        out.push(Blocker {
            kind: BlockerKind::VscodeServer,
            name: distro.to_owned(),
            restart_after: false,
        });
    }
    out
}

const RUNNER_QUERY: &str = "systemctl list-units --type=service --state=running --no-legend --plain 'actions.runner*' 2>/dev/null";

/// The disks on `mount` and where each could go. Read-only: nothing is
/// stopped or moved, and a stopped distro is not started.
#[must_use]
pub fn plan(mount: &str) -> Option<MovePlan> {
    let mount = drive_role::normalise_mount(mount).to_ascii_uppercase();
    let facts = target_facts();
    facts
        .iter()
        .find(|f| f.mount.eq_ignore_ascii_case(&mount))?;
    let version = probe::wsl_version();
    let running = probe::running_distros();
    let is_running = |d: &str| running.iter().any(|r| r.eq_ignore_ascii_case(d));
    let mut disks = Vec::new();
    for reg in registrations() {
        if reg.name.starts_with("docker-desktop") {
            continue;
        }
        let path = reg.vhd_path();
        if !is_on(&path, &mount) {
            continue;
        }
        let size = std::fs::metadata(&path).ok().map(|m| m.len());
        let unavailable = if reg.wsl2 {
            move_refusal(version)
        } else {
            Some(MoveUnavailable::Wsl1)
        }
        .or_else(|| (!is_safe_distro_name(&reg.name)).then_some(MoveUnavailable::UnsafeName));
        let targets = facts
            .iter()
            .map(|f| {
                let folder = wsl_target_folder(&f.mount, &reg.name);
                Target {
                    refusal: assess_folder(&mount, size, f, folder.as_deref(), &reg.vhd_file),
                    facts: f.clone(),
                }
            })
            .collect();
        let up = is_running(&reg.name);
        disks.push(MovableDisk {
            owner: DiskOwner::Distro,
            blockers: if unavailable.is_none() {
                distro_blockers(&reg.name, up)
            } else {
                Vec::new()
            },
            name: reg.name,
            path,
            size,
            running: up,
            unavailable,
            targets,
        });
    }
    let docker = docker_disk();
    if let Some(path) = docker.path.as_ref().filter(|p| is_on(p, &mount)) {
        disks.push(MovableDisk {
            owner: DiskOwner::DockerDesktop,
            name: "docker-desktop".to_owned(),
            path: path.clone(),
            size: docker.size,
            running: is_running("docker-desktop"),
            unavailable: None,
            targets: facts
                .iter()
                .map(|f| Target {
                    refusal: assess_folder(
                        &mount,
                        docker.size,
                        f,
                        docker_target_folder(&f.mount).as_deref(),
                        "disk\\docker_data.vhdx",
                    ),
                    facts: f.clone(),
                })
                .collect(),
            blockers: Vec::new(),
        });
    }
    Some(MovePlan {
        mount,
        wsl_version: version,
        disks,
        docker,
    })
}

// ---------------------------------------------------------------------------
// Docker Desktop
// ---------------------------------------------------------------------------

/// `CustomWslDistroDir` from Docker Desktop's `settings-store.json`.
#[must_use]
pub fn parse_custom_wsl_dir(json: &str) -> Option<PathBuf> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value
        .get("CustomWslDistroDir")?
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(strip_verbatim)
}

/// `4.95.0` from a `DisplayVersion` line of `reg query`.
#[must_use]
pub fn parse_display_version(text: &str) -> Option<[u32; 3]> {
    let line = text.lines().find(|l| l.contains("DisplayVersion"))?;
    let token = line.split_whitespace().last()?;
    let mut parts = token.split('.').map(str::parse::<u32>);
    let major = parts.next()?.ok()?;
    let minor = parts.next()?.ok()?;
    let patch = parts.next().and_then(Result::ok).unwrap_or(0);
    Some([major, minor, patch])
}

fn docker_version() -> Option<[u32; 3]> {
    ["HKCU", "HKLM"].into_iter().find_map(|hive| {
        let key =
            format!(r"{hive}\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Docker Desktop");
        let (ok, text) = dc::run_tool(
            &system32("reg.exe"),
            &["query", &key, "/v", "DisplayVersion"],
            None,
        );
        ok.then(|| parse_display_version(&text)).flatten()
    })
}

/// The data disk under a disk image location folder.
#[must_use]
pub fn docker_data_vhdx(location: &Path) -> PathBuf {
    location.join(r"disk\docker_data.vhdx")
}

/// Where Docker Desktop keeps its data disk now: under the configured disk
/// image location, else under the default `%LOCALAPPDATA%\Docker\wsl`.
#[must_use]
pub fn docker_disk() -> DockerDisk {
    let location = std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join(r"Docker\settings-store.json"))
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|j| parse_custom_wsl_dir(&j));
    let default = std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join(r"Docker\wsl"));
    let path = location
        .iter()
        .chain(default.iter())
        .map(|l| docker_data_vhdx(l))
        .find(|p| p.is_file());
    DockerDisk {
        size: path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len()),
        path,
        location,
        version: docker_version(),
        installed: dc::docker_desktop_exe().is_some(),
    }
}

/// Starts Docker Desktop for the user, unelevated, so its settings can be
/// opened. Docker documents no link to a settings page, so it opens on its
/// dashboard.
pub fn open_docker_desktop() -> std::io::Result<()> {
    let exe = dc::docker_desktop_exe().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Docker Desktop is not installed",
        )
    })?;
    std::process::Command::new(exe).spawn().map(|_| ())
}

// ---------------------------------------------------------------------------
// Running the move
// ---------------------------------------------------------------------------

/// The stages a move reports. `moving` cannot be interrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveStage {
    Checking,
    Stopping,
    Moving,
    Verifying,
    Restarting,
}

impl MoveStage {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Stopping => "stopping",
            Self::Moving => "moving",
            Self::Verifying => "verifying",
            Self::Restarting => "restarting",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveProgress {
    pub stage: MoveStage,
    /// Bytes written at the destination so far, while moving.
    pub done: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveStatus {
    /// Moved and every check passed.
    Moved,
    /// WSL reports it moved, but a check after it did not pass.
    MovedWithProblems,
    /// Nothing moved: refused before, or WSL failed and rolled back.
    NotMoved,
}

impl MoveStatus {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Moved => "moved",
            Self::MovedWithProblems => "movedWithProblems",
            Self::NotMoved => "notMoved",
        }
    }
}

/// What was checked after the move. `None` = not checked (the move did not
/// happen), never "passed".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verification {
    pub registered_at_target: Option<bool>,
    pub file_at_target: Option<bool>,
    pub old_file_gone: Option<bool>,
    pub starts: Option<bool>,
    /// Free space gained on the source drive.
    pub freed: Option<u64>,
}

impl Verification {
    fn all_passed(&self) -> bool {
        [
            self.registered_at_target,
            self.file_at_target,
            self.old_file_gone,
            self.starts,
        ]
        .iter()
        .all(|c| *c == Some(true))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveReport {
    pub status: MoveStatus,
    /// The refusal or WSL's own message, trimmed.
    pub message: Option<String>,
    pub refusal: Option<TargetRefusal>,
    pub command: Option<String>,
    pub from: Option<PathBuf>,
    pub to: Option<PathBuf>,
    pub verification: Verification,
    pub restarted: Vec<Blocker>,
    pub restart_failed: Vec<Blocker>,
}

impl MoveReport {
    fn not_moved(message: impl Into<String>) -> Self {
        Self {
            status: MoveStatus::NotMoved,
            message: Some(message.into()),
            refusal: None,
            command: None,
            from: None,
            to: None,
            verification: Verification::default(),
            restarted: Vec::new(),
            restart_failed: Vec::new(),
        }
    }
}

/// How long a terminated distro may take to leave `--running`.
const STOP_WAIT: Duration = Duration::from_secs(30);

fn wsl_tool(args: &[&str]) -> (bool, String) {
    dc::run_tool(&system32("wsl.exe"), args, None)
}

fn stop_distro(distro: &str) -> bool {
    let _ = wsl_tool(&["--terminate", distro]);
    let deadline = Instant::now() + STOP_WAIT;
    loop {
        if !probe::running_distros()
            .iter()
            .any(|r| r.eq_ignore_ascii_case(distro))
        {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Runs `wsl --manage --move`, reporting the bytes written at the
/// destination every half second (WSL prints no progress of its own).
fn spawn_move(
    args: &[OsString],
    dest: &Path,
    total: Option<u64>,
    progress: &mut dyn FnMut(MoveProgress),
) -> (bool, String) {
    let child = hidden(system32("wsl.exe"))
        .args(args)
        .env("WSL_UTF8", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(err) => return (false, err.to_string()),
    };
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                progress(MoveProgress {
                    stage: MoveStage::Moving,
                    done: std::fs::metadata(dest).ok().map(|m| m.len()),
                    total,
                });
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(err) => return (false, err.to_string()),
        }
    }
    match child.wait_with_output() {
        Ok(out) => {
            let mut text = dcp::decode_wsl(&out.stdout);
            text.push_str(&dcp::decode_wsl(&out.stderr));
            (
                out.status.success(),
                text.trim().chars().take(600).collect(),
            )
        }
        Err(err) => (false, err.to_string()),
    }
}

/// Everything a move needs, re-read and re-checked at run time.
struct Prepared {
    from: PathBuf,
    dest: PathBuf,
    folder: PathBuf,
    args: Vec<OsString>,
    size: Option<u64>,
    source_root: PathBuf,
}

/// Every condition the plan showed, checked again minutes later. A refusal
/// here has stopped nothing.
fn prepare_move(distro: &str, target_mount: &str) -> Result<Prepared, Box<MoveReport>> {
    let refuse = |why: &str| Box::new(MoveReport::not_moved(why));
    if let Some(why) = move_refusal(probe::wsl_version()) {
        return Err(refuse(why.key()));
    }
    let Some(reg) = registrations().into_iter().find(|r| r.name == distro) else {
        return Err(refuse("the distro is no longer registered"));
    };
    if !reg.wsl2 || !is_safe_distro_name(&reg.name) {
        return Err(refuse("this distro cannot be moved from here"));
    }
    let from = reg.vhd_path();
    let Some(source_mount) = drive_of(&from) else {
        return Err(refuse("the disk is not on a drive letter"));
    };
    let size = std::fs::metadata(&from).ok().map(|m| m.len());
    let target_mount = drive_role::normalise_mount(target_mount).to_ascii_uppercase();
    let Some(facts) = target_facts()
        .into_iter()
        .find(|f| f.mount.eq_ignore_ascii_case(&target_mount))
    else {
        return Err(refuse("the target drive is not mounted"));
    };
    let Some(folder) = wsl_target_folder(&facts.mount, distro) else {
        return Err(refuse("no folder for the target"));
    };
    if let Some(refusal) = assess_folder(&source_mount, size, &facts, Some(&folder), &reg.vhd_file)
    {
        let mut r = refuse(refusal.key());
        r.refusal = Some(refusal);
        return Err(r);
    }
    Ok(Prepared {
        args: move_args(distro, &folder),
        dest: folder.join(&reg.vhd_file),
        source_root: PathBuf::from(format!("{source_mount}\\")),
        folder,
        from,
        size,
    })
}

/// Starts the distro to prove it works and sorts what came back. A distro
/// that was stopped before is stopped again: it is left as it was found.
fn restart_after(
    distro: &str,
    was_running: bool,
    runners: Vec<String>,
) -> (bool, Vec<Blocker>, Vec<Blocker>) {
    let (started, _) = wsl_tool(&["-d", distro, "--exec", "true"]);
    let mut restarted = Vec::new();
    let mut restart_failed = Vec::new();
    if !was_running {
        let _ = wsl_tool(&["--terminate", distro]);
        return (started, restarted, restart_failed);
    }
    let mut sort = |b: Blocker, ok: bool| {
        if ok {
            restarted.push(b);
        } else {
            restart_failed.push(b);
        }
    };
    sort(
        Blocker {
            kind: BlockerKind::WslDistro,
            name: distro.to_owned(),
            restart_after: true,
        },
        started,
    );
    for unit in runners {
        let active = probe::in_distro(distro, &format!("systemctl is-active '{unit}'"))
            .is_some_and(|t| t.trim() == "active");
        sort(
            Blocker {
                kind: BlockerKind::GithubRunner,
                name: format!("{distro}: {unit}"),
                restart_after: true,
            },
            active,
        );
    }
    (started, restarted, restart_failed)
}

/// Moves `distro`'s disk to `target_mount`. Every condition the plan showed
/// is checked again here, minutes later: a refusal now moves nothing.
pub fn run_move(
    distro: &str,
    target_mount: &str,
    progress: &mut dyn FnMut(MoveProgress),
) -> MoveReport {
    let say = |progress: &mut dyn FnMut(MoveProgress), stage| {
        progress(MoveProgress {
            stage,
            done: None,
            total: None,
        });
    };
    say(progress, MoveStage::Checking);
    let p = match prepare_move(distro, target_mount) {
        Ok(p) => p,
        Err(report) => return *report,
    };
    let free_before = managed::free_bytes(&p.source_root);

    let was_running = probe::running_distros()
        .iter()
        .any(|r| r.eq_ignore_ascii_case(distro));
    let runners: Vec<String> = if was_running {
        probe::in_distro(distro, RUNNER_QUERY)
            .map(|t| dcp::parse_runner_units(&t))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    say(progress, MoveStage::Stopping);
    if !stop_distro(distro) {
        return MoveReport::not_moved("the distro did not stop, so nothing was moved");
    }

    let (mut ok, mut text) = spawn_move(&p.args, &p.dest, p.size, progress);
    // Something (a scheduled task, Docker's integration) can start the
    // distro between the stop and the move; WSL then refuses with nothing
    // changed. One more stop and try is safe for that reason.
    if !ok && text.contains("DISTRO_NOT_STOPPED") && stop_distro(distro) {
        (ok, text) = spawn_move(&p.args, &p.dest, p.size, progress);
    }

    say(progress, MoveStage::Verifying);
    let after = registrations().into_iter().find(|r| r.name == distro);
    let registered = after.as_ref().map(|r| same_folder(&r.base_path, &p.folder));
    let mut verification = Verification {
        registered_at_target: registered,
        file_at_target: Some(p.dest.is_file()),
        old_file_gone: Some(!p.from.exists()),
        starts: None,
        freed: free_before
            .zip(managed::free_bytes(&p.source_root))
            .map(|(b, a)| a.saturating_sub(b)),
    };
    let moved = registered == Some(true);

    say(progress, MoveStage::Restarting);
    let (started, restarted, restart_failed) = restart_after(distro, was_running, runners);
    verification.starts = Some(started);

    let status = match (moved, verification.all_passed()) {
        (true, true) => MoveStatus::Moved,
        (true, false) => MoveStatus::MovedWithProblems,
        (false, _) => MoveStatus::NotMoved,
    };
    if status == MoveStatus::NotMoved {
        // WSL rolled back; the checks above described a move that did not
        // happen, so they are not reported as failures of one.
        verification = Verification::default();
    }
    MoveReport {
        status,
        message: (!ok || status != MoveStatus::Moved)
            .then_some(text)
            .filter(|t| !t.is_empty()),
        refusal: None,
        command: Some(display_command(&p.args)),
        from: Some(p.from),
        to: Some(p.dest),
        verification,
        restarted,
        restart_failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn drive(mount: &str, kind: DiskKind, fs: &str, free: u64) -> TargetFacts {
        TargetFacts {
            mount: mount.to_owned(),
            label: None,
            kind,
            file_system: Some(fs.to_owned()),
            free,
            total: free.saturating_mul(4),
        }
    }

    #[test]
    fn the_drive_the_disk_is_on_is_never_a_target() {
        let c = drive("C:", DiskKind::Nvme, "NTFS", 900 * GB);
        assert_eq!(
            assess_target("c:\\", Some(10 * GB), &c),
            Some(TargetRefusal::SameDrive)
        );
    }

    #[test]
    fn removable_network_and_optical_drives_are_refused_however_much_space_they_have() {
        for kind in [DiskKind::Removable, DiskKind::Network, DiskKind::Optical] {
            let d = drive("E:", kind, "NTFS", 4_000 * GB);
            assert_eq!(
                assess_target("C:", Some(GB), &d),
                Some(TargetRefusal::NotFixed),
                "{kind:?}"
            );
        }
        let fixed = drive("E:", DiskKind::Ssd, "NTFS", 4_000 * GB);
        assert_eq!(assess_target("C:", Some(GB), &fixed), None);
    }

    #[test]
    fn only_ntfs_is_accepted_and_an_unread_file_system_is_not_taken_as_ntfs() {
        for fs in ["exFAT", "FAT32", "ReFS"] {
            let d = drive("E:", DiskKind::Ssd, fs, 4_000 * GB);
            assert_eq!(
                assess_target("C:", Some(GB), &d),
                Some(TargetRefusal::NotNtfs)
            );
        }
        let mut unread = drive("E:", DiskKind::Ssd, "NTFS", 4_000 * GB);
        unread.file_system = None;
        assert_eq!(
            assess_target("C:", Some(GB), &unread),
            Some(TargetRefusal::NotNtfs)
        );
    }

    #[test]
    fn a_drive_needs_the_disk_plus_ten_percent_plus_the_margin_free() {
        let size = 100 * GB;
        let need = required_bytes(size);
        assert_eq!(need, 110 * GB + MOVE_MARGIN);
        let just = drive("E:", DiskKind::Hdd, "NTFS", need);
        assert_eq!(assess_target("C:", Some(size), &just), None);
        let short = drive("E:", DiskKind::Hdd, "NTFS", need - 1);
        assert_eq!(
            assess_target("C:", Some(size), &short),
            Some(TargetRefusal::TooLittleSpace)
        );
        // A disk exactly the size of the free space does not fit.
        let exact = drive("E:", DiskKind::Hdd, "NTFS", size);
        assert_eq!(
            assess_target("C:", Some(size), &exact),
            Some(TargetRefusal::TooLittleSpace)
        );
    }

    #[test]
    fn an_unmeasured_disk_size_is_refused_rather_than_assumed_to_fit() {
        let huge = drive("E:", DiskKind::Hdd, "NTFS", u64::MAX);
        assert_eq!(
            assess_target("C:", None, &huge),
            Some(TargetRefusal::SizeUnknown)
        );
    }

    #[test]
    fn a_folder_with_spaces_stays_one_argument_and_is_quoted_for_display() {
        let folder = Path::new(r"E:\My Disks\Vitals\WSL\Ubuntu-24.04");
        let args = move_args("Ubuntu-24.04", folder);
        assert_eq!(args.len(), 4);
        assert_eq!(
            args[3],
            OsString::from(r"E:\My Disks\Vitals\WSL\Ubuntu-24.04")
        );
        assert_eq!(
            display_command(&args),
            r#"wsl --manage Ubuntu-24.04 --move "E:\My Disks\Vitals\WSL\Ubuntu-24.04""#
        );
        let mut cmd = std::process::Command::new("wsl.exe");
        cmd.args(&args);
        assert_eq!(cmd.get_args().count(), 4);
    }

    #[test]
    fn the_target_folder_is_built_only_from_a_name_wsl_itself_would_accept() {
        assert_eq!(
            wsl_target_folder("e:\\", "Ubuntu-24.04"),
            Some(PathBuf::from(r"e:\Vitals\WSL\Ubuntu-24.04"))
        );
        for bad in ["", "..", r"..\Windows", "a b", "x/y", "name;rm", "ü"] {
            assert_eq!(wsl_target_folder("E:", bad), None, "{bad}");
        }
        assert_eq!(wsl_target_folder(r"\\server\share", "Ubuntu"), None);
    }

    #[test]
    fn registrations_carry_guid_base_path_and_disk_file_with_the_verbatim_prefix_removed() {
        let text = r"
HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Lxss
    DefaultDistribution    REG_SZ    {99474088-c7f5-4faf-ad88-058576493cf8}

HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Lxss\{0fe744dc-4600-44e4-ad08-480089a71579}
    State    REG_DWORD    0x1
    DistributionName    REG_SZ    Ubuntu-24.04
    BasePath    REG_SZ    C:\Users\v\AppData\Local\wsl\{0fe744dc-4600-44e4-ad08-480089a71579}
    Flags    REG_DWORD    0xf
    VhdFileName    REG_SZ    ext4.vhdx

HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Lxss\{37038028-0b5c-46b4-b4b4-16e99817d26e}
    DistributionName    REG_SZ    docker-desktop
    BasePath    REG_SZ    \\?\E:\Vitals\WSL\docker desktop
    Flags    REG_DWORD    0x7

HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Lxss\{11111111-0000-0000-0000-000000000000}
    DistributionName    REG_SZ    Broken
";
        let regs = parse_registrations(text);
        assert_eq!(regs.len(), 2);
        assert_eq!(regs[0].guid, "{0fe744dc-4600-44e4-ad08-480089a71579}");
        assert_eq!(regs[0].name, "Ubuntu-24.04");
        assert!(regs[0].wsl2);
        assert_eq!(
            regs[0].vhd_path(),
            PathBuf::from(
                r"C:\Users\v\AppData\Local\wsl\{0fe744dc-4600-44e4-ad08-480089a71579}\ext4.vhdx"
            )
        );
        assert_eq!(
            regs[1].base_path,
            PathBuf::from(r"E:\Vitals\WSL\docker desktop")
        );
        assert_eq!(regs[1].vhd_file, "ext4.vhdx");
        assert!(!regs[1].wsl2, "flags 0x7 lack VM mode");
    }

    #[test]
    fn a_base_path_matches_the_target_folder_whatever_its_case_prefix_or_trailing_slash() {
        let target = Path::new(r"E:\Vitals\WSL\Ubuntu");
        assert!(same_folder(Path::new(r"\\?\e:\vitals\wsl\ubuntu\"), target));
        assert!(!same_folder(Path::new(r"E:\Vitals\WSL\Ubuntu2"), target));
    }

    #[test]
    fn a_wsl_before_2_3_11_or_an_unread_version_is_never_offered_the_move() {
        assert_eq!(
            move_refusal(Some(WslVersion([2, 3, 10, 0]))),
            Some(MoveUnavailable::WslTooOld)
        );
        assert_eq!(move_refusal(None), Some(MoveUnavailable::WslVersionUnknown));
        assert_eq!(move_refusal(Some(WslVersion([2, 3, 11, 0]))), None);
        assert_eq!(move_refusal(Some(WslVersion([2, 7, 13, 0]))), None);
    }

    #[test]
    fn docker_settings_and_version_are_read_without_guessing() {
        let json = r#"{"CustomWslDistroDir":"D:\\Docker\\wsl","DiskSizeMiB":65536}"#;
        assert_eq!(
            parse_custom_wsl_dir(json),
            Some(PathBuf::from(r"D:\Docker\wsl"))
        );
        assert_eq!(parse_custom_wsl_dir(r#"{"CustomWslDistroDir":""}"#), None);
        assert_eq!(parse_custom_wsl_dir("not json"), None);
        let reg = "\r\nHKEY_CURRENT_USER\\...\\Docker Desktop\r\n    DisplayVersion    REG_SZ    4.95.0\r\n";
        assert_eq!(parse_display_version(reg), Some([4, 95, 0]));
        let old = DockerDisk {
            path: None,
            size: None,
            location: None,
            version: Some([4, 91, 3]),
            installed: true,
        };
        assert_eq!(old.cross_drive_ok(), Some(false));
        let new = DockerDisk {
            version: Some([4, 92, 0]),
            ..old.clone()
        };
        assert_eq!(new.cross_drive_ok(), Some(true));
        let unknown = DockerDisk {
            version: None,
            ..old
        };
        assert_eq!(unknown.cross_drive_ok(), None);
    }

    #[test]
    fn a_move_that_did_not_happen_has_no_checks_that_passed() {
        let v = Verification::default();
        assert!(!v.all_passed());
        let partial = Verification {
            registered_at_target: Some(true),
            file_at_target: Some(true),
            old_file_gone: Some(true),
            starts: Some(false),
            freed: Some(GB),
        };
        assert!(!partial.all_passed());
    }
}
