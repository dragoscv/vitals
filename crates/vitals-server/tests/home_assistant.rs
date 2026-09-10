//! The Home Assistant package reads JSON paths that nothing compiles.
//!
//! `docs/integrations/home-assistant/vitals.yaml` is Jinja over the body of
//! `/api/v1/snapshot`. If a Rust field is renamed, the template does not
//! fail — Jinja resolves a missing attribute to `undefined`, the sensor reads
//! `unknown`, and nobody notices until a user asks why their CPU gauge died
//! last month. This test pulls every `value_json.<path>` out of the YAML and
//! resolves it against a real serialised frame, so the rename breaks here.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;

use serde_json::Value;
use vitals_core::fixtures;

const PACKAGE: &str = include_str!("../../../docs/integrations/home-assistant/vitals.yaml");

/// Every `value_json.…` path in the file, as written.
///
/// Hand-rolled rather than a regex crate: the grammar is one prefix followed
/// by identifier / `.` / `[digits]` characters, and a dev-dependency for
/// that would cost more than it saves.
fn extract_paths(yaml: &str) -> BTreeSet<String> {
    const PREFIX: &str = "value_json.";
    let mut paths = BTreeSet::new();
    let mut rest = yaml;
    while let Some(start) = rest.find(PREFIX) {
        let after = &rest[start + PREFIX.len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '[' | ']')))
            .unwrap_or(after.len());
        paths.insert(after[..end].to_owned());
        rest = &after[end..];
    }
    paths
}

/// Follows a dotted path, treating `name[0]` as an array index, and returns
/// Every `map(attribute='…')` name in the file. The aggregate templates
/// ("busiest disk", "busiest adapter") address list element fields this way
/// rather than through a `value_json.` path, so the extractor above cannot
/// see them — and a rename of `rx` would be exactly as silent.
fn extract_attributes(yaml: &str) -> BTreeSet<String> {
    const PREFIX: &str = "attribute='";
    let mut names = BTreeSet::new();
    let mut rest = yaml;
    while let Some(start) = rest.find(PREFIX) {
        let after = &rest[start + PREFIX.len()..];
        let end = after.find('\'').unwrap_or(after.len());
        names.insert(after[..end].to_owned());
        rest = &after[end..];
    }
    names
}

/// Follows a dotted path, treating `name[0]` as an array index, and returns
/// the value found — or the segment that did not resolve.
fn resolve<'a>(root: &'a Value, path: &str) -> Result<&'a Value, String> {
    let mut current = root;
    for segment in path.split('.') {
        let (key, index) = match segment.split_once('[') {
            Some((key, idx)) => (key, Some(idx.trim_end_matches(']'))),
            None => (segment, None),
        };
        current = current
            .get(key)
            .ok_or_else(|| format!("`{key}` is not a key in the frame (path `{path}`)"))?;
        if let Some(index) = index {
            let index: usize = index
                .parse()
                .map_err(|_| format!("bad index in `{segment}`"))?;
            current = current
                .get(index)
                .ok_or_else(|| format!("`{key}` has no element {index} (path `{path}`)"))?;
        }
    }
    Ok(current)
}

/// The fixture has no GPU, and the wire-format guarantee "absent means
/// `null`, never `0`" is exactly what the YAML's guards rely on. Give the
/// fixture one GPU with `utilization: null` so `gpus[0].utilization` is a
/// present-but-null key, which is the case the template must handle.
fn snapshot_json() -> Value {
    let mut frame = fixtures::keyframe(1, vec![fixtures::process("x", 1, 1.0)]);
    if let vitals_core::sample::FramePayload::Keyframe { system, .. } = &mut frame.payload {
        system.gpus.push(vitals_core::metrics::GpuMetrics {
            id: vitals_core::ids::GpuId(0),
            name: "fixture".to_owned(),
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
    }
    serde_json::to_value(&frame).expect("a frame serialises")
}

#[test]
fn every_snapshot_path_in_the_package_exists_on_the_wire() {
    let snapshot = snapshot_json();
    let health = serde_json::json!({ "ok": true, "version": "0.0.0", "modelVersion": 1 });

    let paths = extract_paths(PACKAGE);
    assert!(
        paths.len() >= 8,
        "expected the package to read several paths, found {paths:?}"
    );

    let mut failures = Vec::new();
    for path in &paths {
        // `/api/v1/health` has its own shape; the only path read from it is
        // top-level `ok`. Everything under `payload` is the snapshot.
        let root = if path.starts_with("payload") {
            &snapshot
        } else {
            &health
        };
        if let Err(reason) = resolve(root, path) {
            failures.push(reason);
        }
    }
    assert!(
        failures.is_empty(),
        "the Home Assistant package reads fields that are not on the wire:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn every_list_attribute_in_the_package_exists_on_the_wire() {
    let snapshot = snapshot_json();
    let attributes = extract_attributes(PACKAGE);
    assert!(
        !attributes.is_empty(),
        "the package should aggregate over the disk and network lists"
    );

    // Each name must exist on at least one list element; the fixture has one
    // of each, so "on the first" is the same assertion and gives a clearer
    // failure.
    let mut failures = Vec::new();
    for name in &attributes {
        let found = ["payload.system.disks[0]", "payload.system.networks[0]"]
            .iter()
            .any(|base| resolve(&snapshot, &format!("{base}.{name}")).is_ok());
        if !found {
            failures.push(format!(
                "`{name}` is not a field of a disk or a network adapter"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n  "));
}

#[test]
fn the_nullable_fields_the_yaml_guards_really_arrive_as_null() {
    // If either of these ever serialises as `0` instead of `null`, the
    // `is not none` guards in the YAML become dead code and a machine with
    // an unreadable sensor shows 0 °C. The fixture leaves both unmeasured.
    let snapshot = snapshot_json();
    assert!(
        resolve(&snapshot, "payload.system.cpu.temperature")
            .unwrap()
            .is_null(),
        "unmeasured CPU temperature must be null on the wire"
    );
    assert!(
        resolve(&snapshot, "payload.system.gpus[0].utilization")
            .unwrap()
            .is_null(),
        "a GPU without engine counters must report null utilisation"
    );
}

#[test]
fn a_typo_in_a_path_is_detected_not_ignored() {
    // The test above is only worth having if it can fail. Prove the resolver
    // rejects a near-miss rather than coercing it.
    let snapshot = snapshot_json();
    assert!(resolve(&snapshot, "payload.system.cpu.totl").is_err());
    assert!(resolve(&snapshot, "payload.system.cpu.process_count").is_err());
    assert!(resolve(&snapshot, "payload.system.gpus[7].utilization").is_err());
}
