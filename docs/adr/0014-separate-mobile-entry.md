# 0014 — A separate lean `mobile.html` entry with its own component tree

- Status: Accepted
- Date: 2026-09-10
- Tracker: D14

## Context

The desktop bundle is built for a wide window, a sidebar, Radix primitives
and a full router. A phone on a LAN wants one small page that loads in a
second over Wi-Fi and shows a handful of numbers.

## Decision

`apps/desktop/mobile.html` is a distinct Vite entry with its own component
tree under `src/mobile/`. It shares `@vitals/charts`, `@vitals/protocol`,
design tokens and the i18n runtime, and nothing else from the desktop
shell. It is served by `vitals-server` from the same `dist` directory.

## Consequences

- Mobile bundle at first ship: 126 kB gzipped including React, under its own
  150 kB budget; each entry is budgeted from its real module graph.
- The phone rendered data the desktop's incidental filtering had hidden —
  two backend defects surfaced on the first live session.
- Components are not reused between the two trees by design; a shared
  primitive that both need belongs in `@vitals/ui`, not in either entry.
