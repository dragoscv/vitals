//! Turning readings into text.
//!
//! Every formatter here takes the `Option` the model gives it and renders
//! `None` as an em dash. Nothing in this module ever turns an absent value
//! into a zero — that is the one rule the whole project is built on.

use comfy_table::{ContentArrangement, Table, presets};
use vitals_core::metrics::SystemMetrics;
use vitals_core::process::{Process, ProcessState};
use vitals_core::provider::HostInfo;
use vitals_core::units::{Bytes, BytesPerSec, Percent};

/// What an unmeasured reading looks like.
pub const DASH: &str = "—";

pub fn bytes(value: Bytes) -> String {
    human(value.get(), "B")
}

pub fn rate(value: BytesPerSec) -> String {
    human(value.get(), "B/s")
}

pub fn percent(value: Percent) -> String {
    format!("{:.1} %", value.get())
}

/// `None` is a dash, never `0`.
pub fn opt<T>(value: Option<T>, f: impl Fn(T) -> String) -> String {
    value.map_or_else(|| DASH.to_owned(), f)
}

/// 1024-based, one decimal, unit suffix. The desktop shows the same scale.
fn human(raw: u64, unit: &str) -> String {
    const STEPS: [&str; 6] = ["", "K", "M", "G", "T", "P"];
    let mut value = raw as f64;
    let mut step = 0;
    while value >= 1024.0 && step < STEPS.len() - 1 {
        value /= 1024.0;
        step += 1;
    }
    if step == 0 {
        format!("{raw} {unit}")
    } else {
        format!("{value:.1} {}{unit}", STEPS[step])
    }
}

pub const fn state(value: ProcessState) -> &'static str {
    match value {
        ProcessState::Running => "running",
        ProcessState::Suspended => "suspended",
        ProcessState::Waiting => "waiting",
        ProcessState::NotResponding => "not responding",
        ProcessState::Zombie => "zombie",
    }
}

fn table() -> Table {
    let mut t = Table::new();
    t.load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Disabled);
    t
}

/// The process table `ps` and `top` share.
pub fn processes(list: &[Process]) -> String {
    let mut t = table();
    t.set_header([
        "PID", "Name", "CPU %", "Memory", "Disk R", "Disk W", "State",
    ]);
    for p in list {
        t.add_row([
            p.key.pid.get().to_string(),
            p.name.clone(),
            format!("{:.1}", p.cpu.get()),
            bytes(p.memory_private),
            rate(p.disk_read),
            rate(p.disk_write),
            state(p.state).to_owned(),
        ]);
    }
    t.to_string()
}

/// Two-column key/value table.
pub fn pairs(rows: &[(&str, String)]) -> String {
    let mut t = table();
    for (k, v) in rows {
        t.add_row([(*k).to_owned(), v.clone()]);
    }
    t.to_string()
}

pub fn host(info: &HostInfo) -> Vec<(&'static str, String)> {
    vec![
        ("Host", info.hostname.clone()),
        (
            "OS",
            format!(
                "{} {} ({})",
                info.os_name, info.os_version, info.kernel_version
            ),
        ),
        ("Architecture", info.architecture.clone()),
        ("CPU", info.cpu_model.clone()),
        (
            "Cores",
            format!(
                "{} physical, {} logical",
                info.physical_cores, info.logical_cores
            ),
        ),
        ("Memory", bytes(info.total_memory)),
        ("Motherboard", opt(info.motherboard.clone(), |s| s)),
        ("BIOS", opt(info.bios_version.clone(), |s| s)),
        (
            "Virtual machine",
            if info.is_virtual_machine { "yes" } else { "no" }.to_owned(),
        ),
    ]
}

pub fn system(metrics: &SystemMetrics) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("CPU", percent(metrics.cpu.total)),
        (
            "Memory used",
            format!(
                "{} of {} ({})",
                bytes(metrics.memory.used),
                bytes(metrics.memory.total),
                percent(metrics.memory.used_percent())
            ),
        ),
        ("Processes", metrics.cpu.process_count.to_string()),
        ("Threads", metrics.cpu.thread_count.to_string()),
        ("Uptime", uptime(metrics.cpu.uptime_secs)),
        (
            "CPU clock",
            opt(metrics.cpu.effective_clock, |h| {
                format!("{:.2} GHz", h.get() as f64 / 1e9)
            }),
        ),
        (
            "CPU temperature",
            opt(metrics.cpu.temperature, |c| format!("{:.0} °C", c.get())),
        ),
    ];
    for gpu in &metrics.gpus {
        rows.push((
            "GPU",
            format!("{}: {}", gpu.name, opt(gpu.utilization, percent)),
        ));
    }
    rows
}

/// A one-line header for `top`.
pub fn summary_line(metrics: &SystemMetrics, alerts: usize) -> String {
    let gpu = metrics
        .gpus
        .iter()
        .find_map(|g| g.utilization)
        .map_or_else(String::new, |u| format!("  GPU {}", percent(u)));
    format!(
        "CPU {}  Mem {} / {}{gpu}  Alerts {alerts}",
        percent(metrics.cpu.total),
        bytes(metrics.memory.used),
        bytes(metrics.memory.total),
    )
}

pub fn uptime(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else {
        format!("{h}h {m}m")
    }
}

/// Sorts by CPU (busiest first) and truncates to `n` when `n > 0`.
pub fn top_by_cpu(mut list: Vec<Process>, n: usize) -> Vec<Process> {
    list.sort_by(|a, b| b.cpu.get().total_cmp(&a.cpu.get()));
    if n > 0 {
        list.truncate(n);
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::fixtures;

    #[test]
    fn an_unmeasured_reading_renders_as_a_dash_not_zero() {
        assert_eq!(opt(None::<Percent>, percent), DASH);
        assert_eq!(opt(Some(Percent(12.5)), percent), "12.5 %");
        // A GPU with no counters must not read "0.0 %" in the summary.
        let mut metrics = fixtures::system();
        metrics.gpus.push(vitals_core::metrics::GpuMetrics {
            id: vitals_core::ids::GpuId(0),
            name: "Phantom".into(),
            vendor: vitals_core::metrics::GpuVendor::Unknown,
            engines: vec![],
            utilization: None,
            memory_used: None,
            memory_total: None,
            shared_memory_used: None,
            core_clock: None,
            memory_clock: None,
            temperature: None,
            hotspot_temperature: None,
            power: None,
            power_limit: None,
            fan_percent: None,
            fan_rpm: None,
            throttled: None,
            driver_version: None,
        });
        let rows = system(&metrics);
        let gpu = rows.iter().find(|(k, _)| *k == "GPU").expect("gpu row");
        assert!(gpu.1.ends_with(DASH), "{}", gpu.1);
        assert!(!summary_line(&metrics, 0).contains("GPU"));
    }

    #[test]
    fn top_sorts_by_cpu_descending_and_truncates() {
        let list = vec![
            fixtures::process("low.exe", 1, 1.0),
            fixtures::process("high.exe", 2, 50.0),
            fixtures::process("mid.exe", 3, 10.0),
        ];
        let names: Vec<_> = top_by_cpu(list.clone(), 2)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["high.exe", "mid.exe"]);
        assert_eq!(top_by_cpu(list, 0).len(), 3, "0 means no limit");
    }

    #[test]
    fn byte_sizes_scale_by_1024_with_one_decimal() {
        assert_eq!(bytes(Bytes(512)), "512 B");
        assert_eq!(bytes(Bytes(1536)), "1.5 KB");
        assert_eq!(rate(BytesPerSec(3 * 1024 * 1024)), "3.0 MB/s");
    }

    #[test]
    fn the_process_table_has_one_row_per_process_plus_header() {
        let text = processes(&[fixtures::process("a.exe", 7, 3.0)]);
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains("a.exe"));
    }
}
