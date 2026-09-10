# 0007 — Windows only; macOS and Linux remain honest trait stubs

- Status: Accepted
- Date: 2026-09-10
- Tracker: D7

## Context

`vitals-macos` and `vitals-linux` exist as crates. Filling them with partial
implementations would claim support the project cannot verify — there is no
macOS or Linux machine in the loop, and a sampler that has not been compared
against the OS's own counters is a source of confident wrong numbers.

## Decision

Ship Windows. The other two crates implement the `vitals-core` traits by
returning `Unavailable::NotImplemented` for everything, so a build on those
platforms compiles and the UI shows the reason, never a zero.

## Consequences

- CI builds `vitals-core` on Ubuntu to prove the trait boundary has not
  leaked Windows types into the core.
- The README says "Windows — with macOS and Linux to follow" and means it.
- Porting later is an implementation, not a rewrite: the dependency arrow
  points inward and the frontend never sees a platform type.
