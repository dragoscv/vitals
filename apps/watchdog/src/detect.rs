//! When the machine is lagging, and when to say so.
//!
//! Pure, driven by one [`Tick`] a second, so thresholds and the
//! don't-nag rules are testable without loading a real machine.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use vitals_core::ids::ProcessKey;

/// Machine CPU at or above this counts towards a lag episode.
pub const BUSY: f64 = 0.90;
/// Scheduling delay of an ordinary-priority thread, in ms, that counts.
///
/// This is the signal that matches what a person feels: the probe thread
/// runs at the same priority as the cursor-drawing and input threads of
/// every normal app, so when it is woken 40 ms late, so are they.
pub const LAG_MS: f64 = 40.0;
/// `dwMemoryLoad` at or above this counts as memory pressure.
pub const MEMORY_LOAD: u32 = 95;
/// Ticks looked back over.
pub const WINDOW: u32 = 8;
/// Ticks within [`WINDOW`] that must be bad. A single compile spike is not a
/// lag episode; five bad seconds out of eight is.
pub const NEEDED: u32 = 5;
/// Consecutive ticks the foreground window must be hung.
pub const HUNG_TICKS: u32 = 5;

/// One second of observation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Machine CPU, 0..=1.
    pub busy: f64,
    /// Worst wake-up delay of the probe thread during the tick, ms.
    pub lag_ms: f64,
    /// Physical memory in use, percent.
    pub memory_load: u32,
    /// The foreground window's process, when Windows says it is hung.
    pub hung: Option<ProcessKey>,
}

/// Why a notification is warranted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Sustained CPU saturation or scheduling delay.
    Busy,
    /// Sustained memory pressure.
    Memory,
    /// The foreground app stopped answering.
    Hung(ProcessKey),
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
}

/// Turns ticks into triggers.
#[derive(Debug, Default)]
pub struct Detector {
    busy: Window,
    memory: Window,
    hung: Option<(ProcessKey, u32)>,
}

impl Detector {
    /// Feeds one tick; returns what, if anything, is now sustained.
    ///
    /// A hung window outranks memory, which outranks CPU: a frozen app is the
    /// most specific explanation, and paging makes the CPU look idle while
    /// everything waits on the disk.
    pub fn push(&mut self, tick: Tick) -> Option<Trigger> {
        let busy = self.busy.push(tick.busy >= BUSY || tick.lag_ms >= LAG_MS);
        let memory = self.memory.push(tick.memory_load >= MEMORY_LOAD);

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
        if memory >= NEEDED {
            return Some(Trigger::Memory);
        }
        (busy >= NEEDED).then_some(Trigger::Busy)
    }
}

/// Minimum gap between any two proposals.
pub const GAP: Duration = Duration::from_secs(60);
/// How long the same process is not proposed again.
pub const SAME_TARGET: Duration = Duration::from_secs(5 * 60);
/// How long "Ignore" silences an image name.
pub const SNOOZE: Duration = Duration::from_secs(30 * 60);

/// The don't-nag rules.
///
/// A notification that repeats every second while a build finishes is worse
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
    use vitals_core::ids::Pid;

    const CALM: Tick = Tick {
        busy: 0.2,
        lag_ms: 1.0,
        memory_load: 40,
        hung: None,
    };
    const HOT: Tick = Tick { busy: 0.97, ..CALM };

    fn key(pid: u32) -> ProcessKey {
        ProcessKey::new(Pid(pid), 1)
    }

    #[test]
    fn a_short_spike_is_not_an_episode_but_five_bad_seconds_of_eight_are() {
        let mut d = Detector::default();
        for _ in 0..4 {
            assert_eq!(d.push(HOT), None);
        }
        assert_eq!(d.push(CALM), None);
        assert_eq!(d.push(HOT), Some(Trigger::Busy));
    }

    #[test]
    fn scheduling_delay_alone_counts_even_when_the_cpu_meter_looks_fine() {
        let mut d = Detector::default();
        let laggy = Tick {
            lag_ms: 120.0,
            ..CALM
        };
        let got: Vec<_> = (0..NEEDED).map(|_| d.push(laggy)).collect();
        assert_eq!(got.last().copied().flatten(), Some(Trigger::Busy));
    }

    #[test]
    fn an_episode_ends_once_the_window_has_calmed_down() {
        let mut d = Detector::default();
        for _ in 0..WINDOW {
            d.push(HOT);
        }
        let mut last = Some(Trigger::Busy);
        for _ in 0..=(WINDOW - NEEDED) {
            last = d.push(CALM);
        }
        assert_eq!(last, None);
    }

    #[test]
    fn a_hung_window_must_stay_hung_for_the_same_process() {
        let mut d = Detector::default();
        let hung = |pid| Tick {
            hung: Some(key(pid)),
            ..CALM
        };
        for _ in 0..HUNG_TICKS - 1 {
            assert_eq!(d.push(hung(7)), None);
        }
        // Focus moved to another hung app: the count restarts.
        assert_eq!(d.push(hung(8)), None);
        for _ in 0..HUNG_TICKS - 2 {
            assert_eq!(d.push(hung(8)), None);
        }
        assert_eq!(d.push(hung(8)), Some(Trigger::Hung(key(8))));
    }

    #[test]
    fn a_hung_window_outranks_memory_which_outranks_cpu() {
        let mut d = Detector::default();
        let everything = Tick {
            busy: 1.0,
            lag_ms: 500.0,
            memory_load: 99,
            hung: Some(key(3)),
        };
        let mut last = None;
        for _ in 0..HUNG_TICKS {
            last = d.push(everything);
        }
        assert_eq!(last, Some(Trigger::Hung(key(3))));
        let mut d = Detector::default();
        for _ in 0..NEEDED {
            last = d.push(Tick {
                hung: None,
                ..everything
            });
        }
        assert_eq!(last, Some(Trigger::Memory));
    }

    #[test]
    fn the_policy_never_nags_about_the_same_thing() {
        let t0 = Instant::now();
        let mut policy = Policy::default();
        assert!(policy.allow(key(1), "java.exe", t0));
        assert!(
            !policy.allow(key(2), "tsc.exe", t0 + Duration::from_secs(10)),
            "gap"
        );
        assert!(
            !policy.allow(key(1), "java.exe", t0 + Duration::from_secs(120)),
            "same"
        );
        assert!(policy.allow(key(2), "tsc.exe", t0 + Duration::from_secs(120)));
        assert!(policy.allow(key(1), "java.exe", t0 + Duration::from_secs(400)));
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
