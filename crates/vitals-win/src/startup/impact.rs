//! Measured startup impact: what a startup item actually cost this boot.
//!
//! ## Why this exists
//!
//! The module header next door explains that Task Manager's "Startup impact"
//! column ("High"/"Medium"/"Low") comes from a boot trace the OS collects,
//! and that inventing a substitute from image size would be a guess presented
//! as a measurement. That reasoning stands. This module does not guess — it
//! **measures**, with our own sampler, and reports nothing at all when it was
//! not running in time to do so.
//!
//! ## What is measured
//!
//! For the first [`BOOT_WINDOW_SECS`] seconds *after the machine booted*,
//! every tick contributes to a per-executable tally:
//!
//! - `cpu_ms` — CPU milliseconds, from each process's share of the machine
//!   multiplied by the **measured** interval, not the nominal one. Under load
//!   the sampler runs late, and assuming a perfect 1000 ms is how monitors end
//!   up reporting work that never happened.
//! - `disk_bytes` — read plus write bytes over the same interval, from the
//!   per-process rates the sampler already computed.
//!
//! The window is anchored to `uptime_secs`, not to when the app started.
//! Launching Vitals ten minutes into a session must not produce a "startup
//! impact" measured over ten-minutes-in idle — so if the first sample already
//! lands outside the window, this measures nothing and every figure stays
//! `None`. That is the single distinction the whole module exists to preserve:
//! **unmeasured is not zero**. A zero here would tell the user an entry costs
//! nothing at boot, which is exactly the fact they would act on.
//!
//! ## Matching a tally to a startup entry
//!
//! Keyed on the executable, normalised case-insensitively — Windows paths are
//! case-insensitive and the registry, the Start Menu and the task scheduler
//! each spell them differently. A full path is preferred; when the image path
//! could not be resolved (a protected process denies the handle) the file name
//! is used instead, which is weaker but still true of the process observed.

use std::collections::{BTreeMap, HashMap};

use vitals_core::ids::ProcessKey;

/// How long after boot the measurement window stays open.
///
/// Two minutes: long enough for the logon storm to finish on a slow disk,
/// short enough that ordinary use afterwards is not attributed to startup.
/// Anything the user launches themselves in the first two minutes will be
/// counted, which is the honest limitation — this measures what ran, not what
/// Windows decided to run.
pub const BOOT_WINDOW_SECS: u64 = 120;

/// One tick's observation of one process.
///
/// Deliberately not a `&Process`: the accumulator takes exactly the four
/// numbers it uses, so it can be unit-tested without constructing a
/// forty-field process row, and so nothing here depends on the sampler's
/// internal shape.
#[derive(Debug, Clone)]
pub struct Observation {
    /// Race-free identity, used only as a cache key for path resolution.
    pub key: ProcessKey,
    /// Executable file name (`chrome.exe`), which is all the per-tick
    /// enumeration carries.
    pub name: Option<String>,
    /// Share of the whole machine, 0..100.
    pub cpu_percent: f32,
    /// Read plus write bytes per second over this interval.
    pub disk_bytes_per_sec: u64,
}

/// What one executable cost during the boot window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
    pub cpu_ms: u64,
    pub disk_bytes: u64,
}

/// A completed measurement, as persisted and as read back next run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredWindow {
    /// Unix milliseconds at which the window closed. Shown to the user, so
    /// they can see whether the figures describe this boot or the last one.
    pub measured_at_ms: u64,
    /// How long the window was, in seconds. Persisted rather than assumed:
    /// a file written by an older build must not be read as if it used the
    /// current constant.
    pub window_secs: u64,
    /// Normalised executable key to tally. `BTreeMap` so the persisted file
    /// is stable and diffable rather than reordering on every write.
    pub totals: BTreeMap<String, Tally>,
}

impl MeasuredWindow {
    /// The tally for an executable path, if one was measured.
    ///
    /// Tries the full path first and the file name second, matching how
    /// [`ImpactAccumulator`] stores them. Returns `None` — never a zero
    /// tally — when the executable was never seen: "we did not observe this
    /// running" and "this ran and cost nothing" are different statements, and
    /// only the second is a measurement.
    #[must_use]
    pub fn lookup(&self, image_path: &str) -> Option<Tally> {
        let full = normalise_path(image_path);
        if let Some(tally) = self.totals.get(&full) {
            return Some(*tally);
        }
        let name = file_key(&full);
        if name == full {
            return None;
        }
        self.totals.get(&name).copied()
    }
}

/// Lower-cases a path and squares up its separators.
///
/// Windows paths are case-insensitive, and the same executable is spelled
/// `C:\A\b.EXE` in a registry value and `c:/a/b.exe` in a shortcut. Surrounding
/// quotes and whitespace are stripped for the same reason: a `Run` value's
/// command is quoted, the resolved image path is not.
#[must_use]
pub fn normalise_path(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .trim()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

/// The final path segment of an already-normalised path.
#[must_use]
fn file_key(normalised: &str) -> String {
    normalised
        .rsplit('\\')
        .next()
        .unwrap_or(normalised)
        .to_owned()
}

/// Accumulates per-executable cost across the boot window.
///
/// Pure: it performs no syscalls and knows nothing about Windows. Resolving a
/// PID to a full image path needs a process handle, so that is passed in as a
/// closure by the caller, which can cache it and can stop paying for it the
/// moment the window closes.
#[derive(Debug, Default)]
pub struct ImpactAccumulator {
    totals: HashMap<String, Tally>,
    /// Resolved tally key per process, so the caller's `resolve` — a handle
    /// open per call — runs once per process, not once per tick. Keyed on the
    /// full [`ProcessKey`], so a recycled PID is asked afresh.
    keys: HashMap<ProcessKey, Option<String>>,
    /// Whether at least one sample landed inside the window.
    ///
    /// The difference between `Some(0)` and `None` for the whole machine. An
    /// app started an hour into the session never sets this, so it reports
    /// nothing rather than a window of zeroes.
    measured_any: bool,
    /// Set once the window has closed, so later ticks cost nothing at all.
    closed: bool,
}

impl ImpactAccumulator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the window is still open at this uptime.
    ///
    /// Cheap enough to call every tick, and the caller uses it to skip
    /// resolving image paths — the only expensive part — once it is false.
    #[must_use]
    pub const fn is_measuring(&self, uptime_secs: u64) -> bool {
        !self.closed && uptime_secs < BOOT_WINDOW_SECS
    }

    /// Whether anything was measured at all.
    #[must_use]
    pub const fn measured_anything(&self) -> bool {
        self.measured_any
    }

    /// Folds one tick into the tally.
    ///
    /// `resolve` is asked for the full image path of a process the first time
    /// it is seen; returning `None` is fine and falls back to the file name.
    /// Returns `true` when the tick counted, so the caller can tell an open
    /// window from a closed one without a second call.
    pub fn observe<F>(
        &mut self,
        uptime_secs: u64,
        elapsed_ms: u32,
        observations: &[Observation],
        mut resolve: F,
    ) -> bool
    where
        F: FnMut(ProcessKey) -> Option<String>,
    {
        if !self.is_measuring(uptime_secs) {
            // Once past the window it stays past it. Latching means a machine
            // that resumes from sleep — where uptime keeps climbing — cannot
            // somehow re-enter the window, and means the cost of this call
            // afterwards is one comparison.
            self.closed = true;
            return false;
        }

        // A tick that straddles the boundary counts only the part inside it.
        // Without this, a sample taken at 119 s with a 20 s background
        // interval would attribute twenty seconds of ordinary use to startup.
        let remaining_ms = (BOOT_WINDOW_SECS.saturating_sub(uptime_secs)).saturating_mul(1000);
        let counted_ms = u64::from(elapsed_ms).min(remaining_ms);
        if counted_ms == 0 {
            return false;
        }

        self.measured_any = true;

        for observation in observations {
            let Some(key) = self.key_for(observation, &mut resolve) else {
                continue;
            };

            // f64 rather than f32: a 100 % process over a two-minute window is
            // 120 000 cpu-ms, and f32 has only 24 bits of mantissa, so the
            // running total would start losing whole milliseconds part way
            // through the window it is meant to measure.
            let cpu_ms = f64::from(observation.cpu_percent).max(0.0) * counted_ms as f64 / 100.0;
            let disk_bytes = u128::from(observation.disk_bytes_per_sec)
                .saturating_mul(u128::from(counted_ms))
                / 1000;

            let tally = self.totals.entry(key).or_default();
            tally.cpu_ms = tally.cpu_ms.saturating_add(cpu_ms.round() as u64);
            tally.disk_bytes = tally
                .disk_bytes
                .saturating_add(u64::try_from(disk_bytes).unwrap_or(u64::MAX));
        }

        true
    }

    /// The key this observation should be tallied under, resolving the image
    /// path once per process and remembering the answer.
    fn key_for<F>(&mut self, observation: &Observation, resolve: &mut F) -> Option<String>
    where
        F: FnMut(ProcessKey) -> Option<String>,
    {
        if let Some(cached) = self.keys.get(&observation.key) {
            return cached.clone();
        }
        let key = Self::resolve_key(observation, resolve);
        self.keys.insert(observation.key, key.clone());
        key
    }

    fn resolve_key<F>(observation: &Observation, resolve: &mut F) -> Option<String>
    where
        F: FnMut(ProcessKey) -> Option<String>,
    {
        if let Some(path) = resolve(observation.key) {
            let normalised = normalise_path(&path);
            if !normalised.is_empty() {
                return Some(normalised);
            }
        }
        let name = observation.name.as_ref()?;
        let normalised = normalise_path(name);
        if normalised.is_empty() {
            None
        } else {
            Some(file_key(&normalised))
        }
    }

    /// The completed measurement, or `None` when nothing was measured.
    ///
    /// `None` is the whole point: a run that started after the window closed
    /// must report "not measured", never a window in which everything cost
    /// zero.
    #[must_use]
    pub fn finish(&self, measured_at_ms: u64) -> Option<MeasuredWindow> {
        if !self.measured_any {
            return None;
        }
        Some(MeasuredWindow {
            measured_at_ms,
            window_secs: BOOT_WINDOW_SECS,
            totals: self
                .totals
                .iter()
                .map(|(key, tally)| (key.clone(), *tally))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::ids::Pid;

    fn key(pid: u32) -> ProcessKey {
        ProcessKey::new(Pid(pid), u64::from(pid) * 1_000)
    }

    fn observation(pid: u32, name: &str, cpu: f32, disk: u64) -> Observation {
        Observation {
            key: key(pid),
            name: Some(name.to_owned()),
            cpu_percent: cpu,
            disk_bytes_per_sec: disk,
        }
    }

    #[test]
    fn a_windows_path_matches_the_same_path_spelled_in_another_case() {
        assert_eq!(
            normalise_path("C:\\A\\b.EXE"),
            normalise_path("c:\\a\\b.exe")
        );
        // Quoted, forward-slashed, mixed case — how a Run value spells it.
        assert_eq!(normalise_path(" \"C:/A/b.EXE\" "), "c:\\a\\b.exe");
    }

    #[test]
    fn a_tally_is_found_by_the_same_executable_written_differently() {
        let mut accumulator = ImpactAccumulator::new();
        accumulator.observe(0, 1_000, &[observation(1, "b.exe", 50.0, 0)], |_| {
            Some("C:\\A\\b.EXE".to_owned())
        });

        let window = accumulator.finish(7).expect("the window was open");
        assert_eq!(
            window.lookup("c:/a/b.exe").map(|t| t.cpu_ms),
            Some(500),
            "the same image spelled differently must resolve to the same tally"
        );
    }

    #[test]
    fn cpu_milliseconds_use_the_measured_interval_not_the_nominal_one() {
        let mut accumulator = ImpactAccumulator::new();
        // 25 % of the machine for 1 400 ms is 350 cpu-ms. Dividing by a
        // nominal 1 000 ms would claim 250 and understate every figure on a
        // machine that is busy enough to make the sampler run late — which is
        // precisely the machine the user is looking at this screen about.
        accumulator.observe(0, 1_400, &[observation(1, "b.exe", 25.0, 2_000)], |_| None);

        let window = accumulator.finish(7).expect("the window was open");
        let tally = window.lookup("b.exe").expect("b.exe ran");
        assert_eq!(tally.cpu_ms, 350);
        assert_eq!(tally.disk_bytes, 2_800);
    }

    #[test]
    fn a_process_seen_after_the_window_closed_is_not_counted() {
        let mut accumulator = ImpactAccumulator::new();
        accumulator.observe(10, 1_000, &[observation(1, "b.exe", 100.0, 0)], |_| None);
        let counted = accumulator.observe(
            BOOT_WINDOW_SECS + 5,
            1_000,
            &[observation(2, "late.exe", 100.0, 5_000)],
            |_| None,
        );

        assert!(
            !counted,
            "a sample outside the window must not be folded in"
        );
        let window = accumulator.finish(7).expect("the window was open earlier");
        assert_eq!(window.lookup("b.exe").map(|t| t.cpu_ms), Some(1_000));
        assert_eq!(
            window.lookup("late.exe"),
            None,
            "an executable only seen after the window must be absent, not zero"
        );
    }

    #[test]
    fn a_tick_straddling_the_boundary_counts_only_the_part_inside_it() {
        let mut accumulator = ImpactAccumulator::new();
        // Hidden windows sample every 20 s. At 119 s uptime only one second
        // of that interval belongs to the boot window.
        accumulator.observe(
            BOOT_WINDOW_SECS - 1,
            20_000,
            &[observation(1, "b.exe", 100.0, 1_000)],
            |_| None,
        );

        let window = accumulator.finish(7).expect("the window was open");
        let tally = window.lookup("b.exe").expect("b.exe ran");
        assert_eq!(tally.cpu_ms, 1_000, "one second, not twenty");
        assert_eq!(tally.disk_bytes, 1_000);
    }

    #[test]
    fn a_machine_already_past_the_window_at_the_first_sample_measures_nothing() {
        // The failure this whole module is written to avoid: an app launched
        // an hour into the session must not report that every startup item
        // cost zero. Zero is a measurement; this is the absence of one.
        let mut accumulator = ImpactAccumulator::new();
        accumulator.observe(
            BOOT_WINDOW_SECS + 1,
            1_000,
            &[observation(1, "b.exe", 90.0, 9_000)],
            |_| Some("C:\\A\\b.exe".to_owned()),
        );

        assert!(!accumulator.measured_anything());
        assert!(
            accumulator.finish(7).is_none(),
            "no window means None, never an empty window of zeroes"
        );
    }

    #[test]
    fn the_window_does_not_reopen_once_it_has_closed() {
        // Uptime climbs monotonically, but a resume from sleep or a clock
        // adjustment must not be able to re-enter the window and start
        // attributing ordinary use to startup.
        let mut accumulator = ImpactAccumulator::new();
        accumulator.observe(0, 1_000, &[observation(1, "b.exe", 10.0, 0)], |_| None);
        accumulator.observe(BOOT_WINDOW_SECS, 1_000, &[], |_| None);
        let reopened = accumulator.observe(1, 1_000, &[observation(1, "b.exe", 10.0, 0)], |_| None);

        assert!(!reopened);
        assert_eq!(
            accumulator
                .finish(7)
                .and_then(|w| w.lookup("b.exe"))
                .map(|t| t.cpu_ms),
            Some(100),
            "only the first tick counted"
        );
    }

    #[test]
    fn an_unresolvable_image_path_still_tallies_under_its_file_name() {
        // Protected processes deny the handle a full path needs. A file-name
        // match is weaker than a path match, but it is still something we
        // observed — dropping the row would understate the boot.
        let mut accumulator = ImpactAccumulator::new();
        accumulator.observe(0, 1_000, &[observation(1, "guard.exe", 40.0, 0)], |_| None);

        let window = accumulator.finish(7).expect("the window was open");
        assert_eq!(
            window
                .lookup("C:\\Program Files\\Guard\\guard.exe")
                .map(|t| t.cpu_ms),
            Some(400),
            "a full path must fall back to the file-name tally"
        );
    }

    #[test]
    fn the_image_path_is_resolved_once_per_process_not_once_per_tick() {
        // `resolve` opens a process handle. Paying that for ~650 processes
        // on every tick of the window would be the per-tick cost the
        // enumerator exists to avoid.
        let mut accumulator = ImpactAccumulator::new();
        let mut calls = 0_u32;
        for _ in 0..5 {
            accumulator.observe(0, 1_000, &[observation(1, "b.exe", 10.0, 0)], |_| {
                calls += 1;
                Some("C:\\A\\b.exe".to_owned())
            });
        }
        assert_eq!(calls, 1);
    }

    #[test]
    fn an_executable_that_never_ran_has_no_tally_rather_than_a_zero_one() {
        let mut accumulator = ImpactAccumulator::new();
        accumulator.observe(0, 1_000, &[observation(1, "b.exe", 10.0, 0)], |_| None);

        let window = accumulator.finish(7).expect("the window was open");
        assert_eq!(window.lookup("c:\\never\\ran.exe"), None);
    }

    #[test]
    fn a_measured_window_survives_a_round_trip_through_json() {
        // It is written to disk so the next run can show the last boot's
        // figures. A field that does not round-trip would silently become a
        // window of zeroes, which is the one thing this must never show.
        let mut accumulator = ImpactAccumulator::new();
        accumulator.observe(0, 1_000, &[observation(1, "b.exe", 50.0, 4_000)], |_| None);
        let window = accumulator.finish(1_700_000_000_000).expect("measured");

        let json = serde_json::to_string(&window).expect("serialises");
        let read_back: MeasuredWindow = serde_json::from_str(&json).expect("deserialises");

        assert_eq!(read_back, window);
        assert_eq!(read_back.lookup("b.exe").map(|t| t.disk_bytes), Some(4_000));
    }
}
