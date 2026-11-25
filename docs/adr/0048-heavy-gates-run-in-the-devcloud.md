# 0048 — The heavy gates run on the devcloud, and `-Where` decides

- **Status:** accepted
- **Date:** 2026-11-25
- **Tracker:** S26-12
- **Supersedes:** nothing

## Context

`scripts/verify.ps1` is the single entry point CI and a developer both run, and
it was built on the assumption that the machine running it has nothing else to
do. That assumption has been false for a while: this workstation runs several
coding agents at once, each with its own build and test jobs, and a full verify
there is roughly twenty minutes of CPU that the agents then compete with. The
twenty-minute figure is itself the product of that competition — the same gates
run in about two minutes on an idle machine.

The other repositories on this machine solved this first: `agentq run -Resource
build -Where cloud -Ws <workspace>` sends a command to a warm cloud workspace
that already has the repository, the toolchains and a recent build. Vitals was
the last repository still doing all of its work locally.

Moving it is not free. Two things in this repository only a Windows machine with
a JDK and the Android SDK can do (the three debug APKs and the licence/source
check), and one lane is deliberately Linux because its whole purpose is to catch
a platform-boundary leak. So the question was not "move it" but "which parts,
decided by what, and with what happening when the cloud is unreachable".

## Decision

**Placement is a parameter of the gate, resolved once, with the local machine as
it only fallback, and a lane name as the only thing that crosses the boundary.**

Concretely:

- `scripts/verify.ps1` takes `-Where local|cloud|auto` (default `auto`), `-Lane`
  (one lane, or `all`), and `-AndroidCloud`. `auto` means cloud when agentq
  exists and can reach `ws-vitals-win`; `AGENTQ_WHERE=local` — the variable
  agentq itself reads — overrides it everywhere, so any script, any shell and
  any CI job can opt out without an argument.
- A cloud lane is the same script: the submission is
  `pwsh -NoProfile -File scripts/verify.ps1 -Lane <name> -Where local`. The gate
  definitions exist once; the boundary carries a name, not a copy.
- Each lane reports its gates as
  `LANE_RESULT|<lane>|<gate>|<pass|fail>|<seconds>|<where>`, and the caller
  rebuilds the same summary table from those lines. The trailing field names the
  workspace that answered (`local`, `cloud:vitals-win`, `cloud:codai` for the
  Linux clippy gate, which is submitted from a lane that runs here), because one
  gate can be answered by three different machines and a pass without that name
  is only half the answer. A lane that
  failed before reaching a gate reports exactly that as a row, so a sync failure
  cannot be mistaken for a pass or silently dropped from the table.
- **A lane waits for its gate's process, never for its output.** Each gate runs
  as its own `pwsh` with stdout and stderr redirected to files, under a deadline
  after which the process tree is killed and the gate is red. The obvious
  implementation — invoke the gate's script block and read its pipeline — ends on
  EOF, and a toolchain daemon outlives the gate while keeping the handles it
  inherited, so the reader waits for a daemon that is doing nothing. Measured
  2026-11-25: a gate that finished in 0.2 s left a 40 s grandchild behind and the
  lane returned after 41 s; on the devcloud that is a job that never reports an
  exit code, because the runner waits for the lane. The same rule had already
  bitten the runner itself (see `docs/devcloud.md`), which is the argument for
  writing it down: an end defined by a closed pipe is an end someone else owns.
- **A job narrates itself, because from outside a working gate and a wedged one
  look identical.** `scripts/lib/devcloud-progress.ps1` appends a timestamped
  line per phase to the job's own prefix in the jobs bucket
  (`jobs/<id>/stage`), and both the lane and the android gate use it. The
  redirect that makes a build's output readable also makes it invisible while it
  runs, and eleven minutes of a hung assemble were diagnosed from the instance's
  CPU graph on 2026-11-25 before this existed.
- **A skipped lane is not a passing lane.** `check-android-features.ps1` gains
  `-Strict`, which turns "no SDK here, nothing to check" into a red gate; the
  android lane is submitted with it, so a workspace that lost its toolchain
  fails loudly instead of certifying nothing.
- The cloud lane's toolchains live outside the repository, under
  `C:\devcloud-tools`, and one idempotent script installs them
  (`scripts/ci/provision-cloud-workspace.ps1`). A `.gitignore`d toolchain, or one
  installed inside the workspace, would be deleted by the job's own clean.
- `AGENTQ_CLOUD_STRICT=1` turns the local fallback off, for a run where "did this
  actually happen in the cloud" matters.
- The Linux clippy lane stays on `ws-codai`, as it already was. Only the
  Windows-shaped lanes moved.

## Consequences

- A machine that is merely _busy_ is no longer a machine that produces a red
  gate. The failure mode that motivated this — a gate timing out because four
  agents were compiling at once, reported as a real defect — stops here.
- The default is now a network call. A workspace that is absent, stopped or
  being reimaged costs a resume (about twelve seconds) or, with
  `AGENTQ_CLOUD_STRICT=1`, a clear failure; without it, the run falls back here
  and the summary says which machine each lane used, so the fallback is visible
  rather than implied.
- Every job cleans the workspace tree before it runs, so a lane cannot depend on
  something another job left behind: the toolchains must be outside, and a helper
  script that has to be there belongs in `scripts/`, not in `.copilot-tmp`
  (which is ignored and therefore never synchronised).
- The android lane is the expensive one to get right: `compileSdk = 37` resolves
  to `platforms/android-37.0` (API 37 ships minor releases, there is no bare
  `-37`), and the clipboard tool's package syntax is now `platforms/android-37.0`
  rather than `platforms;android-37`. Provisioning that is part of this decision
  because a lane that cannot build is worth less than no lane at all.
- Reversing it is one argument: `-Where local`, or `AGENTQ_WHERE=local` for the
  whole shell.
