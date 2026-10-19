//! Tauri commands for the per-drive clean-up wizard
//! (`vitals_win::storage::drive_cleanup`).
//!
//! The analysis is kept here behind a `scanId`; each item gets an id minted
//! from it. `run_drive_cleanup` takes ids only, so the webview chooses among
//! what the analysis found and can never name a path, a volume or a distro
//! of its own. Shapes match `features/storage/drivecleanup/model.ts`.
//!
//! Desktop-only, like the developer clean-up: a paired phone must not free
//! space.

#![cfg(windows)]

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;
use tauri::Emitter as _;
use vitals_win::storage::drive_cleanup::{self as dcw, Action, Blocker, Plan};

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

const ANALYSE_PROGRESS: &str = "vitals://storage/drive-analyse-progress";
const CLEAN_PROGRESS: &str = "vitals://storage/drive-clean-progress";
/// At most this many items per confirmation.
const RUN_MAX: usize = 2_000;

static CANCEL: AtomicBool = AtomicBool::new(false);
static NEXT_SCAN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct Kept {
    id: u64,
    mount: String,
    items: HashMap<String, Action>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
// The wire contract (`PlanItem` in model.ts): each flag is a separate fact
// the UI shows, not one state spread over booleans.
pub struct PlanItemDto {
    id: String,
    kind: &'static str,
    label: Option<String>,
    size: Option<u64>,
    safety: &'static str,
    default_selected: bool,
    irreversible: bool,
    needs_elevation: bool,
    stops_wsl: bool,
    stops_docker: bool,
    detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanGroupDto {
    key: &'static str,
    items: Vec<PlanItemDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockerDto {
    kind: &'static str,
    name: String,
    restart_after: bool,
}

impl From<&Blocker> for BlockerDto {
    fn from(b: &Blocker) -> Self {
        Self {
            kind: b.kind.key(),
            name: b.name.clone(),
            restart_after: b.restart_after,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveCleanupPlanDto {
    scan_id: u64,
    mount: String,
    role: &'static str,
    role_reasons: Vec<&'static str>,
    free_bytes: u64,
    total_bytes: u64,
    groups: Vec<PlanGroupDto>,
    blockers: Vec<BlockerDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveReportItemDto {
    id: String,
    status: &'static str,
    freed: Option<u64>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveCleanupReportDto {
    items: Vec<DriveReportItemDto>,
    drive_freed_bytes: Option<u64>,
    restarted: Vec<BlockerDto>,
    restart_failed: Vec<BlockerDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalyseProgressDto {
    stage: &'static str,
    done: u64,
    total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanProgressDto {
    stage: &'static str,
    current: Option<String>,
    done: u64,
    total: u64,
    disk_index: Option<u64>,
    disk_count: Option<u64>,
}

// ---------------------------------------------------------------------------
// Plan → DTO, keeping the actions
// ---------------------------------------------------------------------------

/// Mints an id per item and keeps what it resolves to. Items that are
/// shown but cannot be done get an id too (the UI needs a key) but are
/// never kept, so they cannot be run.
fn to_dto(scan_id: u64, plan: &Plan) -> (DriveCleanupPlanDto, HashMap<String, Action>) {
    let mut kept = HashMap::new();
    let mut n = 0_usize;
    let groups = plan
        .groups()
        .into_iter()
        .map(|(group, items)| PlanGroupDto {
            key: group.key(),
            items: items
                .into_iter()
                .map(|item| {
                    let id = format!("{scan_id}-{n}");
                    n += 1;
                    if item.available {
                        kept.insert(id.clone(), item.action.clone());
                    }
                    PlanItemDto {
                        id,
                        kind: item.kind,
                        label: item.label.clone(),
                        size: item.size,
                        safety: item.safety.key(),
                        default_selected: item.default_selected && item.available,
                        irreversible: item.irreversible,
                        needs_elevation: item.needs_elevation,
                        stops_wsl: item.stops_wsl,
                        stops_docker: item.stops_docker,
                        detail: item.detail.clone(),
                    }
                })
                .collect(),
        })
        .collect();
    let dto = DriveCleanupPlanDto {
        scan_id,
        mount: plan.mount.clone(),
        role: plan.role.key(),
        role_reasons: plan.role_reasons.clone(),
        free_bytes: plan.free_bytes,
        total_bytes: plan.total_bytes,
        groups,
        blockers: plan.blockers.iter().map(BlockerDto::from).collect(),
    };
    (dto, kept)
}

/// `C:` from `C:`, `c:\` or `C:/`; anything that is not one drive letter
/// is refused before a volume is looked at.
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

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn analyse_drive_cleanup(
    app: tauri::AppHandle,
    mount: String,
) -> CommandResult<DriveCleanupPlanDto> {
    let mount = vet_mount(&mount)?;
    let busy = crate::devclean::Busy::claim("drive analysis")?;
    CANCEL.store(false, Ordering::Release);
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = busy;
        let mut progress = |stage: &'static str, done: u64, total: u64| {
            let _ = app.emit(ANALYSE_PROGRESS, AnalyseProgressDto { stage, done, total });
        };
        let Some(plan) = dcw::analyse(&mount, &CANCEL, &mut progress) else {
            return Err(CommandError::NotFound {
                message: format!("{mount} is not a mounted drive"),
            });
        };
        let scan_id = NEXT_SCAN.fetch_add(1, Ordering::Relaxed);
        let (dto, items) = to_dto(scan_id, &plan);
        if let Ok(mut kept) = KEPT.lock() {
            *kept = Some(Kept {
                id: scan_id,
                mount: plan.mount.clone(),
                items,
            });
        }
        Ok(dto)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the analysis thread did not finish: {err}"),
    })?
}

/// Flips the flag the analysis polls between stages and inside the walks.
#[tauri::command]
pub fn cancel_drive_cleanup_analysis() {
    CANCEL.store(true, Ordering::Release);
}

/// Checks a run request before anything is claimed or resolved:
/// confirmation first, then the size of the request. Kept separate so the
/// refusal paths are testable without a Tauri runtime.
fn vet_request(confirmed: bool, ids: &[String]) -> CommandResult<()> {
    if !confirmed {
        return Err(CommandError::Refused {
            message: "this frees space permanently and was not confirmed, so nothing ran".into(),
        });
    }
    if ids.is_empty() || ids.len() > RUN_MAX {
        return Err(CommandError::Refused {
            message: format!("choose between 1 and {RUN_MAX} items"),
        });
    }
    Ok(())
}

/// Resolves ids against the kept analysis. One unknown id refuses the whole
/// request: it is a bug or a forgery, and neither should run half of it.
fn resolve(scan_id: u64, ids: &[String]) -> CommandResult<(String, Vec<(String, Action)>)> {
    let kept = KEPT.lock().map_err(|_| CommandError::Internal {
        message: "the kept analysis is unavailable".into(),
    })?;
    let Some(kept) = kept.as_ref().filter(|k| k.id == scan_id) else {
        return Err(CommandError::NotFound {
            message: "that analysis is no longer current; analyse again".into(),
        });
    };
    let mut seen = std::collections::HashSet::new();
    let items = ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .map(|id| {
            kept.items
                .get(id)
                .map(|a| (id.clone(), a.clone()))
                .ok_or_else(|| CommandError::Refused {
                    message: format!("{id} is not an item of this analysis"),
                })
        })
        .collect::<CommandResult<Vec<_>>>()?;
    Ok((kept.mount.clone(), items))
}

#[tauri::command]
pub async fn run_drive_cleanup(
    app: tauri::AppHandle,
    scan_id: u64,
    ids: Vec<String>,
    confirmed: bool,
) -> CommandResult<DriveCleanupReportDto> {
    vet_request(confirmed, &ids)?;
    let (mount, items) = resolve(scan_id, &ids)?;
    // Both slots: the developer clean-up deletes from the same folders and
    // stops the same WSL; a Windows cleanup runs the same tools.
    let busy = crate::devclean::Busy::claim("drive clean-up")?;
    let managed = crate::inventory::claim_managed("a drive clean-up")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _held = (busy, managed);
        let mut progress = |p: dcw::run::RunProgress| {
            let _ = app.emit(
                CLEAN_PROGRESS,
                CleanProgressDto {
                    stage: p.stage,
                    current: p.current,
                    done: p.done,
                    total: p.total,
                    disk_index: p.disk_index,
                    disk_count: p.disk_count,
                },
            );
        };
        let report = dcw::run::run(
            &mount,
            &scan_id.to_string(),
            &items,
            &AtomicBool::new(false),
            &mut progress,
        );
        // An analysis is used once: what it measured has changed.
        if let Ok(mut kept) = KEPT.lock() {
            kept.take();
        }
        DriveCleanupReportDto {
            items: report
                .items
                .into_iter()
                .map(|r| DriveReportItemDto {
                    id: r.id,
                    status: r.status.key(),
                    freed: r.freed,
                    message: r.message,
                })
                .collect(),
            drive_freed_bytes: report.drive_freed,
            restarted: report.restarted.iter().map(BlockerDto::from).collect(),
            restart_failed: report.restart_failed.iter().map(BlockerDto::from).collect(),
        }
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the clean-up thread did not finish: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vitals_win::storage::drive_cleanup::{Group, PlannedItem, Safety};
    use vitals_win::storage::drive_role::DriveRole;

    use super::*;

    fn item(group: Group, kind: &'static str, action: Action, available: bool) -> PlannedItem {
        PlannedItem {
            group,
            kind,
            label: None,
            size: None,
            safety: Safety::Safe,
            default_selected: true,
            irreversible: true,
            needs_elevation: false,
            stops_wsl: false,
            stops_docker: false,
            detail: None,
            available,
            action,
        }
    }

    fn plan() -> Plan {
        Plan {
            mount: "C:".into(),
            role: DriveRole::System,
            role_reasons: vec!["role.reason.holdsWindows"],
            free_bytes: 1,
            total_bytes: 2,
            items: vec![
                item(
                    Group::Temp,
                    "tempOld",
                    Action::TempOld(PathBuf::from(r"C:\t")),
                    true,
                ),
                item(Group::Windows, "hiberfilOrphan", Action::Unavailable, false),
            ],
            blockers: Vec::new(),
        }
    }

    #[test]
    fn an_unconfirmed_empty_or_oversized_request_is_refused_before_anything_is_resolved() {
        let one = vec!["1-0".to_owned()];
        assert!(matches!(
            vet_request(false, &one),
            Err(CommandError::Refused { .. })
        ));
        assert!(matches!(
            vet_request(true, &[]),
            Err(CommandError::Refused { .. })
        ));
        let many = vec!["x".to_owned(); RUN_MAX + 1];
        assert!(matches!(
            vet_request(true, &many),
            Err(CommandError::Refused { .. })
        ));
        assert!(vet_request(true, &one).is_ok());
    }

    #[test]
    fn an_id_from_another_analysis_or_a_made_up_one_or_an_unavailable_item_is_refused() {
        let (dto, items) = to_dto(41, &plan());
        *KEPT.lock().expect("lock") = Some(Kept {
            id: 41,
            mount: "C:".into(),
            items,
        });
        let group = |k: &str| dto.groups.iter().find(|g| g.key == k).expect("group");
        let temp_id = group("temp").items[0].id.clone();
        let hiber_id = group("windows").items[0].id.clone();
        assert!(!group("windows").items[0].default_selected);

        assert!(matches!(
            resolve(40, std::slice::from_ref(&temp_id)),
            Err(CommandError::NotFound { .. })
        ));
        assert!(matches!(
            resolve(41, &[temp_id.clone(), "41-999".into()]),
            Err(CommandError::Refused { .. })
        ));
        assert!(matches!(
            resolve(41, &[hiber_id]),
            Err(CommandError::Refused { .. })
        ));
        let (mount, ok) = resolve(41, &[temp_id.clone(), temp_id]).expect("resolves");
        assert_eq!(mount, "C:");
        // A repeated id runs once.
        assert_eq!(ok.len(), 1);
    }

    #[test]
    fn the_plan_serialises_with_the_contract_field_names() {
        let (dto, _) = to_dto(7, &plan());
        let json = serde_json::to_value(&dto).expect("json");
        for key in [
            "scanId",
            "mount",
            "role",
            "roleReasons",
            "freeBytes",
            "totalBytes",
            "groups",
            "blockers",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        // Groups come in the wizard order: Windows before temporary files.
        assert_eq!(json["groups"][0]["key"], "windows");
        assert_eq!(json["groups"][1]["key"], "temp");
        let item = &json["groups"][1]["items"][0];
        for key in [
            "id",
            "kind",
            "label",
            "size",
            "safety",
            "defaultSelected",
            "irreversible",
            "needsElevation",
            "stopsWsl",
            "stopsDocker",
            "detail",
        ] {
            assert!(item.get(key).is_some(), "{key}");
        }
        // Unmeasured travels as null, never as 0.
        assert!(item["size"].is_null());
    }

    #[test]
    fn only_a_single_drive_letter_is_accepted_as_a_mount() {
        assert_eq!(vet_mount("c:\\").expect("ok"), "C:");
        assert_eq!(vet_mount("E:").expect("ok"), "E:");
        for bad in ["", "C", "CC:", r"\\server\share", "C:\\Windows", "1:"] {
            assert!(vet_mount(bad).is_err(), "{bad}");
        }
    }
}
