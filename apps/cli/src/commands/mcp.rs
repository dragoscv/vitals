//! `vitals mcp` — this machine, served to an AI agent over the Model Context
//! Protocol on standard input and output.
//!
//! The source is the one every other subcommand uses: attach to the running
//! desktop app if there is one, otherwise sample the machine here. What the
//! agent gets is therefore the same numbers `vitals ps` prints, not a second
//! opinion about the machine.
//!
//! Read-only, and not merely by omission: [`vitals_mcp::Tool`] has no tool
//! that changes anything, and the crate fails its own test if one is ever
//! added. An agent launched on this machine can watch it and cannot touch it.

use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use vitals_core::MachineSample;
use vitals_core::alerts::Alert;
use vitals_core::diagnosis::{Diagnosis, diagnose};
use vitals_core::provider::HostInfo;
use vitals_core::remote::{SensorLine, Summary};
use vitals_mcp::{McpHost, Reading, serve};

use crate::commands::serve::machine_host;
use crate::fold::View;
use crate::source::Source;

/// What the MCP host needs from a machine reading.
///
/// [`Source`] is the real implementation — attach, or sample — and the tests
/// below drive a fixture through the same code an agent reaches. A trait
/// rather than a bundle of closures because the ways a source fails are part
/// of the answer: an agent is told *why* there is no sensor list, and that
/// sentence has to survive the whole path to be worth writing.
trait MachineSource {
    fn snapshot(&mut self) -> Result<View>;
    fn host(&mut self) -> Result<Option<HostInfo>>;
    fn alerts(&mut self) -> Result<Vec<Alert>>;
    fn sensors(&self) -> Result<Vec<SensorLine>>;
    fn history(&self, seconds: u32) -> Result<Vec<MachineSample>>;
}

impl MachineSource for Source {
    // Named through `Source::` on purpose: without it these would be calls to
    // the trait method being defined, and recurse.
    fn snapshot(&mut self) -> Result<View> {
        Source::snapshot(self)
    }

    fn host(&mut self) -> Result<Option<HostInfo>> {
        Source::host(self)
    }

    fn alerts(&mut self) -> Result<Vec<Alert>> {
        Source::alerts(self)
    }

    fn sensors(&self) -> Result<Vec<SensorLine>> {
        Source::sensors(self)
    }

    fn history(&self, seconds: u32) -> Result<Vec<MachineSample>> {
        Source::history(self, seconds)
    }
}

/// One source, one name, one caller at a time.
///
/// The mutex is not for concurrency — the stdio loop is single-threaded — but
/// because `Source::snapshot` needs `&mut` and the MCP methods are given
/// `&self`. It also means a second agent pointed at the same process could
/// not interleave two half-finished readings.
struct CliHost<S: MachineSource> {
    source: Mutex<S>,
    machine: String,
}

impl<S: MachineSource> CliHost<S> {
    fn new(source: S, machine: String) -> Self {
        Self {
            source: Mutex::new(source),
            machine,
        }
    }

    fn source(&self) -> MutexGuard<'_, S> {
        self.source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The machine now, or the reason it could not be read.
    ///
    /// Every failure becomes a sentence rather than an empty reading: an
    /// agent that is told "the app is not running" can act on that, where an
    /// empty process list would be read as an idle machine.
    fn view(&self) -> Reading<View> {
        self.source()
            .snapshot()
            .map_err(|error| format!("{error:#}"))
    }
}

impl<S: MachineSource> McpHost for CliHost<S> {
    fn machine(&self) -> &str {
        &self.machine
    }

    fn host_info(&self) -> Option<HostInfo> {
        self.source().host().ok().flatten()
    }

    fn summary(&self, top: usize) -> Reading<Option<Summary>> {
        let view = self.view()?;
        if !view.is_primed() {
            return Ok(None);
        }
        Ok(Some(Summary::from_parts(
            view.seq,
            view.timestamp_ms,
            view.system.clone(),
            &view.processes(),
            top,
        )))
    }

    fn sensors(&self) -> Reading<Vec<SensorLine>> {
        self.source()
            .sensors()
            .map_err(|error| format!("{error:#}"))
    }

    fn alerts(&self) -> Reading<Vec<Alert>> {
        self.source().alerts().map_err(|error| format!("{error:#}"))
    }

    fn history(&self, seconds: u32) -> Reading<Vec<MachineSample>> {
        self.source()
            .history(seconds)
            .map_err(|error| format!("{error:#}"))
    }

    fn diagnosis(&self) -> Reading<Option<Diagnosis>> {
        let view = self.view()?;
        if !view.is_primed() {
            return Ok(None);
        }
        let alerts = self
            .source()
            .alerts()
            .map_err(|error| format!("{error:#}"))?;
        let processes = view.processes();
        Ok(Some(diagnose(&alerts, &view.system, &processes)))
    }
}

/// Serves the agent on this process's standard input and output until either
/// ends.
///
/// Nothing but protocol goes to standard output — an agent parses every line
/// it reads there — so the "sampling directly" note `main` prints goes to
/// standard error, where it belongs.
///
/// # Errors
/// A failure reading standard input or writing standard output.
pub fn run(source: Source) -> Result<()> {
    let host = CliHost::new(source, machine_host());
    serve(host).context("serving MCP on standard input and output")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    use vitals_core::alerts::{AlertKind, AlertRoute, Severity};
    use vitals_core::fixtures;

    use super::*;

    /// A machine the test controls: the readings an agent asks for, and the
    /// reasons a real source gives when it cannot take them.
    struct FakeSource {
        view: View,
        alerts: Reading<Vec<Alert>>,
        sensors: Reading<Vec<SensorLine>>,
        history: Reading<Vec<MachineSample>>,
        host: Option<HostInfo>,
        /// The span the last `history` call asked for, so a test can prove
        /// the agent's argument travels instead of being dropped.
        last_span: AtomicU32,
    }

    impl FakeSource {
        /// Three processes, one of them hot, with the alert that explains it.
        fn running() -> Self {
            let mut view = View::default();
            view.apply(&fixtures::keyframe(
                9,
                vec![
                    fixtures::process("chrome.exe", 4242, 71.0),
                    fixtures::process("explorer.exe", 100, 4.0),
                    fixtures::process("idle", 0, 0.0),
                ],
            ));
            Self {
                view,
                alerts: Ok(vec![cpu_alert()]),
                sensors: Ok(vec![SensorLine {
                    key: "cpu.package".to_owned(),
                    label: "CPU package".to_owned(),
                    value: 81.0,
                    unit: "temperature".to_owned(),
                    source: "kernelDriver".to_owned(),
                    quality: "measured".to_owned(),
                }]),
                history: Ok(vec![MachineSample::from_metrics(
                    1_700_000_000,
                    &fixtures::system(),
                )]),
                host: Some(host_info()),
                last_span: AtomicU32::new(0),
            }
        }

        /// A source that has produced no frame at all.
        fn before_the_first_frame() -> Self {
            Self {
                view: View::default(),
                ..Self::running()
            }
        }
    }

    impl MachineSource for FakeSource {
        fn snapshot(&mut self) -> Result<View> {
            Ok(self.view.clone())
        }

        fn host(&mut self) -> Result<Option<HostInfo>> {
            Ok(self.host.clone())
        }

        fn alerts(&mut self) -> Result<Vec<Alert>> {
            self.alerts.clone().map_err(anyhow::Error::msg)
        }

        fn sensors(&self) -> Result<Vec<SensorLine>> {
            self.sensors.clone().map_err(anyhow::Error::msg)
        }

        fn history(&self, seconds: u32) -> Result<Vec<MachineSample>> {
            self.last_span.store(seconds, Ordering::SeqCst);
            self.history.clone().map_err(anyhow::Error::msg)
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

    fn host_info() -> HostInfo {
        HostInfo {
            hostname: "test-pc".to_owned(),
            os_name: "Windows 11".to_owned(),
            os_version: "10.0".to_owned(),
            kernel_version: "10.0".to_owned(),
            architecture: "x86_64".to_owned(),
            cpu_model: "Test".to_owned(),
            cpu_vendor: "Test".to_owned(),
            physical_cores: 4,
            logical_cores: 8,
            core_topology: None,
            total_memory: vitals_core::units::Bytes(8 * 1024 * 1024 * 1024),
            boot_time_ms: 0,
            is_virtual_machine: false,
            motherboard: None,
            bios_version: None,
        }
    }

    fn host_of(source: FakeSource) -> CliHost<FakeSource> {
        CliHost::new(source, "test-pc".to_owned())
    }

    #[test]
    fn a_source_that_has_not_sampled_yet_reports_nothing_rather_than_an_idle_machine() {
        let host = host_of(FakeSource::before_the_first_frame());
        assert!(
            host.summary(5).unwrap().is_none(),
            "an unprimed view is 'no reading yet', not a machine with no processes"
        );
        assert!(host.diagnosis().unwrap().is_none());
    }

    #[test]
    fn the_summary_reports_the_frame_the_source_gave_it_with_the_span_the_agent_asked_for() {
        let host = host_of(FakeSource::running());
        let summary = host.summary(2).unwrap().expect("a sampled machine");
        assert_eq!(
            summary.seq, 9,
            "the frame's own sequence, not a fabricated 0"
        );
        assert_eq!(
            summary.process_count, 3,
            "the count covers the whole machine"
        );
        assert_eq!(summary.top.len(), 2, "and the list is what was asked for");
        assert_eq!(summary.top[0].name, "chrome.exe", "busiest first");
    }

    #[test]
    fn a_reading_the_source_could_not_take_reaches_the_agent_as_its_reason() {
        let source = FakeSource {
            sensors: Err("no sensor service is running".to_owned()),
            ..FakeSource::running()
        };
        let why = host_of(source).sensors().unwrap_err();
        assert_eq!(why, "no sensor service is running");
    }

    #[test]
    fn an_empty_reading_is_a_value_rather_than_a_failure() {
        let source = FakeSource {
            sensors: Ok(Vec::new()),
            alerts: Ok(Vec::new()),
            history: Ok(Vec::new()),
            ..FakeSource::running()
        };
        let host = host_of(source);
        assert!(host.sensors().unwrap().is_empty());
        assert!(host.alerts().unwrap().is_empty());
        assert!(host.history(60).unwrap().is_empty());
    }

    #[test]
    fn the_history_tool_passes_the_span_the_agent_asked_for_to_the_source() {
        let host = host_of(FakeSource::running());
        let samples = host.history(7_200).unwrap();
        assert_eq!(samples.len(), 1);
        let source = host.source();
        assert_eq!(
            source.last_span.load(Ordering::SeqCst),
            7_200,
            "the argument must travel, not be replaced by a default"
        );
    }

    #[test]
    fn the_diagnosis_is_built_from_the_machine_the_source_actually_saw() {
        let host = host_of(FakeSource::running());
        let diagnosis = host.diagnosis().unwrap().expect("a sampled machine");
        let verdict = diagnosis.verdict.expect("a cpu alert is a verdict");
        assert_eq!(verdict.subsystem, vitals_core::diagnosis::Subsystem::Cpu);
        assert_eq!(
            verdict.contributors[0].name, "chrome.exe",
            "the contributor comes from the view, not from a fixture of its own"
        );
    }

    #[test]
    fn a_source_with_no_host_facts_reports_none_and_never_an_empty_one() {
        let source = FakeSource {
            host: None,
            ..FakeSource::running()
        };
        assert!(host_of(source).host_info().is_none());
    }

    #[test]
    fn the_machine_the_agent_sees_is_the_one_the_cli_was_started_on() {
        let host = CliHost::new(FakeSource::running(), "test-pc".to_owned());
        assert_eq!(host.machine(), "test-pc");
    }
}
