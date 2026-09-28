//! Writes the JSON the Android apps must decode (ADR-0033).
//!
//! The Kotlin models are hand-written, so nothing generates them from Rust.
//! This test serialises real values — every optional field populated, and a
//! second copy with every optional field `null` — into
//! `apps/android/core/src/test/resources/contract/`. The JVM test in `:core`
//! decodes each file with `ignoreUnknownKeys = false`, so a field renamed or
//! added on either side fails one of the two builds. `check-drift.ps1` fails
//! when the committed files differ from what this test writes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;

use vitals_core::MachineSample;
use vitals_core::alerts::{Alert, AlertKind, AlertRoute, AlertValue, Severity};
use vitals_core::fixtures;
use vitals_core::ids::{GpuId, Pid};
use vitals_core::metrics::{
    BatteryMetrics, DiskHealth, GpuEngine, GpuMetrics, GpuVendor, ThrottleReason,
};
use vitals_core::process::{IntegrityLevel, ProcessFlags, ProcessState, ProtectionLevel};
use vitals_core::remote::{SensorLine, Summary};
use vitals_core::sample::{Frame, FramePayload, FrameSeq};
use vitals_core::units::{Bytes, Celsius, Hertz, Percent, Watts};

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/android/core/src/test/resources/contract")
}

fn write(name: &str, value: &impl serde::Serialize) {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    let path = dir.join(name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(path, text).unwrap();
    }
}

/// A machine with every optional field measured.
fn full_system() -> vitals_core::metrics::SystemMetrics {
    let mut s = fixtures::system();
    s.cpu.effective_clock = Some(Hertz(5_200_000_000));
    s.cpu.max_clock = Some(Hertz(6_000_000_000));
    s.cpu.temperature = Some(Celsius(81.5));
    s.cpu.power = Some(Watts(169.2));
    s.cpu.throttled = Some(ThrottleReason::PowerLimit);
    s.cpu.handle_count = Some(120_000);
    s.cpu.context_switches = Some(90_000);
    s.cpu.interrupts = Some(40_000);
    s.memory.swap_used = Some(Bytes(1_073_741_824));
    s.memory.page_faults_per_sec = Some(1_200);
    s.memory.speed = Some(Hertz(5_200_000_000));
    s.memory.slots_used = Some(4);
    s.memory.slots_total = Some(4);
    s.memory.form_factor = Some("DIMM".into());
    let d = &mut s.disks[0];
    d.response_ms = Some(0.4);
    d.queue_depth = Some(1.5);
    d.temperature = Some(Celsius(40.0));
    d.health = Some(DiskHealth {
        life_remaining: Some(Percent(97.0)),
        power_on_hours: Some(4_000),
        total_written: Some(Bytes(50_000_000_000_000)),
        reallocated_sectors: Some(0),
        failing: false,
    });
    let n = &mut s.networks[0];
    n.ipv6 = Some("fe80::1".into());
    n.mac = Some("AA-BB-CC-DD-EE-FF".into());
    n.signal = Some(Percent(80.0));
    n.ssid = Some("home".into());
    n.errors_per_sec = Some(0);
    s.gpus = vec![GpuMetrics {
        id: GpuId(0),
        name: "NVIDIA GeForce RTX 3060 Ti".into(),
        vendor: GpuVendor::Nvidia,
        engines: vec![GpuEngine {
            name: "3D".into(),
            utilization: Percent(16.0),
        }],
        utilization: Some(Percent(16.0)),
        memory_used: Some(Bytes(2_000_000_000)),
        memory_total: Some(Bytes(8_000_000_000)),
        shared_memory_used: Some(Bytes(100_000_000)),
        core_clock: Some(Hertz(1_800_000_000)),
        memory_clock: Some(Hertz(7_000_000_000)),
        temperature: Some(Celsius(46.0)),
        hotspot_temperature: Some(Celsius(55.0)),
        power: Some(Watts(64.3)),
        power_limit: Some(Watts(200.0)),
        fan_percent: Some(Percent(80.0)),
        fan_rpm: Some(1500),
        throttled: Some(ThrottleReason::Thermal),
        driver_version: Some("581.29".into()),
    }];
    s.power_draw = Some(Watts(320.0));
    s.battery = Some(BatteryMetrics {
        charge: Percent(64.0),
        charging: true,
        time_remaining_secs: Some(3_600),
        power: Some(Watts(-12.0)),
        health: Some(Percent(91.0)),
        cycle_count: Some(210),
        temperature: Some(Celsius(31.0)),
    });
    s
}

fn full_process() -> vitals_core::process::Process {
    let mut p = fixtures::process("chrome.exe", 4242, 12.5);
    p.parent = Some(Pid(1000));
    p.state = ProcessState::NotResponding;
    p.flags = ProcessFlags::SIGNED | ProcessFlags::HAS_WINDOW;
    p.integrity = Some(IntegrityLevel::Medium);
    p.protection = ProtectionLevel::Light;
    p.net_rx = Some(vitals_core::units::BytesPerSec(1_000));
    p.net_tx = Some(vitals_core::units::BytesPerSec(500));
    p.gpu = Some(Percent(3.0));
    p.gpu_memory = Some(Bytes(300_000_000));
    p
}

#[test]
fn writes_the_android_contract_fixtures() {
    let sparse = fixtures::keyframe(1, vec![fixtures::process("idle.exe", 7, 0.0)]);
    write("keyframe_sparse.json", &sparse);

    let full = Frame {
        seq: FrameSeq(2),
        timestamp_ms: 1_700_000_001_000,
        elapsed_ms: 1_003,
        payload: FramePayload::Keyframe {
            system: full_system(),
            processes: vec![full_process()],
        },
    };
    write("keyframe_full.json", &full);

    let delta = Frame {
        seq: FrameSeq(3),
        timestamp_ms: 1_700_000_002_000,
        elapsed_ms: 997,
        payload: FramePayload::Delta {
            system: full_system(),
            changed: vec![full_process()],
            exited: vec![Pid(7)],
        },
    };
    write("delta.json", &delta);

    write(
        "summary.json",
        &Summary::from_parts(3, 1_700_000_002_000, full_system(), &[full_process()], 5),
    );

    write(
        "history.json",
        &vec![
            MachineSample::from_metrics(1_700_000_000, &full_system()),
            MachineSample::from_metrics(1_700_000_060, &fixtures::system()),
        ],
    );

    write(
        "sensors.json",
        &vec![SensorLine {
            key: "cpu.package".into(),
            label: "CPU package".into(),
            value: 81.0,
            unit: "temperature".into(),
            source: "kernelDriver".into(),
            quality: "measured".into(),
        }],
    );

    let mut values = HashMap::new();
    values.insert("percent".to_owned(), AlertValue::Number(97.0));
    values.insert("name".to_owned(), AlertValue::Text("C:".into()));
    write(
        "alerts.json",
        &vec![Alert {
            kind: AlertKind::ThermalCpu,
            severity: Severity::Critical,
            subject: String::new(),
            title: "dashboard.alert.thermalCpu.title".into(),
            cause: "dashboard.alert.thermalCpu.hot".into(),
            values,
            route: Some(AlertRoute::Performance),
            since_sample: 42,
        }],
    );

    write(
        "host.json",
        &vitals_core::provider::HostInfo {
            hostname: "DESKTOP-VITALS".into(),
            os_name: "Windows 11 Pro".into(),
            os_version: "10.0.26200".into(),
            kernel_version: "26200.6584".into(),
            architecture: "x86_64".into(),
            cpu_model: "Intel(R) Core(TM) i9-14900K".into(),
            cpu_vendor: "GenuineIntel".into(),
            physical_cores: 24,
            logical_cores: 32,
            core_topology: Some(vec![
                vitals_core::provider::CoreClass::Performance,
                vitals_core::provider::CoreClass::Efficiency,
            ]),
            total_memory: Bytes(206_158_430_208),
            boot_time_ms: 1_700_000_000_000,
            is_virtual_machine: false,
            motherboard: Some("Z790 AORUS ELITE AX".into()),
            bios_version: Some("FM".into()),
        },
    );
}
