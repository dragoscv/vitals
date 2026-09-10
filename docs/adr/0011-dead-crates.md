# 0011 — Implement `vitals-store` for real; delete `vitals-plugin`; keep `vitals-ipc`

- Status: Accepted
- Date: 2026-09-10
- Tracker: D11

## Context

The audit (F7, F9) found `vitals-store` contained only a retention policy
and no storage, `vitals-plugin` was declared and referenced by nobody, and
`rusqlite --features bundled` was a workspace dependency nothing compiled. A
crate that exists in the tree but does nothing is a promise the code does
not keep.

## Decision

- `vitals-store` gains a real SQLite time-series store, retention
  enforcement, and the flight recorder built on it.
- `vitals-plugin` is deleted. A plugin system is not on the roadmap and a
  stub crate invites speculative design.
- `vitals-ipc` stays: the elevated helper's command protocol lives there,
  and the helper (ADR 0012) is still planned.

## Consequences

- History survives a crash (F11) and is bounded (F12).
- `cargo run -p vitals-store --example prove_store` writes real frames and
  reads them back — the prover pattern's first instance.
- Fourteen unused workspace dependencies were removed alongside.
