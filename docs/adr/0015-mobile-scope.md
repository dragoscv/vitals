# 0015 — Mobile scope: overview, processes, alerts, process control, multi-machine

- Status: Accepted
- Date: 2026-09-10
- Tracker: D15

## Context

A read-only phone view is safe but limited; the moment someone sees a
runaway process on their phone they want to end it. Several PCs on one
network is the normal home-lab case.

## Decision

The phone app shows an overview card per machine, a process list sorted by
CPU, the alerts feed (polled from `GET /api/v1/alerts`), and can end,
suspend or resume a process **only** with a `Control`-scoped token, behind a
two-tap confirm. Multiple machines are paired side by side; each pairing is
independent.

## Consequences

- The default pairing is read-only; control is an explicit second decision
  the user makes on the desktop.
- A 403 from a read token is explained on the phone rather than swallowed.
- The confirm-step test fails if the first tap calls `control()`; that test
  was decorative once and was made real by mutation.
