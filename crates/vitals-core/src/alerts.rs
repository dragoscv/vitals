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
    /// A condition the user defined themselves. The reading, the comparator
    /// and the threshold all arrive in `values`, because one variant has to
    /// cover every rule a person can write.
    Custom,
    /// An automatic service that was running has stopped.
    ServiceStopped,
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

    /// Whether this kind came from a user rule.
    ///
    /// A rule carries its own window, so the engine must not apply the
    /// built-in one to it.
    #[must_use]
    pub const fn is_user_rule(self) -> bool {
        matches!(self, Self::Custom)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum RuleMetric {
    CpuPercent,
    CpuTemperature,
    MemoryPercent,
    DiskActivePercent,
    BatteryPercent,
    NetworkErrorsPerSec,
}

impl RuleMetric {
    /// The reading for this metric, or `None` when the machine cannot measure
    /// it right now.
    ///
    /// `None` must not fire a rule: a laptop with no battery sensor does not
    /// have "0 % battery", and a Mac has no CPU temperature — the same rule
    /// the rest of Vitals follows, applied to a condition the user wrote.
    #[must_use]
    pub fn reading(self, s: &SystemMetrics) -> Option<f64> {
        match self {
            Self::CpuPercent => Some(f64::from(s.cpu.total.get())),
            Self::CpuTemperature => s.cpu.temperature.map(|t| f64::from(t.get())),
            Self::MemoryPercent => {
                let total = s.memory.total.get();
                if total == 0 {
                    return None;
                }
                #[allow(clippy::cast_precision_loss)]
                Some(s.memory.used.get() as f64 / total as f64 * 100.0)
            }
            // Absent where the platform has no busy-time counter (macOS). An
            // unmeasured disk must not enter the maximum: a fabricated zero
            // would let a busy disk on a Mac look idle and never alert.
            Self::DiskActivePercent => s
                .disks
                .iter()
                .filter_map(|d| d.active_time)
                .map(|active| f64::from(active.get()))
                .reduce(f64::max),
            Self::BatteryPercent => s.battery.as_ref().map(|b| f64::from(b.charge.get())),
            Self::NetworkErrorsPerSec => s
                .networks
                .iter()
                .filter_map(|n| n.errors_per_sec)
                .map(|e| {
                    #[allow(clippy::cast_precision_loss)]
                    {
                        e as f64
                    }
                })
                .reduce(f64::max),
        }
    }
}

/// Which side of the threshold the condition is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum Comparator {
    Above,
    Below,
}

impl Comparator {
    /// Whether `value` satisfies the rule.
    #[must_use]
    pub fn holds(self, value: f64, threshold: f64) -> bool {
        match self {
            Self::Above => value > threshold,
            Self::Below => value < threshold,
        }
    }

    /// The i18n key under `dashboard.alert.custom.<slug>` for the cause.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Above => "above",
            Self::Below => "below",
        }
    }
}

/// A rule the user wrote, as the settings screen stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// Stable across edits, and the key of the alert: two rules on one metric
    /// must not shadow each other.
    pub id: String,
    /// What the user calls it, shown as the alert's subject.
    pub name: String,
    pub metric: RuleMetric,
    pub comparator: Comparator,
    pub threshold: f64,
    /// Seconds the condition must hold before the alert is raised. `0` means
    /// the first true sample is enough.
    pub sustain_secs: u32,
    /// Seconds of silence before the same rule may notify again.
    pub cooldown_secs: u32,
    pub severity: Severity,
    /// Which channels this rule goes to, by id. Empty means every channel
    /// that is switched on — which is what a rule written before channels
    /// existed behaves as, and what most users will want anyway.
    #[serde(default)]
    pub channels: Vec<String>,
}

/// The longest a window may be. A day: past that the alert is a report, not a
/// notification.
pub const MAX_RULE_WINDOW_SECS: u32 = 86_400;

impl Rule {
    /// Whether the rule can be used at all.
    ///
    /// A rule with no id cannot be tracked, one with no name cannot be shown,
    /// and a non-finite threshold cannot be compared. Anything else is
    /// clamped rather than refused: a user who types 99 999 seconds gets the
    /// longest sane window instead of a rule that silently never fires.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        !self.id.trim().is_empty() && !self.name.trim().is_empty() && self.threshold.is_finite()
    }

    /// The windows in samples. `sample_ms` is how often `poll` is called.
    #[must_use]
    fn windows(&self, sample_ms: u32) -> (u32, u32) {
        let per_second = 1000.0 / f64::from(sample_ms.max(1));
        let samples = |secs: u32| {
            let clamped = secs.min(MAX_RULE_WINDOW_SECS);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let count = (f64::from(clamped) * per_second).ceil() as u32;
            count.max(1)
        };
        (samples(self.sustain_secs), samples(self.cooldown_secs))
    }

    /// The alert this rule raises, with the reading that triggered it.
    ///
    /// The reading, comparator and threshold travel in `values` so the one
    /// `custom` variant can render a sentence about any rule, in whatever
    /// language, without the engine knowing the words.
    #[must_use]
    fn alert(&self, value: f64) -> Alert {
        alert(
            AlertKind::Custom,
            self.severity,
            self.name.clone(),
            self.comparator.slug(),
            vec![
                ("name", self.name.as_str().into()),
                ("threshold", self.threshold.into()),
                ("value", round(value)),
                ("seconds", u64::from(self.sustain_secs).into()),
                ("metric", self.metric.slug().into()),
            ],
            None,
        )
    }
}

impl RuleMetric {
    /// The i18n key suffix for a readable metric name.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::CpuPercent => "cpuPercent",
            Self::CpuTemperature => "cpuTemperature",
            Self::MemoryPercent => "memoryPercent",
            Self::DiskActivePercent => "diskActivePercent",
            Self::BatteryPercent => "batteryPercent",
            Self::NetworkErrorsPerSec => "networkErrorsPerSec",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum ServiceStartType {
    Boot,
    System,
    Auto,
    Manual,
    Disabled,
    Unknown,
}

/// One service, as the host last saw it.
///
/// Not part of the per-second sample: enumerating every service on Windows is
/// hundreds of registry and SCM round trips, and the row that asked for this
/// alert says so. The host polls at its own slow rate and hands the result to
/// [`Engine::set_services`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct ServiceState {
    /// The SCM name, which is what a `net start` would use.
    pub name: String,
    /// What the user sees in the services list.
    pub display_name: String,
    pub start_type: ServiceStartType,
    pub running: bool,
    /// Whether the service starts on a system trigger rather than at boot.
    ///
    /// The distinction is the whole reason this alert is not noise. A great
    /// many Windows services are `Auto` **and** stopped, because a trigger
    /// starts them on their first use — reporting those would put a permanent
    /// warning on every machine in the world.
    pub triggered: bool,
}

impl ServiceState {
    /// Whether a stopped service here is worth telling the user about.
    ///
    /// Only one that is meant to be running anyway, and that nothing else is
    /// waiting to start on demand.
    #[must_use]
    pub fn is_expected_running(&self) -> bool {
        !self.triggered
            && matches!(
                self.start_type,
                ServiceStartType::Boot | ServiceStartType::System | ServiceStartType::Auto
            )
    }
}

/// Identity of an alert across ticks: the kind plus the thing it is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    kind: AlertKind,
    subject: String,
}

/// A condition to evaluate, with the windows that apply to *it*.
///
/// The built-in conditions share one sustain window and one cooldown; a user
/// rule carries its own, so the windows cannot live on the engine.
struct Candidate {
    alert: Alert,
    /// Consecutive samples the condition must hold before it is raised.
    sustain: u32,
    /// Samples of silence before the same condition may notify again.
    cooldown: u32,
}

/// Wraps a built-in alert in the engine's own windows.
fn built_in(alert: Alert) -> Candidate {
    Candidate {
        sustain: if alert.kind.requires_sustain() {
            SUSTAIN_SAMPLES
        } else {
            1
        },
        cooldown: RENOTIFY_AFTER_SAMPLES,
        alert,
    }
}

/// A rule with its windows already counted in samples.
#[derive(Debug, Clone)]
struct CompiledRule {
    rule: Rule,
    sustain: u32,
    cooldown: u32,
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
    /// The user's rules, with their windows already in samples.
    rules: Vec<CompiledRule>,
    /// The last service list the host handed over. Empty until it does, and
    /// refreshed at the host's own slow rate rather than per sample.
    services: Vec<ServiceState>,
    /// Services this process has seen running at least once.
    ///
    /// A service that was already stopped when Vitals started is not news and
    /// is never reported: without this, every machine with a deliberately
    /// stopped automatic service would show a permanent warning in the first
    /// fifteen seconds.
    services_seen_running: std::collections::HashSet<String>,
    /// Services the user stopped from Vitals itself. Stopping something on
    /// purpose is not the failure this alert is for.
    stopped_on_purpose: std::collections::HashSet<String>,
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

    /// Replaces the user's rules.
    ///
    /// `sample_ms` is how often [`Engine::poll`] is called, because the
    /// windows the user writes are in seconds and the engine counts samples.
    /// A rule that is not usable is dropped rather than half-applied, and the
    /// tracks behind rules that no longer exist are removed so a deleted rule
    /// cannot leave an alert standing for an hour.
    pub fn set_rules(&mut self, rules: Vec<Rule>, sample_ms: u32) {
        self.rules = rules
            .into_iter()
            .filter(Rule::is_usable)
            .map(|rule| {
                let (sustain, cooldown) = rule.windows(sample_ms);
                CompiledRule {
                    rule,
                    sustain,
                    cooldown,
                }
            })
            .collect();
        let names: Vec<String> = self.rules.iter().map(|r| r.rule.name.clone()).collect();
        self.tracks
            .retain(|key, _| key.kind != AlertKind::Custom || names.contains(&key.subject));
    }

    /// How many rules are in force, for a status line.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// The channels a rule routed this alert to.
    ///
    /// Empty means "every channel", which is also the answer for a built-in
    /// alert: those are not anybody's rule and go wherever the user's channels
    /// say. A rule is found by its name because that is what the alert carries
    /// as its subject — the two are written from the same field.
    #[must_use]
    pub fn routed_channels(&self, alert: &Alert) -> Vec<String> {
        if !alert.kind.is_user_rule() {
            return Vec::new();
        }
        self.rules
            .iter()
            .find(|compiled| compiled.rule.name == alert.subject)
            .map(|compiled| compiled.rule.channels.clone())
            .unwrap_or_default()
    }

    /// Replaces the service list.
    ///
    /// Called at the host's own slow rate, not every sample: see
    /// [`ServiceState`]. Between refreshes the list is stale but stable, so
    /// nothing flaps.
    pub fn set_services(&mut self, services: Vec<ServiceState>) {
        self.services = services;
    }

    /// Records that the user stopped a service from Vitals itself.
    ///
    /// The entry is forgotten as soon as that service runs again, so a
    /// deliberate stop is respected once and does not silence the service
    /// forever — the next time it stops by itself, the user is told.
    pub fn note_stopped_on_purpose(&mut self, service: &str) {
        let _ = self.stopped_on_purpose.insert(service.to_owned());
    }

    /// A service that should be running and is not, or `None`.
    ///
    /// Three conditions, and all three matter:
    ///
    /// 1. it is meant to be running (automatic, and not trigger-started);
    /// 2. this process has **seen it running** — a service that was already
    ///    stopped when Vitals started is not news;
    /// 3. the user did not stop it from Vitals itself just now.
    #[must_use]
    pub fn stopped_service(&self) -> Option<&ServiceState> {
        self.services.iter().find(|service| {
            !service.running
                && service.is_expected_running()
                && self.services_seen_running.contains(&service.name)
                && !self.stopped_on_purpose.contains(&service.name)
        })
    }

    /// The alerts for services that should be running and are not.
    ///
    /// The list is taken out and put back so the borrow of it and the mutable
    /// borrow of the memory beside it do not overlap; it is thirty or so
    /// entries, so moving it costs nothing worth counting.
    fn service_candidates(&mut self) -> Vec<Alert> {
        let mut out = Vec::new();
        let services = std::mem::take(&mut self.services);
        for service in &services {
            if service.running {
                let _ = self.services_seen_running.insert(service.name.clone());
                // A service that runs again is no longer a deliberate stop,
                // so the next time it goes down the user hears about it.
                let _ = self.stopped_on_purpose.remove(&service.name);
                continue;
            }
            if service.is_expected_running()
                && self.services_seen_running.contains(&service.name)
                && !self.stopped_on_purpose.contains(&service.name)
            {
                out.push(alert(
                    AlertKind::ServiceStopped,
                    Severity::Warning,
                    service.display_name.clone(),
                    "cause",
                    vec![
                        // The title is the sentence, so it needs the name as a
                        // value: the renderer interpolates `values`, and
                        // `subject` is not one of them.
                        ("serviceName", service.display_name.as_str().into()),
                        ("service", service.name.as_str().into()),
                    ],
                    None,
                ));
            }
        }
        self.services = services;
        out
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
        let mut candidates: Vec<Candidate> = evaluate(system).into_iter().map(built_in).collect();
        // A stopped service is not a threshold condition: what matters is that
        // this process *saw it running* and now does not. The memory is
        // updated here, in the same tick, so the "not seen means false" pass
        // below clears a service that comes back up.
        candidates.extend(self.service_candidates().into_iter().map(built_in));
        // A rule whose reading is absent raises nothing: "cannot measure" is
        // not "below the threshold".
        for compiled in &self.rules {
            let Some(value) = compiled.rule.metric.reading(system) else {
                continue;
            };
            if compiled
                .rule
                .comparator
                .holds(value, compiled.rule.threshold)
            {
                candidates.push(Candidate {
                    alert: compiled.rule.alert(value),
                    sustain: compiled.sustain,
                    cooldown: compiled.cooldown,
                });
            }
        }
        let mut seen: Vec<Key> = Vec::with_capacity(candidates.len());

        let mut transitions = AlertTransitions {
            raised: Vec::new(),
            raised_quietly: Vec::new(),
            cleared: Vec::new(),
        };

        for candidate in candidates {
            let key = Key {
                kind: candidate.alert.kind,
                subject: candidate.alert.subject.clone(),
            };
            seen.push(key.clone());
            let track = self.tracks.entry(key).or_default();
            track.false_run = 0;

            if let Some(raised) = &mut track.raised {
                // Already up: refresh the numbers (the cause string shows the
                // *current* value) but keep the original since_sample and
                // let a severity escalation through — 4 % free becoming 0.9 %
                // is news even though the alert is not new.
                let escalated = candidate.alert.severity > raised.severity;
                let since = raised.since_sample;
                *raised = Alert {
                    since_sample: since,
                    ..candidate.alert
                };
                if escalated {
                    transitions.raised.push(raised.clone());
                }
                continue;
            }

            track.true_run = track.true_run.saturating_add(1);
            if track.true_run < candidate.sustain {
                continue;
            }

            let alert = Alert {
                since_sample: now,
                ..candidate.alert
            };
            track.raised = Some(alert.clone());
            track.true_run = 0;

            let in_cooldown = track
                .last_cleared
                .is_some_and(|at| now.saturating_sub(at) < u64::from(candidate.cooldown));
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
        AlertKind::Custom => "custom",
        AlertKind::ServiceStopped => "serviceStopped",
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

    if let (Some(committed), Some(limit)) = (s.memory.committed, s.memory.commit_limit)
        && limit.get() > 0
    {
        #[allow(clippy::cast_precision_loss)]
        let commit_percent = committed.get() as f64 / limit.get() as f64 * 100.0;
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

        if disk
            .active_time
            .is_some_and(|active| active.get() > thresholds::DISK_ACTIVE_PERCENT)
        {
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
                // `ms` only when measured: an unmeasured latency is absent,
                // not 0 ms (`DiskLatency` implies it is present).
                std::iter::once(("disk", label.as_str().into()))
                    .chain(disk.response_ms.map(|ms| ("ms", round(f64::from(ms)))))
                    .collect(),
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

    /// A rule on CPU usage above `threshold`, with the given windows.
    fn cpu_rule(threshold: f64, sustain_secs: u32, cooldown_secs: u32) -> Rule {
        Rule {
            id: "r1".into(),
            name: "Hot CPU".into(),
            metric: RuleMetric::CpuPercent,
            comparator: Comparator::Above,
            threshold,
            sustain_secs,
            cooldown_secs,
            severity: Severity::Warning,
            channels: Vec::new(),
        }
    }

    fn busy(cpu: f32) -> SystemMetrics {
        let mut s = fixtures::system();
        s.cpu.total = crate::units::Percent(cpu);
        s
    }

    #[test]
    fn a_user_rule_fires_only_after_its_own_sustained_window() {
        let mut engine = Engine::new();
        engine.set_rules(vec![cpu_rule(90.0, 3, 0)], 1000);
        assert!(engine.active().is_empty());
        assert!(
            engine.poll(&busy(95.0)).raised.is_empty(),
            "one second is not three"
        );
        assert!(engine.poll(&busy(95.0)).raised.is_empty());
        let third = engine.poll(&busy(95.0));
        assert_eq!(third.raised.len(), 1, "the third second raises it");
        assert_eq!(third.raised[0].kind, AlertKind::Custom);
        assert_eq!(third.raised[0].subject, "Hot CPU");
        assert_eq!(third.raised[0].cause, "alert.custom.above");
        assert_eq!(engine.active().len(), 1);
    }

    #[test]
    fn a_rule_whose_reading_is_absent_never_fires() {
        let mut engine = Engine::new();
        engine.set_rules(
            vec![Rule {
                metric: RuleMetric::BatteryPercent,
                comparator: Comparator::Below,
                threshold: 20.0,
                ..cpu_rule(0.0, 0, 0)
            }],
            1000,
        );
        let mut s = fixtures::system();
        s.battery = None;
        for _ in 0..10 {
            engine.poll(&s);
        }
        assert!(
            engine.active().is_empty(),
            "a machine with no battery sensor does not have an empty battery"
        );
    }

    #[test]
    fn a_rules_cooldown_quiets_the_notification_without_hiding_the_alert() {
        let mut engine = Engine::new();
        engine.set_rules(vec![cpu_rule(90.0, 0, 100)], 1000);
        assert_eq!(engine.poll(&busy(95.0)).raised.len(), 1);
        // Clear it: the built-in clear window, then it is gone.
        for _ in 0..(CLEAR_SAMPLES + 2) {
            engine.poll(&busy(1.0));
        }
        assert!(engine.active().is_empty());
        // True again inside the rule's cooldown: listed, not notified.
        let again = engine.poll(&busy(95.0));
        assert!(again.raised.is_empty(), "inside the cooldown");
        assert_eq!(again.raised_quietly.len(), 1);
        assert_eq!(engine.active().len(), 1, "still in the list");
    }

    #[test]
    fn deleting_a_rule_takes_its_raised_alert_with_it() {
        let mut engine = Engine::new();
        engine.set_rules(vec![cpu_rule(90.0, 0, 3600)], 1000);
        engine.poll(&busy(95.0));
        assert_eq!(engine.active().len(), 1);
        engine.set_rules(Vec::new(), 1000);
        assert!(
            engine.active().is_empty(),
            "a rule the user deleted must stop alerting immediately"
        );
        assert_eq!(engine.rule_count(), 0);
    }

    #[test]
    fn a_rule_with_no_name_is_dropped_rather_than_half_applied() {
        let mut engine = Engine::new();
        let mut rule = cpu_rule(90.0, 0, 0);
        rule.name = "   ".into();
        engine.set_rules(vec![rule], 1000);
        assert_eq!(engine.rule_count(), 0);
        engine.poll(&busy(95.0));
        assert!(engine.active().is_empty());
    }

    #[test]
    fn the_window_the_user_writes_is_seconds_not_samples() {
        // At 4 Hz, three seconds is twelve samples — not three.
        let mut engine = Engine::new();
        engine.set_rules(vec![cpu_rule(90.0, 3, 0)], 250);
        for _ in 0..11 {
            assert!(engine.poll(&busy(95.0)).raised.is_empty());
        }
        assert_eq!(engine.poll(&busy(95.0)).raised.len(), 1);
    }

    /// An automatic service that is meant to be running.
    fn service(name: &str, running: bool) -> ServiceState {
        ServiceState {
            name: name.into(),
            display_name: format!("{name} service"),
            start_type: ServiceStartType::Auto,
            running,
            triggered: false,
        }
    }

    #[test]
    fn a_service_that_stops_is_reported_with_its_display_name() {
        let mut engine = Engine::new();
        engine.set_services(vec![service("Spooler", true)]);
        engine.poll(&busy(5.0));
        assert!(engine.active().is_empty(), "running is not news");

        engine.set_services(vec![service("Spooler", false)]);
        let transitions = engine.poll(&busy(5.0));
        assert_eq!(transitions.raised.len(), 1);
        let alert = &transitions.raised[0];
        assert_eq!(alert.kind, AlertKind::ServiceStopped);
        assert_eq!(alert.subject, "Spooler service", "what the user sees");
        assert_eq!(alert.values["service"], AlertValue::Text("Spooler".into()));
    }

    #[test]
    fn a_service_that_was_already_stopped_is_never_reported() {
        // The trap this exists for: every Windows machine has automatic
        // services that are stopped and fine. Reporting those would put a
        // permanent warning on every install.
        let mut engine = Engine::new();
        engine.set_services(vec![service("Dormant", false)]);
        for _ in 0..40 {
            let transitions = engine.poll(&busy(5.0));
            assert!(transitions.raised.is_empty(), "it was never running");
        }
        assert!(engine.active().is_empty());
    }

    #[test]
    fn a_trigger_started_service_is_not_reported_when_it_stops() {
        // Auto **and** stopped is the normal state of a trigger-started
        // service, and it is the single biggest source of false positives.
        let mut engine = Engine::new();
        let mut triggered = service("wuauserv", true);
        triggered.triggered = true;
        engine.set_services(vec![triggered.clone()]);
        engine.poll(&busy(5.0));
        triggered.running = false;
        engine.set_services(vec![triggered]);
        assert!(engine.poll(&busy(5.0)).raised.is_empty());
        assert!(
            !ServiceState {
                triggered: true,
                ..service("x", false)
            }
            .is_expected_running()
        );
    }

    #[test]
    fn a_manual_service_stopping_is_nobody_s_business() {
        let mut engine = Engine::new();
        let mut manual = service("Manual", true);
        manual.start_type = ServiceStartType::Manual;
        engine.set_services(vec![manual.clone()]);
        engine.poll(&busy(5.0));
        manual.running = false;
        engine.set_services(vec![manual]);
        assert!(engine.poll(&busy(5.0)).raised.is_empty());
    }

    #[test]
    fn a_service_the_user_stopped_from_vitals_is_not_reported_until_it_runs_again() {
        let mut engine = Engine::new();
        engine.set_services(vec![service("Spooler", true)]);
        engine.poll(&busy(5.0));

        engine.note_stopped_on_purpose("Spooler");
        engine.set_services(vec![service("Spooler", false)]);
        assert!(
            engine.poll(&busy(5.0)).raised.is_empty(),
            "stopping it on purpose is not a failure"
        );

        // It runs again, so the deliberate stop is forgotten...
        engine.set_services(vec![service("Spooler", true)]);
        engine.poll(&busy(5.0));
        // ...and the next stop by itself is reported.
        engine.set_services(vec![service("Spooler", false)]);
        assert_eq!(engine.poll(&busy(5.0)).raised.len(), 1);
    }

    #[test]
    fn a_service_coming_back_clears_its_alert() {
        let mut engine = Engine::new();
        engine.set_services(vec![service("Spooler", true)]);
        engine.poll(&busy(5.0));
        engine.set_services(vec![service("Spooler", false)]);
        engine.poll(&busy(5.0));
        assert_eq!(engine.active().len(), 1);
        engine.set_services(vec![service("Spooler", true)]);
        for _ in 0..(CLEAR_SAMPLES + 2) {
            engine.poll(&busy(5.0));
        }
        assert!(
            engine.active().is_empty(),
            "it is running again, so the warning must go"
        );
    }

    #[test]
    fn an_empty_service_list_is_not_every_service_being_down() {
        let mut engine = Engine::new();
        engine.poll(&busy(5.0));
        assert!(engine.active().is_empty());
        assert!(engine.stopped_service().is_none());
    }
}
