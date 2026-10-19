//! The weekly safe clean-up and the per-volume drive history.
//!
//! One background thread, started with the app:
//!
//! - every hour (and a minute after launch) it reads used and free space of
//!   each local volume — one `GetDiskFreeSpaceExW` each — into
//!   `drive-history.sqlite`, and once a day thins and prunes that file;
//! - every few minutes it looks at the weekly schedules the webview pushed
//!   (`set_drive_schedules`) and runs a due one through
//!   `vitals_win::storage::drive_cleanup::schedule`, which decides both
//!   *whether* (idle, mains power, once per slot, a missed slot at the next
//!   start) and *what* (only safe, self-restoring, unelevated items).
//!
//! A scheduled run never elevates, so it can never raise a UAC prompt, and
//! never stops WSL or Docker. It re-analyses first and does nothing — not
//! even a notification — when nothing qualifies.
//!
//! Desktop-only: a paired phone neither sees nor sets this.

#![cfg(windows)]

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::Manager as _;
use tauri_plugin_notification::NotificationExt as _;
use vitals_core::metrics::DiskKind;
use vitals_store::drive_history::{
    DriveHistory, ItemSize, RunRecord, VolumePoint, changes, downsample, growth,
};
use vitals_win::storage::drive_cleanup::{
    self as dcw, Plan,
    history::{measured_sizes, volume_key},
    run::{Report, Status},
    schedule::{self, Decision, Mains, Moment, Slot},
};

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// First volume reading after launch: late enough not to compete with the
/// first paint, early enough that "at app start" is true.
const FIRST_SAMPLE_AFTER: Duration = Duration::from_secs(60);
const SAMPLE_EVERY: Duration = Duration::from_secs(60 * 60);
const PRUNE_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_EVERY: Duration = Duration::from_secs(5 * 60);
/// How far back the sparkline reaches, and how many points it gets.
const SERIES_DAYS: i64 = 30;
const SERIES_POINTS: usize = 120;
const RUNS_SHOWN: u32 = 5;

fn history_path() -> std::path::PathBuf {
    crate::state::data_dir().join("drive-history.sqlite")
}

fn open() -> Option<DriveHistory> {
    let path = history_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    DriveHistory::open(&path)
        .map_err(|error| tracing::warn!(%error, "drive history unavailable"))
        .ok()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

// ---------------------------------------------------------------------------
// State pushed from the webview
// ---------------------------------------------------------------------------

/// One drive's weekly schedule, as `settings.driveCleanupSchedules` keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleDto {
    /// `volume_key` of the drive: survives a change of letter.
    pub volume: String,
    /// The letter last seen, for the notification when the volume is gone.
    pub mount: String,
    /// 0 = Monday.
    pub weekday: u8,
    /// Minute of the local day.
    pub minute: u16,
    /// The drive's saved choices; the backend keeps only what is safe.
    pub kinds: Vec<String>,
    /// When it was turned on (unix seconds): a slot before it is not due.
    pub enabled_at: i64,
}

/// Toast text in the user's language. `{mount}` and `{size}` are filled in
/// here, because the toast fires with the window hidden.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStrings {
    pub title: String,
    pub freed: String,
    pub freed_unknown: String,
    pub decimal: String,
}

#[derive(Debug, Default)]
pub struct DriveSchedules {
    entries: Mutex<Vec<ScheduleDto>>,
    strings: Mutex<Option<ScheduleStrings>>,
}

/// Replaces the schedules. Sent on hydration and on every change.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn set_drive_schedules(
    state: tauri::State<'_, DriveSchedules>,
    schedules: Vec<ScheduleDto>,
) -> CommandResult<()> {
    let valid = schedules
        .into_iter()
        .filter(|s| Slot::new(s.weekday, s.minute).is_some() && !s.volume.is_empty())
        .collect();
    *state.entries.lock() = valid;
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn set_drive_schedule_strings(
    state: tauri::State<'_, DriveSchedules>,
    strings: ScheduleStrings,
) -> CommandResult<()> {
    *state.strings.lock() = Some(strings);
    Ok(())
}

// ---------------------------------------------------------------------------
// Recording, from the wizard and from the schedule
// ---------------------------------------------------------------------------

fn sizes(plan: &Plan) -> Vec<ItemSize> {
    measured_sizes(plan)
        .into_iter()
        .map(|m| ItemSize {
            kind: m.kind,
            label: m.label,
            size: m.size,
        })
        .collect()
}

/// Remembers what an analysis measured and how full the drive was.
/// Best effort: the history is a convenience, never a reason to fail.
pub fn remember_analysis(plan: &Plan) {
    let Some(history) = open() else { return };
    let volume = volume_key(&plan.mount);
    let ts = now_secs();
    if let Err(error) = history.record_analysis(&volume, ts, &sizes(plan)) {
        tracing::warn!(%error, "drive history: analysis not recorded");
    }
    if plan.total_bytes > 0 {
        let point = VolumePoint {
            ts,
            used: plan.total_bytes.saturating_sub(plan.free_bytes),
            free: plan.free_bytes,
        };
        let _ = history.record_volume(&volume, point);
    }
}

fn run_record(report: &Report, scheduled: bool) -> RunRecord {
    let failed: Vec<&str> = report
        .items
        .iter()
        .filter(|i| i.status == Status::Failed)
        .map(|i| i.message.as_deref().unwrap_or("failed"))
        .collect();
    RunRecord {
        ts: now_secs(),
        scheduled,
        freed: report.drive_freed,
        items: u32::try_from(report.items.len()).unwrap_or(u32::MAX),
        failed: u32::try_from(failed.len()).unwrap_or(u32::MAX),
        errors: failed.into_iter().map(str::to_owned).collect(),
    }
}

/// Remembers a clean-up run.
pub fn remember_run(mount: &str, report: &Report, scheduled: bool) {
    let Some(history) = open() else { return };
    if let Err(error) = history.record_run(&volume_key(mount), &run_record(report, scheduled)) {
        tracing::warn!(%error, "drive history: run not recorded");
    }
}

// ---------------------------------------------------------------------------
// Reading it back
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointDto {
    ts: i64,
    used: u64,
    free: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrowthDto {
    day: Option<i64>,
    week: Option<i64>,
    month: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDto {
    kind: String,
    label: Option<String>,
    then: u64,
    now: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDto {
    ts: i64,
    scheduled: bool,
    freed: Option<u64>,
    items: u32,
    failed: u32,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveHistoryDto {
    volume: String,
    series: Vec<PointDto>,
    growth: GrowthDto,
    /// When the latest and the earlier analysis were taken; `None` when
    /// there is no such analysis.
    latest_analysis: Option<i64>,
    previous_analysis: Option<i64>,
    changes: Vec<ChangeDto>,
    runs: Vec<RunDto>,
}

fn read_history(history: &DriveHistory, volume: &str, now: i64) -> CommandResult<DriveHistoryDto> {
    let map = |e: vitals_store::StoreError| CommandError::Internal {
        message: format!("drive history: {e}"),
    };
    let points = history
        .volume_series(volume, now - SERIES_DAYS * 86_400)
        .map_err(map)?;
    let (latest, previous) = history.latest_analyses(volume).map_err(map)?;
    let changed = match (&latest, &previous) {
        (Some(l), Some(p)) => changes(&p.items, &l.items),
        _ => Vec::new(),
    };
    let g = growth(&points, now);
    Ok(DriveHistoryDto {
        volume: volume.to_owned(),
        series: downsample(&points, SERIES_POINTS)
            .into_iter()
            .map(|p| PointDto {
                ts: p.ts,
                used: p.used,
                free: p.free,
            })
            .collect(),
        growth: GrowthDto {
            day: g.day,
            week: g.week,
            month: g.month,
        },
        latest_analysis: latest.map(|a| a.ts),
        previous_analysis: previous.map(|a| a.ts),
        changes: changed
            .into_iter()
            .map(|c| ChangeDto {
                kind: c.kind,
                label: c.label,
                then: c.then,
                now: c.now,
            })
            .collect(),
        runs: history
            .runs(volume, RUNS_SHOWN)
            .map_err(map)?
            .into_iter()
            .map(|r| RunDto {
                ts: r.ts,
                scheduled: r.scheduled,
                freed: r.freed,
                items: r.items,
                failed: r.failed,
                errors: r.errors,
            })
            .collect(),
    })
}

/// The history of one drive, for the wizard.
#[tauri::command]
pub async fn get_drive_history(mount: String) -> CommandResult<DriveHistoryDto> {
    let mount = crate::drivecleanup::vet_mount(&mount)?;
    tauri::async_runtime::spawn_blocking(move || {
        let volume = volume_key(&mount);
        let Some(history) = open() else {
            return Err(CommandError::Internal {
                message: "the drive history file could not be opened".into(),
            });
        };
        read_history(&history, &volume, now_secs())
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the history read did not finish: {err}"),
    })?
}

// ---------------------------------------------------------------------------
// The background thread
// ---------------------------------------------------------------------------

pub fn spawn(app: &tauri::AppHandle) {
    let app = app.clone();
    // A plain thread, like the updater's: it sleeps between checks.
    let _ = std::thread::Builder::new()
        .name("vitals-drive-schedule".into())
        .stack_size(512 * 1024)
        .spawn(move || {
            std::thread::sleep(FIRST_SAMPLE_AFTER);
            let mut last_sample: Option<Instant> = None;
            let mut last_prune: Option<Instant> = None;
            loop {
                if last_sample.is_none_or(|t| t.elapsed() >= SAMPLE_EVERY) {
                    last_sample = Some(Instant::now());
                    let prune = last_prune.is_none_or(|t| t.elapsed() >= PRUNE_EVERY);
                    sample_volumes(prune);
                    if prune {
                        last_prune = Some(Instant::now());
                    }
                }
                check_schedules(&app);
                std::thread::sleep(CHECK_EVERY);
            }
        });
}

/// One reading of every local volume. A volume that cannot be read is
/// skipped (the enumeration already drops it), never written as zero.
fn sample_volumes(prune: bool) {
    let Some(history) = open() else { return };
    let now = now_secs();
    for volume in vitals_win::disk::volumes::enumerate_volumes_cached() {
        if matches!(volume.kind, DiskKind::Network | DiskKind::Optical) {
            continue;
        }
        let point = VolumePoint {
            ts: now,
            used: volume.used().0,
            free: volume.available.0,
        };
        if let Err(error) = history.record_volume(&volume_key(&volume.mount), point) {
            tracing::warn!(%error, mount = %volume.mount, "drive history: reading not recorded");
        }
    }
    if prune && let Err(error) = history.prune(now) {
        tracing::warn!(%error, "drive history: prune failed");
    }
}

fn mains() -> Mains {
    use vitals_win::sensors::power::{LineStatus, read_power_state};
    let state = read_power_state();
    match state.line {
        LineStatus::Ac => Mains::Yes,
        LineStatus::Battery => Mains::No,
        // A desktop with no battery is on mains whatever the firmware says.
        LineStatus::Unknown if !state.has_battery => Mains::Yes,
        LineStatus::Unknown => Mains::Unknown,
    }
}

/// The mount a volume key is on now, if it is mounted.
fn mount_of(volume: &str) -> Option<String> {
    vitals_win::disk::volumes::enumerate_volumes_cached()
        .into_iter()
        .map(|v| v.mount)
        .find(|m| volume_key(m) == volume)
}

fn check_schedules(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<DriveSchedules>() else {
        return;
    };
    let entries = state.entries.lock().clone();
    if entries.is_empty() {
        return;
    }
    let Some(history) = open() else { return };
    let now = now_secs();
    let offset = schedule::local_offset_secs().unwrap_or(0);
    for entry in entries {
        let Some(slot) = Slot::new(entry.weekday, entry.minute) else {
            continue;
        };
        let last = history.last_scheduled_run(&entry.volume).ok().flatten();
        let moment = Moment {
            now,
            slot: slot.latest(now, offset),
            settled: Some(last.map_or(entry.enabled_at, |l| l.max(entry.enabled_at))),
            idle_secs: vitals_win::input::idle_secs(),
            mains: mains(),
            session_ending: vitals_win::input::session_ending(),
        };
        if schedule::decide(moment) != Decision::Run {
            continue;
        }
        // Not mounted now (a USB disk that is away): asked again next check.
        let Some(mount) = mount_of(&entry.volume) else {
            continue;
        };
        run_scheduled(app, &history, &entry, &mount);
    }
}

fn run_scheduled(app: &tauri::AppHandle, history: &DriveHistory, entry: &ScheduleDto, mount: &str) {
    // The same slots the wizard takes: a clean-up the user started keeps
    // the machine, and this one waits for the next check.
    let Ok(busy) = crate::devclean::Busy::claim("weekly clean-up") else {
        return;
    };
    let Ok(managed) = crate::inventory::claim_managed("the weekly clean-up") else {
        return;
    };
    let _held = (busy, managed);
    let cancel = AtomicBool::new(false);
    let Some(plan) = dcw::analyse(mount, &cancel, &mut |_, _, _| {}) else {
        return;
    };
    remember_analysis(&plan);
    let items: Vec<(String, dcw::Action)> = schedule::selection(&plan, &entry.kinds)
        .into_iter()
        .enumerate()
        .map(|(n, item)| (format!("weekly-{n}"), item.action.clone()))
        .collect();
    tracing::info!(mount, items = items.len(), "weekly clean-up is due");
    if items.is_empty() {
        // Settles the slot without a toast: there was nothing to say.
        let _ = history.record_run(
            &entry.volume,
            &RunRecord {
                ts: now_secs(),
                scheduled: true,
                freed: None,
                items: 0,
                failed: 0,
                errors: Vec::new(),
            },
        );
        return;
    }
    let report = dcw::run::run(mount, "weekly", &items, &cancel, &mut |_| {});
    if let Err(error) = history.record_run(&entry.volume, &run_record(&report, true)) {
        tracing::warn!(%error, "drive history: weekly run not recorded");
    }
    notify(app, mount, report.drive_freed);
}

fn notify(app: &tauri::AppHandle, mount: &str, freed: Option<u64>) {
    let Some(strings) = app
        .try_state::<DriveSchedules>()
        .and_then(|s| s.strings.lock().clone())
    else {
        // Without the user's language there is nothing honest to say.
        return;
    };
    let body = toast_body(&strings, mount, freed);
    if let Err(error) = app
        .notification()
        .builder()
        .title(&strings.title)
        .body(body)
        .show()
    {
        tracing::warn!(%error, "weekly clean-up notification failed");
    }
}

fn toast_body(strings: &ScheduleStrings, mount: &str, freed: Option<u64>) -> String {
    match freed {
        Some(bytes) => strings
            .freed
            .replace("{mount}", mount)
            .replace("{size}", &format_bytes(bytes, &strings.decimal)),
        None => strings.freed_unknown.replace("{mount}", mount),
    }
}

/// Binary units with the same precision rule as `formatBytes` in
/// `@vitals/ui`, so the toast and the wizard agree on "4.2 GB".
fn format_bytes(bytes: u64, decimal: &str) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)]
    // A displayed size: two significant decimals are all that survive.
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let digits = if value >= 100.0 {
        0
    } else if value >= 10.0 {
        1
    } else {
        2
    };
    let text = format!("{value:.digits$}");
    format!("{} {}", text.replace('.', decimal), UNITS[unit])
}

#[cfg(test)]
mod tests {
    use vitals_win::storage::drive_cleanup::run::ItemResult;

    use super::*;

    fn strings() -> ScheduleStrings {
        ScheduleStrings {
            title: "Vitals".into(),
            freed: "Weekly clean-up of {mount} freed {size}".into(),
            freed_unknown: "Weekly clean-up of {mount} finished".into(),
            decimal: ",".into(),
        }
    }

    #[test]
    fn the_toast_names_the_drive_and_the_size_in_the_users_decimal_style() {
        assert_eq!(
            toast_body(&strings(), "E:", Some(4_509_715_660)),
            "Weekly clean-up of E: freed 4,20 GB"
        );
        assert_eq!(
            toast_body(&strings(), "E:", None),
            "Weekly clean-up of E: finished",
            "an unmeasured gain is not reported as 0 B"
        );
    }

    #[test]
    fn sizes_use_the_same_precision_steps_as_the_webview() {
        assert_eq!(format_bytes(512, "."), "512 B");
        assert_eq!(format_bytes(1536, "."), "1.50 KB");
        assert_eq!(format_bytes(15 * 1024 * 1024, "."), "15.0 MB");
        assert_eq!(format_bytes(150 * 1024 * 1024 * 1024, "."), "150 GB");
    }

    #[test]
    fn a_run_record_counts_failures_and_keeps_their_messages() {
        let report = Report {
            items: vec![
                ItemResult {
                    id: "a".into(),
                    status: Status::Done,
                    freed: Some(10),
                    message: None,
                },
                ItemResult {
                    id: "b".into(),
                    status: Status::Failed,
                    freed: None,
                    message: Some("docker is not running".into()),
                },
            ],
            drive_freed: None,
            restarted: Vec::new(),
            restart_failed: Vec::new(),
        };
        let r = run_record(&report, true);
        assert_eq!((r.items, r.failed, r.freed), (2, 1, None));
        assert_eq!(r.errors, ["docker is not running"]);
        assert!(r.scheduled);
    }

    #[test]
    fn the_history_reply_uses_the_contract_field_names_and_says_when_nothing_is_known() {
        let history = DriveHistory::in_memory().expect("history");
        let now = 40 * 86_400;
        for d in 0..=10 {
            history
                .record_volume(
                    "v",
                    VolumePoint {
                        ts: now - (10 - d) * 86_400,
                        used: 100 + u64::try_from(d).unwrap_or(0) * 86_400,
                        free: 5,
                    },
                )
                .expect("record");
        }
        let json =
            serde_json::to_value(read_history(&history, "v", now).expect("read")).expect("json");
        for key in [
            "volume",
            "series",
            "growth",
            "latestAnalysis",
            "previousAnalysis",
            "changes",
            "runs",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        assert_eq!(json["growth"]["day"], 86_400);
        assert!(json["growth"]["month"].is_null(), "too short to call");
        assert!(json["latestAnalysis"].is_null());
        assert_eq!(json["series"].as_array().map(Vec::len), Some(11));
    }

    #[test]
    fn a_schedule_with_an_impossible_slot_is_dropped_on_arrival() {
        let raw = serde_json::json!([
            {"volume": "v", "mount": "C:", "weekday": 6, "minute": 600, "kinds": ["tempOld"], "enabledAt": 1},
            {"volume": "w", "mount": "D:", "weekday": 9, "minute": 600, "kinds": [], "enabledAt": 1}
        ]);
        let parsed: Vec<ScheduleDto> = serde_json::from_value(raw).expect("parse");
        let valid: Vec<&ScheduleDto> = parsed
            .iter()
            .filter(|s| Slot::new(s.weekday, s.minute).is_some())
            .collect();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].enabled_at, 1);
    }

    #[test]
    fn the_history_file_lives_beside_the_others() {
        assert!(history_path().ends_with("drive-history.sqlite"));
    }
}
