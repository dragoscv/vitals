//! The weekly clean-up: which items it may run, and when.
//!
//! # Only what cannot hurt, whatever the profile says
//!
//! The schedule runs with nobody watching, so it takes the drive's saved
//! profile and keeps only the items that are safe, refill themselves, need
//! no administrator and stop nothing. The rule is a whitelist over
//! [`Action`] with no catch-all arm: an action added later is unschedulable
//! until someone decides otherwise here, and a profile edited by hand to
//! name `compactVhd` still never compacts. Nothing irreversible in the
//! danger sense, nothing elevated (so never a UAC prompt), never WSL or
//! Docker stopped.
//!
//! # When
//!
//! A slot is a weekday and a local time. It is due once per slot: the most
//! recent slot is always within a week, so a PC that was off at the time
//! runs it at the next start, once. It waits for the PC to be idle and on
//! mains power; after [`RUN_ANYWAY_AFTER`] of waiting it runs on mains even
//! with someone at the keyboard, because what it does is a few prunes, and a
//! machine that is never idle should still be cleaned. On battery it waits.

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

use super::{Action, DistroTask, Plan, PlannedItem, Safety};

const DAY: i64 = 86_400;
const WEEK: i64 = 7 * DAY;
/// Seconds without keyboard or mouse that count as nobody there.
pub const IDLE_SECS: u64 = 10 * 60;
/// How long a due slot waits for an idle moment before running anyway.
pub const RUN_ANYWAY_AFTER: i64 = DAY;

/// The item kinds the weekly clean-up may ever run. A second fence after
/// the action check: both must agree. The webview mirrors this list to say
/// which choices the schedule includes (`contract.test.ts` keeps them equal).
pub const SCHEDULABLE_KINDS: [&str; 10] = [
    "tempOld",
    "npmCache",
    "pnpmCache",
    "nugetCache",
    "dockerBuildCache",
    "dockerOldImages",
    "wslAptCache",
    "wslJournal",
    "wslTmp",
    "wslDockerPrune",
];

/// Whether the action itself may ever run unattended. Exhaustive on
/// purpose: see the module docs.
const fn action_schedulable(action: &Action) -> bool {
    match action {
        Action::TempOld(_) | Action::DockerBuildCache | Action::DockerOldImages => true,
        Action::DeleteCache { kind, .. } => kind.self_restoring(),
        Action::Distro { task, .. } => matches!(
            task,
            DistroTask::Apt
                | DistroTask::Dnf
                | DistroTask::Journal
                | DistroTask::Tmp
                | DistroTask::Docker
        ),
        Action::Managed(_)
        | Action::HiberfilOrphan
        | Action::Unavailable
        | Action::RecycleBin(_)
        | Action::PnpmPrune(_)
        | Action::Artefact { .. }
        | Action::DockerRemoveContainer { .. }
        | Action::DockerAnonVolumes(_)
        | Action::DockerNamedVolume(_)
        | Action::Compact { .. }
        | Action::Sparse { .. } => false,
    }
}

/// Whether the weekly clean-up may run this item.
#[must_use]
pub fn schedulable(item: &PlannedItem) -> bool {
    item.available
        && item.safety == Safety::Safe
        && item.default_selected
        && !item.needs_elevation
        && !item.stops_wsl
        && !item.stops_docker
        && !item.action.elevated()
        && !item.action.stops_wsl()
        && SCHEDULABLE_KINDS.contains(&item.kind)
        && action_schedulable(&item.action)
}

/// The items of `plan` the weekly clean-up runs: in the profile, and
/// schedulable.
#[must_use]
pub fn selection<'a>(plan: &'a Plan, kinds: &[String]) -> Vec<&'a PlannedItem> {
    plan.items
        .iter()
        .filter(|item| kinds.iter().any(|k| k == item.kind))
        .filter(|item| schedulable(item))
        .collect()
}

/// A weekday (0 = Monday) and a minute of the local day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub weekday: u8,
    pub minute: u16,
}

impl Slot {
    /// `None` for a day past Sunday or a minute past 23:59.
    #[must_use]
    pub const fn new(weekday: u8, minute: u16) -> Option<Self> {
        if weekday <= 6 && minute < 1_440 {
            Some(Self { weekday, minute })
        } else {
            None
        }
    }

    /// The most recent occurrence at or before `now` (UTC seconds), with
    /// the local clock `offset` seconds ahead of UTC.
    ///
    /// The offset is today's: across a daylight-saving change the slot
    /// lands an hour off for that one week, which a weekly clean-up can
    /// live with.
    #[must_use]
    pub fn latest(self, now: i64, offset: i64) -> i64 {
        let local = now + offset;
        let day = local.div_euclid(DAY);
        // 1970-01-01 was a Thursday: day 0 is weekday 3 counting from Monday.
        let today = (day + 3).rem_euclid(7);
        let back = (today - i64::from(self.weekday)).rem_euclid(7);
        let mut at = (day - back) * DAY + i64::from(self.minute) * 60;
        if at > local {
            at -= WEEK;
        }
        at - offset
    }
}

/// What the machine is doing, read once per check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mains {
    /// Plugged in, or a desktop with no battery.
    Yes,
    No,
    /// Windows could not say and there is a battery.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    pub now: i64,
    /// The slot being considered ([`Slot::latest`]).
    pub slot: i64,
    /// The later of the last scheduled run and when the schedule was turned
    /// on: turning it on on Thursday must not run Monday's slot at once.
    pub settled: Option<i64>,
    pub idle_secs: Option<u64>,
    pub mains: Mains,
    pub session_ending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// This slot was handled, or never applies.
    NotDue,
    /// Due, but not now; asked again at the next check.
    Wait,
    Run,
}

/// Whether to run now.
///
/// An unknown idle time is not an absence, and an unknown power line on a
/// machine with a battery is not mains.
#[must_use]
pub fn decide(m: Moment) -> Decision {
    if m.settled.is_some_and(|s| s >= m.slot) || m.now < m.slot || m.now - m.slot >= WEEK {
        return Decision::NotDue;
    }
    if m.session_ending || m.mains != Mains::Yes {
        return Decision::Wait;
    }
    if m.idle_secs.is_some_and(|s| s >= IDLE_SECS) || m.now - m.slot >= RUN_ANYWAY_AFTER {
        Decision::Run
    } else {
        Decision::Wait
    }
}

/// Seconds the local clock is ahead of UTC right now; `None` when Windows
/// cannot say.
#[must_use]
pub fn local_offset_secs() -> Option<i64> {
    let mut utc = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut local = utc;
    // SAFETY: both are live locals of the declared type.
    unsafe { GetSystemTimeAsFileTime(&raw mut utc) };
    // SAFETY: as above; the input is the value just read.
    if unsafe { FileTimeToLocalFileTime(&raw const utc, &raw mut local) } == 0 {
        return None;
    }
    let ticks = |f: FILETIME| (i64::from(f.dwHighDateTime) << 32) | i64::from(f.dwLowDateTime);
    // 100-nanosecond intervals.
    Some((ticks(local) - ticks(utc)) / 10_000_000)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::{CacheFolder, FolderKind, Group};
    use super::*;
    use crate::storage::devclean::ArtefactKind;
    use crate::storage::managed::{Handler, ManagedTool};

    /// Every action there is, so a test can ask about each of them.
    fn every_action() -> Vec<Action> {
        let p = || PathBuf::from(r"C:\x");
        let mut out = vec![
            Action::TempOld(p()),
            Action::Managed(ManagedTool::ComponentCleanup),
            Action::Managed(ManagedTool::DiskCleanup(Handler::ThumbnailCache)),
            Action::Managed(ManagedTool::HibernateOff),
            Action::HiberfilOrphan,
            Action::Unavailable,
            Action::RecycleBin(p()),
            Action::PnpmPrune(p()),
            Action::Artefact {
                path: p(),
                kind: ArtefactKind::CargoTarget,
            },
            Action::DockerBuildCache,
            Action::DockerOldImages,
            Action::DockerRemoveContainer {
                id: "a".into(),
                name: "b".into(),
            },
            Action::DockerAnonVolumes(vec!["v".into()]),
            Action::DockerNamedVolume("db".into()),
            Action::Compact {
                path: p(),
                docker: true,
            },
            Action::Compact {
                path: p(),
                docker: false,
            },
            Action::Sparse {
                distro: "U".into(),
                path: p(),
            },
        ];
        for kind in CacheFolder::ALL {
            out.push(Action::DeleteCache { kind, path: p() });
        }
        for task in [
            DistroTask::Apt,
            DistroTask::Dnf,
            DistroTask::Journal,
            DistroTask::Tmp,
            DistroTask::Docker,
            DistroTask::Folder {
                path: "/x".into(),
                kind: FolderKind::Build,
            },
        ] {
            out.push(Action::Distro {
                distro: "U".into(),
                task,
            });
        }
        out
    }

    /// The kind the planner gives `action`, or `tempOld` — an allowed kind —
    /// for the rest, so the action check is tested on its own.
    fn kind_for(action: &Action) -> &'static str {
        match action {
            Action::DockerBuildCache => "dockerBuildCache",
            Action::DockerOldImages => "dockerOldImages",
            Action::DeleteCache { kind, .. } => kind.kind(),
            Action::Distro { task, .. } => match task {
                DistroTask::Apt | DistroTask::Dnf => "wslAptCache",
                DistroTask::Journal => "wslJournal",
                DistroTask::Tmp => "wslTmp",
                DistroTask::Docker => "wslDockerPrune",
                DistroTask::Folder { .. } => "wslBuildDir",
            },
            _ => "tempOld",
        }
    }

    /// The most permissive item the planner could build for `action`:
    /// marked safe and preselected, with an allowed kind where the planner
    /// would not give one — the lies a schedule must survive.
    fn as_safe(action: Action) -> PlannedItem {
        let kind = kind_for(&action);
        PlannedItem::new(Group::Temp, kind, action).safe_default()
    }

    #[test]
    fn a_kind_outside_the_list_is_never_scheduled_even_with_a_harmless_action() {
        let item = PlannedItem::new(
            Group::Temp,
            "compactVhd",
            Action::TempOld(PathBuf::from(r"C:\t")),
        )
        .safe_default();
        assert!(!schedulable(&item));
    }

    #[test]
    fn nothing_elevated_compacting_sparse_or_stopping_wsl_or_docker_is_ever_schedulable() {
        for action in every_action() {
            let item = as_safe(action.clone());
            if action.elevated() || action.stops_wsl() || item.stops_docker {
                assert!(!schedulable(&item), "{action:?}");
            }
            if matches!(action, Action::Compact { .. } | Action::Sparse { .. }) {
                assert!(!schedulable(&item), "{action:?}");
            }
        }
    }

    #[test]
    fn a_danger_or_review_item_is_never_schedulable_even_with_a_safe_action() {
        let mut danger = as_safe(Action::TempOld(PathBuf::from(r"C:\t")));
        danger.safety = Safety::Danger;
        let mut review = as_safe(Action::DockerBuildCache);
        review.safety = Safety::Review;
        let mut unticked = as_safe(Action::DockerOldImages);
        unticked.default_selected = false;
        let unavailable = as_safe(Action::TempOld(PathBuf::new()))
            .because(super::super::UnavailableReason::HibernationRefused);
        for item in [danger, review, unticked, unavailable] {
            assert!(!schedulable(&item), "{item:?}");
        }
    }

    #[test]
    fn a_named_docker_volume_containers_artefacts_and_the_recycle_bin_are_never_scheduled() {
        let never = [
            Action::DockerNamedVolume("postgres".into()),
            Action::DockerAnonVolumes(vec!["x".into()]),
            Action::DockerRemoveContainer {
                id: "1".into(),
                name: "n".into(),
            },
            Action::Artefact {
                path: PathBuf::from(r"E:\p\target"),
                kind: ArtefactKind::CargoTarget,
            },
            Action::RecycleBin(PathBuf::from(r"C:\")),
            Action::PnpmPrune(PathBuf::from(r"E:\.pnpm-store")),
            Action::DeleteCache {
                kind: CacheFolder::CargoRegistry,
                path: PathBuf::from(r"C:\c"),
            },
        ];
        for action in never {
            assert!(!schedulable(&as_safe(action.clone())), "{action:?}");
        }
    }

    #[test]
    fn only_old_temp_files_self_restoring_caches_docker_prunes_and_distro_caches_qualify() {
        let allowed: Vec<String> = every_action()
            .into_iter()
            .filter(|a| schedulable(&as_safe(a.clone())))
            .map(|a| match a {
                Action::DeleteCache { kind, .. } => format!("cache:{kind:?}"),
                Action::Distro { task, .. } => format!("distro:{task:?}"),
                other => format!("{other:?}")
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or("")
                    .to_owned(),
            })
            .collect();
        assert_eq!(
            allowed,
            [
                "TempOld",
                "DockerBuildCache",
                "DockerOldImages",
                "cache:Npm",
                "cache:PnpmMetadata",
                "cache:NugetHttp",
                "distro:Apt",
                "distro:Dnf",
                "distro:Journal",
                "distro:Tmp",
                "distro:Docker",
            ]
        );
    }

    #[test]
    fn the_selection_is_the_profile_kinds_that_are_schedulable_and_nothing_else() {
        let mut compact = PlannedItem::new(
            Group::Compaction,
            "compactVhd",
            Action::Compact {
                path: PathBuf::from(r"C:\d.vhdx"),
                docker: true,
            },
        );
        // Even forced to look safe, compaction stays out.
        compact.safety = Safety::Safe;
        compact.default_selected = true;
        let plan = Plan {
            mount: "C:".into(),
            role: super::super::DriveRole::System,
            role_reasons: Vec::new(),
            free_bytes: 1,
            total_bytes: 2,
            items: vec![
                as_safe(Action::TempOld(PathBuf::from(r"C:\t"))),
                PlannedItem::new(Group::Docker, "dockerBuildCache", Action::DockerBuildCache)
                    .safe_default(),
                compact,
            ],
            blockers: Vec::new(),
        };
        let kinds = vec!["tempOld".to_owned(), "compactVhd".to_owned()];
        let picked: Vec<&str> = selection(&plan, &kinds).iter().map(|i| i.kind).collect();
        assert_eq!(picked, ["tempOld"]);
    }

    // 2026-10-19 is a Monday; 12:00 UTC.
    const MONDAY_NOON: i64 = 1_792_411_200;

    #[test]
    fn the_latest_slot_is_this_weeks_when_it_has_passed_and_last_weeks_when_not() {
        let monday_ten = Slot::new(0, 10 * 60).unwrap();
        assert_eq!(monday_ten.latest(MONDAY_NOON, 0), MONDAY_NOON - 2 * 3_600);
        let monday_two_pm = Slot::new(0, 14 * 60).unwrap();
        assert_eq!(
            monday_two_pm.latest(MONDAY_NOON, 0),
            MONDAY_NOON + 2 * 3_600 - WEEK
        );
        let sunday_nine = Slot::new(6, 9 * 60).unwrap();
        assert_eq!(
            sunday_nine.latest(MONDAY_NOON, 0),
            MONDAY_NOON - DAY - 3 * 3_600
        );
    }

    #[test]
    fn the_slot_is_read_on_the_local_clock() {
        // Bucharest in October: UTC+3. 14:00 local is 11:00 UTC, an hour ago.
        let slot = Slot::new(0, 14 * 60).unwrap();
        assert_eq!(slot.latest(MONDAY_NOON, 3 * 3_600), MONDAY_NOON - 3_600);
    }

    #[test]
    fn an_impossible_day_or_minute_is_not_a_slot() {
        assert!(Slot::new(7, 0).is_none());
        assert!(Slot::new(0, 1_440).is_none());
        assert!(Slot::new(6, 1_439).is_some());
    }

    fn moment() -> Moment {
        Moment {
            now: 10 * DAY,
            slot: 10 * DAY - 3_600,
            settled: Some(5 * DAY),
            idle_secs: Some(IDLE_SECS),
            mains: Mains::Yes,
            session_ending: false,
        }
    }

    #[test]
    fn an_idle_pc_on_mains_runs_a_due_slot_once() {
        assert_eq!(decide(moment()), Decision::Run);
        let after = Moment {
            settled: Some(moment().now),
            ..moment()
        };
        assert_eq!(decide(after), Decision::NotDue);
    }

    #[test]
    fn a_schedule_turned_on_after_the_slot_waits_for_the_next_one() {
        let m = Moment {
            settled: Some(moment().slot + 60),
            ..moment()
        };
        assert_eq!(decide(m), Decision::NotDue);
    }

    #[test]
    fn on_battery_or_with_an_unknown_power_line_it_waits_however_long_it_is_overdue() {
        for mains in [Mains::No, Mains::Unknown] {
            let m = Moment {
                mains,
                slot: moment().now - 3 * DAY,
                ..moment()
            };
            assert_eq!(decide(m), Decision::Wait, "{mains:?}");
        }
    }

    #[test]
    fn someone_at_the_keyboard_delays_it_for_a_day_then_it_runs_anyway() {
        let busy = Moment {
            idle_secs: Some(30),
            ..moment()
        };
        assert_eq!(decide(busy), Decision::Wait);
        let unknown = Moment {
            idle_secs: None,
            ..moment()
        };
        assert_eq!(decide(unknown), Decision::Wait, "unknown idle is not idle");
        let overdue = Moment {
            slot: busy.now - RUN_ANYWAY_AFTER,
            ..busy
        };
        assert_eq!(decide(overdue), Decision::Run);
    }

    #[test]
    fn a_pc_that_was_off_at_the_slot_runs_it_at_the_next_start_but_not_while_windows_ends() {
        let missed = Moment {
            slot: moment().now - 6 * DAY,
            settled: None,
            ..moment()
        };
        assert_eq!(decide(missed), Decision::Run);
        let ending = Moment {
            session_ending: true,
            ..missed
        };
        assert_eq!(decide(ending), Decision::Wait);
        let too_old = Moment {
            slot: moment().now - WEEK,
            ..missed
        };
        assert_eq!(decide(too_old), Decision::NotDue);
    }

    #[test]
    fn the_local_offset_is_whole_quarter_hours_within_a_day() {
        let offset = local_offset_secs().expect("Windows answers");
        assert!(offset.abs() <= 14 * 3_600, "{offset}");
        assert_eq!(offset % 900, 0, "{offset}");
    }
}
