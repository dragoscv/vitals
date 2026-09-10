# 0026 — `panic = "abort"` in the release profile

- Status: Accepted
- Date: 2026-09-10
- Tracker: S10-02

## Context

The sampler runs on a dedicated thread. With unwinding, a panic there kills
the thread and leaves the window open showing frozen numbers — the worst
failure mode for a monitor, because it looks like the machine is fine.
Unwinding also costs binary size and prevents some optimisations, and FFI
boundaries in `vitals-win` cannot unwind safely anyway.

## Decision

`[profile.release] panic = "abort"`, with `lto = "fat"`,
`codegen-units = 1`, `opt-level = 3` and `strip = true`. A panic anywhere
takes the process down loudly rather than half-working.

## Consequences

- `unwrap_used` and `expect_used` are clippy `warn`s in production code and
  an `#[allow]` needs a written reason: with abort, either one in the
  sampler is a process crash.
- Tests keep unwinding (the profile only applies to release), so
  `#[should_panic]` still works.
- A `bench-perf` profile inherits release but keeps debug info and symbols
  for profiling.
