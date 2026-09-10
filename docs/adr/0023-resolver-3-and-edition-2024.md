# 0023 — Cargo resolver 3 and Rust edition 2024

- Status: Accepted
- Date: 2026-09-10
- Tracker: S10-02

## Context

The workspace has ten crates with a shared `[workspace.package]`. Edition
2021 with resolver 2 was the default when it was created; both have newer
stable successors.

## Decision

`resolver = "3"` at the workspace root and `edition = "2024"` in
`[workspace.package]`, with `rust-version = "1.90"`.

Resolver 3 makes Cargo respect `rust-version` when picking dependency
versions, so a `cargo update` cannot pull a crate that needs a newer
compiler than the one CI installs. Edition 2024 brings `let` chains, the
`unsafe extern` and `unsafe_op_in_unsafe_fn` rules, and `gen` as a reserved
keyword — the unsafe changes matter in `vitals-win`, which is largely FFI.

## Consequences

- `dtolnay/rust-toolchain@stable` in CI must be at least 1.85; it is.
- Every `unsafe fn` body in `vitals-win` states its `unsafe {}` blocks
  explicitly, which is where most of the migration diff went.
- Third-party crates that have not moved to 2024 are unaffected; edition is
  per crate.
