//! When the machine is lagging, and when to say so.
//!
//! Pure, driven by one [`Tick`] a second, so thresholds and the
//! don't-nag rules are testable without loading a real machine.
//!
//! ## What counts
//!
//! The first version counted CPU at or above 90 %. On a developer's machine
//! that fired seven times in ten minutes — `tsc`, VS Code, `python`, `rg` —
//! and every one of those ticks had **0 ms** of scheduling delay: the
//! machine was busy, not slow. Measured on the same machine at 97–100 %
//! CPU, the foreground window answered a message in 0.1–8 ms. A busy CPU is
//! not a frozen desktop, so CPU is no longer a trigger at all; it only
//! decides whom to blame once something the user can feel has happened.
//!
//! What the user feels, each measured directly:
//!
//! - **The window they are using does not answer.** `SendMessageTimeout`
//!   with `WM_NULL` to the foreground window, timed. Windows' own "Not
//!   Responding" (`IsHungAppWindow`) is the extreme of the same signal.
//! - **The scheduler is starved.** A Normal-priority thread asks to sleep
//!   10 ms and is woken late. The cursor and input threads of every
//!   ordinary app run at that priority, so they are late too.
//! - **The disk is standing in for RAM.** Hard faults a second, with memory
//!   nearly full. Paging makes the CPU look idle while everything waits.
//! - **Windows is about to run out of memory it can promise.** Commit charge
//!   near the commit limit (RAM + page file). Not felt yet — that is the
//!   point: on 2026-10-03 and 2026-10-05 `dwm.exe` died three times with
//!   `0xc00001ad` (no memory) at the moment commit ran out, with 70 GB of
//!   RAM still free. The only useful warning is the one before.
//!
//! And only while the person is there: no input for a minute means nobody
//! is waiting on the screen, so a nightly build can saturate what it likes.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use vitals_core::ids::ProcessKey;

use crate::config::Thresholds;

/// Physical memory load at or above which hard faults count as paging.
pub const MEMORY_LOAD: u32 = 90;
/// Commit charge, percent of the commit limit, at or above which a warning
/// is due. At 100 % allocations fail, and `dwm` is among the first to die.
pub const COMMIT_LOAD: u32 = 90;
/// Ticks looked back over.
pub const WINDOW: u32 = 6;
/// Ticks within [`WINDOW`] that must be bad. One stall is a hiccup; four
/// seconds out of six is something the user is sitting through.
pub const NEEDED: u32 = 4;
/// Consecutive ticks the foreground window must be "Not Responding".
pub const HUNG_TICKS: u32 = 5;
/// Seconds without keyboard or mouse input after which nobody is watching.
pub const IDLE_SECS: u64 = 60;

/// One second of observation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Round-trip of `WM_NULL` to the foreground window, ms. `None` when
    /// there is no foreground window to ask (desktop, lock screen).
    pub window_ms: Option<f64>,
    /// Worst wake-up delay of the Normal-priority probe during the tick, ms.
    pub scheduler_ms: f64,
    /// Hard page faults in the last second, machine-wide.
    pub hard_faults: f64,
    /// Physical memory in use, percent.
    pub memory_load: u32,
    /// Commit charge, percent of the commit limit. `None` when Windows did
    /// not say, which is not the same as an empty commit.
    pub commit_load: Option<u32>,
    /// The foreground window's process, when Windows says it is hung.
    pub hung: Option<ProcessKey>,
    /// Seconds since the last keyboard or mouse input.
    pub idle_secs: u64,
}

/// Why a notification is warranted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The desktop is stalling: windows slow to answer or the scheduler
    /// starved. The culprit is chosen by CPU.
    Stall,
    /// Sustained paging. The culprit is chosen by memory.
    Paging,
    /// Commit charge near the limit. The culprit is chosen by memory
    /// (private bytes, which is what commit counts).
    Commit,
    /// The foreground app stopped answering altogether.
    Hung(ProcessKey),
}

/// Commit charge as a percentage of the commit limit, rounded down.
///
/// `GlobalMemoryStatusEx` calls these `ullTotalPageFile` and
/// `ullAvailPageFile`, but they are the commit limit (RAM plus every page
/// file) and what is left of it — not the page file alone. A zero limit is
/// an unreadable answer, not an empty machine, so it is `None`.
#[must_use]
pub fn commit_load(limit: u64, available: u64) -> Option<u32> {
    if limit == 0 {
        return None;
    }
    let used = u128::from(limit.saturating_sub(available));
    u32::try_from(used * 100 / u128::from(limit)).ok()
}

/// A sliding count of bad ticks.
#[derive(Debug, Default, Clone, Copy)]
struct Window(u32);

impl Window {
    fn push(&mut self, bad: bool) -> u32 {
        let mask = (1_u32 << WINDOW) - 1;
        self.0 = ((self.0 << 1) | u32::from(bad)) & mask;
        self.0.count_ones()
    }

    fn clear(&mut self) {
        self.0 = 0;
    }
}

/// Turns ticks into triggers.
#[derive(Debug, Default)]
pub struct Detector {
    stall: Window,
    paging: Window,
    commit: Window,
    hung: Option<(ProcessKey, u32)>,
}

impl Detector {
    /// Feeds one tick; returns what, if anything, is now sustained.
    ///
    /// A hung window outranks commit, which outranks paging, which outranks
    /// a stall. A hung window is the most specific explanation and the one
    /// the user is already sitting through. Commit comes next because its
    /// end is not slowness but the desktop crashing, and when commit is
    /// short the paging it causes is a symptom: naming the largest holder of
    /// commit is the remedy for both, while a paging warning would blame
    /// whatever happens to fault most. Paging stalls everything while making
    /// the CPU look idle, so it in turn explains a stall better than CPU.
    pub fn push(&mut self, tick: Tick, limits: Thresholds) -> Option<Trigger> {
        if tick.idle_secs >= IDLE_SECS {
            // Nobody is at the machine. Forget what was building up, so the
            // user's first keystroke back is not met by a stale episode. The
            // commit warning waits too: there is nobody to act on it, and
            // it is still true when they return.
            self.stall.clear();
            self.paging.clear();
            self.commit.clear();
            self.hung = None;
            return None;
        }

        let window_slow = tick.window_ms.is_some_and(|ms| ms >= limits.window_ms);
        let stall = self
            .stall
            .push(window_slow || tick.scheduler_ms >= limits.scheduler_ms);
        let paging = self
            .paging
            .push(tick.memory_load >= MEMORY_LOAD && tick.hard_faults >= limits.hard_faults);
        let commit = self
            .commit
            .push(tick.commit_load.is_some_and(|load| load >= COMMIT_LOAD));

        self.hung = match (tick.hung, self.hung) {
            (Some(key), Some((same, n))) if same == key => Some((key, n + 1)),
            (Some(key), _) => Some((key, 1)),
            (None, _) => None,
        };

        if let Some((key, n)) = self.hung
            && n >= HUNG_TICKS
        {
            return Some(Trigger::Hung(key));
        }
        if commit >= NEEDED {
            return Some(Trigger::Commit);
        }
        if paging >= NEEDED {
            return Some(Trigger::Paging);
        }
        (stall >= NEEDED).then_some(Trigger::Stall)
    }
}

/// Minimum gap between any two proposals.
pub const GAP: Duration = Duration::from_secs(3 * 60);
/// How long the same process is not proposed again.
pub const SAME_TARGET: Duration = Duration::from_secs(15 * 60);
/// How long "Ignore" silences an image name.
pub const SNOOZE: Duration = Duration::from_secs(30 * 60);

/// The don't-nag rules.
///
/// A notification that repeats every minute while a build finishes is worse
/// than none: it teaches the user to dismiss it unread, which is exactly
/// when the one that matters gets ignored.
#[derive(Debug, Default)]
pub struct Policy {
    last: Option<Instant>,
    proposed: HashMap<ProcessKey, Instant>,
    snoozed: HashMap<String, Instant>,
}

impl Policy {
    /// Whether `key` / `name` may be proposed at `now`. Records it if so.
    pub fn allow(&mut self, key: ProcessKey, name: &str, now: Instant) -> bool {
        let within = |at: &Instant, span: Duration| now.saturating_duration_since(*at) < span;
        if self.last.as_ref().is_some_and(|at| within(at, GAP))
            || self
                .proposed
                .get(&key)
                .is_some_and(|at| within(at, SAME_TARGET))
            || self
                .snoozed
                .get(&name.to_ascii_lowercase())
                .is_some_and(|at| within(at, SNOOZE))
        {
            return false;
        }
        self.proposed.retain(|_, at| within(at, SAME_TARGET));
        self.last = Some(now);
        self.proposed.insert(key, now);
        true
    }

    /// Silences `name` for [`SNOOZE`].
    pub fn snooze(&mut self, name: &str, now: Instant) {
        self.snoozed.insert(name.to_ascii_lowercase(), now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Sensitivity;
    use vitals_core::ids::Pid;

    const CALM: Tick = Tick {
        window_ms: Some(2.0),
        scheduler_ms: 0.0,
        hard_faults: 0.0,
        memory_load: 45,
        commit_load: Some(40),
        hung: None,
        idle_secs: 0,
    };

    fn normal() -> Thresholds {
        Sensitivity::Normal.thresholds()
    }

    fn key(pid: u32) -> ProcessKey {
        ProcessKey::new(Pid(pid), 1)
    }

    fn run(d: &mut Detector, tick: Tick, n: u32) -> Option<Trigger> {
        (0..n).map(|_| d.push(tick, normal())).last().flatten()
    }

    #[test]
    fn a_saturated_cpu_that_the_desktop_does_not_feel_never_notifies() {
        // The 2026-09-28 log: tsc / VS Code / rg at 93-100 % CPU, the probe
        // at 0 ms, the foreground window answering in single milliseconds.
        // CPU is not an input any more, so there is nothing to feed it here;
        // the point is that the ticks such a machine produces stay silent.
        let mut d = Detector::default();
        let busy_but_fine = Tick {
            window_ms: Some(7.9),
            scheduler_ms: 0.0,
            ..CALM
        };
        assert_eq!(run(&mut d, busy_but_fine, 120), None);
    }

    #[test]
    fn a_window_slow_to_answer_for_four_seconds_of_six_is_a_stall() {
        let mut d = Detector::default();
        let slow = Tick {
            window_ms: Some(400.0),
            ..CALM
        };
        for _ in 0..NEEDED - 1 {
            assert_eq!(d.push(slow, normal()), None);
        }
        assert_eq!(d.push(CALM, normal()), None);
        assert_eq!(d.push(slow, normal()), Some(Trigger::Stall));
    }

    #[test]
    fn a_starved_scheduler_counts_even_when_the_window_still_answers() {
        let mut d = Detector::default();
        let starved = Tick {
            scheduler_ms: 80.0,
            ..CALM
        };
        assert_eq!(run(&mut d, starved, NEEDED), Some(Trigger::Stall));
    }

    #[test]
    fn a_single_hiccup_is_not_an_episode() {
        let mut d = Detector::default();
        let slow = Tick {
            window_ms: Some(900.0),
            scheduler_ms: 200.0,
            ..CALM
        };
        for _ in 0..20 {
            assert_eq!(d.push(slow, normal()), None);
            for _ in 0..WINDOW {
                assert_eq!(d.push(CALM, normal()), None);
            }
        }
    }

    #[test]
    fn sensitivity_moves_the_line_a_stall_must_cross() {
        let tick = Tick {
            window_ms: Some(300.0),
            ..CALM
        };
        let outcome = |s: Sensitivity| {
            let mut d = Detector::default();
            (0..NEEDED)
                .map(|_| d.push(tick, s.thresholds()))
                .last()
                .flatten()
        };
        assert_eq!(outcome(Sensitivity::Relaxed), None);
        assert_eq!(outcome(Sensitivity::Normal), Some(Trigger::Stall));
        assert_eq!(outcome(Sensitivity::Sensitive), Some(Trigger::Stall));
    }

    #[test]
    fn nothing_is_reported_while_nobody_is_at_the_machine() {
        let mut d = Detector::default();
        let away_and_awful = Tick {
            window_ms: Some(5_000.0),
            scheduler_ms: 900.0,
            idle_secs: IDLE_SECS,
            ..CALM
        };
        assert_eq!(run(&mut d, away_and_awful, 30), None);
    }

    #[test]
    fn coming_back_does_not_inherit_what_built_up_before_leaving() {
        let mut d = Detector::default();
        let slow = Tick {
            window_ms: Some(400.0),
            ..CALM
        };
        run(&mut d, slow, NEEDED - 1);
        d.push(
            Tick {
                idle_secs: IDLE_SECS,
                ..CALM
            },
            normal(),
        );
        assert_eq!(d.push(slow, normal()), None);
    }

    #[test]
    fn paging_needs_both_a_full_memory_and_a_fault_storm() {
        let mut d = Detector::default();
        let faults_with_room = Tick {
            hard_faults: 5_000.0,
            memory_load: 60,
            ..CALM
        };
        assert_eq!(run(&mut d, faults_with_room, 10), None);
        let full_and_quiet = Tick {
            memory_load: 97,
            ..CALM
        };
        assert_eq!(run(&mut d, full_and_quiet, 10), None);
        let thrashing = Tick {
            hard_faults: 5_000.0,
            memory_load: 97,
            ..CALM
        };
        assert_eq!(run(&mut d, thrashing, NEEDED), Some(Trigger::Paging));
    }

    #[test]
    fn a_hung_window_must_stay_hung_for_the_same_process() {
        let mut d = Detector::default();
        let hung = |pid| Tick {
            hung: Some(key(pid)),
            ..CALM
        };
        for _ in 0..HUNG_TICKS - 1 {
            assert_eq!(d.push(hung(7), normal()), None);
        }
        assert_eq!(d.push(hung(8), normal()), None);
        for _ in 0..HUNG_TICKS - 2 {
            assert_eq!(d.push(hung(8), normal()), None);
        }
        assert_eq!(d.push(hung(8), normal()), Some(Trigger::Hung(key(8))));
    }

    #[test]
    fn a_hung_window_outranks_paging_which_outranks_a_stall() {
        let everything = Tick {
            window_ms: Some(2_000.0),
            scheduler_ms: 500.0,
            hard_faults: 9_000.0,
            memory_load: 99,
            commit_load: Some(97),
            hung: Some(key(3)),
            idle_secs: 0,
        };
        let mut d = Detector::default();
        assert_eq!(
            run(&mut d, everything, HUNG_TICKS),
            Some(Trigger::Hung(key(3)))
        );
        let mut d = Detector::default();
        let no_hang = Tick {
            hung: None,
            ..everything
        };
        assert_eq!(run(&mut d, no_hang, NEEDED), Some(Trigger::Commit));
        let mut d = Detector::default();
        let room_to_commit = Tick {
            commit_load: Some(50),
            ..no_hang
        };
        assert_eq!(run(&mut d, room_to_commit, NEEDED), Some(Trigger::Paging));
    }

    #[test]
    fn commit_near_the_limit_for_four_seconds_of_six_warns_before_anything_is_felt() {
        let mut d = Detector::default();
        // The 2026-10-05 shape: RAM half free, no faults, windows answering,
        // and commit about to run out.
        let short = Tick {
            commit_load: Some(COMMIT_LOAD),
            ..CALM
        };
        for _ in 0..NEEDED - 1 {
            assert_eq!(d.push(short, normal()), None);
        }
        assert_eq!(d.push(CALM, normal()), None);
        assert_eq!(d.push(short, normal()), Some(Trigger::Commit));
    }

    #[test]
    fn commit_just_below_the_line_or_unreadable_never_warns() {
        let mut d = Detector::default();
        let below = Tick {
            commit_load: Some(COMMIT_LOAD - 1),
            ..CALM
        };
        assert_eq!(run(&mut d, below, 30), None);
        let unknown = Tick {
            commit_load: None,
            ..CALM
        };
        assert_eq!(run(&mut d, unknown, 30), None);
    }

    #[test]
    fn a_hung_window_outranks_a_commit_warning() {
        let mut d = Detector::default();
        let both = Tick {
            commit_load: Some(99),
            hung: Some(key(4)),
            ..CALM
        };
        assert_eq!(run(&mut d, both, HUNG_TICKS), Some(Trigger::Hung(key(4))));
    }

    #[test]
    fn commit_load_is_the_share_of_the_limit_already_promised() {
        const GB: u64 = 1 << 30;
        // 192 GB RAM + 16 GB page file, 20 GB left: the 2026-10-05 machine.
        assert_eq!(commit_load(208 * GB, 20 * GB), Some(90));
        assert_eq!(commit_load(208 * GB, 208 * GB), Some(0));
        assert_eq!(commit_load(208 * GB, 0), Some(100));
        // Available above the limit is a torn read, not negative use.
        assert_eq!(commit_load(100, 150), Some(0));
        assert_eq!(commit_load(0, 0), None);
        assert_eq!(commit_load(u64::MAX, 0), Some(100));
    }

    #[test]
    fn the_policy_never_nags_about_the_same_thing() {
        let t0 = Instant::now();
        let mut policy = Policy::default();
        assert!(policy.allow(key(1), "java.exe", t0));
        assert!(
            !policy.allow(key(2), "tsc.exe", t0 + Duration::from_secs(60)),
            "gap"
        );
        assert!(!policy.allow(key(1), "java.exe", t0 + GAP), "same process");
        assert!(policy.allow(key(2), "tsc.exe", t0 + GAP));
        assert!(policy.allow(key(1), "java.exe", t0 + SAME_TARGET + GAP));
    }

    #[test]
    fn ignore_silences_every_process_with_that_image_name() {
        let t0 = Instant::now();
        let mut policy = Policy::default();
        policy.snooze("Java.exe", t0);
        assert!(!policy.allow(key(9), "java.exe", t0 + Duration::from_secs(100)));
        assert!(policy.allow(key(9), "java.exe", t0 + SNOOZE));
    }
}
