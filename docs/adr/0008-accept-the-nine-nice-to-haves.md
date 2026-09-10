# 0008 — Accept all nine "nice-to-have" features

- Status: Accepted
- Date: 2026-09-10
- Tracker: D8

## Context

The README listed nine features under "Around the edges" that had never been
scheduled: command palette, tray with live graph, alerts and rules, floating
HUD, flight recorder, efficiency mode and affinity presets, table export,
updater, handle/DLL finder. Several already had half-built backends (a
`tray-icon` feature enabled with no tray constructed; a `global-shortcut`
plugin registered with no shortcut).

## Decision

Build all nine, each as a full vertical slice — backend, command, UI, both
locales, LAN exposure where it applies — rather than leaving declared-but-
unfed capabilities in the tree.

## Consequences

- Slices S3, S7, S8 and S9 exist because of this decision.
- The size budget had to be raised (ADR 0018); each feature's cost is
  recorded in the tracker.
- "Built but unfed" (a backend nobody calls) is now the pattern the audit
  section of `TRACKER.md` hunts for, and the drift gate warns on a registered
  command nothing invokes.
