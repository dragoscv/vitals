//! Prometheus text exposition.
//!
//! Hand-written rather than `prometheus-client`. The registry crates exist to
//! manage mutable counters across a codebase; here there is exactly one place
//! that renders exactly one snapshot, from data that already exists. A
//! registry would add a dependency, a second copy of every value, and a
//! lifecycle to keep in step with the sampler — to produce the same string.
//!
//! Format: <https://prometheus.io/docs/instrumenting/exposition_formats/>.
//! The rules that matter and are easy to get wrong: `# HELP`/`# TYPE` at most
//! once per metric name and before its samples; label values escape `\`, `"`
//! and newline; a value is a Go float, so `NaN`/`Inf` must be spelled that
//! way; and a metric that cannot be measured is **omitted**, never zero —
//! same principle as the rest of Vitals, and Prometheus handles absence
//! correctly (`absent()`) whereas a fabricated zero poisons every average.

use std::fmt::Write as _;

use vitals_core::metrics::SystemMetrics;
use vitals_core::process::Process;

/// Renders the exposition body for one sample.
///
/// One long, flat function on purpose: the exposition format is a list, and
/// the reader should be able to check it against the output top to bottom
/// without following calls.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn render(system: &SystemMetrics, processes: &[Process], top_n: usize) -> String {
    let mut out = String::with_capacity(4096);

    gauge(
        &mut out,
        "vitals_cpu_percent",
        "Total CPU utilisation across all cores.",
        |o| {
            let _ = writeln!(o, "vitals_cpu_percent {}", num(system.cpu.total.0));
        },
    );

    gauge(
        &mut out,
        "vitals_cpu_kernel_percent",
        "Share of CPU spent in kernel mode.",
        |o| {
            let _ = writeln!(o, "vitals_cpu_kernel_percent {}", num(system.cpu.kernel.0));
        },
    );

    gauge(
        &mut out,
        "vitals_cpu_core_percent",
        "Per logical processor utilisation.",
        |o| {
            for (i, core) in system.cpu.per_core.iter().enumerate() {
                let _ = writeln!(o, "vitals_cpu_core_percent{{core=\"{i}\"}} {}", num(core.0));
            }
        },
    );

    gauge(
        &mut out,
        "vitals_memory_bytes",
        "Physical memory, by state.",
        |o| {
            let _ = writeln!(
                o,
                "vitals_memory_bytes{{state=\"total\"}} {}",
                system.memory.total.0
            );
            let _ = writeln!(
                o,
                "vitals_memory_bytes{{state=\"used\"}} {}",
                system.memory.used.0
            );
            let _ = writeln!(
                o,
                "vitals_memory_bytes{{state=\"available\"}} {}",
                system.memory.available.0
            );
            let _ = writeln!(
                o,
                "vitals_memory_bytes{{state=\"cached\"}} {}",
                system.memory.cached.0
            );
        },
    );

    if !system.disks.is_empty() {
        gauge(
            &mut out,
            "vitals_disk_bytes_per_second",
            "Disk throughput.",
            |o| {
                for d in &system.disks {
                    let name = escape(&d.name);
                    let _ = writeln!(
                        o,
                        "vitals_disk_bytes_per_second{{disk=\"{name}\",direction=\"read\"}} {}",
                        d.read.0
                    );
                    let _ = writeln!(
                        o,
                        "vitals_disk_bytes_per_second{{disk=\"{name}\",direction=\"write\"}} {}",
                        d.write.0
                    );
                }
            },
        );
        gauge(
            &mut out,
            "vitals_disk_active_percent",
            "Share of time the disk had IO outstanding.",
            |o| {
                for d in &system.disks {
                    let _ = writeln!(
                        o,
                        "vitals_disk_active_percent{{disk=\"{}\"}} {}",
                        escape(&d.name),
                        num(d.active_time.0)
                    );
                }
            },
        );
        gauge(
            &mut out,
            "vitals_disk_capacity_bytes",
            "Volume capacity and free space.",
            |o| {
                for d in &system.disks {
                    let name = escape(&d.name);
                    let _ = writeln!(
                        o,
                        "vitals_disk_capacity_bytes{{disk=\"{name}\",state=\"total\"}} {}",
                        d.total.0
                    );
                    let _ = writeln!(
                        o,
                        "vitals_disk_capacity_bytes{{disk=\"{name}\",state=\"free\"}} {}",
                        d.free.0
                    );
                }
            },
        );
    }

    if !system.networks.is_empty() {
        gauge(
            &mut out,
            "vitals_network_bytes_per_second",
            "Adapter throughput.",
            |o| {
                for n in &system.networks {
                    let name = escape(&n.name);
                    let _ = writeln!(
                        o,
                        "vitals_network_bytes_per_second{{adapter=\"{name}\",direction=\"rx\"}} {}",
                        n.rx.0
                    );
                    let _ = writeln!(
                        o,
                        "vitals_network_bytes_per_second{{adapter=\"{name}\",direction=\"tx\"}} {}",
                        n.tx.0
                    );
                }
            },
        );
    }

    if !system.gpus.is_empty() {
        // Same rule as memory below: an adapter with no engine counters has
        // no series, rather than a 0 % that would pull `avg()` down.
        let measured: Vec<_> = system
            .gpus
            .iter()
            .filter_map(|g| g.utilization.map(|u| (g, u)))
            .collect();
        if !measured.is_empty() {
            gauge(
                &mut out,
                "vitals_gpu_percent",
                "Busiest engine on each GPU.",
                |o| {
                    for (g, utilization) in &measured {
                        let _ = writeln!(
                            o,
                            "vitals_gpu_percent{{gpu=\"{}\"}} {}",
                            escape(&g.name),
                            num(utilization.0)
                        );
                    }
                },
            );
        }
        // Only emitted for GPUs that report it. A card whose driver hides VRAM
        // must not appear to have zero.
        let with_memory: Vec<_> = system
            .gpus
            .iter()
            .filter(|g| g.memory_used.is_some())
            .collect();
        if !with_memory.is_empty() {
            gauge(
                &mut out,
                "vitals_gpu_memory_bytes",
                "GPU memory in use, where reported.",
                |o| {
                    for g in &with_memory {
                        if let Some(used) = g.memory_used {
                            let _ = writeln!(
                                o,
                                "vitals_gpu_memory_bytes{{gpu=\"{}\"}} {}",
                                escape(&g.name),
                                used.0
                            );
                        }
                    }
                },
            );
        }
    }

    // Optional machine-wide readings. Omitted, not zeroed, when absent.
    if let Some(clock) = system.cpu.effective_clock {
        gauge(
            &mut out,
            "vitals_cpu_clock_hertz",
            "Average effective CPU frequency over the last interval.",
            |o| {
                let _ = writeln!(o, "vitals_cpu_clock_hertz {}", clock.0);
            },
        );
    }
    let gpu_temps: Vec<_> = system
        .gpus
        .iter()
        .filter_map(|g| g.temperature.map(|t| (g, t)))
        .collect();
    if !gpu_temps.is_empty() {
        gauge(
            &mut out,
            "vitals_gpu_temperature_celsius",
            "GPU core temperature, where the driver reports it.",
            |o| {
                for (g, t) in &gpu_temps {
                    let _ = writeln!(
                        o,
                        "vitals_gpu_temperature_celsius{{gpu=\"{}\"}} {}",
                        escape(&g.name),
                        num(t.0)
                    );
                }
            },
        );
    }
    let disk_temps: Vec<_> = system
        .disks
        .iter()
        .filter_map(|d| d.temperature.map(|t| (d, t)))
        .collect();
    if !disk_temps.is_empty() {
        gauge(
            &mut out,
            "vitals_disk_temperature_celsius",
            "Drive temperature, per volume, where the drive reports it.",
            |o| {
                for (d, t) in &disk_temps {
                    let _ = writeln!(
                        o,
                        "vitals_disk_temperature_celsius{{disk=\"{}\"}} {}",
                        escape(d.mount.as_deref().unwrap_or(&d.name)),
                        num(t.0)
                    );
                }
            },
        );
    }
    let disk_life: Vec<_> = system
        .disks
        .iter()
        .filter_map(|d| d.health.as_ref()?.life_remaining.map(|l| (d, l)))
        .collect();
    if !disk_life.is_empty() {
        gauge(
            &mut out,
            "vitals_disk_life_remaining_percent",
            "Remaining rated write endurance an SSD reports about itself.",
            |o| {
                for (d, l) in &disk_life {
                    let _ = writeln!(
                        o,
                        "vitals_disk_life_remaining_percent{{disk=\"{}\"}} {}",
                        escape(d.mount.as_deref().unwrap_or(&d.name)),
                        num(l.0)
                    );
                }
            },
        );
    }
    if let Some(temp) = system.cpu.temperature {
        gauge(
            &mut out,
            "vitals_cpu_temperature_celsius",
            "CPU package temperature.",
            |o| {
                let _ = writeln!(o, "vitals_cpu_temperature_celsius {}", num(temp.0));
            },
        );
    }
    if let Some(power) = system.cpu.power {
        gauge(
            &mut out,
            "vitals_cpu_power_watts",
            "CPU package power (RAPL), from the optional sensors service.",
            |o| {
                let _ = writeln!(o, "vitals_cpu_power_watts {}", num(power.0));
            },
        );
    }
    if !system.fans.is_empty() {
        gauge(
            &mut out,
            "vitals_fan_rpm",
            "Motherboard fan speed, from the optional sensors service.",
            |o| {
                for fan in &system.fans {
                    let _ = writeln!(
                        o,
                        "vitals_fan_rpm{{fan=\"{}\"}} {}",
                        escape(&fan.name),
                        fan.rpm
                    );
                }
            },
        );
    }
    if let Some(power) = system.power_draw {
        gauge(
            &mut out,
            "vitals_power_draw_watts",
            "Whole-system power draw.",
            |o| {
                let _ = writeln!(o, "vitals_power_draw_watts {}", num(power.0));
            },
        );
    }
    if let Some(battery) = &system.battery {
        gauge(&mut out, "vitals_battery_percent", "Battery charge.", |o| {
            let _ = writeln!(o, "vitals_battery_percent {}", num(battery.charge.0));
        });
    }

    gauge(
        &mut out,
        "vitals_process_count",
        "Processes and threads running.",
        |o| {
            let _ = writeln!(o, "vitals_process_count {}", system.cpu.process_count);
        },
    );
    gauge(&mut out, "vitals_thread_count", "Threads running.", |o| {
        let _ = writeln!(o, "vitals_thread_count {}", system.cpu.thread_count);
    });
    gauge(&mut out, "vitals_uptime_seconds", "Time since boot.", |o| {
        let _ = writeln!(o, "vitals_uptime_seconds {}", system.cpu.uptime_secs);
    });

    // Top processes only. One series per process on a 600-process machine
    // would be 1200 series a second — enough to make a small Prometheus
    // unhappy, and the tail is noise anyway.
    if top_n > 0 && !processes.is_empty() {
        let mut ranked: Vec<&Process> = processes.iter().collect();
        ranked.sort_by(|a, b| {
            b.cpu
                .0
                .partial_cmp(&a.cpu.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory_private.0.cmp(&a.memory_private.0))
        });
        ranked.truncate(top_n);

        gauge(
            &mut out,
            "vitals_process_cpu_percent",
            "CPU by process (top N).",
            |o| {
                for p in &ranked {
                    let _ = writeln!(
                        o,
                        "vitals_process_cpu_percent{{name=\"{}\",pid=\"{}\"}} {}",
                        escape(&p.name),
                        p.key.pid.0,
                        num(p.cpu.0)
                    );
                }
            },
        );
        gauge(
            &mut out,
            "vitals_process_memory_bytes",
            "Private bytes by process (top N).",
            |o| {
                for p in &ranked {
                    let _ = writeln!(
                        o,
                        "vitals_process_memory_bytes{{name=\"{}\",pid=\"{}\"}} {}",
                        escape(&p.name),
                        p.key.pid.0,
                        p.memory_private.0
                    );
                }
            },
        );
    }

    out
}

/// Writes the `# HELP` / `# TYPE` preamble once, then the samples.
fn gauge(out: &mut String, name: &str, help: &str, body: impl FnOnce(&mut String)) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
    body(out);
}

/// Escapes a label value per the exposition format.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

/// Formats a float the way Prometheus expects.
fn num(v: f32) -> String {
    if v.is_nan() {
        "NaN".to_owned()
    } else if v.is_infinite() {
        if v.is_sign_positive() { "+Inf" } else { "-Inf" }.to_owned()
    } else {
        format!("{v:.3}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::fixtures;
    use vitals_core::metrics::DiskKind;
    use vitals_core::units::{Bytes, BytesPerSec, Celsius, Percent};

    #[test]
    fn every_metric_declares_help_and_type_exactly_once() {
        // Two `# TYPE` lines for one name makes a scrape fail outright.
        let out = render(&fixtures::system(), &[], 0);
        for line in out.lines().filter(|l| l.starts_with("# TYPE ")) {
            let name = line.split_whitespace().nth(2).unwrap();
            let types = out
                .lines()
                .filter(|l| l.starts_with(&format!("# TYPE {name} ")))
                .count();
            assert_eq!(types, 1, "{name} declared {types} times");
        }
    }

    #[test]
    fn unmeasured_readings_are_absent_rather_than_zero() {
        // The whole project's principle, and Prometheus gets it right: a
        // missing series is queryable with absent(); a fake zero silently
        // drags every average down.
        let out = render(&fixtures::system(), &[], 0);
        assert!(!out.contains("vitals_cpu_temperature_celsius"));
        assert!(!out.contains("vitals_cpu_power_watts"));
        assert!(!out.contains("vitals_power_draw_watts"));
        assert!(!out.contains("vitals_battery_percent"));

        let mut with_temp = fixtures::system();
        with_temp.cpu.temperature = Some(Celsius(61.5));
        let out = render(&with_temp, &[], 0);
        assert!(
            out.contains("vitals_cpu_temperature_celsius 61.500"),
            "{out}"
        );

        let mut with_power = fixtures::system();
        with_power.cpu.power = Some(vitals_core::units::Watts(142.25));
        let out = render(&with_power, &[], 0);
        assert!(out.contains("vitals_cpu_power_watts 142.250"), "{out}");

        let out = render(&fixtures::system(), &[], 0);
        assert!(out.contains(r#"vitals_fan_rpm{fan="Fan 1"} 1467"#), "{out}");
        let mut no_fans = fixtures::system();
        no_fans.fans.clear();
        let out = render(&no_fans, &[], 0);
        assert!(!out.contains("vitals_fan_rpm"), "no fans means no series");
    }

    #[test]
    fn label_values_are_escaped() {
        let mut s = fixtures::system();
        s.disks[0].name = r#"Weird "disk"\name"#.to_owned();
        let out = render(&s, &[], 0);
        assert!(out.contains(r#"disk="Weird \"disk\"\\name""#), "{out}");
    }

    #[test]
    fn per_core_series_are_labelled_not_named() {
        let out = render(&fixtures::system(), &[], 0);
        assert!(
            out.contains(r#"vitals_cpu_core_percent{core="0"} 30.000"#),
            "{out}"
        );
        assert!(
            out.contains(r#"vitals_cpu_core_percent{core="1"} 37.000"#),
            "{out}"
        );
    }

    #[test]
    fn disks_and_adapters_emit_both_directions() {
        let out = render(&fixtures::system(), &[], 0);
        assert!(
            out.contains(r#"vitals_disk_bytes_per_second{disk="C:",direction="read"} 1048576"#),
            "{out}"
        );
        assert!(
            out.contains(
                r#"vitals_network_bytes_per_second{adapter="Ethernet",direction="tx"} 65536"#
            ),
            "{out}"
        );
    }

    #[test]
    fn process_series_are_capped_and_ranked_by_cpu() {
        // 600 processes x 2 series a second is enough to hurt a small
        // Prometheus, and the tail is noise.
        let procs: Vec<_> = (0..50)
            .map(|i| fixtures::process(&format!("p{i}.exe"), 1000 + i, i as f32))
            .collect();
        let out = render(&fixtures::system(), &procs, 3);
        let cpu_lines = out
            .lines()
            .filter(|l| l.starts_with("vitals_process_cpu_percent{"))
            .count();
        assert_eq!(cpu_lines, 3);
        assert!(
            out.contains(r#"name="p49.exe""#),
            "highest CPU must be first"
        );
        assert!(!out.contains(r#"name="p0.exe""#));
    }

    #[test]
    fn gpu_memory_is_omitted_for_cards_that_do_not_report_it() {
        let mut s = fixtures::system();
        s.gpus.push(vitals_core::metrics::GpuMetrics {
            id: vitals_core::ids::GpuId(0),
            name: "Test GPU".into(),
            vendor: vitals_core::metrics::GpuVendor::Unknown,
            engines: Vec::new(),
            utilization: Some(Percent(17.0)),
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
        let out = render(&s, &[], 0);
        assert!(
            out.contains(r#"vitals_gpu_percent{gpu="Test GPU"} 17.000"#),
            "{out}"
        );
        assert!(!out.contains("vitals_gpu_memory_bytes"), "{out}");
    }

    #[test]
    fn an_adapter_with_no_counters_has_no_gpu_percent_series() {
        // Two phantom display adapters on the dev machine reported 0 %
        // beside a real GPU at 16 %; averaged in Grafana that read as 5 %.
        let mut s = fixtures::system();
        s.gpus.push(vitals_core::metrics::GpuMetrics {
            id: vitals_core::ids::GpuId(1),
            name: "Phantom".into(),
            vendor: vitals_core::metrics::GpuVendor::Unknown,
            engines: Vec::new(),
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
        let out = render(&s, &[], 0);
        assert!(!out.contains(r#"gpu="Phantom""#), "{out}");
        assert!(
            !out.contains("vitals_gpu_percent"),
            "no measured GPU, no series at all: {out}"
        );
    }

    #[test]
    fn floats_are_formatted_the_way_prometheus_parses_them() {
        assert_eq!(num(1.0), "1.000");
        assert_eq!(num(f32::NAN), "NaN");
        assert_eq!(num(f32::INFINITY), "+Inf");
        assert_eq!(num(f32::NEG_INFINITY), "-Inf");
    }

    #[test]
    fn a_disk_kind_change_does_not_break_rendering() {
        let mut s = fixtures::system();
        s.disks[0].kind = DiskKind::Hdd;
        s.disks[0].total = Bytes(1);
        s.disks[0].read = BytesPerSec(0);
        assert!(render(&s, &[], 0).contains("vitals_disk_capacity_bytes"));
    }
}
