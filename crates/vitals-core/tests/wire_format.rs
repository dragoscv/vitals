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
        description: None,
        kind: ProcessKind::App,
        // Multi-word on purpose: `running` spells the same in every casing,
        // so it cannot catch a kebab-case enum. This one can.
        state: ProcessState::NotResponding,
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

/// Collects every key that is not camelCase, at any depth.
///
/// Recursive because the failure was in a nested field: a top-level check
/// would have passed while `system.power_draw` was still wrong.
///
/// Checks for `-` as well as `_`: `ProcessKey` shipped with
/// `rename_all = "kebab-case"` for months, serialising `start-time` against a
/// TypeScript binding that said `startTime`. The underscore-only version of
/// this test passed the whole time.
fn snake_case_keys(value: &serde_json::Value, path: &str, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key.contains('_') || key.contains('-') {
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

/// Collects every string value that looks like a kebab-case enum variant.
///
/// The key sweep above passed for months while fourteen enums shipped
/// `rename_all = "kebab-case"` beside a `ts_rs` export that says `camelCase`:
/// `"not-responding"` on the wire, `"notResponding"` in the union. Every
/// `t('state.' + row.state)` lookup and `=== 'powerLimit'` comparison
/// downstream silently missed for exactly the multi-word variants. Keys
/// and values drift independently, so both need a sweep.
fn kebab_case_values(value: &serde_json::Value, path: &str, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                kebab_case_values(child, &format!("{path}.{key}"), found);
            }
        }
        serde_json::Value::Array(items) => {
            if let Some(first) = items.first() {
                kebab_case_values(first, &format!("{path}[0]"), found);
            }
        }
        serde_json::Value::String(text) => {
            // An enum variant is short, lowercase ASCII with an inner dash;
            // free text (names, paths, versions) is excluded by the charset.
            let looks_like_variant = text.len() < 24
                && text.contains('-')
                && text.chars().all(|c| c.is_ascii_lowercase() || c == '-');
            if looks_like_variant {
                found.push(format!("{path} = {text:?}"));
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

    let mut values = Vec::new();
    kebab_case_values(&json, label, &mut values);
    assert!(
        values.is_empty(),
        "{label} sends kebab-case enum values against a camelCase TypeScript union: {values:#?}\n\
         The enum's serde rename_all must say camelCase, like its ts attribute."
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
fn multi_word_enum_variants_serialise_to_the_literals_the_ui_compares_against() {
    // Single-word variants spell the same in every casing, so a fixture with
    // only those proves nothing. These are the variants that differed live:
    // the connections table showed a raw `state.syn-sent` key, the CPU panel
    // a raw `throttle.power-limit`, and the process details `state.not-responding`.
    use vitals_core::metrics::{NetworkKind, ThrottleReason};
    use vitals_core::process::ProcessState;
    use vitals_core::provider::{ConnectionState, CoreClass, Priority};

    let cases: [(&str, serde_json::Value); 6] = [
        (
            "notResponding",
            serde_json::to_value(ProcessState::NotResponding).expect("serialise"),
        ),
        (
            "powerLimit",
            serde_json::to_value(ThrottleReason::PowerLimit).expect("serialise"),
        ),
        (
            "synSent",
            serde_json::to_value(ConnectionState::SynSent).expect("serialise"),
        ),
        (
            "wiFi",
            serde_json::to_value(NetworkKind::WiFi).expect("serialise"),
        ),
        (
            "lowPower",
            serde_json::to_value(CoreClass::LowPower).expect("serialise"),
        ),
        (
            "belowNormal",
            serde_json::to_value(Priority::BelowNormal).expect("serialise"),
        ),
    ];
    for (expected, actual) in cases {
        assert_eq!(actual, serde_json::Value::String(expected.into()));
    }
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

#[test]
fn disk_counter_source_serialises_to_the_literals_the_ui_compares_against() {
    // The UI does `source === 'storageStack'`. The generated union says
    // `"storageStack" | "allIo"` because ts_rs renames to camelCase; serde
    // must produce the same spelling or the comparison is never true and the
    // tooltip falls through to "not yet known" — which is exactly what it
    // did live, with every gate green.
    use vitals_core::process::DiskCounterSource;

    assert_eq!(
        serde_json::to_value(DiskCounterSource::StorageStack).expect("serialise"),
        serde_json::Value::String("storageStack".into())
    );
    assert_eq!(
        serde_json::to_value(DiskCounterSource::AllIo).expect("serialise"),
        serde_json::Value::String("allIo".into())
    );
}
