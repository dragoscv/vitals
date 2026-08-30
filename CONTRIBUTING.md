# Contributing to Vitals

Thank you for being here. Vitals is free forever and gets better only because
people report what is broken and send fixes.

## The most valuable contributions

You do not need to write Rust to help. In rough order of usefulness:

1. **Report a wrong number.** If a metric disagrees with Task Manager, HWiNFO
   or your BIOS, that is a bug and we want it. Include your hardware.
2. **Report a missing sensor.** Motherboard sensor layouts vary enormously.
   If your board reports something we do not show, tell us which board.
3. **Tell us what Task Manager never let you do.** Feature requests grounded
   in a real frustration are worth more than a list of ideas.
4. **Improve a translation.** See `packages/i18n/src/locales/`.
5. **Send a patch.**

## Getting set up

Requires [Rust](https://rustup.rs) stable, [Node](https://nodejs.org) 22+,
[pnpm](https://pnpm.io) 10+, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
git clone https://github.com/vitals-app/vitals
cd vitals
pnpm install
pnpm --filter @vitals/desktop dev
```

## Before you open a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint && pnpm typecheck && pnpm test
```

CI runs all of these plus a check that the generated TypeScript matches the
Rust model. If you changed anything in `crates/vitals-core`, run
`pnpm protocol:generate` and commit the result.

## House rules

These are the conventions the codebase already follows. They exist for
reasons, and the reasons are usually written next to the code.

**Never fabricate a number.** A metric this platform cannot provide is
`None`/`null`, never `0`. Zero and unknown are different facts and the UI
renders them differently. This is the single most important rule here.

**Declare capabilities; do not discover them by failing.** If an operation
needs elevation or a missing component, say so through the capability model
so the UI can disable the affordance with a reason. A button that throws when
clicked is a bug.

**Never act on a bare PID.** Use `ProcessKey`, which pairs the PID with the
process start time. Windows recycles PIDs within seconds; a stale PID will
eventually kill the wrong process.

**Keep the core OS-agnostic.** `crates/vitals-core` must never depend on a
platform crate. If you need something platform-specific, add it to the
provider traits.

**Do not slow down the sampler.** Anything on the per-tick path is
performance-critical. Per-process work that needs a handle open, a heap walk
or a WMI query belongs behind a cache or on a slower cadence.

**Write the "why", not the "what".** A comment explaining what a line does is
noise. A comment explaining why it is written that way — a race, a platform
quirk, a measured trade-off — is the most valuable thing in the file.

## Tests

Test behaviour and edge cases, not implementation. The tests worth writing are
the ones that encode a decision:

- A dropped frame is not zero CPU.
- Elevation does not help against a protected process.
- A PID recycled inside one frame must not delete the new process.

Every one of those was a real bug caught by a test written to fail first.

## Commits

[Conventional Commits](https://www.conventionalcommits.org):
`feat(processes): add handle finder`, `fix(charts): stop axis jitter`.
Imperative, lowercase, no trailing period.

Small, focused pull requests get reviewed quickly. Large ones do not.

## Security

Vitals runs an optional service with SYSTEM privileges. If you find a way to
misuse it, please **do not** open a public issue — email the address in
[SECURITY.md](SECURITY.md) instead.

## Code of conduct

Be decent to people. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
