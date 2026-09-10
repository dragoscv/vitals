# 0027 — `ts-rs` rather than `specta` for generated TypeScript bindings

- Status: Accepted
- Date: 2026-09-10
- Tracker: S10-02

## Context

Every type crossing the IPC and LAN boundary must exist identically in Rust
and TypeScript. Hand-written duplicates drift silently. The two generators
in the Tauri ecosystem are `ts-rs` and `specta` (via `tauri-specta`).

## Decision

`ts-rs` 12, with the `serde-compat` feature. `cargo test -p vitals-core
--features ts` writes `packages/protocol/src/generated`; CI fails if the
committed output differs.

`specta-typescript` only supports a `2.0.0-rc` pre-release of specta, and the
last stable specta line ships no TypeScript exporter at all. The latest-
stable policy (ADR 0005) rules it out.

## Consequences

- `ts-rs` emits camelCase unconditionally, while serde defaults to the Rust
  field name. Any struct deriving `TS` must carry
  `#[serde(rename_all = "camelCase")]`; `scripts/check-drift.ps1` enforces
  this after `ProcessKey` shipped `start-time` against a binding that said
  `startTime`.
- Command signatures are not generated (that is what `tauri-specta` would
  add); `invoke('name')` strings are checked against `#[tauri::command]`
  by the same drift script instead.
