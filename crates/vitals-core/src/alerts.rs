//! Turns readings into the one sentence the user came for — and remembers
//! having said it.
//!
//! # Why this is in Rust
//!
//! The rules first lived in the webview, recomputed from scratch every second
//! and thrown away. That was fine for a dashboard widget and useless for
//! everything else: a notification needs to fire once, not sixty times a
//! minute; the tray tooltip needs the list while the window is hidden; the
//! LAN API and the phone need it at all. One evaluation here feeds every
//! consumer, and it keeps running if the webview crashes.
//!
//! # The problem with the numbers alone
//!
//! "Memory 87 %" is not actionable: alarming on a machine that is thrashing,
//! completely normal on one with a large file cache. The distinction is in
//! the page-fault rate, which no consumer task manager surfaces. Every rule
//! here is written against the signal that actually predicts a machine
//! feeling slow, and each alert carries a `cause` key explaining *why* rather
//! than restating the number.
//!
//! # Sustained, then sticky
//!
//! Two kinds of memory, both essential to not being ignored:
//!
//! - **Sustain.** A condition must hold for [`SUSTAIN_SAMPLES`] consecutive
//!   samples before it is raised. CPU touches 100 % during any application
//!   launch; a monitor that shouts about it teaches the user to tune it out.
//! - **Hysteresis.** Once raised, an alert clears only when the condition
//!   has been false for [`CLEAR_SAMPLES`] consecutive samples. Without this,
//!   a reading hovering at the threshold raises and clears every second.
//!
//! On top of that, [`Engine::poll`] reports *transitions* — raised, cleared —
//! separately from the current list, so a notifier acts once per episode and
//! a cooldown ([`RENOTIFY_AFTER_SAMPLES`]) stops a flapping condition from
//! producing a toast storm even when it legitimately re-raises.
//!
//! # Nothing here is a diagnosis
//!
//! These are heuristics over what the OS reports, phrased as observations
//! ("the disk has been saturated for a minute") rather than verdicts. The one
//! exception is SMART, which is the drive's own assessment of itself.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::metrics::{SystemMetrics, ThrottleReason};

/// Consecutive samples a condition must hold before it is raised. Fifteen
/// seconds at 1 Hz.
pub const SUSTAIN_SAMPLES: u32 = 15;

/// Consecutive samples a condition must be false before a raised alert
/// clears. Shorter than the raise window on purpose: the cost of clearing a
/// moment early is a re-raise (throttled by the cooldown); the cost of
/// clearing late is a stale warning the user learns to distrust.
pub const CLEAR_SAMPLES: u32 = 10;

/// Samples that must pass after an alert clears before the same alert may
/// notify again. Five minutes at 1 Hz. It still appears in the list
/// immediately — only the *notification* is suppressed.
pub const RENOTIFY_AFTER_SAMPLES: u32 = 300;

/// Every condition the engine knows. Serialised in camelCase because the
/// name is part of the wire contract and the key into both locales
/// (`dashboard.alert.<kind>.title`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum AlertKind {
    CpuSustained,
    CpuThrottled,
    MemoryPressure,
    MemoryCommit,
    DiskSaturated,
    DiskLatency,
    DiskSpace,
    DiskHealth,
    GpuThrottled,
    ThermalCpu,
    NetworkErrors,
    BatteryLow,
    BatteryHealth,
}

impl AlertKind {
    /// Whether the condition must be sustained before it is raised.
    ///
    /// Instantaneous conditions are the ones where a single true sample is
    /// already meaningful: SMART failing, a battery at 9 %, a throttle flag
    /// the firmware set. Rates and utilisations spike and need the window.
    #[must_use]
    pub const fn requires_sustain(self) -> bool {
        matches!(
            self,
            Self::CpuSustained
                | Self::DiskSaturated
                | Self::DiskLatency
                | Self::ThermalCpu
                | Self::NetworkErrors
                | Self::MemoryPressure
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Where the user should go to act on an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum AlertRoute {
    Performance,
    Processes,
    Storage,
    Network,
    Devices,
}

/// An alert as consumers see it.
///
/// `title` and `cause` are i18n keys, not prose: the desktop, the phone and
/// a notification all render the same alert in the user's language, and the
/// server must not bake one language into the wire. `values` are already
/// rounded for display so no consumer re-derives them differently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub kind: AlertKind,
    pub severity: Severity,
    /// Distinguishes several instances of one kind: the disk mount, the GPU
    /// name, the adapter. Empty for machine-wide alerts.
    pub subject: String,
    /// Key into `dashboard.alert.<kind>.title`.
    pub title: String,
    /// Key into `dashboard.alert.<kind>.<cause>` — the "why".
    pub cause: String,
    /// Sorted by key so the same alert serialises to the same bytes: with a
    /// `HashMap` the order changed per process, so the Android contract
    /// fixture rewrote itself on every test run and `check-drift` flapped.
    pub values: BTreeMap<String, AlertValue>,
    pub route: Option<AlertRoute>,
    /// Sample index at which this alert was raised, for "for 4 minutes".
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub since_sample: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(untagged)]
pub enum AlertValue {
    Number(f64),
    Text(String),
}

impl From<f64> for AlertValue {
    fn from(v: f64) -> Self {
        Self::Number(v)
    }
}
impl From<u64> for AlertValue {
    fn from(v: u64) -> Self {
        // Precision loss above 2^53 is irrelevant for display values.
        #[allow(clippy::cast_precision_loss)]
        Self::Number(v as f64)
    }
}
impl From<i64> for AlertValue {
    fn from(v: i64) -> Self {
        #[allow(clippy::cast_precision_loss)]
        Self::Number(v as f64)
    }
}
impl From<&str> for AlertValue {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}
impl From<String> for AlertValue {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}

/// What changed on this tick. A notifier acts on these; a list view ignores
/// them and reads [`Engine::active`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AlertTransitions {
    /// Newly raised this tick **and** past the cooldown — safe to notify.
    pub raised: Vec<Alert>,
    /// Newly raised but inside the cooldown from a recent episode. Shown in
    /// the list, not toasted.
    pub raised_quietly: Vec<Alert>,
    pub cleared: Vec<Alert>,
}

impl AlertTransitions {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.raised.is_empty() && self.raised_quietly.is_empty() && self.cleared.is_empty()
    }
}

/// Thresholds, chosen against a documented failure rather than a round
/// number. Public so a test asserts the boundary rather than re-deriving it.
pub mod thresholds {
    /// Below this a machine still has headroom for interactive work.
    pub const CPU_SUSTAINED_PERCENT: f32 = 90.0;
    /// Page faults per second indicating real memory pressure — the signal
    /// that separates "RAM full of useful cache" from "swapping".
    pub const PAGE_FAULTS_PER_SEC: u64 = 2000;
    /// Available memory below this share, *together with* faulting.
    pub const AVAILABLE_RATIO: f64 = 0.10;
    /// Commit approaching the limit means allocations are about to fail.
    pub const COMMIT_PERCENT: f64 = 90.0;
    /// Disk busy essentially all the time.
    pub const DISK_ACTIVE_PERCENT: f32 = 95.0;
    /// Only meaningful alongside saturation: the combination indicates a
    /// queue rather than a slow medium.
    pub const DISK_RESPONSE_MS: f32 = 25.0;
    /// Free space below which Windows itself starts misbehaving.
    pub const DISK_FREE_PERCENT: f64 = 5.0;
    /// Below this, critical rather than warning.
    pub const DISK_FREE_CRITICAL_PERCENT: f64 = 1.0;
    pub const CPU_TEMPERATURE_C: f32 = 95.0;
    /// Dropped packets per second — the first sign of a bad cable.
    pub const NETWORK_ERRORS_PER_SEC: u64 = 10;
    pub const BATTERY_LOW_PERCENT: f32 = 10.0;
    /// Design-capacity retention below which a battery is worth replacing.
    pub const BATTERY_HEALTH_PERCENT: f32 = 60.0;
}

/// Identity of an alert across ticks: the kind plus the thing it is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    kind: AlertKind,
    subject: String,
}

/// Per-key memory.
#[derive(Debug, Clone, Default)]
struct Track {
    /// Consecutive samples the condition has been true (while not raised).
    true_run: u32,
    /// Consecutive samples the condition has been false (while raised).
    false_run: u32,
    /// The alert as raised, or `None` while not raised.
    raised: Option<Alert>,
    /// Sample index at which this key last cleared, for the cooldown.
    last_cleared: Option<u64>,
}

/// The stateful evaluator. One per machine, fed every sample.
#[derive(Debug, Default)]
pub struct Engine {
    tracks: HashMap<Key, Track>,
    sample: u64,
}

impl Engine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Alerts currently raised, most serious first, stable within a severity.
    ///
    /// Stability matters: this is read every second, and an unstable order
    /// would let two warnings swap places on every tick.
    #[must_use]
    pub fn active(&self) -> Vec<Alert> {
        let mut out: Vec<Alert> = self
            .tracks
            .values()
            .filter_map(|t| t.raised.clone())
            .collect();
        out.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.since_sample.cmp(&b.since_sample))
                .then_with(|| a.subject.cmp(&b.subject))
        });
        out
    }

    /// Number of samples seen. Exposed so a consumer can turn `since_sample`
    /// into a duration.
    #[must_use]
    pub const fn sample_index(&self) -> u64 {
        self.sample
    }

    /// Feeds one sample and returns what changed.
    ///
    /// Every condition is evaluated every tick, including those that are not
    /// currently true — that is how a raised alert learns it should clear.
    pub fn poll(&mut self, system: &SystemMetrics) -> AlertTransitions {
        self.sample = self.sample.saturating_add(1);
        let now = self.sample;

        // Everything that is true right now. Keys absent from this set are
        // false this tick.
        let candidates = evaluate(system);
        let mut seen: Vec<Key> = Vec::with_capacity(candidates.len());

        let mut transitions = AlertTransitions {
            raised: Vec::new(),
            raised_quietly: Vec::new(),
            cleared: Vec::new(),
        };

        for candidate in candidates {
            let key = Key {
                kind: candidate.kind,
                subject: candidate.subject.clone(),
            };
            seen.push(key.clone());
            let track = self.tracks.entry(key).or_default();
            track.false_run = 0;

            if let Some(raised) = &mut track.raised {
                // Already up: refresh the numbers (the cause string shows the
                // *current* value) but keep the original since_sample and
                // let a severity escalation through — 4 % free becoming 0.9 %
                // is news even though the alert is not new.
                let escalated = candidate.severity > raised.severity;
                let since = raised.since_sample;
                *raised = Alert {
                    since_sample: since,
                    ..candidate
                };
                if escalated {
                    transitions.raised.push(raised.clone());
                }
                continue;
            }

            track.true_run = track.true_run.saturating_add(1);
            let needed = if candidate.kind.requires_sustain() {
                SUSTAIN_SAMPLES
            } else {
                1
            };
            if track.true_run < needed {
                continue;
            }

            let alert = Alert {
                since_sample: now,
                ..candidate
            };
            track.raised = Some(alert.clone());
            track.true_run = 0;

            let in_cooldown = track
                .last_cleared
                .is_some_and(|at| now.saturating_sub(at) < u64::from(RENOTIFY_AFTER_SAMPLES));
            if in_cooldown {
                transitions.raised_quietly.push(alert);
            } else {
                transitions.raised.push(alert);
            }
        }

        // Everything not seen this tick is false this tick.
        for (key, track) in &mut self.tracks {
            if seen.contains(key) {
                continue;
            }
            track.true_run = 0;
            if track.raised.is_none() {
                continue;
            }
            track.false_run = track.false_run.saturating_add(1);
            if track.false_run >= CLEAR_SAMPLES {
                if let Some(alert) = track.raised.take() {
                    transitions.cleared.push(alert);
                }
                track.last_cleared = Some(now);
                track.false_run = 0;
            }
        }

        // Forget keys that are idle and past the cooldown, or the map grows
        // with every disk ever mounted and every adapter ever seen.
        self.tracks.retain(|_, t| {
            t.raised.is_some()
                || t.true_run > 0
                || t.last_cleared
                    .is_some_and(|at| now.saturating_sub(at) < u64::from(RENOTIFY_AFTER_SAMPLES))
        });

        transitions
    }
}

fn throttle_slug(reason: ThrottleReason) -> &'static str {
    match reason {
        ThrottleReason::Thermal => "thermal",
        ThrottleReason::PowerLimit => "powerLimit",
        ThrottleReason::CurrentLimit => "currentLimit",
        ThrottleReason::VoltageDrop => "voltageDrop",
        ThrottleReason::PowerPolicy => "powerPolicy",
        ThrottleReason::Unknown => "unknown",
    }
}

fn alert(
    kind: AlertKind,
    severity: Severity,
    subject: impl Into<String>,
    cause: &str,
    values: Vec<(&str, AlertValue)>,
    route: Option<AlertRoute>,
) -> Alert {
    let name = serde_variant_name(kind);
    Alert {
        kind,
        severity,
        subject: subject.into(),
        title: format!("alert.{name}.title"),
        cause: format!("alert.{name}.{cause}"),
        values: values.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        route,
        since_sample: 0,
    }
}

/// The camelCase name serde uses — the same string the locales key on.
fn serde_variant_name(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::CpuSustained => "cpuSustained",
        AlertKind::CpuThrottled => "cpuThrottled",
        AlertKind::MemoryPressure => "memoryPressure",
        AlertKind::MemoryCommit => "memoryCommit",
        AlertKind::DiskSaturated => "diskSaturated",
        AlertKind::DiskLatency => "diskLatency",
        AlertKind::DiskSpace => "diskSpace",
        AlertKind::DiskHealth => "diskHealth",
        AlertKind::GpuThrottled => "gpuThrottled",
        AlertKind::ThermalCpu => "thermalCpu",
        AlertKind::NetworkErrors => "networkErrors",
        AlertKind::BatteryLow => "batteryLow",
        AlertKind::BatteryHealth => "batteryHealth",
    }
}

fn round(v: f64) -> AlertValue {
    AlertValue::Number(v.round())
}

/// Stateless: every condition that is true *right now*. The engine adds the
/// memory.
#[allow(clippy::too_many_lines)] // One rule per condition, each a few lines; splitting hides the list.
fn evaluate(s: &SystemMetrics) -> Vec<Alert> {
    let mut out = Vec::new();

    if s.cpu.total.get() > thresholds::CPU_SUSTAINED_PERCENT {
        out.push(alert(
            AlertKind::CpuSustained,
            Severity::Warning,
            "",
            "cause",
            vec![
                ("percent", round(f64::from(s.cpu.total.get()))),
                ("seconds", u64::from(SUSTAIN_SAMPLES).into()),
            ],
            Some(AlertRoute::Processes),
        ));
    }

    if let Some(reason) = s.cpu.throttled {
        out.push(alert(
            AlertKind::CpuThrottled,
            Severity::Warning,
            "",
            throttle_slug(reason),
            vec![],
            Some(AlertRoute::Performance),
        ));
    }

    // Gated on faulting, not on the percentage. See the module note.
    if let Some(faults) = s.memory.page_faults_per_sec {
        let total = s.memory.total.get();
        #[allow(clippy::cast_precision_loss)]
        let available_ratio = if total > 0 {
            s.memory.available.get() as f64 / total as f64
        } else {
            1.0
        };
        if faults > thresholds::PAGE_FAULTS_PER_SEC && available_ratio < thresholds::AVAILABLE_RATIO
        {
            out.push(alert(
                AlertKind::MemoryPressure,
                Severity::Critical,
                "",
                "cause",
                vec![("faults", faults.into())],
                Some(AlertRoute::Processes),
            ));
        }
    }

    if s.memory.commit_limit.get() > 0 {
        #[allow(clippy::cast_precision_loss)]
        let commit_percent =
            s.memory.committed.get() as f64 / s.memory.commit_limit.get() as f64 * 100.0;
        if commit_percent > thresholds::COMMIT_PERCENT {
            out.push(alert(
                AlertKind::MemoryCommit,
                Severity::Warning,
                "",
                "cause",
                vec![("percent", round(commit_percent))],
                Some(AlertRoute::Performance),
            ));
        }
    }

    for disk in &s.disks {
        let label = disk.mount.clone().unwrap_or_else(|| disk.name.clone());

        if disk.active_time.get() > thresholds::DISK_ACTIVE_PERCENT {
            // Latency and saturation have different causes and fixes: a
            // saturated disk is a workload problem; one that is slow *and*
            // saturated has a queue — hardware or driver.
            let slow = disk
                .response_ms
                .is_some_and(|ms| ms > thresholds::DISK_RESPONSE_MS);
            out.push(alert(
                if slow {
                    AlertKind::DiskLatency
                } else {
                    AlertKind::DiskSaturated
                },
                Severity::Warning,
                label.clone(),
                "cause",
                vec![
                    ("disk", label.as_str().into()),
                    ("ms", round(f64::from(disk.response_ms.unwrap_or(0.0)))),
                ],
                Some(AlertRoute::Processes),
            ));
        }

        if disk.total.get() > 0 {
            #[allow(clippy::cast_precision_loss)]
            let free_percent = disk.free.get() as f64 / disk.total.get() as f64 * 100.0;
            if free_percent < thresholds::DISK_FREE_PERCENT {
                out.push(alert(
                    AlertKind::DiskSpace,
                    if free_percent < thresholds::DISK_FREE_CRITICAL_PERCENT {
                        Severity::Critical
                    } else {
                        Severity::Warning
                    },
                    label.clone(),
                    "cause",
                    vec![
                        ("disk", label.as_str().into()),
                        ("percent", round(free_percent)),
                    ],
                    Some(AlertRoute::Storage),
                ));
            }
        }

        // The drive's own verdict, not ours — the string says so.
        if disk.health.as_ref().is_some_and(|h| h.failing) {
            out.push(alert(
                AlertKind::DiskHealth,
                Severity::Critical,
                label.clone(),
                "cause",
                vec![("disk", label.as_str().into())],
                Some(AlertRoute::Storage),
            ));
        }
    }

    for gpu in &s.gpus {
        if let Some(reason) = gpu.throttled {
            out.push(alert(
                AlertKind::GpuThrottled,
                Severity::Warning,
                gpu.name.clone(),
                throttle_slug(reason),
                vec![("gpu", gpu.name.as_str().into())],
                Some(AlertRoute::Performance),
            ));
        }
    }

    if let Some(temp) = s.cpu.temperature
        && temp.get() > thresholds::CPU_TEMPERATURE_C
    {
        out.push(alert(
            AlertKind::ThermalCpu,
            Severity::Critical,
            "",
            "cause",
            vec![("celsius", round(f64::from(temp.get())))],
            Some(AlertRoute::Performance),
        ));
    }

    for nic in &s.networks {
        if nic.connected
            && nic
                .errors_per_sec
                .is_some_and(|e| e > thresholds::NETWORK_ERRORS_PER_SEC)
        {
            out.push(alert(
                AlertKind::NetworkErrors,
                Severity::Warning,
                nic.name.clone(),
                "cause",
                vec![
                    ("adapter", nic.name.as_str().into()),
                    ("errors", nic.errors_per_sec.unwrap_or(0).into()),
                ],
                Some(AlertRoute::Network),
            ));
        }
    }

    if let Some(battery) = &s.battery {
        if battery.charge.get() < thresholds::BATTERY_LOW_PERCENT && !battery.charging {
            out.push(alert(
                AlertKind::BatteryLow,
                Severity::Warning,
                "",
                "cause",
                vec![("percent", round(f64::from(battery.charge.get())))],
                None,
            ));
        }
        if let Some(health) = battery.health
            && health.get() < thresholds::BATTERY_HEALTH_PERCENT
        {
            out.push(alert(
                AlertKind::BatteryHealth,
                Severity::Info,
                "",
                "cause",
                vec![("percent", round(f64::from(health.get())))],
                Some(AlertRoute::Devices),
            ));
        }
    }

    out
}

#[cfg(all(test, feature = "fixtures"))]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::units::Percent;

    fn failing_health() -> crate::metrics::DiskHealth {
        crate::metrics::DiskHealth {
            life_remaining: None,
            power_on_hours: None,
            total_written: None,
            reallocated_sectors: None,
            failing: true,
        }
    }

    fn hot() -> SystemMetrics {
        let mut s = fixtures::system();
        s.cpu.total = Percent(97.0);
        s
    }

    fn calm() -> SystemMetrics {
        let mut s = fixtures::system();
        s.cpu.total = Percent(12.0);
        s
    }

    fn feed(engine: &mut Engine, s: &SystemMetrics, n: u32) -> Vec<AlertTransitions> {
        (0..n).map(|_| engine.poll(s)).collect()
    }

    #[test]
    fn a_spike_shorter_than_the_sustain_window_raises_nothing() {
        // The single most important property: an application launch pegs the
        // CPU for a few seconds and must not produce an alert.
        let mut e = Engine::new();
        let ticks = feed(&mut e, &hot(), SUSTAIN_SAMPLES - 1);
        assert!(ticks.iter().all(AlertTransitions::is_empty));
        assert!(e.active().is_empty());
    }

    #[test]
    fn the_sustain_window_raises_exactly_once() {
        let mut e = Engine::new();
        let ticks = feed(&mut e, &hot(), SUSTAIN_SAMPLES + 5);
        let raised: Vec<_> = ticks.iter().flat_map(|t| t.raised.iter()).collect();
        assert_eq!(raised.len(), 1, "one episode, one raise");
        assert_eq!(raised[0].kind, AlertKind::CpuSustained);
        assert_eq!(raised[0].since_sample, u64::from(SUSTAIN_SAMPLES));
        assert_eq!(e.active().len(), 1);
    }

    #[test]
    fn a_single_calm_sample_does_not_clear_a_raised_alert() {
        // Hysteresis. A reading hovering at the threshold would otherwise
        // raise and clear every second.
        let mut e = Engine::new();
        feed(&mut e, &hot(), SUSTAIN_SAMPLES);
        let t = e.poll(&calm());
        assert!(t.cleared.is_empty());
        assert_eq!(e.active().len(), 1, "still raised after one calm sample");
    }

    #[test]
    fn the_clear_window_clears_exactly_once_and_the_list_empties() {
        let mut e = Engine::new();
        feed(&mut e, &hot(), SUSTAIN_SAMPLES);
        let ticks = feed(&mut e, &calm(), CLEAR_SAMPLES + 3);
        let cleared: Vec<_> = ticks.iter().flat_map(|t| t.cleared.iter()).collect();
        assert_eq!(cleared.len(), 1);
        assert!(e.active().is_empty());
    }

    #[test]
    fn a_re_raise_inside_the_cooldown_is_quiet() {
        // The toast-storm guard. Flapping shows in the list but does not
        // notify again for five minutes.
        let mut e = Engine::new();
        feed(&mut e, &hot(), SUSTAIN_SAMPLES);
        feed(&mut e, &calm(), CLEAR_SAMPLES);
        let ticks = feed(&mut e, &hot(), SUSTAIN_SAMPLES);
        let last = ticks.last().expect("ticks");
        assert!(
            last.raised.is_empty(),
            "must not notify inside the cooldown"
        );
        assert_eq!(last.raised_quietly.len(), 1);
        assert_eq!(e.active().len(), 1, "but it IS in the list");
    }

    #[test]
    fn a_re_raise_after_the_cooldown_notifies_again() {
        let mut e = Engine::new();
        feed(&mut e, &hot(), SUSTAIN_SAMPLES);
        feed(&mut e, &calm(), CLEAR_SAMPLES + RENOTIFY_AFTER_SAMPLES);
        let ticks = feed(&mut e, &hot(), SUSTAIN_SAMPLES);
        assert_eq!(ticks.last().expect("ticks").raised.len(), 1);
    }

    #[test]
    fn a_gap_in_the_condition_resets_the_sustain_count() {
        // Fourteen hot, one calm, fourteen hot: never fifteen in a row.
        let mut e = Engine::new();
        feed(&mut e, &hot(), SUSTAIN_SAMPLES - 1);
        e.poll(&calm());
        let ticks = feed(&mut e, &hot(), SUSTAIN_SAMPLES - 1);
        assert!(ticks.iter().all(|t| t.raised.is_empty()));
    }

    #[test]
    fn instantaneous_conditions_raise_on_the_first_sample() {
        let mut s = fixtures::system();
        s.disks[0].health = Some(failing_health());
        let mut e = Engine::new();
        let t = e.poll(&s);
        assert_eq!(t.raised.len(), 1);
        assert_eq!(t.raised[0].kind, AlertKind::DiskHealth);
        assert_eq!(
            t.raised[0].subject,
            s.disks[0].mount.clone().unwrap_or_default()
        );
    }

    #[test]
    fn two_disks_are_two_alerts_not_one() {
        // Subject is part of identity. Without it a second failing disk
        // would be swallowed as "already raised".
        let mut s = fixtures::system();
        let mut second = s.disks[0].clone();
        second.id = crate::ids::DiskId(99);
        second.mount = Some("E:".into());
        s.disks.push(second);
        for d in &mut s.disks {
            d.health = Some(failing_health());
        }
        let mut e = Engine::new();
        let t = e.poll(&s);
        assert_eq!(t.raised.len(), 2);
    }

    #[test]
    fn a_severity_escalation_is_reported_even_though_the_alert_is_not_new() {
        // 4 % free is a warning; 0.9 % is critical. The second is news.
        let mut s = fixtures::system();
        s.disks[0].total = crate::units::Bytes(1000);
        s.disks[0].free = crate::units::Bytes(40);
        let mut e = Engine::new();
        let t = e.poll(&s);
        assert_eq!(t.raised.len(), 1);
        assert_eq!(t.raised[0].severity, Severity::Warning);

        s.disks[0].free = crate::units::Bytes(5);
        let t = e.poll(&s);
        assert_eq!(t.raised.len(), 1, "escalation must be reported");
        assert_eq!(t.raised[0].severity, Severity::Critical);
        assert_eq!(e.active().len(), 1, "but it is still one alert");
    }

    #[test]
    fn active_is_ordered_most_serious_first_and_stable() {
        let mut s = fixtures::system();
        s.cpu.total = Percent(97.0);
        s.disks[0].health = Some(failing_health());
        let mut e = Engine::new();
        feed(&mut e, &s, SUSTAIN_SAMPLES);
        let a = e.active();
        assert_eq!(a[0].kind, AlertKind::DiskHealth, "critical before warning");
        assert_eq!(a[1].kind, AlertKind::CpuSustained);
        assert_eq!(e.active(), a, "same input, same order");
    }

    #[test]
    fn memory_percentage_alone_never_raises_pressure() {
        // The whole thesis of the module. 95 % used with no faulting is a
        // healthy machine with a big file cache.
        let mut s = fixtures::system();
        s.memory.total = crate::units::Bytes(100);
        s.memory.used = crate::units::Bytes(95);
        s.memory.available = crate::units::Bytes(5);
        s.memory.page_faults_per_sec = Some(50);
        let mut e = Engine::new();
        feed(&mut e, &s, SUSTAIN_SAMPLES + 1);
        assert!(
            !e.active()
                .iter()
                .any(|a| a.kind == AlertKind::MemoryPressure)
        );
    }

    #[test]
    fn idle_tracks_are_forgotten_after_the_cooldown() {
        // A machine that mounts and unmounts USB drives must not grow the map
        // forever.
        let mut e = Engine::new();
        feed(&mut e, &hot(), SUSTAIN_SAMPLES);
        feed(&mut e, &calm(), CLEAR_SAMPLES + RENOTIFY_AFTER_SAMPLES + 1);
        assert!(e.tracks.is_empty());
    }

    #[test]
    fn title_and_cause_keys_match_the_serde_names() {
        // The locale files key on the serialised variant name. If serde and
        // the key builder disagree, every alert renders as its raw key.
        for kind in [
            AlertKind::CpuSustained,
            AlertKind::DiskHealth,
            AlertKind::BatteryHealth,
        ] {
            let json = serde_json::to_string(&kind).expect("serialise");
            let name = json.trim_matches('"');
            assert_eq!(serde_variant_name(kind), name);
        }
    }

    #[test]
    fn values_serialise_in_key_order_whatever_order_they_were_added_in() {
        // A HashMap here made the Android contract fixture rewrite itself
        // on every run, so the drift gate failed on an untouched tree.
        let make = |pairs: &[(&str, AlertValue)]| {
            let alert = alert(
                AlertKind::DiskSpace,
                Severity::Warning,
                "C:",
                "hot",
                pairs.to_vec(),
                None,
            );
            serde_json::to_string(&alert).expect("serialise")
        };
        let forward = make(&[
            ("percent", AlertValue::Number(97.0)),
            ("name", AlertValue::Text("C:".into())),
        ]);
        let backward = make(&[
            ("name", AlertValue::Text("C:".into())),
            ("percent", AlertValue::Number(97.0)),
        ]);
        assert_eq!(forward, backward);
        assert!(forward.find("\"name\"") < forward.find("\"percent\""));
    }
}
