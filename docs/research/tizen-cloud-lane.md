> Status: research snapshot, 2026-11-25. Not normative — see `docs/TRACKER.md`
> and the ADRs for decisions. Read `AGENTS.md` first; it overrides anything
> below. Read-only research: nothing was edited, staged, built, installed or
> deployed, and no cloud job was submitted.

# A devcloud lane for the Tizen `.wgt` — can it be built and packaged on `ws-vitals-win`?

**Question.** The owner's standing rule is that heavy and platform builds run in
the cloud, never on the shared workstation; `scripts/e2e-cloud.ps1` and
`scripts/dev-server-cloud.ps1` are the two existing lanes. The Samsung Tizen app
(`apps/tizen`) is the missing one. Can it be built _and packaged_ on the devcloud
Windows workspace `ws-vitals-win`, and if so what is the exact recipe?

## Short answer

**Split the question, because the two halves have different answers.**

- **Building the bundle** (`tsc --noEmit && vite build` → `apps/tizen/dist`) needs
  only Node. The workspace already has Node 24 and already runs exactly this
  command in CI. This half is **trivially cloud-capable** and needs no
  provisioning at all. (VERIFIED)
- **Packaging the signed `.wgt`** (`tizen package -t wgt -s mixai-samsung …`) needs
  **Tizen Studio** (`tizen.bat` + `sdb.exe`) and the **`mixai-samsung` signing
  profile** (a `profiles.xml`, a keystore `.p12` and its password file) — all of
  which live in `%USERPROFILE%\tizen-studio*` on **this workstation** and are
  **provisioned nowhere on the workspace**. Whether Tizen Studio installs
  unattended on the workspace, and whether the profile can be placed there
  without an interactive Samsung-account step, **could not be verified from
  here**. (VERIFIED that they are absent; NOT VERIFIED that they can be added.)

So the honest recommendation is **conditional**: the two halves are worth
different lanes, and only the packaging half needs a probe first. Details and the
single proving command are at the end.

---

## 1. What `apps/tizen` is and what it takes to package it

`apps/tizen` is a Tizen **web app** — React 19 + i18next + `@vitals/client` — a
single classic script that targets Chromium 94 and is served from `file://` once
packaged. It is not a native app and it is not a store app.

- **The widget manifest** is `apps/tizen/public/config.xml` (copied into `dist` by
  Vite because it lives in `public/`). It declares the package:
  - `id="…/tv" version="1.4.0"` and `<tizen:application id="VitalsTv01.Vitals"
package="VitalsTv01" required_version="7.0"/>` — the comment at
    `apps/tizen/public/config.xml:7` records that the package id must be exactly
    **ten alphanumeric characters**; `VitalsTv01` is. (VERIFIED)
  - `<tizen:profile name="tv-samsung"/>` (`config.xml:11`) and the `internet` +
    `tv.inputdevice` privileges. (VERIFIED)
- **The build scripts** are in `apps/tizen/package.json`:
  - `build`: `tsc --noEmit && vite build` → `apps/tizen/dist` (`package.json:8`).
  - `build:wgt`: `pnpm build && pwsh -NoProfile -File scripts/tv-deploy.ps1 -NoInstall`
    (`package.json:9`) — the bundle, then the signing step. (VERIFIED)
- **The packaging script** is `apps/tizen/scripts/tv-deploy.ps1`. It is the only
  place that knows the exact prerequisites. (VERIFIED, read in full)

## 2. What packaging actually needs, and which parts are one-time

`tv-deploy.ps1` names every prerequisite itself:

| Prerequisite                                                                         | Where it is named                                                                                                | One-time or per-build? |
| ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- | ---------------------- |
| Tizen Studio CLI `tizen.bat` at `%USERPROFILE%\tizen-studio\tools\ide\bin\tizen.bat` | `tv-deploy.ps1:35`, asserted at `:37` (`throw "missing $tool — install Tizen Studio"`)                           | **One-time** provision |
| `sdb.exe` at `%USERPROFILE%\tizen-studio\tools\sdb.exe`                              | `tv-deploy.ps1:36-37`                                                                                            | **One-time** provision |
| Signing profile `mixai-samsung`                                                      | `tv-deploy.ps1:28` (`$CertProfile = 'mixai-samsung'`), used at `:56`                                             | **One-time** provision |
| The signed output                                                                    | `tizen package -t wgt -s $CertProfile -o $out -- $stage` (`:56`), copied to `apps/tizen/dist/Vitals.wgt` (`:72`) | **Per-build**          |

Two facts that make this cheaper than it looks:

- **`-NoInstall` never touches a device.** The `-NoInstall` path (`tv-deploy.ps1:66-74`)
  packages only: no `sdb connect`, no install, no launch. The distributor
  certificate names a **device** DUID, but signing does not need that device
  present — a `.wgt` can be produced on any machine holding the profile. This is
  the path CI uses, and the path a cloud lane would use. (VERIFIED from the
  script; the DUID/per-device claim is VERIFIED in `docs/distribution.md:135-138`.)
- **`tizen package` signs in place**, so the script stages from a TEMP copy of
  `dist` and never stages the `.wgt` back into it (`tv-deploy.ps1:6-8`, `:52-54`).
  A cloud job that cleans the tree is unaffected. (VERIFIED)

The prerequisites divide cleanly into what is **static install output** and what is
**a secret**:

- Static, copyable, not secret: `%USERPROFILE%\tizen-studio\` (the CLI + sdb, plus
  a bundled JDK) and `%USERPROFILE%\tizen-studio-data\profile\profiles.xml`.
- **A secret:** `%USERPROFILE%\tizen-studio-data\keystore\mixai_author.p12` and
  its sibling `mixai_author.pwd` (the keystore password). These are the author
  certificate and its passphrase and must never be committed or echoed
  (VERIFIED that they exist on the workstation by listing the directory; their
  contents were **not** read).

## 3. Whether the workspace can host it

The workspace is reached **only** through `~/.copilot/bin/agentq-cloud.ps1`; there
is no interactive session. The runner's own header documents the contract
(VERIFIED, `agentq-cloud.ps1:1-33`): it uploads a git bundle + working-tree patch,
queues one job against `ws-vitals-win`, streams `jobs/<id>/log`, and returns the
**remote exit code**. A lane therefore has to be an argv array submitted as one
job — exactly the shape `e2e-cloud.ps1` and `dev-server-cloud.ps1` already use.

What the workspace has and does not have:

- **It has Node 24 and Rust.** `ws-vitals-win` is registered in the runner
  (`agentq-cloud.ps1`, `$Registry['vitals-win']` → project `codai-training`, zone
  `europe-west4-a`; VM `ws-vitals-win`), and `.devcloud/profile.json` lists every
  lane it runs — `checks`, `ts`, `rust`, `linux`, `android`. **There is no `tizen`
  lane.** (VERIFIED)
- **It does not have Tizen Studio.** The one provisioning script for the workspace
  is `scripts/ci/provision-cloud-workspace.ps1`, and it installs only `jdk-21` and
  the Android SDK under `C:\devcloud-tools` (`provision-cloud-workspace.ps1:1-32`).
  No script anywhere provisions Tizen Studio or a Tizen profile —
  `rg -n -i tizen --glob '!node_modules'` finds Tizen references only in
  `apps/tizen`, `scripts/check-drift.ps1` (locale parity), `scripts/version.ps1`
  (the version string), the docs, and `.github/workflows/ci.yml`. (VERIFIED)
- **A toolchain outside the repo survives the job's clean.**
  `docs/devcloud.md` records that the job runs `git clean -ffdx` before every run,
  keeping only `node_modules`, `target`, `.gradle`, `.turbo`, `.pnpm-store`, and
  that toolchains therefore live under `C:\devcloud-tools`. Tizen Studio's default
  install root is `%USERPROFILE%\tizen-studio`, also outside the checkout, so it
  **would** survive a clean — but it is not there today. (VERIFIED for the clean
  behaviour from the docs; the install-root survival is EXPECTED.)

**What could not be verified from here, and would settle on the workspace:**

1. **Does Tizen Studio install unattended?** The Tizen Studio installer is a
   GUI Java application; there is a CLI package-manager path in recent versions
   but it has not been exercised here. Whether it needs a licence click or an
   interactive prompt is **NOT VERIFIED**.
2. **Can the profile be placed without a Samsung account on the workspace?** The
   `mixai-samsung` profile was created once, on this workstation, through Tizen
   Studio's Certificate Manager, which signs into a Samsung account. If the
   resulting `profiles.xml` + keystore are simply **files**, copying them to the
   workspace's `%USERPROFILE%\tizen-studio-data\` is enough and no Samsung login
   is needed at build time. Whether the CLI accepts a copied profile without a
   first-run login is **NOT VERIFIED**.
3. **Is the keystore password deliverable without leaking it?** `mixai_author.pwd`
   is a secret. The lane needs a provisioning step that writes it onto the
   workspace (e.g. into `%USERPROFILE%\tizen-studio-data\keystore\`) from a source
   outside git — the runner's job arguments are streamed into a log, so a password
   must **not** travel as a command-line argument. Whether the Tizen CLI can read
   the password from the profile alone (no separate prompt) is **NOT VERIFIED**.

None of the three is a code question; all three are answered by one job on the
workspace (the proving command is at the end).

## 4. What the lane's pass condition should be

The repository's rule is that a gate decides on **content**, not on an exit code —
`scripts/check-size.ps1` fails over measured bytes, and `apps/tizen` already names
the artefact. So a Tizen lane must not pass on `tizen package` returning 0. It
should:

1. **Assert the artefact exists** at `apps/tizen/dist/Vitals.wgt` (the name
   `tv-deploy.ps1:72` copies to) and is **non-empty**.
2. **Assert it is a signed package, not a zip.** `ci.yml:401-421` makes the
   distinction explicit: a hosted runner with no Tizen Studio falls back to
   `Compress-Archive` and produces `Vitals-unsigned.wgt`, which "a TV will not
   install". A signed `.wgt` is a zip whose root carries `author-signature.xml`
   and `signature1.xml`; the lane should open the archive and fail if those are
   absent. (EXPECTED — from Tizen's packaging model; the file names should be
   confirmed against the first real signed output.)
3. **Print and record the SHA-256** of the `.wgt` — the durable evidence a later
   reader cites, mirroring `docs/devcloud.md`'s rule that a run's result belongs
   in `docs/TRACKER.md`, not only in `runs/<id>.log`. This also makes "the
   workspace produced the same bytes as the workstation" a checkable claim once a
   local baseline hash exists.

## 5. Cost, and whether the lane is worth having

- **Building the bundle:** seconds. This already runs in CI on every change to
  `apps/tizen` (`ci.yml:378-399`).
- **Packaging, once provisioned:** EXPECTED ~30-90 s warm (a JVM start plus a zip
  and sign); the CI job's budget is 15 minutes (`ci.yml:383`), generous because it
  is a hosted runner. NOT VERIFIED on the workspace — no packaging has run there.
- **One-time provisioning:** EXPECTED 10-20 min and roughly 1-2 GB for Tizen
  Studio plus its bundled JDK. NOT VERIFIED.

**Where the `.wgt` is produced and shipped today.** Not in `release.yml` — a
grep of `.github/workflows/` for `wgt|tizen` matches **only `ci.yml`**, and
`docs/distribution.md:143-145` states the opposite of a release artefact: "the
Tizen app is **not** a downloadable artefact in a release: `release.yml` does not
carry it and the download page does not offer it. It is built and installed by
hand, on the owner's machine." (VERIFIED). The only automation that produces a
`.wgt` is the CI `tizen` job (`ci.yml:407-431`), which builds a **signed** `.wgt`
only where Tizen Studio happens to be present and an **unsigned** one otherwise.

That changes the cost/benefit. A packaging lane on **every change** would buy
little: the signed package is sideload-only, one device, not a release artefact,
and the bundle is already compiled in CI. A packaging lane is worth having only
if it replaces the _manual, on-the-workstation_ packaging step that
`docs/distribution.md:158-164` still describes — i.e. at **release / on demand**,
not on every push.

---

## What is not verified, in one place

- **NOT VERIFIED** — that Tizen Studio installs unattended on `ws-vitals-win`.
- **NOT VERIFIED** — that the `mixai-samsung` profile (copyable files) is
  accepted by the CLI on the workspace with no interactive Samsung login.
- **NOT VERIFIED** — that the keystore password can be supplied without a
  command-line argument (which the runner streams into the log).
- **NOT VERIFIED** — warm-vs-cold packaging times on the workspace; a signed
  `.wgt` has never been produced there.
- **NOT VERIFIED** — the exact signature file names inside a signed `.wgt`
  (`author-signature.xml`, `signature1.xml` are EXPECTED, not read).
- **ABSENT** — the two memory notes named in the brief,
  `/memories/repo/vitals-tizen-tv-facts.md` and
  `/memories/repo/vitals-tizen-tv.md`, **do not exist** in this machine's memory
  store (searched the memory root and `.codai`). Their content could not be
  taken into account.
- **VERIFIED and current** — Tizen Studio _is_ installed on **this workstation**:
  `%USERPROFILE%\tizen-studio\tools\ide\bin\tizen.bat` and
  `%USERPROFILE%\tizen-studio\tools\sdb.exe` both exist, and
  `%USERPROFILE%\tizen-studio-data\` holds `profile\profiles.xml` (which names
  `mixai-samsung`), `keystore\mixai_author.p12` and `keystore\mixai_author.pwd`.

## Recommendation

**Build the lane — but only the packaging half, gated behind a one-job probe, and
do not wire it onto every change.**

Rationale: the split above is real. The **bundle** is already built in CI and
needs no lane. The **signed `.wgt`** is the one step that today pins itself to the
shared workstation (`docs/distribution.md:158-164`), and it is buildable in
principle anywhere that has Tizen Studio and the profile — no TV is required for
`-NoInstall`. But it is **not proven** on `ws-vitals-win`, and the three unknowns
in §3 are all a single job away from being settled. Do that job before writing the
lane, not after.

### The single command that proves it

Submit one job to the workspace that asks the three questions at once — is Tizen
Studio present or installable, does the profile load, does the CLI package — and
prints the artefact hash. The narrowest form that answers the load-bearing
question (is the toolchain there at all) is:

```powershell
pwsh -NoProfile -File "$env:USERPROFILE\.copilot\bin\agentq-cloud.ps1" `
  -Ws vitals-win -Purpose 'probe Tizen Studio + profile on the workspace' `
  -Lane gate -TimeoutMin 40 -Repo 'E:\gh\vitals' -- `
  pwsh -NoProfile -Command `
  '$t = Join-Path $env:USERPROFILE "tizen-studio\tools\ide\bin\tizen.bat"; "tizen.bat: $(Test-Path $t)"; if (Test-Path $t) { & $t version; & $t security-profiles list }'
```

If that returns `tizen.bat: True` and a version and a profile list, provisioning
is only a **copy** step and the lane is trivial. If it returns `False`, the next
probe must attempt the unattended install — which is exactly unknown #1 above.

### The exact provisioning steps (run once, on the workspace, outside the repo)

They mirror `scripts/ci/provision-cloud-workspace.ps1`, which already establishes
the pattern: install outside the checkout, under `%USERPROFILE%` or
`C:\devcloud-tools`, so `git clean -ffdx` cannot remove them.

1. **Install Tizen Studio** to `%USERPROFILE%\tizen-studio` (unattended — the
   unknown to settle) so `tools\ide\bin\tizen.bat` and `tools\sdb.exe` exist,
   matching `tv-deploy.ps1:35-36`.
2. **Copy the profile, not the account.** Copy this workstation's
   `%USERPROFILE%\tizen-studio-data\profile\profiles.xml` to the same path on the
   workspace.
3. **Deliver the keystore as a secret, never as an argument.** Place
   `mixai_author.p12` and `mixai_author.pwd` in
   `%USERPROFILE%\tizen-studio-data\keystore\` from a source outside git (the
   runner streams job argv into the log, so the password must not appear there).
   The job's clean does not reach `%USERPROFILE%`, so this survives.
4. **Idempotent, like the existing provisioner:** each step is skipped when its
   result is already present, so a second run costs seconds
   (`provision-cloud-workspace.ps1:1-28`).

### The lane's argv (the shape of `e2e-cloud.ps1` / `dev-server-cloud.ps1`)

Follow the two existing lanes exactly: check for `agentq-cloud.ps1`, build the
remote command as an **array** (never a joined string), honour `-DryRun`, submit
as one job via a child process so the remote log is capturable and the exit code
is ours (`e2e-cloud.ps1:100-140`, `dev-server-cloud.ps1:70-95`).

```powershell
# scripts/tizen-cloud.ps1  (new script, modelled on scripts/dev-server-cloud.ps1)
$packageArgs = @('pwsh', '-NoProfile', '-File', 'scripts/tizen-package.ps1')
# tizen-package.ps1 runs, in order:
#   pnpm --filter @vitals/tizen build
#   pwsh -NoProfile -File apps/tizen/scripts/tv-deploy.ps1 -NoInstall
#   assert apps/tizen/dist/Vitals.wgt is non-empty and carries the signature XML
#   Get-FileHash apps/tizen/dist/Vitals.wgt -Algorithm SHA256   # the content gate
```

Submitted as one job, mirroring the existing runners verbatim:

```powershell
$argv = @('-Ws', $Ws, '-Purpose', 'package the Tizen .wgt',
          '-Lane', 'gate', '-TimeoutMin', $TimeoutMin, '-Repo', $root, '--') + $packageArgs
```

A **release-time-only** lane is the right default: wire it so a release calls it
and it is not on the per-push path. That keeps the workstation free (the owner's
rule) without paying a per-change cost for a sideload-only, single-device artefact
that CI already compiles.
