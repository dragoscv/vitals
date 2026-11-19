//! Turns one machine sample into the readings that are published.
//!
//! This is the only place that decides *which* numbers leave the machine, and
//! it keeps the whole of Vitals' Option discipline: a reading whose source is
//! absent is not in the list, so it is neither announced to Home Assistant nor
//! published. There is no `0` for "the platform cannot measure this" — a
//! laptop without a battery sensor does not report an empty battery, and a
//! Mac does not report a CPU temperature of zero.

use serde_json::{Value, json};

use vitals_core::metrics::SystemMetrics;

use crate::discovery::EntitySpec;
use crate::topics::Component;

/// One number, ready to publish, with the entity it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub spec: EntitySpec,
    pub value: Value,
}

const CPU: EntitySpec = EntitySpec {
    object: "cpu",
    name: "CPU",
    key: "summary/cpu",
    component: Component::Sensor,
    unit: Some("%"),
    device_class: None,
    state_class: Some("measurement"),
};

const CPU_TEMP: EntitySpec = EntitySpec {
    object: "cpu_temperature",
    name: "CPU temperature",
    key: "summary/cpu_temperature",
    component: Component::Sensor,
    unit: Some("°C"),
    device_class: Some("temperature"),
    state_class: Some("measurement"),
};

const MEMORY: EntitySpec = EntitySpec {
    object: "memory",
    name: "Memory in use",
    key: "summary/memory",
    component: Component::Sensor,
    unit: Some("%"),
    device_class: None,
    state_class: Some("measurement"),
};

const PROCESSES: EntitySpec = EntitySpec {
    object: "processes",
    name: "Processes",
    key: "summary/processes",
    component: Component::Sensor,
    unit: None,
    device_class: None,
    state_class: Some("measurement"),
};

const ALERTS: EntitySpec = EntitySpec {
    object: "alerts",
    name: "Active alerts",
    key: "alerts/active",
    component: Component::Sensor,
    unit: None,
    device_class: None,
    state_class: Some("measurement"),
};

const PROBLEM: EntitySpec = EntitySpec {
    object: "problem",
    name: "Problem",
    key: "alerts/problem",
    component: Component::BinarySensor,
    unit: None,
    device_class: Some("problem"),
    state_class: None,
};

const BATTERY: EntitySpec = EntitySpec {
    object: "battery",
    name: "Battery",
    key: "summary/battery",
    component: Component::Sensor,
    unit: Some("%"),
    device_class: Some("battery"),
    state_class: Some("measurement"),
};

/// Every entity Vitals can announce, so the discovery pass and the state pass
/// cannot drift apart. A spec that is announced but never published leaves a
/// permanently `unknown` entity on the dashboard.
#[must_use]
pub fn all_specs() -> Vec<EntitySpec> {
    vec![CPU, CPU_TEMP, MEMORY, PROCESSES, ALERTS, PROBLEM, BATTERY]
}

/// The readings for one sample.
///
/// `alert_count` comes from the same frame the system metrics do; the caller
/// already has it and this keeps the function pure. The process count is read
/// from the frame's own CPU metrics, so a platform that cannot count them
/// publishes the entity as absent rather than zero.
#[must_use]
pub fn readings(system: &SystemMetrics, alert_count: usize) -> Vec<Reading> {
    let mut out = Vec::with_capacity(all_specs().len());

    out.push(Reading {
        spec: CPU,
        value: json!(round(f64::from(system.cpu.total.0))),
    });

    // Absent on a platform with no CPU temperature sensor, and absent means
    // absent — never a zero that would draw a flat line on a dashboard.
    if let Some(temp) = system.cpu.temperature {
        out.push(Reading {
            spec: CPU_TEMP,
            value: json!(round(f64::from(temp.0))),
        });
    }

    if system.memory.total.0 > 0 {
        let percent = system.memory.used.0 as f64 / system.memory.total.0 as f64 * 100.0;
        out.push(Reading {
            spec: MEMORY,
            value: json!(round(percent)),
        });
    }

    // Absent where the platform will not let the sampler enumerate processes
    // (a sandboxed Mac build). A zero there is a lie, and `absent()` in Home
    // Assistant handles an omitted state correctly.
    if let Some(process_count) = system.cpu.process_count {
        out.push(Reading {
            spec: PROCESSES,
            value: json!(process_count),
        });
    }
    out.push(Reading {
        spec: ALERTS,
        value: json!(alert_count),
    });
    // A binary_sensor carries the vocabulary Home Assistant reads, not a
    // boolean: `ON` and `OFF` are what the default plugin expects.
    out.push(Reading {
        spec: PROBLEM,
        value: Value::from(if alert_count > 0 { "ON" } else { "OFF" }),
    });

    if let Some(battery) = &system.battery {
        out.push(Reading {
            spec: BATTERY,
            value: json!(round(f64::from(battery.charge.0))),
        });
    }

    out
}

/// One decimal place. A dashboard shows more precision than the sampler has,
/// and a value that jitters in the ninth decimal place makes Home Assistant's
/// history useless.
fn round(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::metrics::{CpuMetrics, MemoryMetrics, SystemMetrics};
    use vitals_core::units::{Bytes, Celsius, Percent};

    fn machine(cpu_temp: Option<f32>, battery: Option<f32>) -> SystemMetrics {
        SystemMetrics {
            cpu: CpuMetrics {
                total: Percent(42.25),
                process_count: Some(214),
                temperature: cpu_temp.map(Celsius),
                ..CpuMetrics::default()
            },
            memory: MemoryMetrics {
                total: Bytes(16 * 1024 * 1024 * 1024),
                used: Bytes(8 * 1024 * 1024 * 1024),
                ..MemoryMetrics::default()
            },
            battery: battery.map(|charge| vitals_core::metrics::BatteryMetrics {
                charge: Percent(charge),
                charging: false,
                time_remaining_secs: None,
                power: None,
                health: None,
                cycle_count: None,
                temperature: None,
            }),
            ..SystemMetrics::default()
        }
    }

    fn value_of(readings: &[Reading], object: &str) -> Option<Value> {
        readings
            .iter()
            .find(|r| r.spec.object == object)
            .map(|r| r.value.clone())
    }

    #[test]
    fn a_normal_machine_publishes_the_core_readings() {
        let r = readings(&machine(Some(48.0), Some(77.0)), 2);
        // 42.25 rounded to one decimal, half away from zero.
        assert_eq!(value_of(&r, "cpu"), Some(json!(42.3)));
        assert_eq!(value_of(&r, "cpu_temperature"), Some(json!(48.0)));
        assert_eq!(value_of(&r, "memory"), Some(json!(50.0)));
        assert_eq!(value_of(&r, "processes"), Some(json!(214)));
        assert_eq!(value_of(&r, "alerts"), Some(json!(2)));
        assert_eq!(value_of(&r, "problem"), Some(Value::from("ON")));
        assert_eq!(value_of(&r, "battery"), Some(json!(77.0)));
    }

    #[test]
    fn an_absent_temperature_is_omitted_not_zero() {
        let r = readings(&machine(None, None), 0);
        assert_eq!(value_of(&r, "cpu_temperature"), None);
        assert_eq!(value_of(&r, "battery"), None);
        assert_eq!(value_of(&r, "problem"), Some(Value::from("OFF")));
    }

    #[test]
    fn a_machine_that_cannot_count_processes_publishes_no_process_entity() {
        // The sandboxed Mac build never enumerates processes. It must leave
        // the entity out rather than feed Home Assistant a fabricated `0`.
        let mut mac = machine(None, None);
        mac.cpu.process_count = None;
        let r = readings(&mac, 0);
        assert_eq!(value_of(&r, "processes"), None);
    }

    #[test]
    fn the_problem_sensor_is_on_exactly_when_an_alert_is() {
        assert_eq!(
            value_of(&readings(&machine(None, None), 0), "problem"),
            Some(Value::from("OFF"))
        );
        assert_eq!(
            value_of(&readings(&machine(None, None), 1), "problem"),
            Some(Value::from("ON"))
        );
    }

    #[test]
    fn every_spec_published_for_a_full_machine_has_a_reading_and_vice_versa() {
        // The two directions matter: an announced entity that is never fed is
        // a permanent `unknown` on the dashboard, and a published key with no
        // announcement is invisible.
        let full = readings(&machine(Some(40.0), Some(50.0)), 1);
        for spec in all_specs() {
            assert!(
                full.iter().any(|r| r.spec.object == spec.object),
                "{} announced but the full machine never publishes it",
                spec.object
            );
        }
        for reading in &full {
            assert!(
                all_specs().iter().any(|s| s.object == reading.spec.object),
                "{} published but not announced",
                reading.spec.object
            );
        }
    }
}
