//! Properties of the pure model that every consumer of a frame relies on.
//!
//! The example tests beside each module pin one case each. These state the
//! guarantee for every value, because the bugs that corrupt what a user sees
//! live in the values nobody thought to write down: a measured zero, a
//! `start_time` above 2^53, an alert whose values were inserted in a
//! different order on a different run.
//!
//! What is deliberately *not* here: folding a keyframe and its deltas into
//! the current process set. `vitals-core` defines the frame but not the fold
//! — that lives in `vitals-server` (`state.rs`), `vitals-ipc` (`attach.rs`),
//! the CLI (`fold.rs`) and `@vitals/protocol` (`guards.ts`). The TypeScript
//! fold is covered by `apps/desktop/src/mobile/lib/frames.property.test.ts`.

// Integration tests are not covered by clippy's `allow-expect-in-tests`,
// which only recognises `#[cfg(test)]` modules.
#![allow(clippy::expect_used, clippy::unwrap_used)]
// The fixtures sit behind a feature so they can never reach a release
// binary. `cargo test --workspace` enables it through the dependents'
// dev-dependencies; alone, run with `--features fixtures`.
#![cfg(feature = "fixtures")]

use std::collections::BTreeMap;

use proptest::prelude::*;
use serde_json::Value;
use vitals_core::alerts::{Alert, AlertKind, AlertValue, Severity};
use vitals_core::fixtures;
use vitals_core::ids::{Pid, ProcessKey};
use vitals_core::metrics::SystemMetrics;
use vitals_core::process::Process;
use vitals_core::sample::{Frame, FramePayload, FrameSeq};
use vitals_core::units::{Bytes, BytesPerSec, Celsius, Hertz, Percent, Watts};

/// Enough to find the edge cases these properties are about while keeping
/// the whole file under a second or two.
const CASES: u32 = 256;

/// Roughly the year 2500 as a Windows `FILETIME`: every start time the
/// sampler will ever see, with room to spare.
const FILETIME_CEILING: u64 = 1 << 58;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// A `u64` reading that lands on zero often. A measured zero is the value
/// most easily confused with "unmeasured", so it must not be left to chance.
fn reading() -> impl Strategy<Value = u64> {
    prop_oneof![Just(0_u64), 1_u64..=1_000_000, any::<u64>()]
}

fn count() -> impl Strategy<Value = u32> {
    prop_oneof![Just(0_u32), any::<u32>()]
}

fn float_reading(max: f32) -> impl Strategy<Value = f32> {
    prop_oneof![Just(0.0_f32), 0.0_f32..=max]
}

fn arb_mac() -> impl Strategy<Value = String> {
    any::<[u8; 6]>().prop_map(|bytes| {
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join("-")
    })
}

/// A value that must reach the wire exactly as sampled.
#[derive(Debug, Clone)]
enum Measured {
    Int(u64),
    Float(f32),
    Text(String),
}

/// The randomised part of a machine; everything else comes from the fixture.
#[derive(Debug, Clone)]
struct Readings {
    cpu_total: f32,
    temperature: Option<f32>,
    effective_clock: Option<u64>,
    handle_count: Option<u32>,
    context_switches: Option<u64>,
    memory_used: u64,
    swap_used: Option<u64>,
    page_faults: Option<u64>,
    power_draw: Option<f32>,
    response_ms: Option<f32>,
    errors_per_sec: Option<u64>,
    link_speed: Option<u64>,
    mac: Option<String>,
}

impl Readings {
    fn system(&self) -> SystemMetrics {
        let mut s = fixtures::system();
        s.cpu.total = Percent(self.cpu_total);
        s.cpu.temperature = self.temperature.map(Celsius);
        s.cpu.effective_clock = self.effective_clock.map(Hertz);
        s.cpu.handle_count = self.handle_count;
        s.cpu.context_switches = self.context_switches;
        s.memory.used = Bytes(self.memory_used);
        s.memory.swap_used = self.swap_used.map(Bytes);
        s.memory.page_faults_per_sec = self.page_faults;
        s.power_draw = self.power_draw.map(Watts);
        for disk in &mut s.disks {
            disk.response_ms = self.response_ms;
        }
        for nic in &mut s.networks {
            nic.errors_per_sec = self.errors_per_sec;
            nic.link_speed = self.link_speed;
            nic.mac.clone_from(&self.mac);
        }
        s
    }

    /// Every optional field set above, by its JSON pointer on the wire.
    fn optional_fields(&self) -> Vec<(&'static str, Option<Measured>)> {
        vec![
            ("/cpu/temperature", self.temperature.map(Measured::Float)),
            (
                "/cpu/effectiveClock",
                self.effective_clock.map(Measured::Int),
            ),
            (
                "/cpu/handleCount",
                self.handle_count.map(|v| Measured::Int(u64::from(v))),
            ),
            (
                "/cpu/contextSwitches",
                self.context_switches.map(Measured::Int),
            ),
            ("/memory/swapUsed", self.swap_used.map(Measured::Int)),
            (
                "/memory/pageFaultsPerSec",
                self.page_faults.map(Measured::Int),
            ),
            ("/powerDraw", self.power_draw.map(Measured::Float)),
            ("/disks/0/responseMs", self.response_ms.map(Measured::Float)),
            (
                "/networks/0/errorsPerSec",
                self.errors_per_sec.map(Measured::Int),
            ),
            ("/networks/0/linkSpeed", self.link_speed.map(Measured::Int)),
            ("/networks/0/mac", self.mac.clone().map(Measured::Text)),
        ]
    }
}

fn arb_readings() -> impl Strategy<Value = Readings> {
    (
        (
            0.0_f32..=100.0,
            prop::option::of(float_reading(120.0)),
            prop::option::of(reading()),
            prop::option::of(count()),
            prop::option::of(reading()),
            reading(),
            prop::option::of(reading()),
        ),
        (
            prop::option::of(reading()),
            prop::option::of(float_reading(500.0)),
            prop::option::of(float_reading(250.0)),
            prop::option::of(reading()),
            prop::option::of(reading()),
            prop::option::of(arb_mac()),
        ),
    )
        .prop_map(
            |(
                (
                    cpu_total,
                    temperature,
                    effective_clock,
                    handle_count,
                    context_switches,
                    memory_used,
                    swap_used,
                ),
                (page_faults, power_draw, response_ms, errors_per_sec, link_speed, mac),
            )| Readings {
                cpu_total,
                temperature,
                effective_clock,
                handle_count,
                context_switches,
                memory_used,
                swap_used,
                page_faults,
                power_draw,
                response_ms,
                errors_per_sec,
                link_speed,
                mac,
            },
        )
}

/// The randomised part of one process row.
#[derive(Debug, Clone)]
struct ProcessReadings {
    pid: u32,
    start_time: u64,
    cpu: f32,
    parent: Option<u32>,
    net_rx: Option<u64>,
    net_tx: Option<u64>,
    gpu: Option<f32>,
    gpu_memory: Option<u64>,
    handle_count: Option<u32>,
    user: Option<String>,
}

impl ProcessReadings {
    fn process(&self) -> Process {
        let mut p = fixtures::process("app.exe", self.pid, self.cpu);
        p.key = ProcessKey::new(Pid(self.pid), self.start_time);
        p.parent = self.parent.map(Pid);
        p.net_rx = self.net_rx.map(BytesPerSec);
        p.net_tx = self.net_tx.map(BytesPerSec);
        p.gpu = self.gpu.map(Percent);
        p.gpu_memory = self.gpu_memory.map(Bytes);
        p.handle_count = self.handle_count;
        p.user.clone_from(&self.user);
        p
    }

    fn optional_fields(&self) -> Vec<(&'static str, Option<Measured>)> {
        vec![
            ("/parent", self.parent.map(|v| Measured::Int(u64::from(v)))),
            ("/netRx", self.net_rx.map(Measured::Int)),
            ("/netTx", self.net_tx.map(Measured::Int)),
            ("/gpu", self.gpu.map(Measured::Float)),
            ("/gpuMemory", self.gpu_memory.map(Measured::Int)),
            (
                "/handleCount",
                self.handle_count.map(|v| Measured::Int(u64::from(v))),
            ),
            ("/user", self.user.clone().map(Measured::Text)),
        ]
    }
}

fn arb_process() -> impl Strategy<Value = ProcessReadings> {
    (
        (
            any::<u32>(),
            0_u64..FILETIME_CEILING,
            0.0_f32..=100.0,
            prop::option::of(any::<u32>()),
            prop::option::of(reading()),
        ),
        (
            prop::option::of(reading()),
            prop::option::of(float_reading(100.0)),
            prop::option::of(reading()),
            prop::option::of(count()),
            prop::option::of((0_u16..1000).prop_map(|n| format!("VITALS\\user{n}"))),
        ),
    )
        .prop_map(
            |(
                (pid, start_time, cpu, parent, net_rx),
                (net_tx, gpu, gpu_memory, handle_count, user),
            )| {
                ProcessReadings {
                    pid,
                    start_time,
                    cpu,
                    parent,
                    net_rx,
                    net_tx,
                    gpu,
                    gpu_memory,
                    handle_count,
                    user,
                }
            },
        )
}

/// A keyframe or a delta, with every numeric field and optional reading
/// randomised.
fn arb_frame() -> impl Strategy<Value = Frame> {
    (
        any::<u64>(),
        any::<u64>(),
        any::<u32>(),
        arb_readings(),
        prop::collection::vec(arb_process(), 0..6),
        prop::option::of(prop::collection::vec(any::<u32>(), 0..6)),
    )
        .prop_map(|(seq, timestamp_ms, elapsed_ms, readings, rows, exited)| {
            let system = readings.system();
            let processes: Vec<Process> = rows.iter().map(ProcessReadings::process).collect();
            let payload = match exited {
                None => FramePayload::Keyframe { system, processes },
                Some(exited) => FramePayload::Delta {
                    system,
                    changed: processes,
                    exited: exited.into_iter().map(Pid).collect(),
                },
            };
            Frame {
                seq: FrameSeq(seq),
                timestamp_ms,
                elapsed_ms,
                payload,
            }
        })
}

/// Alert values with unique keys, in a random insertion order.
fn arb_alert_values() -> impl Strategy<Value = Vec<(String, AlertValue)>> {
    prop::collection::btree_map(
        0_u16..500,
        prop_oneof![
            (-1.0e9_f64..1.0e9).prop_map(AlertValue::Number),
            (0_u32..1000).prop_map(|n| AlertValue::Text(format!("v{n}"))),
        ],
        0..12,
    )
    .prop_map(|values| {
        values
            .into_iter()
            .map(|(k, v)| (format!("k{k}"), v))
            .collect::<Vec<_>>()
    })
    .prop_shuffle()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
}

/// `None` must be null or absent; `Some` must be the exact value — including
/// a zero, which must never be dropped to look unmeasured.
fn assert_reported_as_sampled(
    json: &Value,
    path: &str,
    expected: Option<&Measured>,
) -> Result<(), TestCaseError> {
    let found = json.pointer(path);
    match expected {
        None => {
            prop_assert!(
                found.is_none_or(Value::is_null),
                "{} was unmeasured but reached the wire as {:?}",
                path,
                found
            );
        }
        Some(Measured::Int(v)) => {
            prop_assert_eq!(found.and_then(Value::as_u64), Some(*v), "{}", path);
        }
        Some(Measured::Float(v)) => {
            // Through the text form, so the comparison is the f32 the
            // sampler produced rather than its widened f64.
            let parsed = found
                .and_then(Value::as_number)
                .and_then(|n| n.to_string().parse::<f32>().ok());
            prop_assert_eq!(parsed.map(f32::to_bits), Some(v.to_bits()), "{}", path);
        }
        Some(Measured::Text(v)) => {
            prop_assert_eq!(found.and_then(Value::as_str), Some(v.as_str()), "{}", path);
        }
    }
    Ok(())
}

fn frame_with_elapsed(elapsed_ms: u32) -> Frame {
    Frame {
        seq: FrameSeq(1),
        timestamp_ms: 0,
        elapsed_ms,
        payload: FramePayload::Delta {
            system: SystemMetrics::default(),
            changed: vec![],
            exited: vec![],
        },
    }
}

fn alert_with(values: BTreeMap<String, AlertValue>) -> Alert {
    Alert {
        kind: AlertKind::DiskSpace,
        severity: Severity::Warning,
        subject: "C:".to_owned(),
        title: "alert.diskSpace.title".to_owned(),
        cause: "alert.diskSpace.cause".to_owned(),
        values,
        route: None,
        since_sample: 0,
    }
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    // --- The wire -----------------------------------------------------------

    #[test]
    fn any_frame_survives_a_json_round_trip_byte_for_byte(frame in arb_frame()) {
        let first = serde_json::to_string(&frame).unwrap();
        let back: Frame = serde_json::from_str(&first).unwrap();
        prop_assert_eq!(serde_json::to_string(&back).unwrap(), first);
    }

    #[test]
    fn an_unmeasured_machine_reading_is_null_on_the_wire_and_a_measured_zero_is_zero(
        readings in arb_readings(),
    ) {
        let json = to_json(&readings.system());
        for (path, expected) in readings.optional_fields() {
            assert_reported_as_sampled(&json, path, expected.as_ref())?;
        }
    }

    #[test]
    fn an_unmeasured_process_reading_is_null_on_the_wire_and_a_measured_zero_is_zero(
        row in arb_process(),
    ) {
        let json = to_json(&row.process());
        for (path, expected) in row.optional_fields() {
            assert_reported_as_sampled(&json, path, expected.as_ref())?;
        }
    }

    #[test]
    fn identifiers_are_hidden_from_the_closure_and_restored_exactly_afterwards(
        mut frame in arb_frame(),
    ) {
        let before = serde_json::to_string(&frame).unwrap();

        let hidden = frame.with_identifiers_removed(|f| {
            let (system, processes) = match &f.payload {
                FramePayload::Keyframe { system, processes } => (system, processes),
                FramePayload::Delta { system, changed, .. } => (system, changed),
            };
            processes.iter().all(|p| p.user.is_none())
                && system.networks.iter().all(|n| n.mac.is_none())
        });

        prop_assert!(hidden, "an owner or a MAC address reached the closure");
        prop_assert_eq!(serde_json::to_string(&frame).unwrap(), before);
    }

    // --- Rates --------------------------------------------------------------

    #[test]
    fn a_rate_never_claims_more_bytes_than_were_observed(
        delta in any::<u64>(),
        elapsed in 1_u32..,
    ) {
        let rate = frame_with_elapsed(elapsed).rate_per_sec(delta);
        prop_assert!(
            u128::from(rate) * u128::from(elapsed) <= u128::from(delta) * 1000,
            "{} B over {} ms reported as {} B/s",
            delta,
            elapsed,
            rate
        );
    }

    #[test]
    fn a_late_frame_never_reports_a_higher_rate_for_the_same_bytes(
        delta in any::<u64>(),
        a in 1_u32..,
        b in 1_u32..,
    ) {
        let (prompt, late) = (a.min(b), a.max(b));
        prop_assert!(
            frame_with_elapsed(late).rate_per_sec(delta)
                <= frame_with_elapsed(prompt).rate_per_sec(delta)
        );
    }

    #[test]
    fn a_zero_interval_reports_no_rate_rather_than_dividing_by_zero(delta in any::<u64>()) {
        prop_assert_eq!(frame_with_elapsed(0).rate_per_sec(delta), 0);
    }

    #[test]
    fn bytes_over_exactly_one_second_are_that_many_bytes_per_second(delta in any::<u64>()) {
        prop_assert_eq!(frame_with_elapsed(1000).rate_per_sec(delta), delta);
    }

    // --- Process identity ---------------------------------------------------

    #[test]
    fn a_normalised_start_time_survives_a_javascript_number_and_normalising_again(
        raw in 0_u64..FILETIME_CEILING,
    ) {
        let once = ProcessKey::normalise_start_time(raw);
        prop_assert_eq!((once as f64) as u64, once);
        prop_assert_eq!(ProcessKey::normalise_start_time(once), once);
    }

    #[test]
    fn normalising_a_start_time_moves_it_by_under_two_microseconds(
        raw in 0_u64..FILETIME_CEILING,
    ) {
        // 100 ns units: 20 of them is the 2 µs the field docs promise.
        let moved = ProcessKey::normalise_start_time(raw).abs_diff(raw);
        prop_assert!(moved < 20, "{} moved by {} × 100 ns", raw, moved);
    }

    #[test]
    fn normalising_never_reorders_two_processes_by_start_time(
        a in 0_u64..FILETIME_CEILING,
        b in 0_u64..FILETIME_CEILING,
    ) {
        let (earlier, later) = (a.min(b), a.max(b));
        prop_assert!(
            ProcessKey::normalise_start_time(earlier) <= ProcessKey::normalise_start_time(later)
        );
    }

    #[test]
    fn a_key_read_back_from_the_webview_matches_the_one_the_sampler_holds(
        pid in any::<u32>(),
        raw in 0_u64..FILETIME_CEILING,
    ) {
        let key = ProcessKey::new(Pid(pid), raw);
        // What the webview does: parse to a JS number, send it back.
        let js = to_json(&key)["startTime"].as_f64().unwrap();
        prop_assert_eq!(ProcessKey::new(Pid(pid), js as u64), key);
    }

    // --- Units --------------------------------------------------------------

    #[test]
    fn a_percentage_is_always_between_zero_and_one_hundred(v in -1.0e6_f32..1.0e6) {
        let p = Percent::new(v).get();
        prop_assert!((0.0..=100.0).contains(&p), "{} became {}", v, p);
    }

    #[test]
    fn clamping_a_percentage_twice_changes_nothing(v in -1.0e6_f32..1.0e6) {
        let once = Percent::new(v);
        prop_assert_eq!(Percent::new(once.get()).get().to_bits(), once.get().to_bits());
    }

    #[test]
    fn a_ratio_is_a_percentage_and_a_share_of_nothing_is_zero(
        part in any::<u64>(),
        whole in prop_oneof![Just(0_u64), any::<u64>()],
    ) {
        let p = Percent::ratio(part, whole);
        prop_assert!((0.0..=100.0).contains(&p.get()));
        if whole == 0 {
            prop_assert_eq!(p, Percent::ZERO);
        }
    }

    #[test]
    fn a_larger_share_never_reads_as_a_smaller_percentage(
        a in any::<u64>(),
        b in any::<u64>(),
        whole in 1_u64..,
    ) {
        let (less, more) = (a.min(b), a.max(b));
        prop_assert!(Percent::ratio(less, whole) <= Percent::ratio(more, whole));
    }

    #[test]
    fn binary_unit_conversions_are_exact_or_saturate_never_wrap(v in any::<u64>()) {
        // Stated through u128 rather than `saturating_mul`, so the oracle is
        // not the same expression as the code it checks.
        let expected = |factor: u64| u64::try_from(u128::from(v) * u128::from(factor)).unwrap_or(u64::MAX);
        prop_assert_eq!(Bytes::from_kib(v).get(), expected(Bytes::KIB));
        prop_assert_eq!(Bytes::from_mib(v).get(), expected(Bytes::MIB));
        prop_assert_eq!(Hertz::from_mhz(v).get(), expected(1_000_000));
    }

    #[test]
    fn a_unit_reaches_the_wire_as_its_bare_number(v in any::<u64>()) {
        let expected = v.to_string();
        prop_assert_eq!(serde_json::to_string(&Bytes(v)).unwrap(), expected.clone());
        prop_assert_eq!(serde_json::to_string(&BytesPerSec(v)).unwrap(), expected.clone());
        prop_assert_eq!(serde_json::to_string(&Hertz(v)).unwrap(), expected);
    }

    // --- Alerts -------------------------------------------------------------

    #[test]
    fn alert_values_serialise_in_key_order_whatever_order_they_were_inserted(
        values in arb_alert_values(),
    ) {
        let forwards: BTreeMap<_, _> = values.iter().cloned().collect();
        let backwards: BTreeMap<_, _> = values.iter().rev().cloned().collect();
        let json = serde_json::to_string(&alert_with(forwards)).unwrap();

        prop_assert_eq!(serde_json::to_string(&alert_with(backwards)).unwrap(), json.clone());

        let mut keys: Vec<&str> = values.iter().map(|(k, _)| k.as_str()).collect();
        keys.sort_unstable();
        let positions: Vec<usize> = keys
            .iter()
            .map(|k| json.find(&format!("\"{k}\":")).unwrap())
            .collect();
        prop_assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "keys out of order in {}",
            json
        );
    }

    #[test]
    fn a_numeric_looking_text_alert_value_never_comes_back_as_a_number(n in any::<u32>()) {
        // The enum is untagged: only the JSON type tells the variants apart.
        let text = AlertValue::Text(n.to_string());
        let back: AlertValue =
            serde_json::from_str(&serde_json::to_string(&text).unwrap()).unwrap();
        prop_assert_eq!(back, text);
    }

    #[test]
    fn a_numeric_alert_value_comes_back_as_a_number(v in -1.0e12_f64..1.0e12) {
        let back: AlertValue =
            serde_json::from_str(&serde_json::to_string(&AlertValue::Number(v)).unwrap()).unwrap();
        prop_assert!(matches!(back, AlertValue::Number(_)), "{:?}", back);
    }
}
