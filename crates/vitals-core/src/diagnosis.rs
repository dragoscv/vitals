//! "Why is my PC slow?" — one sentence, then the evidence for it.
//!
//! # The gap this fills
//!
//! The alert engine says *what* is wrong ("the disk has been saturated for
//! four minutes"). That is already better than a wall of gauges, but it still
//! leaves the user to work out what to do about it. The missing half is
//! *who*: the disk is saturated **because** one process is doing 71 % of the
//! writing.
//!
//! Only the backend can make that join — it is the only place that has both
//! the alerts and the process list — so it happens here, once, rather than
//! being re-derived by each surface that wants to show it.
//!
//! # Attribution is a claim, so it is hedged
//!
//! A process's share of a subsystem is computed from the same tick as the
//! alert, and processes that started and exited between samples are invisible
//! to both. So a contributor is reported with its measured share and nothing
//! stronger: "Code.exe — 71 % of disk writes", never "Code.exe is the cause".
//! Where nothing accounts for a meaningful share, the verdict says so instead
//! of naming whoever happens to be top of a very flat list — a monitor that
//! confidently blames the wrong process is worse than one that admits the
//! load is spread.

use serde::{Deserialize, Serialize};

use crate::alerts::{Alert, AlertKind};
use crate::metrics::SystemMetrics;
use crate::process::Process;

/// A process held partly responsible for a subsystem being busy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Contributor {
    pub name: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pid: u32,
    /// Share of the subsystem's total, 0-100. Rounded for display.
    pub share: f32,
    /// The raw reading behind the share, for the tooltip: **hundredths of a
    /// percent** for CPU and GPU (an integer, so 55.6 % is `5560` — the
    /// scoring needs integers to sum without float drift), bytes for memory,
    /// bytes per second for disk.
    ///
    /// The unit used to be documented as "percent", and the report printed
    /// "5,560.0 %" for a process at 55.6 %.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub value: u64,
}

/// Which subsystem a verdict is about. Drives the icon and the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum Subsystem {
    Cpu,
    Memory,
    Disk,
    Gpu,
    Network,
    Thermal,
    Storage,
    Battery,
}

/// The answer to the question, ready to render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    /// `null` when nothing is wrong — which is a real answer, not an error.
    pub verdict: Option<Verdict>,
    /// Everything else currently raised, so the report is complete without
    /// pretending the secondary problems are the cause.
    pub also: Vec<Alert>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    /// The alert this verdict is built on, carrying its own i18n keys and
    /// values so the sentence renders in the user's language.
    pub alert: Alert,
    pub subsystem: Subsystem,
    /// Most responsible first. Empty when the load is genuinely spread, and
    /// the UI must say that rather than showing an empty list.
    pub contributors: Vec<Contributor>,
    /// True when no single process accounts for a meaningful share.
    pub diffuse: bool,
}

/// Below this share, naming a process is more misleading than helpful.
const MEANINGFUL_SHARE: f32 = 15.0;

/// How many contributors are worth showing. Three fits the screen and is
/// about as many as anyone acts on.
const MAX_CONTRIBUTORS: usize = 3;

impl AlertKind {
    /// The subsystem this alert is about, for correlation.
    #[must_use]
    pub const fn subsystem(self) -> Subsystem {
        match self {
            Self::CpuSustained | Self::CpuThrottled => Subsystem::Cpu,
            Self::MemoryPressure | Self::MemoryCommit => Subsystem::Memory,
            Self::DiskSaturated | Self::DiskLatency => Subsystem::Disk,
            Self::DiskSpace | Self::DiskHealth => Subsystem::Storage,
            Self::GpuThrottled => Subsystem::Gpu,
            Self::ThermalCpu => Subsystem::Thermal,
            Self::NetworkErrors => Subsystem::Network,
            Self::BatteryLow | Self::BatteryHealth => Subsystem::Battery,
        }
    }

    /// Whether naming processes makes sense for this alert.
    ///
    /// A failing drive and a worn battery are not anybody's fault; listing
    /// the busiest processes beside them would imply a causal link that does
    /// not exist.
    #[must_use]
    pub const fn has_contributors(self) -> bool {
        matches!(
            self,
            Self::CpuSustained
                | Self::MemoryPressure
                | Self::MemoryCommit
                | Self::DiskSaturated
                | Self::DiskLatency
                | Self::ThermalCpu
        )
    }
}

/// Builds the report from the current alerts, metrics and process list.
///
/// The verdict is the most severe alert, breaking ties by whichever has been
/// raised longest — a critical that has held for four minutes is a better
/// answer than one raised this second.
#[must_use]
pub fn diagnose(alerts: &[Alert], system: &SystemMetrics, processes: &[Process]) -> Diagnosis {
    let Some(primary) = alerts.iter().min_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.since_sample.cmp(&b.since_sample))
    }) else {
        return Diagnosis {
            verdict: None,
            also: Vec::new(),
        };
    };

    let subsystem = primary.kind.subsystem();
    let contributors = if primary.kind.has_contributors() {
        top_contributors(subsystem, system, processes)
    } else {
        Vec::new()
    };

    // "Diffuse" is a finding, not a failure: a machine loaded evenly by
    // thirty processes has no single culprit, and saying so is the honest
    // answer. Only claimed when we actually looked.
    let diffuse = primary.kind.has_contributors()
        && contributors
            .first()
            .is_none_or(|c| c.share < MEANINGFUL_SHARE);

    Diagnosis {
        verdict: Some(Verdict {
            alert: primary.clone(),
            subsystem,
            contributors: if diffuse { Vec::new() } else { contributors },
            diffuse,
        }),
        also: alerts
            .iter()
            .filter(|a| !(a.kind == primary.kind && a.subject == primary.subject))
            .cloned()
            .collect(),
    }
}

/// The processes accounting for the most of one subsystem, largest first.
fn top_contributors(
    subsystem: Subsystem,
    system: &SystemMetrics,
    processes: &[Process],
) -> Vec<Contributor> {
    // Value per process, and the total to take a share of. The total comes
    // from the process list, not from the machine-wide reading: the kernel's
    // own total includes work by processes that have since exited, and
    // dividing by it would produce shares that never reach 100 % and quietly
    // understate every contributor.
    let mut scored: Vec<(&Process, u64)> = match subsystem {
        Subsystem::Cpu | Subsystem::Thermal => processes
            .iter()
            .map(|p| (p, (p.cpu.get().max(0.0) * 100.0) as u64))
            .collect(),
        Subsystem::Memory => processes
            .iter()
            .map(|p| (p, p.memory_private.get()))
            .collect(),
        Subsystem::Disk => processes
            .iter()
            .map(|p| (p, p.disk_read.get().saturating_add(p.disk_write.get())))
            .collect(),
        Subsystem::Gpu => processes
            .iter()
            .map(|p| (p, (p.gpu.unwrap_or_default().get().max(0.0) * 100.0) as u64))
            .collect(),
        // No per-process attribution exists for these: Windows does not
        // report per-process network bytes without ETW, and free disk space
        // is not a rate anybody is currently consuming.
        Subsystem::Network | Subsystem::Storage | Subsystem::Battery => Vec::new(),
    };

    let _ = system;
    let total: u64 = scored.iter().map(|(_, v)| *v).sum();
    if total == 0 {
        return Vec::new();
    }

    scored.sort_unstable_by_key(|(_, value)| std::cmp::Reverse(*value));
    scored
        .into_iter()
        .take(MAX_CONTRIBUTORS)
        .filter(|(_, value)| *value > 0)
        .map(|(process, value)| Contributor {
            name: process.name.clone(),
            pid: process.key.pid.get(),
            #[allow(clippy::cast_precision_loss)]
            share: ((value as f64 / total as f64) * 100.0) as f32,
            value,
        })
        .collect()
}

#[cfg(all(test, feature = "fixtures"))]
mod tests {
    use super::*;
    use crate::alerts::{AlertValue, Engine, Severity};
    use crate::fixtures;
    use crate::units::{Bytes, BytesPerSec, Percent};
    use std::collections::HashMap;

    #[test]
    fn a_cpu_contributor_value_is_hundredths_of_a_percent() {
        // The report divides by 100 (report.ts). Change the unit here and
        // the report reads 100x off again, as it did: "5,560.0 %".
        let system = fixtures::system();
        let processes = vec![
            fixtures::process("busy.exe", 10, 55.6),
            fixtures::process("idle.exe", 11, 10.0),
        ];
        let top = top_contributors(Subsystem::Cpu, &system, &processes);
        assert_eq!(top.first().map(|c| c.value), Some(5560));
    }

    fn alert(kind: AlertKind, severity: Severity, since: u64) -> Alert {
        Alert {
            kind,
            severity,
            subject: String::new(),
            title: "t".into(),
            cause: "c".into(),
            values: HashMap::<String, AlertValue>::new(),
            route: None,
            since_sample: since,
        }
    }

    fn process(name: &str, pid: u32, cpu: f32, memory: u64, disk: u64) -> Process {
        let mut p = fixtures::process(name, pid, cpu);
        p.memory_private = Bytes(memory);
        p.disk_read = BytesPerSec(disk);
        p.disk_write = BytesPerSec(0);
        p
    }

    #[test]
    fn a_healthy_machine_gets_no_verdict_which_is_an_answer_not_an_error() {
        let d = diagnose(&[], &fixtures::system(), &[]);
        assert!(d.verdict.is_none());
        assert!(d.also.is_empty());
    }

    #[test]
    fn the_verdict_is_the_most_severe_alert() {
        let alerts = vec![
            alert(AlertKind::CpuSustained, Severity::Warning, 1),
            alert(AlertKind::MemoryPressure, Severity::Critical, 9),
        ];
        let d = diagnose(&alerts, &fixtures::system(), &[]);
        assert_eq!(
            d.verdict.expect("verdict").alert.kind,
            AlertKind::MemoryPressure
        );
    }

    #[test]
    fn ties_go_to_the_alert_that_has_lasted_longest() {
        // Two criticals: the one that has held for minutes is the better
        // explanation of "why is it slow" than one raised this second.
        let alerts = vec![
            alert(AlertKind::ThermalCpu, Severity::Critical, 900),
            alert(AlertKind::MemoryPressure, Severity::Critical, 12),
        ];
        let d = diagnose(&alerts, &fixtures::system(), &[]);
        assert_eq!(
            d.verdict.expect("verdict").alert.kind,
            AlertKind::MemoryPressure,
            "raised earlier means a lower sample index"
        );
    }

    #[test]
    fn the_hog_is_named_with_its_measured_share() {
        let processes = vec![
            process("build.exe", 10, 70.0, 0, 0),
            process("idle.exe", 11, 5.0, 0, 0),
            process("also.exe", 12, 5.0, 0, 0),
        ];
        let alerts = vec![alert(AlertKind::CpuSustained, Severity::Warning, 1)];
        let d = diagnose(&alerts, &fixtures::system(), &processes);
        let v = d.verdict.expect("verdict");
        assert!(!v.diffuse);
        assert_eq!(v.contributors[0].name, "build.exe");
        assert!(
            (v.contributors[0].share - 87.5).abs() < 0.5,
            "share was {}",
            v.contributors[0].share
        );
        assert_eq!(v.contributors.len(), 3, "capped at three");
    }

    #[test]
    fn an_evenly_loaded_machine_is_reported_as_diffuse_rather_than_blaming_the_top_row() {
        // Ten processes at 10 % each. Naming whichever sorts first would be
        // a confident lie; this is the case the flag exists for.
        let processes: Vec<Process> = (0..10)
            .map(|i| process(&format!("p{i}.exe"), 100 + i, 10.0, 0, 0))
            .collect();
        let alerts = vec![alert(AlertKind::CpuSustained, Severity::Warning, 1)];
        let d = diagnose(&alerts, &fixtures::system(), &processes);
        let v = d.verdict.expect("verdict");
        assert!(v.diffuse);
        assert!(
            v.contributors.is_empty(),
            "no name is better than a wrong one"
        );
    }

    #[test]
    fn a_failing_drive_never_names_processes() {
        // Nobody's fault. Listing the busiest processes beside it would
        // imply a causal link that does not exist.
        let processes = vec![process("build.exe", 10, 90.0, 0, 0)];
        let alerts = vec![alert(AlertKind::DiskHealth, Severity::Critical, 1)];
        let d = diagnose(&alerts, &fixtures::system(), &processes);
        let v = d.verdict.expect("verdict");
        assert!(v.contributors.is_empty());
        assert!(
            !v.diffuse,
            "not diffuse — attribution simply does not apply"
        );
    }

    #[test]
    fn shares_are_of_the_process_list_not_the_machine_total() {
        // The kernel's machine-wide CPU includes work by processes that have
        // since exited. Dividing by it would make every share too small and
        // turn a real hog into "diffuse".
        let mut system = fixtures::system();
        system.cpu.total = Percent(100.0);
        let processes = vec![process("one.exe", 10, 20.0, 0, 0)];
        let alerts = vec![alert(AlertKind::CpuSustained, Severity::Warning, 1)];
        let d = diagnose(&alerts, &system, &processes);
        let v = d.verdict.expect("verdict");
        assert!((v.contributors[0].share - 100.0).abs() < 0.1);
    }

    #[test]
    fn memory_pressure_ranks_by_private_bytes_not_cpu() {
        let processes = vec![
            process("busy.exe", 10, 90.0, 1_000_000, 0),
            process("fat.exe", 11, 1.0, 9_000_000, 0),
        ];
        let alerts = vec![alert(AlertKind::MemoryPressure, Severity::Critical, 1)];
        let d = diagnose(&alerts, &fixtures::system(), &processes);
        assert_eq!(d.verdict.expect("verdict").contributors[0].name, "fat.exe");
    }

    #[test]
    fn disk_ranks_on_read_plus_write() {
        let processes = vec![
            process("reader.exe", 10, 1.0, 0, 5_000_000),
            process("quiet.exe", 11, 1.0, 0, 1),
        ];
        let alerts = vec![alert(AlertKind::DiskSaturated, Severity::Warning, 1)];
        let d = diagnose(&alerts, &fixtures::system(), &processes);
        assert_eq!(
            d.verdict.expect("verdict").contributors[0].name,
            "reader.exe"
        );
    }

    #[test]
    fn the_secondary_alerts_are_carried_but_the_verdict_is_not_repeated() {
        let alerts = vec![
            alert(AlertKind::MemoryPressure, Severity::Critical, 1),
            alert(AlertKind::CpuSustained, Severity::Warning, 2),
        ];
        let d = diagnose(&alerts, &fixtures::system(), &[]);
        assert_eq!(d.also.len(), 1);
        assert_eq!(d.also[0].kind, AlertKind::CpuSustained);
    }

    #[test]
    fn it_works_against_the_real_engine_output() {
        // The unit tests above build alerts by hand. This one proves the
        // shapes actually meet: engine output straight into diagnose.
        let mut system = fixtures::system();
        system.cpu.total = Percent(98.0);
        let mut engine = Engine::new();
        for _ in 0..crate::alerts::SUSTAIN_SAMPLES {
            engine.poll(&system);
        }
        let processes = vec![process("hog.exe", 10, 95.0, 0, 0)];
        let d = diagnose(&engine.active(), &system, &processes);
        let v = d.verdict.expect("the engine raised something");
        assert_eq!(v.alert.kind, AlertKind::CpuSustained);
        assert_eq!(v.subsystem, Subsystem::Cpu);
        assert_eq!(v.contributors[0].name, "hog.exe");
    }
}
