//! Tauri commands for the guided disk move
//! (`vitals_win::storage::drive_move`, ADR-0040).
//!
//! The plan is kept here behind a `planId`; the webview sends back the plan
//! id, a disk id from it and a target drive the plan marked eligible. It
//! never names a distro or a path of its own, and the backend checks every
//! condition again before anything stops. Shapes match
//! `features/storage/drivecleanup/move/model.ts`.
//!
//! Desktop-only: a paired phone must not move disks.

#![cfg(windows)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use tauri::Emitter as _;
use vitals_win::storage::drive_move::{
    self as dm, DiskOwner, MovableDisk, MovePlan, MoveReport, Target,
};

use crate::commands::CommandError;
use crate::drivecleanup::BlockerDto;

type CommandResult<T> = std::result::Result<T, CommandError>;

const MOVE_PROGRESS: &str = "vitals://storage/drive-move-progress";

static NEXT_PLAN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct Kept {
    id: u64,
    disks: Vec<MovableDisk>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveTargetDto {
    mount: String,
    label: Option<String>,
    file_system: Option<String>,
    free_bytes: u64,
    total_bytes: u64,
    /// `None` when the drive can take the disk.
    refusal: Option<&'static str>,
}

impl From<&Target> for MoveTargetDto {
    fn from(t: &Target) -> Self {
        Self {
            mount: t.facts.mount.clone(),
            label: t.facts.label.clone(),
            file_system: t.facts.file_system.clone(),
            free_bytes: t.facts.free,
            total_bytes: t.facts.total,
            refusal: t.refusal.map(dm::TargetRefusal::key),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovableDiskDto {
    id: String,
    owner: &'static str,
    name: String,
    path: String,
    size: Option<u64>,
    required_bytes: Option<u64>,
    running: bool,
    unavailable: Option<&'static str>,
    targets: Vec<MoveTargetDto>,
    blockers: Vec<BlockerDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerDiskDto {
    installed: bool,
    path: Option<String>,
    size: Option<u64>,
    location: Option<String>,
    version: Option<String>,
    /// `false` when the installed release fails cross-drive moves.
    cross_drive_ok: Option<bool>,
    /// The folder Vitals suggests per eligible target drive.
    suggested: Vec<SuggestedFolderDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedFolderDto {
    mount: String,
    folder: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePlanDto {
    plan_id: u64,
    mount: String,
    wsl_version: Option<String>,
    disks: Vec<MovableDiskDto>,
    docker: DockerDiskDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationDto {
    registered_at_target: Option<bool>,
    file_at_target: Option<bool>,
    old_file_gone: Option<bool>,
    starts: Option<bool>,
    freed_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReportDto {
    status: &'static str,
    message: Option<String>,
    refusal: Option<&'static str>,
    command: Option<String>,
    from: Option<String>,
    to: Option<String>,
    verification: VerificationDto,
    restarted: Vec<BlockerDto>,
    restart_failed: Vec<BlockerDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MoveProgressDto {
    stage: &'static str,
    done: Option<u64>,
    total: Option<u64>,
}

fn path_text(p: &std::path::Path) -> String {
    p.display().to_string()
}

fn docker_dto(d: &dm::DockerDisk, targets: &[Target]) -> DockerDiskDto {
    DockerDiskDto {
        installed: d.installed,
        path: d.path.as_deref().map(path_text),
        size: d.size,
        location: d.location.as_deref().map(path_text),
        version: d.version.map(|[a, b, c]| format!("{a}.{b}.{c}")),
        cross_drive_ok: d.cross_drive_ok(),
        suggested: targets
            .iter()
            .filter(|t| t.refusal.is_none())
            .filter_map(|t| {
                dm::docker_target_folder(&t.facts.mount).map(|f| SuggestedFolderDto {
                    mount: t.facts.mount.clone(),
                    folder: path_text(&f),
                })
            })
            .collect(),
    }
}

fn to_dto(plan_id: u64, plan: &MovePlan) -> MovePlanDto {
    let docker_targets = plan
        .disks
        .iter()
        .find(|d| d.owner == DiskOwner::DockerDesktop)
        .map_or(&[][..], |d| d.targets.as_slice());
    MovePlanDto {
        plan_id,
        mount: plan.mount.clone(),
        wsl_version: plan
            .wsl_version
            .map(|v| v.0.iter().map(u32::to_string).collect::<Vec<_>>().join(".")),
        disks: plan
            .disks
            .iter()
            .enumerate()
            .map(|(n, d)| MovableDiskDto {
                id: format!("{plan_id}-{n}"),
                owner: d.owner.key(),
                name: d.name.clone(),
                path: path_text(&d.path),
                size: d.size,
                required_bytes: d.size.map(dm::required_bytes),
                running: d.running,
                unavailable: d.unavailable.map(dm::MoveUnavailable::key),
                targets: d.targets.iter().map(MoveTargetDto::from).collect(),
                blockers: d.blockers.iter().map(BlockerDto::from).collect(),
            })
            .collect(),
        docker: docker_dto(&plan.docker, docker_targets),
    }
}

fn report_dto(r: &MoveReport) -> MoveReportDto {
    MoveReportDto {
        status: r.status.key(),
        message: r.message.clone(),
        refusal: r.refusal.map(dm::TargetRefusal::key),
        command: r.command.clone(),
        from: r.from.as_deref().map(path_text),
        to: r.to.as_deref().map(path_text),
        verification: VerificationDto {
            registered_at_target: r.verification.registered_at_target,
            file_at_target: r.verification.file_at_target,
            old_file_gone: r.verification.old_file_gone,
            starts: r.verification.starts,
            freed_bytes: r.verification.freed,
        },
        restarted: r.restarted.iter().map(BlockerDto::from).collect(),
        restart_failed: r.restart_failed.iter().map(BlockerDto::from).collect(),
    }
}

/// `C:` from `C:`, `c:\` or `C:/`; anything else is refused.
fn vet_mount(mount: &str) -> CommandResult<String> {
    let m = mount.trim().trim_end_matches(['\\', '/']);
    let mut chars = m.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(l), Some(':'), None) if l.is_ascii_alphabetic() => {
            Ok(format!("{}:", l.to_ascii_uppercase()))
        }
        _ => Err(CommandError::Refused {
            message: format!("{mount} is not a drive"),
        }),
    }
}

/// Resolves a run request against the kept plan: the disk must be a WSL
/// distro that the plan offered, and the target a drive it marked eligible.
fn resolve(
    plan_id: u64,
    disk_id: &str,
    target: &str,
    confirmed: bool,
) -> CommandResult<(String, String)> {
    if !confirmed {
        return Err(CommandError::Refused {
            message: "the move was not confirmed, so nothing was stopped or moved".into(),
        });
    }
    let target = vet_mount(target)?;
    let kept = KEPT.lock().map_err(|_| CommandError::Internal {
        message: "the kept plan is unavailable".into(),
    })?;
    let Some(kept) = kept.as_ref().filter(|k| k.id == plan_id) else {
        return Err(CommandError::NotFound {
            message: "that plan is no longer current; look again".into(),
        });
    };
    let disk = disk_id
        .strip_prefix(&format!("{plan_id}-"))
        .and_then(|n| n.parse::<usize>().ok())
        .and_then(|n| kept.disks.get(n))
        .ok_or_else(|| CommandError::Refused {
            message: format!("{disk_id} is not a disk of this plan"),
        })?;
    if disk.owner != DiskOwner::Distro || disk.unavailable.is_some() {
        return Err(CommandError::Refused {
            message: format!("{} cannot be moved from here", disk.name),
        });
    }
    let eligible = disk
        .targets
        .iter()
        .any(|t| t.refusal.is_none() && t.facts.mount.eq_ignore_ascii_case(&target));
    if !eligible {
        return Err(CommandError::Refused {
            message: format!("{target} cannot take {}", disk.name),
        });
    }
    Ok((disk.name.clone(), target))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// The disks on `mount` and where each could go. Read-only.
#[tauri::command]
pub async fn plan_drive_move(mount: String) -> CommandResult<MovePlanDto> {
    let mount = vet_mount(&mount)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(plan) = dm::plan(&mount) else {
            return Err(CommandError::NotFound {
                message: format!("{mount} is not a mounted drive"),
            });
        };
        let plan_id = NEXT_PLAN.fetch_add(1, Ordering::Relaxed);
        let dto = to_dto(plan_id, &plan);
        if let Ok(mut kept) = KEPT.lock() {
            *kept = Some(Kept {
                id: plan_id,
                disks: plan.disks,
            });
        }
        Ok(dto)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the planning thread did not finish: {err}"),
    })?
}

/// Moves a distro's disk to an eligible drive. Stops only that distro,
/// starts it again after, and reports what it checked.
#[tauri::command]
pub async fn run_drive_move(
    app: tauri::AppHandle,
    plan_id: u64,
    disk_id: String,
    target: String,
    confirmed: bool,
) -> CommandResult<MoveReportDto> {
    let (distro, target) = resolve(plan_id, &disk_id, &target, confirmed)?;
    // The clean-up stops the same WSL and compacts the same disks.
    let busy = crate::devclean::Busy::claim("disk move")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = busy;
        let mut progress = |p: dm::MoveProgress| {
            let _ = app.emit(
                MOVE_PROGRESS,
                MoveProgressDto {
                    stage: p.stage.key(),
                    done: p.done,
                    total: p.total,
                },
            );
        };
        let report = dm::run_move(&distro, &target, &mut progress);
        // A plan is used once: the disk is somewhere else now.
        if let Ok(mut kept) = KEPT.lock() {
            kept.take();
        }
        report_dto(&report)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the move thread did not finish: {err}"),
    })
}

/// Opens Docker Desktop so its disk image location can be changed there.
#[tauri::command]
pub async fn open_docker_desktop() -> CommandResult<()> {
    tauri::async_runtime::spawn_blocking(dm::open_docker_desktop)
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("could not start Docker Desktop: {err}"),
        })?
        .map_err(|err| CommandError::NotFound {
            message: err.to_string(),
        })
}

/// Where Docker Desktop's disk is now, read again after the user moved it.
#[tauri::command]
pub async fn check_docker_disk() -> CommandResult<DockerDiskDto> {
    tauri::async_runtime::spawn_blocking(|| docker_dto(&dm::docker_disk(), &[]))
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the check did not finish: {err}"),
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vitals_core::metrics::DiskKind;
    use vitals_win::storage::drive_move::{DockerDisk, TargetFacts, TargetRefusal};

    use super::*;

    fn target(mount: &str, refusal: Option<TargetRefusal>) -> Target {
        Target {
            facts: TargetFacts {
                mount: mount.into(),
                label: None,
                kind: DiskKind::Ssd,
                file_system: Some("NTFS".into()),
                free: 500,
                total: 1000,
            },
            refusal,
        }
    }

    fn plan() -> MovePlan {
        MovePlan {
            mount: "C:".into(),
            wsl_version: None,
            disks: vec![
                MovableDisk {
                    owner: DiskOwner::Distro,
                    name: "Ubuntu".into(),
                    path: PathBuf::from(r"C:\wsl\ext4.vhdx"),
                    size: Some(10),
                    running: true,
                    unavailable: None,
                    targets: vec![
                        target("C:", Some(TargetRefusal::SameDrive)),
                        target("E:", None),
                    ],
                    blockers: Vec::new(),
                },
                MovableDisk {
                    owner: DiskOwner::DockerDesktop,
                    name: "docker-desktop".into(),
                    path: PathBuf::from(r"C:\d\docker_data.vhdx"),
                    size: None,
                    running: false,
                    unavailable: None,
                    targets: vec![target("E:", None)],
                    blockers: Vec::new(),
                },
            ],
            docker: DockerDisk {
                path: None,
                size: None,
                location: None,
                version: Some([4, 95, 0]),
                installed: true,
            },
        }
    }

    fn keep(id: u64) -> MovePlanDto {
        let p = plan();
        let dto = to_dto(id, &p);
        *KEPT.lock().expect("lock") = Some(Kept { id, disks: p.disks });
        dto
    }

    #[test]
    fn only_a_confirmed_request_for_an_offered_distro_and_an_eligible_drive_resolves() {
        let dto = keep(9);
        let ubuntu = dto.disks[0].id.clone();
        let docker = dto.disks[1].id.clone();
        assert!(matches!(
            resolve(9, &ubuntu, "E:", false),
            Err(CommandError::Refused { .. })
        ));
        assert!(matches!(
            resolve(8, &ubuntu, "E:", true),
            Err(CommandError::NotFound { .. })
        ));
        assert!(matches!(
            resolve(9, &ubuntu, "C:", true),
            Err(CommandError::Refused { .. })
        ));
        assert!(matches!(
            resolve(9, &ubuntu, "F:", true),
            Err(CommandError::Refused { .. })
        ));
        assert!(matches!(
            resolve(9, &docker, "E:", true),
            Err(CommandError::Refused { .. })
        ));
        assert!(matches!(
            resolve(9, "9-99", "E:", true),
            Err(CommandError::Refused { .. })
        ));
        assert!(matches!(
            resolve(9, &ubuntu, r"E:\..\C:", true),
            Err(CommandError::Refused { .. })
        ));
        assert_eq!(
            resolve(9, &ubuntu, "e:\\", true).expect("resolves"),
            ("Ubuntu".to_owned(), "E:".to_owned())
        );
    }

    #[test]
    fn the_plan_serialises_with_the_contract_field_names_and_unmeasured_as_null() {
        let json = serde_json::to_value(to_dto(3, &plan())).expect("json");
        for key in ["planId", "mount", "wslVersion", "disks", "docker"] {
            assert!(json.get(key).is_some(), "{key}");
        }
        let disk = &json["disks"][0];
        for key in [
            "id",
            "owner",
            "name",
            "path",
            "size",
            "requiredBytes",
            "running",
            "unavailable",
            "targets",
            "blockers",
        ] {
            assert!(disk.get(key).is_some(), "{key}");
        }
        assert_eq!(disk["targets"][0]["refusal"], "sameDrive");
        assert!(disk["targets"][1]["refusal"].is_null());
        assert!(json["disks"][1]["size"].is_null());
        assert!(json["disks"][1]["requiredBytes"].is_null());
        assert_eq!(json["docker"]["version"], "4.95.0");
        assert_eq!(json["docker"]["crossDriveOk"], true);
        assert_eq!(
            json["docker"]["suggested"][0]["folder"],
            r"E:\Vitals\Docker"
        );
    }
}
