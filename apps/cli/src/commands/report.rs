//! `vitals report` — record for a while, write one JSON file to share.
//!
//! The shape is deliberately small and flat: someone asking "why is my PC
//! slow" pastes this into a forum, and a reader with no Vitals installed
//! must be able to see the answer in the samples without a schema.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::Serialize;
use vitals_core::alerts::Alert;
use vitals_core::process::Process;
use vitals_core::provider::HostInfo;
use vitals_core::units::{Bytes, Percent};

use crate::fold::View;
use crate::render;
use crate::source::Source;

/// How many processes each sample keeps. Enough to name the culprit, few
/// enough that a minute of samples stays under 100 KB.
const TOP_PER_SAMPLE: usize = 5;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub host: Option<HostInfo>,
    pub alerts: Vec<Alert>,
    pub samples: Vec<Sample>,
    /// Milliseconds since the Unix epoch.
    pub generated_at: u64,
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// The frame's own timestamp, milliseconds since the Unix epoch.
    pub t: u64,
    pub cpu: Percent,
    pub memory_used: Bytes,
    pub memory_total: Bytes,
    pub top_processes: Vec<TopProcess>,
}

/// The few fields a reader needs to name a culprit.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopProcess {
    pub pid: u32,
    pub name: String,
    pub cpu: Percent,
    pub memory_private: Bytes,
}

impl Sample {
    #[must_use]
    pub fn from_view(view: &View) -> Self {
        let top = render::top_by_cpu(view.processes(), TOP_PER_SAMPLE);
        Self {
            t: view.timestamp_ms,
            cpu: view.system.cpu.total,
            memory_used: view.system.memory.used,
            memory_total: view.system.memory.total,
            top_processes: top.iter().map(TopProcess::from).collect(),
        }
    }
}

impl From<&Process> for TopProcess {
    fn from(p: &Process) -> Self {
        Self {
            pid: p.key.pid.get(),
            name: p.name.clone(),
            cpu: p.cpu,
            memory_private: p.memory_private,
        }
    }
}

#[must_use]
pub fn default_path(now_ms: u64) -> PathBuf {
    PathBuf::from(format!("vitals-report-{now_ms}.json"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// # Errors
/// The source failed mid-recording, or the file could not be written.
pub fn run(
    source: &mut Source,
    duration: Duration,
    output: Option<&Path>,
    json: bool,
) -> Result<()> {
    eprintln!(
        "recording for {} s… (Ctrl-C abandons the report)",
        duration.as_secs()
    );
    let host = source.host()?;
    let started = Instant::now();
    let mut samples = Vec::new();
    while started.elapsed() < duration {
        match source.next_frame(Duration::from_secs(1))? {
            Some(view) => samples.push(Sample::from_view(view)),
            None => anyhow::bail!("the stream ended before the recording finished"),
        }
    }
    let alerts = source.alerts()?;

    let report = Report {
        host,
        alerts,
        samples,
        generated_at: now_ms(),
        source: source.kind(),
    };
    let path = output.map_or_else(|| default_path(report.generated_at), Path::to_path_buf);
    let bytes = serde_json::to_vec_pretty(&report)?;
    std::fs::write(&path, &bytes).with_context(|| format!("writing {}", path.display()))?;

    summarise(&report);
    if json {
        println!(
            "{}",
            serde_json::json!({ "path": path, "samples": report.samples.len(), "bytes": bytes.len() })
        );
    } else {
        println!("{}", path.display());
    }
    Ok(())
}

fn summarise(report: &Report) {
    let n = report.samples.len();
    if n == 0 {
        eprintln!("no samples recorded");
        return;
    }
    let sum: f32 = report.samples.iter().map(|s| s.cpu.get()).sum();
    let peak = report
        .samples
        .iter()
        .map(|s| s.cpu.get())
        .fold(0.0_f32, f32::max);
    eprintln!(
        "{n} samples · CPU avg {:.1} % peak {peak:.1} % · {} alerts active",
        sum / n as f32,
        report.alerts.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::fixtures;

    #[test]
    fn the_report_json_carries_exactly_the_documented_top_level_keys() {
        let mut view = View::default();
        view.apply(&fixtures::keyframe(
            1,
            vec![
                fixtures::process("a.exe", 1, 1.0),
                fixtures::process("b.exe", 2, 9.0),
            ],
        ));
        let report = Report {
            host: None,
            alerts: vec![],
            samples: vec![Sample::from_view(&view)],
            generated_at: 1_700_000_000_000,
            source: "direct",
        };
        let value: serde_json::Value = serde_json::to_value(&report).expect("report serialises");
        let mut keys: Vec<_> = value.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["alerts", "generatedAt", "host", "samples", "source"]);

        let sample = &value["samples"][0];
        let mut sample_keys: Vec<_> = sample
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect();
        sample_keys.sort();
        assert_eq!(
            sample_keys,
            ["cpu", "memoryTotal", "memoryUsed", "t", "topProcesses"]
        );
        assert_eq!(sample["topProcesses"][0]["name"], "b.exe", "busiest first");
    }

    #[test]
    fn each_sample_keeps_at_most_five_processes() {
        let mut view = View::default();
        let many: Vec<_> = (1..=12)
            .map(|i| fixtures::process("p.exe", i, i as f32))
            .collect();
        view.apply(&fixtures::keyframe(1, many));
        assert_eq!(Sample::from_view(&view).top_processes.len(), TOP_PER_SAMPLE);
    }
}
