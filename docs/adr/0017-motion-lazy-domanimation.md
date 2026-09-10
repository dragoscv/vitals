# 0017 — `motion/react-m` under `LazyMotion(domAnimation)`

- Status: Accepted
- Date: 2026-09-10
- Tracker: D17

## Context

`motion` was a declared dependency imported nowhere (F20); all transitions
were hand-written CSS. The full `motion/react` import has a 34 kB gzipped
floor, which is a fifth of the initial-load budget.

## Decision

Import `m` from `motion/react-m` and wrap the app in
`<LazyMotion features={domAnimation} strict>`. Measured cost: 19.6 kB
gzipped. `strict` makes an accidental `motion.div` import a build-time
error rather than a silent 14 kB.

## Consequences

- Route transitions and the palette use a shared motion config; reduced
  motion is honoured through the same config.
- Layout animations (`domMax`) are not available; nothing needs them yet,
  and adding them is a conscious 15 kB decision.
