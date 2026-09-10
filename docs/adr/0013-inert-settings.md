# 0013 — Wire every inert setting or delete it

- Status: Accepted
- Date: 2026-09-10
- Tracker: D13

## Context

Twelve of twenty-one settings were pure UI (F1): a toggle wrote a value that
nothing read. A settings page is a list of promises; each inert switch is a
broken one, and users cannot tell which.

## Decision

- Wire what is wireable: `startWithWindows`, `startMinimised`,
  `confirmEndTask`, the notification switches, `historyEnabled`,
  `retentionDays`.
- Delete what has no backend and no near-term plan: `crashReports`,
  `usageData`, `reputationLookups`, `advancedEnabled`.
- The settings schema degrades a single bad or unknown value to its default
  rather than failing to start, so removed keys are harmless in an existing
  store.

## Consequences

- The Settings screen is shorter and every control does something.
- Telemetry, if it ever arrives, needs a fresh decision and a fresh consent
  design rather than a switch that already existed.
