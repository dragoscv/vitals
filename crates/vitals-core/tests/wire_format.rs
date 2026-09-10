//! The wire format must be camelCase, at every depth.
//!
//! ## The bug this exists to prevent
//!
//! Every serialised type carries two attributes that must agree:
//!
//! ```ignore
//! #[cfg_attr(feature = "ts", ts(..., rename_all = "camelCase"))]  // the TypeScript
//! #[serde(rename_all = "camelCase")]                              // the JSON
//! ```
//!
//! Sixteen types had the first and not the second. The generated TypeScript
//! promised `elapsedMs`; the wire delivered `elapsed_ms`. Every multi-word
//! field in every frame arrived as `undefined`, and the app rendered nothing
//! but "No readings are arriving".
//!
//! Nothing caught it. `cargo check` passes — the Rust is valid. `tsc` passes
//! — the TypeScript is internally consistent. `pnpm protocol:check` passes —
//! it regenerates the bindings and diffs them, and the bindings were correct;
//! it is the *runtime* that disagreed with them. Every unit test passes,
//! because they construct values in Rust and never cross the boundary.
//!
//! Only serialising a real value and reading the keys finds it, which is what
//! this file does.

// Integration tests are not covered by clippy's `allow-expect-in-tests`,
// which only recognises `#[cfg(test)]` modules.
#![allow(clippy::expect_used)]

use vitals_core::ids::{Pid, ProcessKey};
use vitals_core::metrics::SystemMetrics;
use vitals_core::process::{Process, ProcessFlags, ProcessKind, ProcessState, ProtectionLevel};
use vitals_core::provider::HostInfo;
use vitals_core::sample::{Frame, FramePayload, FrameSeq};
use vitals_core::units::{Bytes, BytesPerSec, Percent};

/// A process row with every field populated.
///
/// Written out rather than derived from `Default`, which `Process`
/// deliberately does not implement — a zero-valued process is not a
/// meaningful thing for production code to be able to construct.
fn a_process() -> Process {
    Process {
        key: ProcessKey::new(Pid(100), 1),
        parent: None,
        name: "test.exe".into(),
        kind: ProcessKind::App,
        state: ProcessState::Running,
        flags: ProcessFlags::empty(),
        integrity: None,
        protection: ProtectionLevel::None,
        cpu: Percent::ZERO,
        memory_private: Bytes::ZERO,
        memory_working_set: Bytes::ZERO,
        disk_read: BytesPerSec::ZERO,
        disk_write: BytesPerSec::ZERO,
        net_rx: None,
        net_tx: None,
        gpu: None,
        gpu_memory: None,
        thread_count: 1,
        handle_count: None,
        user: None,
        uptime_secs: 0,
    }
}

/// Host info with every field populated, for the same reason.
fn a_host() -> HostInfo {
    HostInfo {
        hostname: "TESTBOX".into(),
        os_name: "Windows 11".into(),
        os_version: "10.0".into(),
        kernel_version: "26200".into(),
        architecture: "x86_64".into(),
        cpu_model: "Test CPU".into(),
        cpu_vendor: "GenuineIntel".into(),
        physical_cores: 8,
        logical_cores: 16,
        core_topology: None,
        total_memory: Bytes(1024),
        boot_time_ms: 0,
        is_virtual_machine: false,
        motherboard: None,
        bios_version: None,
    }
}

/// Collects every key containing an underscore, at any depth.
///
/// Recursive because the failure was in a nested field: a top-level check
/// would have passed while `system.power_draw` was still wrong.
fn snake_case_keys(value: &serde_json::Value, path: &str, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key.contains('_') {
                    found.push(format!("{path}.{key}"));
                }
                snake_case_keys(child, &format!("{path}.{key}"), found);
            }
        }
        serde_json::Value::Array(items) => {
            // One element suffices; every element of an array shares a shape.
            if let Some(first) = items.first() {
                snake_case_keys(first, &format!("{path}[0]"), found);
            }
        }
        _ => {}
    }
}

/// Asserts a value serialises with no `snake_case` keys anywhere.
fn assert_camel_case<T: serde::Serialize>(label: &str, value: &T) {
    let json = serde_json::to_value(value).expect("must serialise");
    let mut found = Vec::new();
    snake_case_keys(&json, label, &mut found);

    assert!(
        found.is_empty(),
        "{label} sends snake_case to a webview expecting camelCase: {found:#?}\n\
         Add #[serde(rename_all = \"camelCase\")] beside the ts_rs attribute."
    );
}

#[test]
fn a_frame_is_camel_case_at_every_depth() {
    // `Default` gives every field, including the nested ones — which is what
    // makes this catch a rename buried three levels down.
    let frame = Frame {
        seq: FrameSeq(1),
        timestamp_ms: 0,
        elapsed_ms: 1000,
        payload: FramePayload::Keyframe {
            system: SystemMetrics::default(),
            processes: vec![a_process()],
        },
    };

    assert_camel_case("frame", &frame);
}

#[test]
fn a_delta_frame_is_camel_case_too() {
    // A different payload variant with a different shape. The keyframe being
    // correct says nothing about this one.
    let frame = Frame {
        seq: FrameSeq(2),
        timestamp_ms: 0,
        elapsed_ms: 1000,
        payload: FramePayload::Delta {
            system: SystemMetrics::default(),
            changed: vec![a_process()],
            exited: vec![],
        },
    };

    assert_camel_case("delta", &frame);
}

#[test]
fn system_metrics_are_camel_case() {
    assert_camel_case("system", &SystemMetrics::default());
}

#[test]
fn a_process_row_is_camel_case() {
    assert_camel_case("process", &a_process());
}

#[test]
fn host_info_is_camel_case() {
    assert_camel_case("hostInfo", &a_host());
}

#[test]
fn the_frame_carries_the_keys_the_client_actually_reads() {
    // Named explicitly rather than trusting the underscore sweep: these are
    // the fields `apps/desktop/src/lib/metrics.ts` destructures off every
    // frame, and a rename that happened to avoid an underscore — `elapsed`
    // instead of `elapsedMs` — would slip past the check above.
    let frame = Frame {
        seq: FrameSeq(1),
        timestamp_ms: 42,
        elapsed_ms: 1000,
        payload: FramePayload::Keyframe {
            system: SystemMetrics::default(),
            processes: vec![],
        },
    };

    let json = serde_json::to_value(&frame).expect("serialise");

    for key in ["seq", "timestampMs", "elapsedMs", "payload"] {
        assert!(
            json.get(key).is_some(),
            "the client reads frame.{key}, which is not on the wire"
        );
    }

    for key in ["kind", "system", "processes"] {
        assert!(
            json["payload"].get(key).is_some(),
            "the client reads frame.payload.{key}, which is not on the wire"
        );
    }
}
