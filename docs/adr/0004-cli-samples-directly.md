# 0004 — The CLI samples directly when the app is not running

- Status: Accepted
- Date: 2026-09-10
- Tracker: D4

## Context

A scriptable `vitals` binary is most useful on servers and in remote shells,
where the desktop app is usually not running. A CLI that only proxies to the
app would be useless exactly where it matters.

## Decision

`apps/cli` links `vitals-win` and samples the machine itself. If the desktop
app is running it attaches instead, over the loopback API on `:7330`
(ADR 0002), so both see one sampler and one set of numbers. Output is a
table by default and JSON with `--json`. Subcommands: `ps`, `top`, `info`,
`report`, `serve`.

## Consequences

- `vitals serve` gives a headless machine the same LAN API the desktop
  offers, with a generated token printed once.
- The CLI carries the full sampler, so it is not tiny, and it inherits every
  privilege limitation the sampler has (for instance, `DiskCounters` needs
  `SeDebugPrivilege`).
- Two code paths — attached and direct — must render identically; the tests
  fold both through the same renderer.
