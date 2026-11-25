# Running a gate on the devcloud

The workstation is shared: several agents build and test in it at the same
time, and a full `verify.ps1` there is twenty minutes of it. The Windows build
workspace `ws-vitals-win` is the same repository on a dedicated machine, so the
heavy gates run there by default and this machine stays free. `-Where local` is
always available, and so is `AGENTQ_WHERE=local` for everything at once.

Measured 2026-11-25 on `ws-vitals-win` (16 vCPU), with the working tree synced:

| lane                         | what it costs            | where it ran before    |
| ---------------------------- | ------------------------ | ---------------------- |
| `checks`                     | seconds                  | here                   |
| `rust` (clippy, tests, perf) | ~2 min cold              | here                   |
| `ts` (turbo + bundle)        | ~2 min cold              | here                   |
| `android` (three APKs)       | ~6 min cold, ~1 min warm | here                   |
| `linux` (CI's clippy)        | ~1 min                   | `ws-codai` (unchanged) |

## The short version

```powershell
# everything, on the devcloud, falling back to this machine if it is unreachable
pwsh -NoProfile -File scripts/verify.ps1

# one red lane, without waiting for the others
pwsh -NoProfile -File scripts/verify.ps1 -Lane rust

# everything here (what the script did before the devcloud existed)
pwsh -NoProfile -File scripts/verify.ps1 -Where local

# an arbitrary command, through the queue
pwsh -NoProfile -File "$env:USERPROFILE\.copilot\bin\agentq.ps1" run `
  -Resource build -Where cloud -Ws vitals-win -Purpose 'why' -- pnpm typecheck
```

`-Where auto`, the default, means cloud when agentq can reach the workspace and
this process is not already holding the build ticket; `AGENTQ_WHERE=local`
overrides it everywhere. `AGENTQ_CLOUD_STRICT=1` turns the fallback off, so a
run that silently happened here cannot be mistaken for a run that did not.

## The end-to-end suite

The Playwright suite in `apps/desktop/e2e/` drives a real Chromium, so it belongs
on the devcloud with the other heavy gates and never on the shared workstation:

```powershell
pwsh -NoProfile -File scripts/e2e-cloud.ps1 -InstallBrowsers   # first run on a workspace
pwsh -NoProfile -File scripts/e2e-cloud.ps1                    # the suite
pwsh -NoProfile -File scripts/e2e-cloud.ps1 -Spec e2e/a11y.spec.ts -Project chromium
```

It submits `pnpm --filter @vitals/desktop exec playwright test` as one job to
`ws-vitals-win` — the same command the repository runs locally — and streams the
log back, so the exit code is the suite's. The HTML report is left on the
workspace in the checkout's `apps/desktop/e2e-report/`. Chromium is not installed
on a fresh workspace, so the first run needs `-InstallBrowsers`, which installs it
as its own step.

## Why a lane is a job, not a copy of the script

A cloud lane is `verify.ps1 -Lane <name> -Where local` executed inside the job.
The gate definitions therefore exist once, and the only thing that crosses the
machine boundary is a name. The lane prints one line per gate:

```
LANE_RESULT|rust|rust: clippy|pass|10.6
```

and the caller rebuilds the summary table from those lines. A lane that never
reached a gate — a failed sync, a missing toolchain, a timeout — reports one
row saying so rather than disappearing from the table.

## What the workspace has to carry

The job cleans the tree (`git clean -ffdx`) before every run, excluding
`node_modules`, `.turbo`, `target`, `.gradle` and the pnpm store, so builds are
warm but nothing you left in the repository survives. Toolchains therefore live
outside it, under `C:\devcloud-tools`:

- `jdk-21` — Temurin, for Gradle
- `android-sdk` — `platform-tools`, `platforms/android-37.0`,
  `build-tools/37.0.0`
- `env.ps1` — `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `PATH`

One command creates all of it, and does nothing the second time:

```powershell
pwsh -NoProfile -File scripts/ci/provision-cloud-workspace.ps1 [-Check]
```

`cargo-deny` is installed with `cargo install cargo-deny --locked` (the same
tool the supply-chain job runs) and is on the PATH already.

`verify.ps1` sources `env.ps1` when `ANDROID_HOME` is unset, because a process
does not see an environment variable that was set after it started — which the
job's own shell, started by the workspace agent service, is.

## Traps, with the symptom that names them

- **sdk-style package paths.** The command-line tools are now a shim over the
  new `android` CLI, whose syntax is `platforms/android-37.0`, not
  `platforms;android-37`. The old form arrives as two names and the tool
  answers `Package platforms not found / Package android-37 not found`, which
  reads like a broken index rather than a separator that changed.
- **There is no `platforms/android-37`.** API 37 is published as minor
  releases: the index lists `android-37.0`, `-37.1`, `-37.2`. Installing the
  base one puts `platforms/android-37.0` on disk, and Gradle is the judge of
  whether that is what `compileSdk = 37` asked for — which is why the android
  lane is proved with a real assemble and not by the provisioning script.
- **`.copilot-tmp` does not reach the workspace.** The synchroniser sends
  tracked files plus untracked ones; ignored ones are not in the patch. A
  helper script that has to run remotely belongs in `scripts/`.
- **`cmd` re-tokenizes a batch file's arguments.** A package name containing
  `;` passed through PowerShell to a `.bat` can arrive split. Quoting it in
  PowerShell does not help (it does not quote a string with no space); the fix
  is a form where the separator is not a cmd delimiter at all.
- **A lane that skips is not a lane that passed.** `check-android-features.ps1`
  answers "nothing to check" on a machine without the SDK, which is honest
  locally and a false pass on a workspace that is supposed to have one. The
  cloud submission passes `-RequireAndroid`, which makes the missing toolchain
  a red gate that names itself. The lane itself is on by default
  (`-AndroidCloud on`): the workspace now has the JDK and the SDK, it was
  measured there on 2026-11-25 (assemble 55 s, five APKs, zero failures), and
  `off` is what an unprovisioned workspace gets.
- **A gate that ends on a closed pipe ends when someone else says so.** The
  obvious way to run a gate from the lane is to invoke its script block and
  collect the pipeline. That pipeline ends on EOF, and a toolchain daemon
  outlives its gate while holding the handles it inherited — the same daemon as
  the trap below, one level up. Measured 2026-11-25: a gate that finished in
  0.2 s left a 40 s grandchild behind and the lane returned after 41 s; on the
  devcloud that is a job that never reports, because the runner waits for the
  lane and the lane waits for a daemon doing nothing. Seven android jobs were
  abandoned and the workspace was reset three times before the cause was found,
  and nothing in the job's own output said which side was stuck. Each gate now
  runs as its own process with its output in files and a 30-minute deadline, and
  the lane waits for that process.
- **A redirect makes a build invisible while it runs.** `check-android-features`
  writes the assemble to a file, so for eleven minutes on 2026-11-25 the only
  external signal of a hung build was the instance's CPU. Anything that can block
  for minutes now says where it is: `scripts/lib/devcloud-progress.ps1` appends a
  timestamped line per phase to `jobs/<id>/stage` in the jobs bucket, and both
  the lane and the gate use it, so one timeline covers the job instead of one
  file per script.
- **A job that starts a daemon never reported an exit code.** The workspace
  agent ended a step when its process exited _and_ its output pipes closed.
  Gradle's daemon is started by the client and outlives it by hours, so the pipe
  stayed open, the step never settled and no `result.json` was ever written: the
  job looked hung while it was in fact finished. Three android jobs were
  abandoned and the workspace reset twice on 2026-11-25 before the last one made
  it visible — its own log said `assemble exit=0`, and there was no result. The
  agent now settles on process exit with half a second to drain what is still in
  the pipe (agentq commit `51308e8`), because a daemon holding a handle to
  a reader that has stopped caring is the toolchain's business and killing the
  daemon to make a bookkeeping event fire would trade a hang for a rebuilt JVM.
  The lesson is not "redirect your build": it is that a step's end has to be
  defined by something the step controls.
- **The agent cannot be reached by ssh on Windows, and does not need to be.** It
  compares itself with the instance metadata key `runner-agent` every minute
  while idle and adopts a newer copy, so a fix is a metadata write plus a
  minute's patience: `gcloud compute instances add-metadata ws-vitals-win
--metadata-from-file runner-agent=…`. The comparison trims both sides, since
  whether the value carries the file's final newline is gcloud's business and a
  one-newline difference used to restart the agent every minute for ever.

## Where a lane says what it needs

Every lane this repository runs is declared in `.devcloud/profile.json`, and the
schema plus its validator are owned by the repository that owns the images:

```powershell
node E:\gh\vmui\infra\devcloud\profile\validate.mjs --root E:\gh --live
```

`--live` asks GCE which families exist, so the answer is "this lane wants a JDK
and there is a baked family for it", not "there should be one somewhere".

The file describes **needs** - toolchains, machine class, how many jobs may share
the machine, how long the lane may take - and never a VM, a zone or a project:
those are placement results, and a lane has to be able to land on a machine that
did not exist when this file was written. Two fields carry a reason: `dedicated`
and `concurrency: 1` are claims about interference, and the one thing a reader
six months from now cannot re-derive from the numbers is why. The android lane's
`why` is the exception that proves it exists - the Gradle daemon outlives its
client and holds the job's stdio open, which is what stopped this lane's job
from ever reporting an exit code on 2026-11-25.

**Honest state of the wiring.** agentq reads its own `cloudWorkspaces` table
today, so this profile is validated and reviewed but not yet what places a job;
the placement that consumes it is vmui ADR-0004's point 3 and is not implemented.
What `verify.ps1` does consume is each lane's `timeoutMin`: the flag that governs
a lane's gates, and the wait the caller gives the queue, are both that number, so
a lane whose work is longer than a placement's default deadline is not killed by
its own runner. Placement — which lane goes to which machine — is still agentq's.
The validator is real: `node validate.mjs --selftest` proves it rejects an
unbaked toolchain, a missing `repo`, a `concurrency: 1` with no reason, and a
field from a future schema version.
