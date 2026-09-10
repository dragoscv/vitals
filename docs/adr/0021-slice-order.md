# 0021 — Slice order: tooling → truth → server → mobile → UI → tray/HUD → metrics → docs/CI

- Status: Accepted
- Date: 2026-09-10
- Tracker: D21

## Context

Eleven slices of work were accepted in one session. Order matters: a gate
added last catches nothing, a mobile client built before the server has
nothing to talk to, and documentation written before the code describes the
wrong thing.

## Decision

1. **S1 tooling** — upgrades and gates first, so every later slice is
   verified by the strictest tools.
2. **S2 truth** — remove false claims (inert settings, phantom
   capabilities, dead crates) before building on them.
3. **S3/S4 store and server** — the backends every client needs.
4. **S5/S6 CLI and mobile** — the new clients, which audit the backend.
5. **S7/S8 UI, tray, HUD, alerts, updater** — the desktop experience.
6. **S9 metrics** — new Windows readings.
7. **S10 docs, ADRs, CI, audits** — last, so it describes what shipped.

## Consequences

- This record and its siblings are written after the fact, from the
  decisions table and the verification log, and describe the tree as it is.
- The typed-lint gate from S1 found real defects in S4–S8 code as it landed.
