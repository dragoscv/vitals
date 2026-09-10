# 0018 — Raise the size budget deliberately; the gate stays strict

- Status: Accepted
- Date: 2026-09-10
- Tracker: D18

## Context

`size-budget.json` gates initial-load and total gzipped size. Nine new
features (ADR 0008) cannot fit in a budget set when the app had none of
them. Loosening the gate to a warning would remove the only thing that has
kept size honest.

## Decision

Raise the numbers in `size-budget.json` by a reviewed amount per feature,
with the cost recorded in the tracker's verification log. The gate remains a
hard failure. `check-size.ps1` budgets each entry (`index.html`, `hud.html`,
`mobile.html`) from its real module graph rather than by filename pattern —
the previous pattern was wrong in both directions.

## Consequences

- A budget increase is a visible diff in a JSON file, not a number in
  someone's head.
- Headroom is small on purpose (initial load at ~99 % of budget after S7);
  the next feature must either pay for itself or raise the budget in its
  own commit.
- `-SkipInstaller` lets CI's frontend job run the check without a Tauri
  build.
