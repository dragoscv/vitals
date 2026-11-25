> Status: research snapshot, 2026-11-25. Not normative — see `docs/TRACKER.md`
> and the ADRs for decisions. Read `AGENTS.md` first; it overrides anything
> below. Read-only research: nothing was edited, staged, built, installed or
> deployed, no cloud job was submitted, and the macOS VM was **not** started.

# Can the Apple half be built, archived or validated off this workstation?

**Question.** The owner's standing rule is that heavy and platform builds run off
the shared workstation — the Windows lanes are `scripts/e2e-cloud.ps1` and
`scripts/dev-server-cloud.ps1`, both of which submit one job to the devcloud
Windows workspace `ws-vitals-win` through `~/.copilot/bin/agentq-cloud.ps1`. Is
the **Apple** half (the iPhone app in `apps/ios`, and the macOS desktop edition)
buildable, archivable or validatable the same way — and if so, what is the exact
recipe and the exact cost and blockers?

## Short answer

**The Apple half is already off the workstation, in two independent ways, and
both are proven. There is no lane to build.**

- **Validation (CI) runs on GitHub-hosted `macos-latest` runners**, not here and
  not on the devcloud workspace: `ci.yml`'s `apple` job (clippy + tests for the
  macOS crates and both sampler flavours, the contract fixtures, the Swift tests
  and an unsigned iPhone compile), plus two screenshot workflows
  (`ios-screenshots.yml`, `macos-screenshots.yml`). (VERIFIED — read in full)
- **Signed builds and store uploads run on a dedicated macOS VM**, `vmui-mac`
  (GCE, OSX-KVM), driven **from Windows** by `scripts/ios-build.ps1` and
  `scripts/macos-appstore.ps1` over Tailscale SSH. This path produced the live
  TestFlight build: `ios-build upload @ 05f41f2d8bfb (build 326) -> rc=0`, build
  326 `VALID`, version 1.3.0 `WAITING_FOR_REVIEW`, recorded in `docs/TRACKER.md`
  on 2026-11-18. (VERIFIED — read in the tracker)
- **The only Apple-shaped blocker is Apple's own review**, not any build
  capability: row S23-16 (the Mac App Store submission) is `blocked` on a
  guideline 4.3 / 4.2.6 rejection, and the 1.3.1 tag is held for the same reason
  (`docs/TRACKER.md`, S23-16 and the "Still blocked" entry). No build recipe
  changes that. (VERIFIED — read in the tracker)

So the honest answer to "is an Apple lane worth building" is **not to build one**.
What is genuinely missing is small and optional: the `vmui-mac` lane is a
hand-rolled SSH driver, not an agentq-queued lane like the two Windows ones, and
it lacks three conveniences the others have. Those are listed under
RECOMMENDATION, and none of them is required for the lane to work.

---

## 1. What the repository already does for Apple

The Apple files divide into three groups by where they can run. The column that
matters is the middle one.

| File                                              | Runs on                      | What it needs                                                                     |
| ------------------------------------------------- | ---------------------------- | --------------------------------------------------------------------------------- |
| `scripts/ios-build.ps1` (153 lines)               | **Windows**                  | `git`, `ssh`, `scp`, `gcloud` on this machine; the real work is sent to the guest |
| `scripts/macos-appstore.ps1` (104 lines)          | **Windows**                  | same                                                                              |
| `scripts/ios-appstore-profiles.mjs`               | **Windows**                  | Node + `~/.copilot/bin/apple-asc.mjs` → App Store Connect REST                    |
| `scripts/macos/appstore-profile.mjs`              | **Windows**                  | same                                                                              |
| `scripts/ios-store.mjs` (+ `scripts/lib/asc.mjs`) | **Windows**                  | Node + ASC REST (repo-local client, so CI can run it)                             |
| `scripts/ios-screenshots-upload.mjs`              | **Windows**                  | Node + `~/.copilot/bin/apple-asc.mjs` → ASC REST                                  |
| `scripts/ios-strings.mjs`                         | **Windows**                  | Node only (reads Android `strings.xml`)                                           |
| `scripts/ios-sample.mjs`                          | **Windows**                  | Node only                                                                         |
| `scripts/check-apple-hold.mjs`                    | **Windows**                  | Node only, fully offline                                                          |
| `apps/ios/scripts/build.sh`                       | **macOS only**               | bash, `xcodebuild`, `xcodegen`, `security`, `swift`                               |
| `apps/ios/scripts/screenshots.sh`                 | **macOS only + GPU**         | `xcodebuild`, `simctl`                                                            |
| `scripts/macos/build-appstore.sh`                 | **macOS only**               | `security`, `codesign`, `productbuild`, `pnpm`/`tauri`, `altool`                  |
| `scripts/macos/sign-notarise.sh`                  | **macOS only**               | `codesign`, `notarytool`, `stapler`, `hdiutil`, `spctl`                           |
| `scripts/macos/appstore-screenshots.sh`           | **macOS only + GUI session** | `tauri build`, window capture                                                     |

Reading them settled the two questions the brief asks:

- **The two drivers run on Windows.** `ios-build.ps1` ships the **committed**
  tree (`git archive <sha>:apps/ios`, never the dirty working tree) to the guest
  and runs `apps/ios/scripts/build.sh <mode>` there; `macos-appstore.ps1` does the
  same with `scripts/macos/build-appstore.sh`. Everything secret is read from
  Secret Manager on this machine, copied to the guest as a `0600` file, and
  deleted in the same remote command. Nothing secret is printed. (VERIFIED)
- **The four `.mjs` API scripts and `check-apple-hold.mjs` work from anywhere.**
  `ios-store.mjs`, `ios-screenshots-upload.mjs`, `ios-appstore-profiles.mjs` and
  `appstore-profile.mjs` only speak the App Store Connect REST API; they need the
  team key and nothing macOS-specific. `ios-strings.mjs`, `ios-sample.mjs` and
  `check-apple-hold.mjs` are pure Node over files in the tree — indeed
  `check-apple-hold.mjs` is already a gate in `scripts/verify.ps1:233`
  (`apple: listing held`), and it runs on this Windows machine today. (VERIFIED)

**The `apps/ios` tree itself splits too.** The Swift sources and
`VitalsCore/Package.swift` are portable source; `project.yml` is an XcodeGen
spec whose `.xcodeproj` is **generated on the Mac and never committed**; and the
only things that need Xcode are compile, sign, archive and the Simulator.

### What CI already validates, and where

`.github/workflows/ci.yml` has an `apple` job (`runs-on: macos-latest`,
`ci.yml:271-322`) that runs, in order: clippy over `vitals-core`, `vitals-macos`,
`vitals-server`, `vitals-cli`; `cargo test -p vitals-macos -p vitals-core`; the
same for `--features app-store`; clippy+test of the desktop crate in its Mac App
Store flavour; the Rust contract-fixture regeneration with
`git diff --exit-code`; `bash apps/ios/scripts/build.sh test` (Swift tests); and
`bash apps/ios/scripts/build.sh build` (an unsigned `iphoneos` compile that
asserts the Watch app is embedded). **This one job proves the port cannot rot,
on every push, on a hosted Mac, with nothing installed here.** (VERIFIED)

Two more hosted jobs cover the parts a GPU-less VM cannot: `ios-screenshots.yml`
and `macos-screenshots.yml`, both `runs-on: macos-latest`. Their own comments say
why — "the devcloud Mac has no GPU and iOS 26 then draws the glass tab bar with
only its selected item" (`ios-screenshots.yml:4-8`) and "vmui-mac has no GPU and
no console user, so it cannot capture" (`scripts/macos/appstore-screenshots.sh`).
(VERIFIED — two independent statements)

### What the release workflow does, and does not, do for Apple

`release.yml` reaches Apple only for **macOS**:

- a `build` leg `macos-universal` (`runner: macos-latest`) that produces an
  **unsigned** `Vitals_<v>_universal.app.zip` and attaches it to nothing;
- a tag-only job **`macos-sign`** (`release.yml:708-757`, `runs-on: macos-latest`,
  the `release` environment, only when `PUBLISH_MACOS` is `true`) that runs
  `scripts/macos/sign-notarise.sh` to Developer ID-sign, notarise, staple and
  build the DMG and the updater tarball.

**There is no iOS job in `release.yml` at all.** The iPhone app is uploaded out of
band, by hand, through `scripts/ios-build.ps1 … -Mode upload`. (VERIFIED — grepped
`release.yml` for `macos|xcode|notar`; the only iOS references are the three
repository-level `APPLE_ASC_*` secrets used by a read-only listing check.)

---

## 2. Where macOS is available to this owner

Two places, and the repository names both.

### `vmui-mac` — the owner's macOS VM on GCE (the signed-build host)

The header of `E:\gh\vmui\infra\devcloud\mac\mac-gce.ps1` (the script that owns
the VM) documents it in full: a **`c4-standard-16`** GCE instance in project
`dragos-devcloud`, zone `europe-central2-a`, running **Ubuntu 24.04 + OSX-KVM
(QEMU) with a macOS guest**, "**Owner only (ADR-0003); never offered to users**",
on demand, with a 300 GB `hyperdisk-balanced` data disk holding the qcow2 chain.
After 15 idle minutes the host suspend itself with macOS still running (resume
~22 s). (VERIFIED — read in `mac-gce.ps1`)

It is reached two ways (from `mac-gce.ps1` and the shared runbook):

- **on the tailnet** — `ssh dragos@vmui-mac.taild1532d.ts.net -p 10022`
  (or the `100.106.244.19` address);
- **over IAP** — `mac-gce.ps1 -Tunnel`, which forwards `10022/5900/5901/5930` to
  localhost.

`scripts/ios-build.ps1` and `scripts/macos-appstore.ps1` hard-code the tailnet
route: `GuestHost 100.106.244.19`, `GuestPort 10022`, `GuestUser dragos`, and the
private key `~/.ssh/id_ed25519_macvm`. `ios-build.ps1` also starts the VM when it
is not `RUNNING` (via `mac-gce.ps1 -Start`) and **refuses to build when the guest
clock is off by more than 120 s** — a dead-on-arrival failure that cost four
attempts on 2026-11-18, because a resumed GCE Mac keeps its old clock and Apple
then answers an opaque `401 … listTeams`. (VERIFIED — read in `ios-build.ps1`)

The canonical, cross-repo description is the shared runbook
**`E:\gh\codai\docs\runbooks\apple-publishing.md`** (229 lines), which
`~/.copilot/bin/apple-asc.mjs` names as its runbook. It records the Apple
account, the team/ASC identifiers, the Secret Manager secret names, the `vmui-mac`
host and Xcode version, and the traps below. **It lives in `codai`, not in
`vitals`** — this repository does not carry its own copy. (VERIFIED — file read;
identifiers deliberately not repeated here)

### GitHub-hosted `macos-latest`

Every CI/validation Apple step (§1) and the release `macos-sign` job run on
GitHub's Mac runners. This is the second macOS source, and it is the one with a
GPU and a console session — which is exactly why the screenshot workflows use it
and `vmui-mac` does not. (VERIFIED)

### What memory says — and a note that is missing

The brief named three memory notes. **All three are absent** from this machine:

- `/memories/repo/vitals-apple-publishing.md` → **not present**
- `/memories/repo/vitals-asc-testflight-facts.md` → **not present**
- `/memories/repo/vitals-mac-apple-4.3-rejection.md` → **not present**

(VERIFIED — `Test-Path` returns `False` for each, and no file whose name starts
with `vitals-` exists anywhere in the memory store.) Their content could not be
taken into account. A fourth note **does** exist and was used instead:
`memories/devcloud-mac.md`, which documents `vmui-mac` (tailnet
`vmui-mac.taild1532d.ts.net`, ports, suspend/resume, the clock trap, the
guest-disk-full trap, and the guest credentials location
`E:\gh\vmui\.private\credentials.env` — **named, not read**). That note is a
memory claim; where this document relies on it, the repository agrees
independently, and it is marked as such below. (VERIFIED that the note exists and
was read; its live facts are re-checked in §4)

---

## 3. What the "cloud Apple lane" concretely is

It already exists in two forms. Neither is the `agentq-cloud` shape of the
Windows lanes, and this is the one structural difference worth recording.

### 3a. The validation lane (CI, hosted macOS)

Nothing to specify — it is `ci.yml`'s `apple` job and the two screenshot
workflows. The pass condition is the job's own steps; the artefact is the job's
uploaded screenshots. This lane needs no host, no key and no SSH. (VERIFIED)

### 3b. The signed-build lane (`vmui-mac`, driven from Windows)

**The exact commands** (this is what `scripts/ios-build.ps1` runs, and the closest
reproduction of them):

```powershell
# place a 0600 file on the guest from Secret Manager, then run build.sh there
ssh -i ~/.ssh/id_ed25519_macvm -p 10022 dragos@100.106.244.19 `
  "cd ~/vitals-ios/<sha> && BUILD_NUMBER=<commit-count> bash scripts/build.sh build"
```

`apps/ios/scripts/build.sh` supports five modes (`test`, `build`, `uitest`,
`upload`, `screenshots`); `ios-build.ps1` exposes the first three plus `upload`.
XcodeGen is **pinned** (`XCODEGEN_VERSION="2.46.0"`, verified by `shasum -a 256`)
and fetched from GitHub on first use, so the generated project is reproducible.
(VERIFIED — read in `build.sh`)

**The pass condition is content, not an exit code** — which is this repository's
rule (`scripts/check-size.ps1` fails over _measured bytes_, and its header says
so in as many words). `build.sh`'s `build` mode ends with:

```bash
[ -d "$APP/Watch/VitalsMonitorWatch.app" ] || { echo "watch app not embedded …" >&2; exit 1; }
```

The watch app rides inside the phone app via "Embed Watch Content"; a build
without it would ship a phone app whose watch half is gone — so the gate asserts
the **embedded** artefact, not merely that `xcodebuild` returned 0. (VERIFIED)

**Where the artefact lands.** On the **guest**, under the per-sha checkout:

```
~/vitals-ios/<sha>/apps/ios/build/dd/Build/Products/Release-iphoneos/VitalsMonitor.app
                                          └── Watch/VitalsMonitorWatch.app
```

`scripts/ios-build.ps1` does **not** copy it back to Windows — the only thing that
returns is the guest log, teed to
`.copilot-tmp\ios-build\<sha>\<mode>.log`. For `upload` mode the real artefact is
the App Store Connect build id, and the tracker records reading it back from the
API rather than trusting the log. (VERIFIED — no `scp -r`/`Copy-Item` in the
driver; the sequence is in `ios-build.ps1`)

The lane detected, on real runs, exactly the class of defect a content gate is
for: the 2026-11-18 upload only succeeded after the four provisioning profiles
were replaced (a stale profile lacked `group.app.vitals.ios`), the distribution
identity was imported at upload time, and the guest's `/System/Volumes/Data` was
freed from 100 % full. Each is recorded with its command output in
`docs/TRACKER.md`. (VERIFIED — read in the tracker)

### 3c. How the two lanes differ from the Windows lanes

|                    | `e2e-cloud.ps1` / `dev-server-cloud.ps1`  | `ios-build.ps1` / `macos-appstore.ps1`                                                           |
| ------------------ | ----------------------------------------- | ------------------------------------------------------------------------------------------------ |
| transport          | `agentq-cloud.ps1` job on `ws-vitals-win` | direct `ssh`/`scp` to `vmui-mac`                                                                 |
| queued             | yes (agentq resource + lane)              | **no**                                                                                           |
| declared as a lane | yes, `.devcloud/profile.json`             | **no `apple` lane exists** (`profile.json` lanes are `checks`, `ts`, `rust`, `android`, `linux`) |
| `-DryRun`          | yes                                       | **no**                                                                                           |
| return value       | remote exit code                          | remote exit code                                                                                 |
| what returns       | log (+ HTML report left on the workspace) | log only                                                                                         |

(VERIFIED — `rg` for `dryrun|agentq|df -|free` in the two Apple drivers returns
nothing; `.devcloud/profile.json` lists five lanes, none Apple; `verify.ps1` has
no Apple/macOS/iOS gate except the offline `check-apple-hold.mjs`.)

---

## 4. Blockers and their cost

### Xcode version and licence

The guest runs **Xcode 26.x / iOS 26 SDK** — the runbook says so, and
`docs/TRACKER.md` records the guest as **Xcode 26.6 (17F113) / macOS 26.7.1** during
the 2026-11-18 work. The Xcode **licence is accepted** in practice, because builds
and archives have run there. **Neither can be confirmed from this Windows
machine**: the guest is a separate machine, its Xcode version is a claim in a
document, and the licence state is on its disk. (Version: EXPECTED, corroborated
by two documents; licence accepted: EXPECTED from the fact that builds succeed —
**NOT VERIFIED** live, and would be settled by `ssh … 'xcodebuild -version;
xcodebuild -checkFirstLaunchStatus'` on the guest.)

One documented trap that _may not_ apply here: Xcode 26 no longer ships the
`metal` toolchain, and any target with a `.metal` source needs
`xcodebuild -downloadComponent MetalToolchain` (~688 MB) — but no `.metal` file
appears anywhere under `apps/ios` in this repository, so vitals' iPhone app does
not need it. (ABSENT from the file tree: VERIFIED; "therefore not needed":
EXPECTED.)

### Signing identities and provisioning profiles — are they machine-bound?

**No, in principle; the design deliberately avoids machine-binding.**

- **iPhone (cloud/manual signing).** `ios-appstore-profiles.mjs` **fetches the
  four `IOS_APP_STORE` profiles from the App Store Connect API at build time** —
  creating or replacing them — and `build.sh` installs them under their UUIDs and
  signs **Manually** against `CODE_SIGN_IDENTITY=Apple Distribution`. A profile
  is explicitly "not a secret, but per-team and expiring", which is why it is
  fetched rather than committed. Nothing here is bound to a machine — it is bound
  to the **team** and the **API key**. (VERIFIED — read in the scripts)
- **macOS.** The Developer ID / Mac Installer Distribution identities travel as
  `.p12` files from Secret Manager and go into a **throwaway keychain created per
  job and deleted at the end** (`build-appstore.sh`, `sign-notarise.sh`). The
  App Store profile is likewise fetched from the API. (VERIFIED)
- **What _is_ local to `vmui-mac`:** the ADR records that the distribution
  certificate's private key "never existed outside vmui-mac and that bundle"
  (ADR-0041), and the shared runbook records that the team's `.p12` certificates
  are not created yet. So the _material_ is transferable by design, but its
  current provenance is that one VM. (VERIFIED as a stated fact; the certificate
  inventory itself is on Apple's side and **NOT VERIFIED** from here.)

### App Store Connect keys

The team's Admin ASC key (`.p8`, key id, issuer, team id) lives in GCP Secret
Manager, project **`dragos-devcloud`**, under the secrets `apple-asc-key-p8`,
`apple-asc-key-id`, `apple-asc-issuer`, `apple-team-id` (plus
`apple-signing-p12-password` and the `.p12` secrets for macOS). Both
`~/.copilot/bin/apple-asc.mjs` and the repo-local `scripts/lib/asc.mjs` resolve
them from Secret Manager or from the `APPLE_ASC_*` environment variables.

**Verified live from this Windows machine today** (values never read): the active
`gcloud` account is `vladulescu.catalin@gmail.com`, `gcloud compute instances
describe vmui-mac` returns `STATUS SUSPENDED / c4-standard-16 /
europe-central2-a`, and `gcloud secrets versions describe latest` reports the
latest version of all four ASC secrets as **`ENABLED`**. So this workstation can
already drive both the ASC REST API and the VM's control plane. (VERIFIED —
command output, 2026-11-25; no secret value, key id or token was printed)

### Cost of the macOS host

**EXPECTED, not read from the repository** — `mac-gce.ps1` records the shape
(`c4-standard-16`, 300 GB `hyperdisk-balanced`, `europe-central2-a`) but no price:

- the instance is billed only while running or suspended, and it **self-suspends
  after 15 idle minutes**, so the dominant steady-state cost is the **300 GB data
  disk** (roughly US$30–40/month at hyperdisk-balanced list rates) plus a small
  suspended-instance charge;
- an active build hour is on the order of **US$1/hour** for a 16-vCPU/60 GB
  instance in that region;
- a **one-time seed** was large: the runbook/notes record uploading the qcow2
  chain from the workstation (≈113 GB, ~2.5 h), which is why the chain lives in
  `gs://dragos-devcloud-mac/v1` and is not re-sent.

**All of the above is EXPECTED** (list-price reasoning, not a billing read). It
would be settled from the `dragos-devcloud` billing export or the GCP pricing
page — not from anything in this repository.

### What cannot be verified from this Windows machine

- Xcode version, licence acceptance, installed SDKs/platforms, and free disk
  **on the guest** (the VM is `SUSPENDED`; starting it is a state change on
  owner-only infrastructure and was not done).
- Whether the four provisioning profiles and the distribution identity are valid
  **right now** (they expire; the scripts re-fetch at build time, so this is
  handled by design rather than by a stored fact).
- The live App Store Connect state (the app's version states, availability, the
  S23-16 review outcome). `scripts/ios-store.mjs --check` and
  `scripts/check-store-listing.mjs` are the commands that read it, and they were
  **not** run here.

---

## What is not verified, in one place

- **NOT VERIFIED** — the guest's Xcode version, licence state, free disk and
  installed platforms. Settle by running `xcodebuild -version` (and
  `-checkFirstLaunchStatus`) over SSH on `vmui-mac`.
- **NOT VERIFIED** — the cost figures in §4 are list-price reasoning, not a
  billing read. Settle from the `dragos-devcloud` billing export.
- **NOT VERIFIED** — the current validity/expiry of the team's signing
  certificates and profiles. Settle by running
  `node scripts/ios-appstore-profiles.mjs <dir>` (it re-reads and re-creates
  them) on this machine.
- **NOT VERIFIED** — the live App Store Connect state, including the S23-16
  review. Settle with `node scripts/ios-store.mjs --check --all`.
- **NOT VERIFIED (contradicted by the tree)** — a memory note says "ios-build
  refuses < 8 GB free". The **vitals** copy of `scripts/ios-build.ps1` has **no**
  free-space check (no `df`/disk text in it); that guard appears to be a later or
  other-repo variant. Treat "the lane guards against a full guest disk" as
  **false for this repository**, and note that the guest's disk filling twice is
  recorded in the tracker.
- **ABSENT** — the three memory notes named in the brief
  (`/memories/repo/vitals-apple-publishing.md`,
  `vitals-asc-testflight-facts.md`, `vitals-mac-apple-4.3-rejection.md`) do not
  exist in this machine's memory store. Their content was not available.
- **VERIFIED and current (2026-11-25)** — `vmui-mac` exists and is `SUSPENDED`;
  this machine can read the four ASC secrets' metadata; both Windows drivers,
  the guest scripts, the shared runbook and the CI/release Apple jobs all exist
  and were read.

---

## RECOMMENDATION

**Do not build a new Apple lane. It exists, it is proven, and it runs off the
workstation.** Validation is CI on hosted `macos-latest`; the signed iPhone and
Mac App Store uploads run on `vmui-mac` through `scripts/ios-build.ps1` and
`scripts/macos-appstore.ps1`, driven from Windows. The last full proof is a real
TestFlight upload (build 326, `VALID`, 1.3.0 `WAITING_FOR_REVIEW`, 2026-11-18),
and the one thing standing in front of the Mac App Store is Apple's review
(S23-16), not a missing capability. Adding a third mechanism would be duplicated
machinery, and a macOS guest is a scarce, single-keychain resource that two
agents must not drive at once.

**Optional, small and bounded — do these only if the owner wants parity with the
Windows lanes.** Ranked, each one file or two:

1. **Add `-DryRun`** to `scripts/ios-build.ps1` and `scripts/macos-appstore.ps1`,
   mirroring `e2e-cloud.ps1`, so a change to a driver can be reviewed without
   spending a build. Cheapest and most useful.
2. **Add a guest free-space guard** to the vitals copies of the drivers (the
   codai copy has one; this one does not), because the guest disk hitting 100 %
   surfaced as an opaque export failure — the tracker records it twice.
3. **Make the pass condition survive the trip**: have the driver log the sizes of
   the guest-side `.app` and `Watch/…Watch.app` (and, for `upload`, the build id
   read back from the API), so the evidence is in the returned log the way
   `check-size.ps1` decides on measured bytes.
4. **Queue the `vmui-mac` lane through agentq** (a `deploy:vmui-mac`-style
   resource), so two agents cannot run against the single macOS guest and its one
   build keychain at the same time.

None of the four is required; all four are omissions relative to the two Windows
lanes, and #1 and #2 are the ones that would have prevented a recorded failure.

### The single command that re-proves the lane, safely

The cheapest end-to-end proof that crosses every boundary — Windows → SSH →
`vmui-mac` → XcodeGen → `xcodebuild` — uses **no signing key, no profile and no
upload**, and its pass condition is a **content** assertion, not an exit code:

```powershell
pwsh -NoProfile -File scripts/ios-build.ps1 -Mode build
```

It resumes `vmui-mac` if suspended, refuses when the guest clock is > 120 s off,
generates the project with the pinned XcodeGen, compiles for `generic/platform=iOS`
with signing disabled, and **exits non-zero unless
`VitalsMonitor.app/Watch/VitalsMonitorWatch.app` exists**. If it returns
`ios-build build @ <sha> (build <n>) -> rc=0`, the lane is intact; if it stops on
the clock, the only remaining move is the owner's one line on the guest
(`sudo sntp -sS time.google.com`) — measured 2026-11-25: Apple's own NTP servers
are unreachable from that network, so `time.apple.com` fails and `time.google.com`
answers.
