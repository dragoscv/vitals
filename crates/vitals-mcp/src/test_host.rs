//! A plausible machine for this crate's tests.
//!
//! Built from `vitals-core`'s `fixtures` so a test never hand-rolls a
//! thirty-field frame: the four quiet copies of "a plausible machine" that
//! turn a field addition into four broken files are exactly what the fixtures
//! feature exists to prevent.
//!
//! Three hosts, because the interesting behaviour is in the differences:
//!
//! - [`TestHost::new`] — a machine with three processes, two sensors, one
//!   alert and a CPU that one process accounts for.
//! - [`TestHost::not_sampling`] — a sampler that has produced no frame yet.
//! - [`TestHost::empty`] — a machine that read fine and has nothing to
//!   report: no sensors, no alerts, no history.

use std::collections::BTreeMap;

use vitals_core::MachineSample;
use vitals_core::alerts::{Alert, AlertKind, AlertRoute, Severity};
use vitals_core::diagnosis::{Diagnosis, diagnose};
use vitals_core::fixtures;
use vitals_core::metrics::SystemMetrics;
use vitals_core::process::Process;
use vitals_core::provider::HostInfo;
use vitals_core::remote::{SensorLine, Summary};
use vitals_core::units::Bytes;

use crate::host::{McpHost, Reading};

/// The machine every test in this crate is about.
pub const MACHINE: &str = "test-machine";

/// Why a host in these tests refuses to answer about sensors.
const NO_SENSOR_SERVICE: &str = "no sensor service is running on this machine";

pub struct TestHost {
    /// `None` stands for a sampler that has not produced a frame yet.
    machine_state: Option<State>,
    sensors: Reading<Vec<SensorLine>>,
}

struct State {
    system: SystemMetrics,
    processes: Vec<Process>,
    alerts: Vec<Alert>,
    history: Vec<MachineSample>,
}

impl TestHost {
    /// A machine with something to report.
    #[must_use]
    pub fn new() -> Self {
        Self {
            machine_state: Some(State {
                system: fixtures::system(),
                processes: vec![
                    fixtures::process("chrome.exe", 4242, 71.0),
                    fixtures::process("explorer.exe", 100, 4.0),
                    fixtures::process("idle", 0, 0.0),
                ],
                alerts: vec![cpu_alert()],
                history: vec![
                    MachineSample::from_metrics(1_700_000_000, &fixtures::system()),
                    MachineSample::from_metrics(1_700_000_060, &fixtures::system()),
                ],
            }),
            sensors: Ok(vec![
                SensorLine {
                    key: "cpu.package".to_owned(),
                    label: "CPU package".to_owned(),
                    value: 81.0,
                    unit: "temperature".to_owned(),
                    source: "kernelDriver".to_owned(),
                    quality: "measured".to_owned(),
                },
                SensorLine {
                    key: "fan.cpu".to_owned(),
                    label: "CPU fan".to_owned(),
                    value: 1_240.0,
                    unit: "fanSpeed".to_owned(),
                    source: "kernelDriver".to_owned(),
                    quality: "measured".to_owned(),
                },
            ]),
        }
    }

    /// A sampler that has not taken its first reading.
    #[must_use]
    pub fn not_sampling() -> Self {
        let mut host = Self::new();
        host.machine_state = None;
        host
    }

    /// A machine that read fine and has nothing measurable or raised.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            machine_state: Some(State {
                system: fixtures::system(),
                processes: Vec::new(),
                alerts: Vec::new(),
                history: Vec::new(),
            }),
            sensors: Ok(Vec::new()),
        }
    }

    /// A host that cannot ask about sensors at all — the `Err` arm of a
    /// reading, which is not the same fact as "none measurable".
    #[must_use]
    pub fn without_sensors() -> Self {
        let mut host = Self::new();
        host.sensors = Err(NO_SENSOR_SERVICE.to_owned());
        host
    }

    fn state(&self) -> Reading<&State> {
        self.machine_state
            .as_ref()
            .ok_or_else(|| "no sample yet".to_owned())
    }
}

fn cpu_alert() -> Alert {
    Alert {
        kind: AlertKind::CpuSustained,
        severity: Severity::Warning,
        subject: "cpu".to_owned(),
        title: "cpuSustained".to_owned(),
        cause: "highLoad".to_owned(),
        values: BTreeMap::new(),
        route: Some(AlertRoute::Performance),
        since_sample: 30,
    }
}

/// Static facts, written out: `vitals-core`'s fixtures cover the readings but
/// not this startup-only struct.
fn host_info() -> HostInfo {
    HostInfo {
        hostname: MACHINE.to_owned(),
        os_name: "Windows 11 Pro".to_owned(),
        os_version: "10.0.26200".to_owned(),
        kernel_version: "10.0.26200".to_owned(),
        architecture: "x86_64".to_owned(),
        cpu_model: "AMD Ryzen 9 5950X 16-Core Processor".to_owned(),
        cpu_vendor: "AuthenticAMD".to_owned(),
        physical_cores: 16,
        logical_cores: 32,
        core_topology: None,
        total_memory: Bytes(68_719_476_736),
        boot_time_ms: 1_700_000_000_000,
        is_virtual_machine: false,
        motherboard: Some("X570 AORUS MASTER".to_owned()),
        bios_version: Some("F36".to_owned()),
    }
}

impl McpHost for TestHost {
    fn machine(&self) -> &str {
        MACHINE
    }

    fn host_info(&self) -> Option<HostInfo> {
        Some(host_info())
    }

    fn summary(&self, top: usize) -> Reading<Option<Summary>> {
        // `Ok(None)` is "not sampling yet"; a host that could not read at all
        // would return `Err` here instead.
        if self.machine_state.is_none() {
            return Ok(None);
        }
        let state = self.state()?;
        Ok(Some(Summary::from_parts(
            1,
            1_700_000_000_000,
            state.system.clone(),
            &state.processes,
            top,
        )))
    }

    fn sensors(&self) -> Reading<Vec<SensorLine>> {
        self.sensors.clone()
    }

    fn alerts(&self) -> Reading<Vec<Alert>> {
        Ok(self.state()?.alerts.clone())
    }

    fn history(&self, _seconds: u32) -> Reading<Vec<MachineSample>> {
        // The fixture keeps two samples; how far back a caller asked is
        // echoed back by the tool rather than filtered here.
        Ok(self.state()?.history.clone())
    }

    fn diagnosis(&self) -> Reading<Option<Diagnosis>> {
        if self.machine_state.is_none() {
            return Ok(None);
        }
        let state = self.state()?;
        Ok(Some(diagnose(
            &state.alerts,
            &state.system,
            &state.processes,
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use vitals_core::ids::Pid;

    #[test]
    fn the_fixture_host_answers_with_the_three_processes_it_declares() {
        let host = TestHost::new();
        let summary = host.summary(10).unwrap().unwrap();
        assert_eq!(summary.process_count, 3);
        assert_eq!(summary.top[0].name, "chrome.exe", "the busiest is first");
        assert_eq!(summary.top[0].key.pid, Pid(4242));
    }

    #[test]
    fn the_fixture_host_has_one_alert_that_a_cpu_verdict_can_name_a_contributor_for() {
        let host = TestHost::new();
        let diagnosis = host.diagnosis().unwrap().unwrap();
        let verdict = diagnosis.verdict.expect("a cpu alert is a verdict");
        assert_eq!(verdict.subsystem, vitals_core::diagnosis::Subsystem::Cpu);
        assert!(
            !verdict.diffuse,
            "one process at 71 % is not a diffuse load"
        );
        assert_eq!(verdict.contributors[0].name, "chrome.exe");
    }

    #[test]
    fn the_not_sampling_host_says_so_instead_of_describing_an_empty_machine() {
        let host = TestHost::not_sampling();
        assert!(host.summary(5).unwrap().is_none());
        assert!(host.diagnosis().unwrap().is_none());
        assert!(
            host.alerts().is_err(),
            "there is no frame to read alerts from"
        );
    }

    #[test]
    fn the_empty_host_reads_fine_and_reports_nothing() {
        let host = TestHost::empty();
        let summary = host.summary(5).unwrap().unwrap();
        assert_eq!(summary.process_count, 0);
        assert!(host.sensors().unwrap().is_empty());
        assert!(host.alerts().unwrap().is_empty());
        assert!(host.history(3_600).unwrap().is_empty());
        assert!(host.diagnosis().unwrap().unwrap().verdict.is_none());
    }

    #[test]
    fn the_host_without_sensors_fails_the_call_rather_than_answering_none() {
        let why = TestHost::without_sensors().sensors().unwrap_err();
        assert_eq!(why, NO_SENSOR_SERVICE);
    }

    #[test]
    fn a_fixture_memory_total_is_a_real_number_so_a_percentage_is_meaningful() {
        // A zero total would make every memory percentage a division by zero
        // that reads as 0 %, which is the defect this crate's rule is about.
        let host = TestHost::new();
        let summary = host.summary(1).unwrap().unwrap();
        assert!(summary.system.memory.total > Bytes(0));
    }
}
