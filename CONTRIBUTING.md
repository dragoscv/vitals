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
git clone https://github.com/dragoscv/vitals
cd vitals
pnpm install
pwsh -NoProfile -File scripts/hooks/install.ps1   # once per clone
pnpm --filter @vitals/desktop dev
```

The hook install puts a pre-commit gate in place that catches formatting,
contract drift, secrets and a stray `.only` in a few seconds — before CI
spends four minutes telling you the same thing.

## Before you open a pull request

One command runs every gate CI runs and exits non-zero on the first failure:

```powershell
pwsh -NoProfile -File scripts/verify.ps1
```

The individual pieces, if you want them separately:

```powershell
pwsh -NoProfile -File scripts/check-drift.ps1   # two seconds; run it first
cargo fmt --all; cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint; pnpm typecheck; pnpm test; pnpm format:check
pwsh -NoProfile -File scripts/check-size.ps1 -SkipInstaller
```

`check-drift.ps1` looks for the things two separately-compiled sides cannot
catch: a struct deriving `ts_rs::TS` without `#[serde(rename_all =
"camelCase")]`, an `invoke('name')` with no `#[tauri::command]` behind it
(or one not in `generate_handler!`), and a locale key present in one language
only. Each of those shipped once before the script existed.

If you changed anything in `crates/vitals-core`, run `pnpm protocol:generate`
and commit the result; CI fails on a dirty `packages/protocol/src/generated`.

**Do not chain `git commit` after the gates with `;`.** The commit runs
whether or not the gates passed. Run the gates, read the output, then commit.

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

**British spelling** in prose and comments.

## Tests

Test the **guarantee**, not the implementation. The tests worth writing are
the ones that encode a decision:

- A dropped frame is not zero CPU.
- Elevation does not help against a protected process.
- A PID recycled inside one frame must not delete the new process.

Every one of those was a real bug caught by a test written to fail first.

**Name a test as a full sentence stating the guarantee:**
`fn exits_apply_before_changes_so_a_recycled_pid_survives()`. If the name
cannot be written that way, the test probably does not assert anything worth
keeping.

**A green test is not evidence until you know what would make it fail.**
Before you commit one, mutate the code it covers and watch it go red. Two
tests in this repo were decorative until someone tried. Prefer one
adversarial test — the wrong token, the recycled PID, the read scope trying
to control — to five happy paths.

**Shared fixtures** live behind `vitals-core`'s `fixtures` feature:
`fixtures::system()`, `::process()`, `::keyframe()`. Do not hand-roll a
thirty-field struct per crate.

### The prover pattern

Fixtures cannot show that a sampler's real output survives serialisation,
storage and a client's parser. Each subsystem with an external boundary has
an example that runs it against the actual machine and prints what it found:

```powershell
cargo run -p vitals-store  --example prove_store    # real frames → SQLite → back
cargo run -p vitals-server --example prove_lan      # sample, serve, print what a phone gets
cargo run -p vitals-win    --example prove_process_detail
cargo run -p vitals-server --example serve_dev      # live API on :7332, token "dev"
```

Run the ones covering the code you touched. Add one when you add a boundary.
Then open the thing you built and read the numbers — two of the defects
found in one day were invisible to every test and obvious within ten
seconds of looking at the page.

## Commits

[Conventional Commits](https://www.conventionalcommits.org):
`feat(processes): add handle finder`, `fix(charts): stop axis jitter`.
Imperative, lowercase, no trailing period.

Small, focused pull requests get reviewed quickly. Large ones do not.

## Developer Certificate of Origin

Every commit must be signed off under the
[Developer Certificate of Origin](https://developercertificate.org) (DCO).
By signing off you certify that you wrote the change, or otherwise have the
right to submit it under the project's MIT licence.

Sign off by committing with `-s`:

```powershell
git commit -s -m "fix(charts): stop axis jitter"
```

This adds a `Signed-off-by: Your Name <you@example.com>` line matching your
git identity. To sign off commits you have already made on your branch, run
`git rebase --signoff main` and force-push the branch. Pull requests with
unsigned commits cannot be merged.

## Security

If you find a vulnerability, please **do not** open a public issue — report
it privately as described in [SECURITY.md](SECURITY.md).

## Code of conduct

Be decent to people. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
