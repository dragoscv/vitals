---
name: add-a-metric
description: Add a new reading (CPU, GPU, disk, network, sensor, per-process field) end to end in Vitals — Rust sampler, core type, wire contract, generated bindings, UI, both locales, Prometheus, SDK. Use when adding or changing anything a user would see as a number, or when a metric shows an em dash it should not.
---

# Adding a metric

A metric is not one change; it is nine. The failure mode is not that a step is
hard — it is that step 4 or 8 is forgotten, and the number exists in the
backend while the UI shows nothing. That has happened in this repo more than
once.

Work in this order. Each step compiles, so a mistake surfaces immediately.

## 1. Decide whether it can be absent

If any supported machine cannot report it, the type is `Option<T>`. A `0` that
means "unknown" is a lie the whole UI then propagates. This decision is
irreversible in practice — widening `T` to `Option<T>` later touches every
consumer.

## 2. The core type — `crates/vitals-core/src/metrics.rs` (or `process.rs`)

Add the field with a unit newtype from `units.rs` (`Percent`, `Bytes`,
`BytesPerSec`, `Celsius`, `Hertz`, `Watts`), never a bare `f32`. Document what
it means in a sentence a user would recognise.

The struct already derives `ts_rs::TS`. Confirm it carries
`#[serde(rename_all = "camelCase")]`, and for an `Option<u64>` add
`#[cfg_attr(feature = "ts", ts(type = "number | null"))]` — ts-rs renders `u64`
as `bigint` otherwise, and `JSON.parse` never produces one.

## 3. Sample it — `crates/vitals-win/src/`

Find the sampler that owns the subsystem (`cpu.rs`, `memory.rs`, `gpu.rs`,
`disk.rs`, `net.rs`, `process/`). Rules that matter here:

- Never allocate per process per tick. The process sampler runs over ~650
  processes at 1 Hz; a `String` per process per tick is 650 allocations a
  second for a value nobody is reading.
- A counter must be turned into a rate against the **measured** elapsed time,
  not the nominal interval. The tick is not exactly 1000 ms.
- Handle the first sample: a rate needs two readings, so the first is `None`,
  not `0`.
- An API that is unavailable on some Windows versions returns `None`; it does
  not panic and does not log per tick.

## 4. The fixtures — `crates/vitals-core/src/fixtures.rs`

Add a plausible **non-zero, non-round** value. A fixture full of zeroes lets a
bug that drops the field pass every assertion. Adding it here is what stops
four crates' tests from breaking with a missing-field error.

## 5. Regenerate the bindings

```
cargo test -p vitals-core --features ts
git diff --stat packages/protocol/src/generated
```

A change here that you did not expect means the serde attributes disagree with
the ts-rs ones. Fix that now, not later.

## 6. The UI — `apps/desktop/src/features/…`

Render `null` as an em dash (`—`), never as `0`, and never hide the row: a
missing reading is information. Use the formatters from `@vitals/ui`
(`formatBytes`, `formatPercent`, `formatThroughput`) so units and precision
match every other number on screen.

## 7. Both locales

`packages/i18n/src/locales/{en,ro}.json`, or
`apps/desktop/src/shell/strings.ts` for shell text. `check-drift.ps1` fails on
a key present in one language only. Write the label for someone who does not
know the jargon.

## 8. The ripple — the step that gets forgotten

- `crates/vitals-server/src/prometheus.rs` — a new gauge, **omitted** when the
  value is `None`. Follow the existing `if let Some(...)` blocks.
- `packages/client` — usually free, since types come from `@vitals/protocol`.
- The CLI, if it prints this subsystem.
- Any history/export path that persists the reading.

## 9. Verify

```
cargo test -p vitals-win               # the sampler
cargo run -p vitals-win --example profile_sample   # see the real value
pwsh -NoProfile -File scripts/check-drift.ps1
pwsh -NoProfile -File scripts/verify.ps1
```

Run the prover and **look at the number**. A metric that compiles and shows
`0.000` on a busy machine is not working — that is precisely how the CPU
sampler's first-sample bug was found.

## Common mistakes

| Symptom                              | Cause                                                           |
| ------------------------------------ | --------------------------------------------------------------- |
| UI shows nothing at all              | serde casing disagrees with the binding — run `check-drift.ps1` |
| Value is always 0                    | rate computed on the first sample, before a delta exists        |
| Value is a `bigint` in JS            | missing `ts(type = "number \| null")` on a `u64`                |
| Four crates fail to compile          | new field, fixtures not updated                                 |
| Reading looks plausible but is wrong | counter divided by the nominal interval, not the measured one   |
