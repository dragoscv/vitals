# Releasing, and the signing key

Vitals updates itself. The app polls a manifest on GitHub, downloads the new
installer, and refuses to run it unless the download is signed by the private
key whose public half is baked into `apps/desktop/src-tauri/tauri.conf.json`.
That is the whole security model, so most of this document is about the key.

Who may cause a release — rulesets on `main` and `v*`, the `release`
environment that waits for the owner's approval before anything is
published, pinned actions and the owner-only steps — is in
[repository-security.md](repository-security.md).

## The key

A minisign keypair, generated once with
`pnpm --filter @vitals/desktop exec tauri signer generate -w <path>`.

On the maintainer's machine it lives **outside the repository**, under
`%USERPROFILE%\.tauri`:

| File                                           | What it is                                                  |
| ---------------------------------------------- | ----------------------------------------------------------- |
| `%USERPROFILE%\.tauri\vitals.key`              | Private key, encrypted with a password. Never commit this.  |
| `%USERPROFILE%\.tauri\vitals.key.password.txt` | The password for that key.                                  |
| `%USERPROFILE%\.tauri\vitals.key.pub`          | Public key. Already committed, as `plugins.updater.pubkey`. |

`.tauri` is a user-profile directory, not a repository path, so there is no
`.gitignore` entry to get wrong and nothing to accidentally stage.

**Back both files up somewhere you will still have in five years** — a
password manager attachment is enough, and is better than a second copy on
the same disk.

### If the private key is lost

There is no recovery. This is the failure worth understanding before it
happens:

- Every already-installed copy of Vitals will only accept updates signed by
  the old key. A new keypair produces signatures those installs reject, so
  they will report a failed update forever and never move again.
- Generating a new keypair and shipping it in `tauri.conf.json` fixes _future_
  installs only.
- The only way to move existing users onto the new key is to have them
  download and run a new installer by hand. In practice that means a release
  announcement, a blog post, and accepting that some users will never see it.

So: losing the key does not compromise anyone, but it does permanently strand
everyone who installed before it was lost.

### If the private key is leaked

Rotate immediately, and treat it as the same migration problem in reverse:
whoever holds the key can sign an installer the app will trust and install
without a prompt. Generate a new pair, ship it, and tell users to reinstall
from GitHub.

## GitHub secrets

Two repository secrets, at
**Settings → Secrets and variables → Actions → New repository secret**:

| Secret name                          | Value                                                                                              |
| ------------------------------------ | -------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | The entire contents of `%USERPROFILE%\.tauri\vitals.key`, including the `untrusted comment:` line. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The contents of `%USERPROFILE%\.tauri\vitals.key.password.txt`.                                    |

The names are not arbitrary — `tauri signer sign` reads exactly these
environment variables. Since 2026-10-17 (SEC-5) only one step ever receives
them: **Sign updater bundles** in the `publish` job, which on a tag runs only
after the owner has approved the `release` environment. The build legs run
repository code and every dependency's build script before anyone has
looked at the run, so they build with `createUpdaterArtifacts: false` and no
key; `publish` then signs each installer (and the macOS `.app.tar.gz`, which
`macos-sign` writes from the notarised app) with
`tauri signer sign --app-version <version> <file>` — the same `<file>.sig`
`tauri build` would have written. That step installs dependencies with
`--ignore-scripts`, so no package script runs while the key is present.

The workflow fails loudly rather than publishing a silently broken release:
on a tag, the `verify` job refuses to start without
`TAURI_SIGNING_PRIVATE_KEY` (it sees only whether the secret is set, never
its value), the signing step throws if a signature is not written, a build
leg throws if it ever produces a `.sig` (the key leaked into it), and the
"Update manifest" step throws if any installer has no signature beside it.

**Check on the first tag after this change**: `latest.json` has a
`signature` for `windows-x86_64` and `windows-aarch64`, and an installed copy
of the previous version updates itself (Settings → About → Check for
updates). A signature made with the wrong key or format would show there as
a failed update, not as a red workflow.

`scripts/ci/update-proof.ps1 -From <previous> -To <new> -Uninstall` does both
on a clean Windows machine, elevated: it checks each signature's minisign key
id against `tauri.conf.json` and its trusted comment against the version,
installs the previous release, waits for the app's own log to say the update
was verified, quits (through `quit_app` when the webview debugging port
answers, else through the tray menu; the release build refuses the port, so
run it from an interactive elevated session) and reads the installed
executable's version. Run it after every tag that touches the signing path.

## What a user sees

The installed app polls
`https://github.com/dragoscv/vitals/releases/latest/download/latest.json`
(`plugins.updater.endpoints` in `tauri.conf.json`), about twenty seconds after
launch so the request never competes with the first paint
(`apps/desktop/src-tauri/src/updates.rs`). A newer version is downloaded
quietly and verified against the minisign public key; when the user quits,
NSIS installs it in **passive** mode (a progress bar, no questions), and the
next launch is the new version. Nothing is installed while the app is in use,
and an update that fails verification is discarded.

**Settings → About → Updates** has the switch _Install updates
automatically_ (on by default; off also drops an update already
downloaded), says when a downloaded version is waiting, and keeps a manual
**Check for updates** for anyone who wants it now. A build without the
signing key reports _Updates are not configured for this build_ rather than
"up to date", so a broken pipeline is never hidden behind a reassuring
sentence.

## Cutting a release

1. Set the version everywhere, then refresh the lockfile:

   ```powershell
   pwsh -NoProfile -File scripts/version.ps1 -Set 0.9.0-beta.2
   cargo update --workspace
   ```

2. In `CHANGELOG.md`, rename `## [Unreleased]` to
   `## [0.9.0-beta.2] — 2026-10-05` and open a fresh `## [Unreleased]` above
   it. The changelog is **hand-curated**: `git cliff --unreleased` (config in
   `cliff.toml`) lists what landed since the last tag, as a draft to write
   from, never as text to paste. If the heading is not renamed, the release
   notes fall back to the Unreleased section, with a warning in the log.
3. `pwsh -NoProfile -File scripts/verify.ps1`, commit, push.
4. Tag the commit `v0.9.0-beta.2` and push the tag.

## What a release does

Pushing a `v*` tag runs `.github/workflows/release.yml`:

1. **plan** decides the version (the tag without its `v`), the release
   title and whether anything is published.
2. **verify** (tags only) refuses to start without the updater key, then
   runs `scripts/version.ps1 -Check <tag>` (every manifest must say the
   tag's version), checks the Play release notes when `PUBLISH_PLAY` is on
   (each `changelogs/default.txt` at most 500 characters and naming the
   version), contract drift, `cargo fmt`, clippy with `-D warnings`,
   `cargo test --workspace`, the bindings drift check, `pnpm` typecheck,
   lint, format check and tests, and the release-build performance budget. A
   tag can point at a commit CI never saw, so it is re-proved.
3. **build**, in parallel:

   | Leg               | Runner           | Output                                                    | Attached to the release |
   | ----------------- | ---------------- | --------------------------------------------------------- | ----------------------- |
   | `windows-x64`     | `windows-latest` | `Vitals_<v>_x64-setup.exe`, `vitals-cli_<v>_x64.zip`      | always                  |
   | `windows-arm64`   | `windows-11-arm` | `Vitals_<v>_arm64-setup.exe`, `vitals-cli_<v>_arm64.zip`  | always                  |
   | `macos-universal` | `macos-latest`   | `Vitals_<v>_universal.app.zip` (unsigned, never attached) | via `macos-sign`        |
   | `linux-x64`       | `ubuntu-22.04`   | `.AppImage`, `.deb`, `.rpm`                               | only if `PUBLISH_LINUX` |

   No leg holds the updater key or writes a `.sig`; `publish` signs (see
   _GitHub secrets_). The Android leg (phone, Wear OS and TV, `github` APKs
   and `play` bundles) builds **unsigned** for the same reason (SEC2-4,
   2026-10-18: it used to decode the upload keystore before anyone had
   approved the run). The `vitals-cli_<v>_<arch>.zip` is the `vitals`
   command-line tool (`apps/cli`) with `LICENSE` and
   `THIRD_PARTY_NOTICES.md`; it is not bundled into the installer yet.

   macOS and Linux build on every run so the port cannot rot. Linux's
   sampler is still an honest stub (ADR 0007); macOS has a real one since
   S23 (ADR 0041). Unpublished legs stay workflow artefacts (downloadable
   from the run page for 30 days); while unpublished, a failure there shows
   red without blocking the Windows release.

   **macos-sign** (tags only, `release` environment, after approval, only
   with `PUBLISH_MACOS`): unzips the bare `.app` and runs
   `scripts/macos/sign-notarise.sh` — Developer ID signature with the
   hardened runtime, notarisation, staple, then the signed, notarised and
   stapled `Vitals_<v>_universal.dmg` and the updater
   `Vitals_<v>_universal.app.tar.gz`. The Apple certificate and the App
   Store Connect key exist only in this job; the build leg never sees them
   for the same reason it never sees the updater key. The script is the
   whole job and runs unchanged by hand on vmui-mac.

   Every leg passes `--target <triple>`, so bundles land in
   `target/<triple>/release/bundle`. Per-run configuration (Authenticode,
   disabling updater artefacts on a keyless nightly) is a JSON merge-patch
   written to the runner's temp directory and passed with `--config`; nothing
   in the tree changes.

4. **sbom** writes CycloneDX SBOMs: `sbom-rust.cdx.json` (`cargo cyclonedx`
   on the desktop crate, on Windows so the Windows-only dependencies are
   included), `sbom-cli.cdx.json` (the same for `apps/cli`) and
   `sbom-js.cdx.json` (`pnpm sbom`, desktop app, production dependencies
   only).
5. **publish** gathers every published artefact and only then writes the
   files that must see all of them at once — doing it in a build leg meant
   two runners racing to upload the same `latest.json`:
   - The updater signatures (`.sig` beside each installer), made after the
     owner's approval.
   - The Android signatures, also after approval: `apksigner` (v2/v3) on the
     three APKs attached to the release, `jarsigner` on the three bundles
     the `play` job uploads. The upload keystore (`ANDROID_KEYSTORE_B64` and
     its passwords) is decoded into a 0600 temp file inside that one step and
     deleted when it ends; a missing secret fails the release rather than
     publishing unsigned APKs. The signing is `scripts/ci/sign-android.sh`:
     it checks each APK for exactly one signer whose certificate SHA-256
     equals the keystore's, a v2/v3 signature and 16 KB alignment, verifies
     each bundle, and prints an `::error::` naming whatever failed. Test it
     before changing it, with a throwaway key and the `android-unsigned`
     artefact of any run (`gh run download <run> -n android-unsigned`):
     `keytool -genkeypair -keystore t.jks -alias upload ...`, then
     `SIGN_KEYSTORE=t.jks ANDROID_KEY_ALIAS=upload ANDROID_KEYSTORE_PASSWORD=...
 ANDROID_KEY_PASSWORD=... bash scripts/ci/sign-android.sh <dir> out/apk out/play`
     (Git Bash works on Windows; `BUILD_TOOLS` picks a build-tools version).
     v1.0.5 never published because the step before it grepped apksigner's
     output for `Signer #1 certificate`, which build-tools 37 prints as
     `V3.0 Signer: certificate`, and the empty grep ended the step with no
     message.
   - `latest.json` with `windows-x86_64` and `windows-aarch64` (plus
     `darwin-universal`, `darwin-aarch64`, `darwin-x86_64` and
     `linux-x86_64` when those are published). `version` has no leading `v`.
   - The stable names `Vitals_x64-setup.exe` and `Vitals_arm64-setup.exe`
     (`scripts/stable-assets.ps1`), copies of the versioned installers, so
     `https://github.com/dragoscv/vitals/releases/latest/download/Vitals_x64-setup.exe`
     always works.
   - `SHA256SUMS.txt` over every published file.
   - Build-provenance attestations for every file, SBOM attestations on the
     installers (Rust and JavaScript) and on the CLI zips (CLI SBOM).
   - Release notes: `scripts/release-notes.ps1` takes the version's section
     of `CHANGELOG.md` and appends downloads and verification instructions;
     GitHub's generated pull-request list, categorised by
     `.github/release.yml`, follows it.
6. **Channels** — npm, winget, Scoop, Chocolatey — each opt-in; see below.

### Why a beta is not marked "pre-release"

`/releases/latest/` — which both the updater endpoint and the stable download
links go through — skips GitHub pre-releases. While every release is a beta,
marking betas as pre-release would leave the installed app polling a
`latest.json` that does not exist and the website linking to a 404. So a tag
with a `-` (`v0.9.0-beta.1`) is published as a normal, _latest_ release
titled **"Vitals 0.9.0-beta.1 (public beta)"**, and the notes open with a
"public beta" banner. The updater still compares SemVer correctly:
`0.9.0-beta.2` is newer than `0.9.0-beta.1`, and `0.9.0` newer than both.

Revisit this once stable releases exist, if betas should reach opt-in
testers only; that needs a second updater endpoint, not a GitHub flag.

### No nightlies; manual builds

Scheduled nightlies were dropped on 2026-10-18 (PM2-7). The last one
published on 2026-09-29; every scheduled run after that skipped all its
jobs, nobody installed them, and a moving `nightly` pre-release beside real
releases confused the download page. The stale `nightly` pre-release
(0.9.0, 2026-09-29) and its tag are left for the owner to delete
(`gh release delete nightly --cleanup-tag`). CI builds the installer and runs the
full installer round trip on every push to `main`, which is what a
nightly used to prove.

_Run workflow_ on **Release** from any branch still builds every platform
(Windows, macOS, Linux, Android) and **publishes nothing**: no tag, no
release, no signing, no channel. Its artefacts (unsigned installers,
unsigned Android builds) stay on the run page for 30 days. It is versioned
`<base>-build.<yyyymmdd>.g<sha>`, so an installer taken from it names one
commit. Each ref has its own concurrency group and a newer manual run
cancels the older; a tag has its own group and is never cancelled.

## What CI proves about the installer

The `Installer round trip` job in `ci.yml` installs the installer built on
every push to `main` and, before uninstalling, switches on everything the
app can leave on a machine, asserting each is present first so the
removal checks cannot pass vacuously (QA2-2):

| Feature             | How it is switched on (as the app does)                                                                                                               | Asserted gone after uninstall                                           |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| CPU sensors service | `sensors\vitals-sensors.exe install --pawnio-setup <pinned PawnIO 2.2.0>`; if the driver cannot load, the same LocalSystem service registered by hand | `sc query vitals-sensors` = 1060, `Program Files\Vitals Sensors` absent |
| Task Manager        | `vitals-desktop.exe --set-taskmgr-replacement on` (the elevated child the switch spawns)                                                              | IFEO `taskmgr.exe` `Debugger` no longer names it                        |
| Start with Windows  | HKCU `Run\Vitals` = `<exe> --minimized` + `StartupApproved\Run`                                                                                       | both values gone                                                        |
| Lag watchdog        | HKCU `Run\VitalsWatchdog` = `"<install>\watchdog\vitals-watchdog.exe"`, process started                                                               | value gone, process gone                                                |

Then a **per-user** pass (`/S /CurrentUser` into
`%LOCALAPPDATA%\Programs\Vitals`, HKCU uninstall entry) and an **upgrade**
pass: the newest published release whose version differs from the build,
then the new installer over it; the version moves, the install stays per
machine, a settings file survives, and the first launch restores an
enabled watchdog.

While the per-machine copy is installed, `scripts/ci/idle-budget.ps1`
measures the app at rest (PERF-1): launched `--minimized` (tray only, what
Start with Windows runs) and with the dashboard visible, 60 s to settle,
then three 40 s windows over the app plus every WebView2 child. CPU is
reported as a percentage of one core, memory as private bytes and working
set; the median of the three is compared with `idle-budget.json` and the
table goes to the run summary. Budgets are the measured median plus 50 %;
raise them in a reviewed diff, never to make CI pass
(`idle-budget.ps1 -Update` rewrites them from a run).

`scripts/ci/roundtrip-proof.ps1` runs all of it on a Windows machine you
own (the devcloud `vitals-win` workspace): it builds the installer, fetches
the previous release and calls the two scripts.

**Update proof** (`update-proof.yml`, manual): installs the previous
published release on a hosted runner, lets it find, download and verify
the latest, quits it as a user would, and asserts the installed version
moved. Run it after approving a release's publish job.

**Dependabot**: a dependency bump changes `THIRD_PARTY_NOTICES.md`, which
CI checks. `dependabot-notices.yml` regenerates the file on the Dependabot
branch and starts CI on the new commit. It runs on `pull_request` (never
`pull_request_target`), only for branches Dependabot opened in this
repository, with `contents: write` for that one job, and executes no
dependency code: `cargo metadata`, `pnpm install --ignore-scripts` and the
notices script.

## Secrets and variables

**Settings → Secrets and variables → Actions.** Secrets are encrypted and
never printed; variables are plain switches. None of these are needed for
CI, only for releases.

| Secret                                                                                            | Needed for               | What it is and where to get it                                                                                                                                                                                         |
| ------------------------------------------------------------------------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`                                                                       | every tag (**required**) | The updater minisign key; see _The key_ above.                                                                                                                                                                         |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                                                              | every tag (**required**) | Its password.                                                                                                                                                                                                          |
| `WINDOWS_CERTIFICATE`                                                                             | Authenticode (optional)  | The code-signing certificate as a base64 PFX (`[Convert]::ToBase64String([IO.File]::ReadAllBytes('vitals.pfx'))`), from the certificate vendor (docs/distribution.md). Absent: unsigned, with a notice in the log.     |
| `WINDOWS_CERTIFICATE_PASSWORD`                                                                    | Authenticode (optional)  | The PFX password.                                                                                                                                                                                                      |
| `NPM_TOKEN`                                                                                       | npm                      | npmjs.com → avatar → _Access Tokens_ → _Generate New Token_ → **Granular**, read and write on the `@vitals` scope (create the `vitals` organisation first).                                                            |
| `WINGET_TOKEN`                                                                                    | winget                   | A **classic** GitHub token with `public_repo`, from an account that has a fork of `microsoft/winget-pkgs`; the action pushes to the fork and opens the pull request from it.                                           |
| `SCOOP_DEPLOY_KEY`                                                                                | Scoop                    | Private half of an ed25519 deploy key with write access on `dragoscv/scoop-vitals` only (the public half is registered there).                                                                                         |
| `CHOCO_API_KEY`                                                                                   | Chocolatey               | community.chocolatey.org → _Account_ → API key.                                                                                                                                                                        |
| `ANDROID_KEYSTORE_B64` + `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | every tag (**required**) | The Play upload key (base64 of the `.jks`) and its passwords. Read only by **Sign Android APKs and bundles** in `publish`, after approval.                                                                             |
| `APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD`                                                | macOS (`PUBLISH_MACOS`)  | Developer ID Application certificate as a base64 `.p12` and its password. Source of truth: dragos-devcloud Secret Manager `apple-developer-id-app-p12` / `apple-signing-p12-password`. **Environment `release` only.** |
| `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_P8`                                           | macOS (`PUBLISH_MACOS`)  | App Store Connect API key id, issuer id and the `.p8` contents, for `notarytool`. Secret Manager `apple-asc-key-id` / `apple-asc-issuer` / `apple-asc-key-p8`. **Environment `release` only.**                         |

| Variable         | Effect when `true`                                                                                                                                |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PUBLISH_MACOS`  | Run `macos-sign`, attach the notarised `.dmg` (plus stable `Vitals_universal.dmg`) and updater archive, add the `darwin-*` keys to `latest.json`. |
| `PUBLISH_LINUX`  | Attach the AppImage, `.deb` and `.rpm` and add `linux-x86_64` to `latest.json`.                                                                   |
| `PUBLISH_NPM`    | Publish `packages/client` as `@vitals/client`, with npm provenance.                                                                               |
| `PUBLISH_WINGET` | Open a winget-pkgs pull request for `Vitals.Vitals`.                                                                                              |
| `PUBLISH_SCOOP`  | Push `bucket/vitals.json` to `dragoscv/scoop-vitals`.                                                                                             |
| `PUBLISH_CHOCO`  | Pack and push the `vitals` Chocolatey package.                                                                                                    |
| `PUBLISH_PLAY`   | Upload the phone, Wear OS and TV bundles to Google Play (track rules below).                                                                      |
| `PLAY_ROLLOUT`   | Production rollout fraction for a stable tag, e.g. `0.2` (the default) or `1` for everyone.                                                       |
| `PLAY_DRAFT_APP` | `true` while the Play app has never passed review: production drafts, committed without sending for review (send from Publishing overview).       |

Publishing macOS or Linux turns that leg from advisory into a hard gate:
`latest.json` would otherwise name a file that does not exist. Do it only
once a real sampler exists for that platform.

A channel whose variable is `true` but whose secret is missing skips with a
notice; it never fails the release.

Two channels can refuse a correct upload for reasons that are the store's
state, not the package (QA2-6, 2026-10-18):

- **Chocolatey** answers a bare `403 (Forbidden)` to every push while the
  first version still waits in human moderation. The job then reads the
  public feed: with no approved version of `vitals` it ends green with a
  warning in the run summary (rerun the job after approval); with an
  approved version a 403 is a real refusal and fails.
- **Google Play** refuses `changesNotSentForReview` in one state ("Changes
  are sent for review automatically ... must not be set", v1.0.4) and
  requires it in another ("Changes cannot be sent for review
  automatically", after a rejected review, v1.0.2). The state cannot be read
  beforehand, so each upload tries the flag `PLAY_DRAFT_APP` suggests and,
  only if that commit fails, retries once with the opposite; the summary
  says when it did. The GitHub release is already public by
  the time the channels run, so a channel failure never un-publishes anything
  — rerun just that job.

### Authenticode, when a certificate exists

With both certificate secrets set, each Windows leg imports the PFX into the
runner's user store and passes Tauri `bundle.windows.certificateThumbprint`,
`digestAlgorithm: sha256` and `timestampUrl: http://timestamp.digicert.com`
through the `--config` override. Tauri then signs the application binary and
the installer during `tauri build`, before the minisign `.sig` is computed —
so the updater signature covers the signed bytes. Nothing in
`tauri.conf.json` changes. A hardware-token or cloud-HSM certificate (the
norm for new OV and EV certificates) cannot be imported as a PFX; that needs
`bundle.windows.signCommand` instead, set through the same override.

## Distribution channels

| Channel         | Automated                   | First listing                                                 |
| --------------- | --------------------------- | ------------------------------------------------------------- |
| GitHub releases | always                      | —                                                             |
| npm             | `PUBLISH_NPM` + `NPM_TOKEN` | automatic, once `packages/client` is publishable (see below)  |
| winget          | `PUBLISH_WINGET` + token    | **by hand**, once — `packaging/winget/README.md`              |
| Scoop           | `PUBLISH_SCOOP` + token     | create `dragoscv/scoop-vitals` — `packaging/winget/README.md` |
| Chocolatey      | `PUBLISH_CHOCO` + API key   | automatic, but moderated — `packaging/winget/README.md`       |
| Google Play     | `PUBLISH_PLAY` (no secret)  | listing by hand once — `apps/android/play/README.md`          |
| Microsoft Store | no                          | by hand, MSIX signed by Microsoft — below                     |

**Google Play.** The `play` job signs in through Workload Identity
Federation (no key exists). A **stable** tag (`v1.0.0`) goes to the
`production`, `wear:production` and `tv:production` tracks as a staged
rollout of `PLAY_ROLLOUT` (default 20 %), so a bad build can be halted in the
Console before it reaches everyone; a tag with a `-` (`v1.1.0-beta.1`) goes to
internal testing (`internal`, `wear:qa`, `tv:qa`) as before. Release notes
come from `apps/android/play/listing/<locale>/changelogs/default.txt` (≤ 500
characters each). Production needs the service account to hold _Release to
production_ for this app in Play Console → Users and permissions (owner
decision 2026-10-14), and a personal developer account must first have
passed Google's closed-testing requirement — see
`docs/store-research-2026-10.md`.

Play gets the **`play` flavour** of the phone and TV apps, which does not
declare all-files access (Google rejected 1.0.1 for it on 2026-10-14, "not a
core feature"); the APKs on the GitHub release are the **`github` flavour**
with the folder scan. The `android` job fails if a Play bundle's manifest
mentions `MANAGE_EXTERNAL_STORAGE`. See `apps/android/README.md`.

Every package manager points at the **versioned** installer URL and the hash
in `SHA256SUMS.txt`, never at the stable-named copy, whose bytes change with
each release.

**npm.** `packages/client` is currently `private` and exports TypeScript
source, so the registry refuses it. Making it publishable (a JavaScript
build, `exports` pointing at it, `private` removed) is a separate change;
leave `PUBLISH_NPM` off until then.

### Microsoft Store (MSIX, signed by Microsoft)

The Store package is an **MSIX** uploaded unsigned; Microsoft signs it, so
this route needs no code-signing certificate. It is built by hand after the
release build, from the same executables as the installer:

1. Release build (as for the installer), then
   `pwsh -NoProfile -File packaging/msix/build-msix.ps1 -Version 0.9.0.0`.
   The fourth version part must be `0`. Identity name and publisher come
   from Partner Center > Product identity and are parameters of the script.
2. Partner Center > the app > new submission > Packages: upload
   `target/msix/Vitals_<version>_x64.msix`.
3. Listing text, screenshots and the privacy URL: `packaging/msix/listing/`.
4. Submit for certification. Updates are new submissions with a new package.

What differs in the Store build is decided at run time from package identity
(`distribution.rs`): no self-update, no Task Manager replacement, no sensors
service, and logon start through `windows.startupTask`. The `runFullTrust`
restricted capability needs a one-line justification at submission:
"Vitals is a Win32 system monitor; it reads process and performance data and
ends processes at the user's request."
