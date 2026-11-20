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

   "Everywhere" is the Cargo workspace, the root and `apps`/`packages`
   `package.json` files, `tauri.conf.json`, `apps/ios/project.yml`
   (`MARKETING_VERSION`, inherited by every Xcode target) and the Tizen
   widget manifest `apps/tizen/public/config.xml`. `scripts/version.ps1`
   writes and checks all six; the tag's `verify` job runs `-Check <tag>`, so
   an iOS or Tizen build that kept last month's version stops the release
   instead of shipping quietly.

2. In `CHANGELOG.md`, rename `## [Unreleased]` to
   `## [0.9.0-beta.2] — 2026-10-05` and open a fresh `## [Unreleased]` above
   it. The changelog is **hand-curated**: `git cliff --unreleased` (config in
   `cliff.toml`) lists what landed since the last tag, as a draft to write
   from, never as text to paste. If the heading is not renamed, the release
   notes fall back to the Unreleased section, with a warning in the log.
3. `pwsh -NoProfile -File scripts/verify.ps1`, commit, push.
4. Tag the commit `v0.9.0-beta.2` and push the tag.
5. **After the `publish` job is approved and the release is public, prove the
   update — required, not optional.** An installer nobody can update _to_
   looks perfect on GitHub; the only thing that shows it is an installed copy
   accepting the new version. `.github/workflows/update-proof.yml`
   (`workflow_dispatch`, no secrets) installs a published version on a hosted
   Windows runner, lets it find, download and verify the new one, quits it as
   a user would, and asserts the installed executable moved. Run **both**
   jobs, because the two install scopes take different paths through the
   updater (D3, 2026-11-12):

| Run                | `scope`  | Installed first    | Ends as         | What it proves                                                                                              |
| ------------------ | -------- | ------------------ | --------------- | ----------------------------------------------------------------------------------------------------------- |
| per-machine-chosen | `legacy` | 1.1.0, per machine | 1.3.0, per user | a copy whose update queued `NeedsApproval` (S24-02) moves to the per-user scope on quit, with no UAC prompt |
| per-user           | `user`   | 1.2.0, per user    | 1.3.0, per user | the ordinary path: download, verify against the baked-in key, install on quit                               |

```powershell
gh workflow run update-proof.yml -f scope=legacy -f from=1.1.0 -f to=1.3.0
gh workflow run update-proof.yml -f scope=user   -f from=1.2.0 -f to=1.3.0
```

Record both run ids in the tracker row for the release. On a machine you
own the same proof runs from a checkout —
`pwsh -NoProfile -File scripts/ci/update-proof.ps1 -From 1.1.0 -To 1.3.0 -Scope legacy -Uninstall`,
elevated and interactive (the release build refuses the webview debugging
port, so the quit goes through the tray menu).

### If `publish` fails with 403 "Resource not accessible by integration"

Recognise it by the shape, not by the token: `POST /releases` answered 403 and
the log names `create-a-release`; a re-run fails identically, and
githubstatus.com stays green. This is GitHub refusing the **`target_commitish`**
field in the request, not a missing `contents: write` — the `publish` job's own
attestation steps prove those scopes are granted. The mechanism, the four
candidate causes that were refuted by command, and the decision are in
[adr/0045-release-creation-without-a-commit-target.md](adr/0045-release-creation-without-a-commit-target.md).

The fix already ships: `Publish release` creates the release with `gh api` and
sends no commit target. It only takes effect from a **new** tag, which is the
part that catches people out — and the reason a plain re-run will not do.

**A re-run cannot rescue a tag that already failed**, because a re-run uses the
workflow file from the tag's own commit, so a fix on `main` cannot reach it. To
publish such a tag, re-create it at a commit that carries the fix:

```powershell
git push origin :refs/tags/v1.3.1        # delete, so the push below is a creation
git tag -a -f v1.3.1 -m "Vitals 1.3.1" <sha-with-the-fix>
git push origin refs/tags/v1.3.1
```

Deleting and re-creating the tag is a deliberate exception to the
`release tags: owner only, immutable tag` ruleset, and it is only defensible
while the release does not exist yet: nothing has been published against the
old commit, so no consumer can be holding it. Do it before the release is
public, never after. (v1.3.1 was recovered this way — the tag moved `cf2949e` →
`36bf23d`.)

## The release doctor

The rule above — a release must update **every** surface that names it, and
the way to check is to grep for the old version before calling it done — is
prose, and prose has not held: the MSIX usage example once stayed a version
behind, and the Store 0.7.1 listing went live with release notes still
describing 0.7.0. `scripts/release-doctor.mjs` is that rule as a command. It
prints one table, one row per surface, and exits non-zero when any row is
`STALE`.

```powershell
node scripts/release-doctor.mjs --version 1.3.1
```

With no `--version` it reads the version from the manifests and checks the
`v<version>` tag. Everything it reads comes from the repository — no network
for the surfaces a release owns, no credential — so it runs in a couple of
seconds, which is why it is safe to put in a pre-commit hook and in CI.

What it checks, one row each:

- **every version manifest**, by re-using `scripts/version.ps1 -Check` (the
  existing source of truth) rather than re-implementing the six places the
  version lives;
- **the `CHANGELOG.md` section** for the version: present, dated, not empty;
- **version literals** in `scripts/`, `packaging/` and `.github/workflows/`
  that name Vitals' own version in a command or an example (a `-Version`
  argument, a workflow-dispatch `version=`, a `Vitals_<v>_` asset name, a
  `/v<v>/` release URL). A dependency's version (`toolchain: 1.99.0`) is not
  a Vitals version and is not flagged; past sections of `CHANGELOG.md` and the
  tracker are history, not surfaces, and are excluded;
- the **MSIX what's-new strings** (`packaging/msix/listing/*.md`) and the
  **Play changelogs** (`apps/android/play/listing/*/changelogs/`) for the
  version;
- the **two Apple listing files** (`apps/ios/store/listing.json`,
  `apps/desktop/store/macos-listing.json`). These are held at the version
  Apple is reviewing, so a file naming an older released version is reported
  `HELD`, with the reason in the row, and never `STALE`;
- the **tag**: `git tag`/`git ls-remote`/`git rev-parse` — it exists, is
  annotated, and its commit is on `origin`;
- the **tracker row** for the release in `docs/tracker.csv`;
- the **live store surfaces** (App Store Connect, Google Play) are reported
  `UNKNOWN — needs credentials`, never skipped and never guessed.
  `scripts/check-store-listing.mjs` is what reads those, with a key.

Exit codes say both what it found and how much of the check could run:
`0` a full check, nothing stale; `1`–`99` a full check with that many `STALE`
surfaces; `100` a partial run (a live store surface needed a credential) with
nothing stale; `101`–`199` a partial run with that many `STALE` surfaces.

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

**The rollout fraction is the one number no build artefact holds.** The
Console is where a staged rollout is raised, and on 2026-11-19 it was raised
to 100 % by hand while every check on `main` stayed green — a release can sit
at 20 %, reach a fifth of users, and have nothing in the repository say so.
Read it back instead of assuming: `scripts/play-rollout.mjs` opens a throwaway
edit, reads the production track and deletes the edit, so the read leaves the
Console exactly as it found it.
`node scripts/check-store-listing.mjs --check --require-full-rollout` fails
when production does not cover everyone; the tag release already passes
`--fail-on-unlisted --require-full-rollout`, and `store-listing.yml` is the job
that can therefore really read the track.

**In CI the credential is federated, not stored** (S24-21). `store-listing.yml`
mints a short-lived token for `vitals-play@hai-small-apps` through Workload
Identity Federation, with `access_token_scopes:
https://www.googleapis.com/auth/androidpublisher` — the action's
`cloud-platform` default is refused by the Play API with the same `403` a local
ADC login hits. The provider's attribute condition admits `refs/heads/main`
**for that one workflow file** (plus every `refs/tags/v`), so the federation is
not a production-release credential for anything else on `main`. Nothing is
stored in the repository: the key that used to sit in the
`PLAY_SERVICE_ACCOUNT_JSON` secret was deleted on 2026-11-19, and the secret
with it. Verified after the revocation: run 44623160752, `main` @ `6a45c4f`,
job success, `play rollout: fully rolled out (1.3.0 completed)`.

One of `PLAY_ACCESS_TOKEN`, `PLAY_SERVICE_ACCOUNT_JSON`, or
`GOOGLE_APPLICATION_CREDENTIALS` naming a file that holds a `service_account`
key must be set. **The machine's ambient ADC is not one of them**: it is minted
with the `cloud-platform` scope and the Play API answers `403 Request had
insufficient authentication scopes`, which reads like a broken check rather
than a credential that was never usable. For a local run, log in with the Play
scope first:

```powershell
gcloud auth application-default login --scopes=https://www.googleapis.com/auth/androidpublisher,https://www.googleapis.com/auth/cloud-platform
$env:PLAY_ACCESS_TOKEN = (gcloud auth print-access-token).Trim()
node scripts/check-store-listing.mjs --check --require-full-rollout
```

**With no credential to hand, read the verdict out of CI instead.** Because
`store-listing.yml` runs `--require-full-rollout` on every push to `main` and
weekly, a green run **is** the proof that production covers everyone — and the
job log prints the line:

```powershell
gh run list --workflow=store-listing.yml --limit 5
gh run view <run-id> --json jobs                                  # the job id
gh api repos/<owner>/<repo>/actions/jobs/<job-id>/logs |
  Select-String 'play (production|rollout)'
# play production: 1.3.0 [versionCodes 44] status=completed userFraction=full
# play rollout: fully rolled out (1.3.0 completed)
```

`gh run view <run-id> --log` prints nothing on this repository, so the
job-level API is the one that works. A **staged** rollout would have failed
that run, which is what makes a green one worth trusting.

Play gets the **`play` flavour** of the phone and TV apps, which does not
declare all-files access (Google rejected 1.0.1 for it on 2026-10-14, "not a
core feature"); the APKs on the GitHub release are the **`github` flavour**
with the folder scan. The `android` job fails if a Play bundle's manifest
mentions `MANAGE_EXTERNAL_STORAGE`. See `apps/android/README.md`.

Every package manager points at the **versioned** installer URL and the hash
in `SHA256SUMS.txt`, never at the stable-named copy, whose bytes change with
each release.

### Store steps no pipeline can prove

Each of these lives in the store's own account, outside this repository, and
each one has already cost a release. They are part of the release, not
housekeeping:

- **A released version is not a listed app.** On 2026-11-18 the iPhone app had
  two versions `READY_FOR_SALE` and no availability record at all, so App
  Store Connect read "This app was removed from sale" and the product page
  404'd while `itunes.apple.com/lookup` returned `resultCount 0`. Check
  `GET /v1/apps/6827931747/appAvailabilityV2` — a **404** means nobody can
  install the app — and set it up under Pricing and Availability if so.
  `scripts/ios-store.mjs` does not set availability; only the Console flow
  does. Changes reach the store within 24 hours.

  This is checked now, so it cannot go unnoticed again:
  `node scripts/check-store-listing.mjs` reads the availability record (and the
  territory count), both apps' version states, the public lookup and the three
  storefront pages, printing one line per check. It exits 1 when the app is not
  for sale and 2 when the check could not run; `--json` prints the same result
  for a pipeline, and `--fail-on-unlisted` states the exit-1 policy at a call
  site. It runs on every push to `main`, weekly and on demand in
  `.github/workflows/store-listing.yml`, and as a pre-flight step of the tag
  release. It needs the three **repository-level** secrets `APPLE_ASC_KEY_P8`,
  `APPLE_ASC_KEY_ID` and `APPLE_ASC_ISSUER` (the gcloud secrets of those names —
  `gcloud secrets versions access latest --secret=apple-asc-key-id
--project=dragos-devcloud | gh secret set APPLE_ASC_KEY_ID`); without them the
  scheduled job fails rather than skipping quietly, and the tag step prints
  SKIPPED and carries on. The Mac app's record is reported but never failed: it
  is not on the Mac App Store by decision (D6), so no availability record there
  is expected. A weekly failure can also be mailed through Brivio
  (`POST /v1/email/send`, scope `emails:send`) once the `BRIVIO_API_KEY` secret
  and the `STORE_ALERT_TO` variable exist.

- **Assigning an App Group invalidates the existing profiles**, and the portal
  says so when you confirm the change. An App ID whose App Groups list is
  empty while its entitlements ask for one makes every archive fail to sign,
  and Xcode reports that as an authentication error rather than a signing one.
  `scripts/ios-appstore-profiles.mjs` replaces any profile whose CMS blob does
  not contain `group.app.vitals.ios`, so run the upload and read the log
  rather than trusting a profile that exists.
- **A permission implies a required feature on Android.** Requesting
  `ACCESS_WIFI_STATE` makes `android.hardware.wifi` required unless the
  manifest says `required="false"`, and Play then restricts the app's
  availability to devices that have it. Play re-reads features only from a new
  bundle, so the fix ships in the next release. Prove it on the built artefact,
  not in the source: `aapt dump badging <apk> | rg uses-feature` must print
  `uses-feature-not-required` for every feature the app does not truly need.
  `pwsh -NoProfile -File scripts/check-android-features.ps1` is that check
  automated — it reads every built APK, compares the required set against
  `scripts/android-features.json` and against what the source declares
  optional, and fails on a disagreement. `CAMERA` implies
  `android.hardware.camera` in the same way, and `android.hardware.camera.any`
  does not cancel it.
- **An incomplete app-level declaration blocks the submission.** Play's trader
  status declaration is per app and required for every developer; a release
  uploaded by CI sits as a draft with `Submit N changes for review` disabled
  until it is filled in.
- **Play Console pages need a fresh tab.** A long-lived tab stops rendering the
  app pages — the shell stays on "Loading Google Play Console" with no failed
  request, and the account home degrades the same way afterwards. Open a new
  tab before concluding the Console is broken.
- **A LAN server needs an inbound firewall rule, and the prompt does not
  always appear.** Measured 2026-11-19 on the test laptop: the app's listener
  was bound and answered on loopback while a _second host_ got `HTTP 000`
  until an Allow rule existed for the program. Two consequences worth knowing
  before a pairing pass. First, the laptop now carries one rule added by hand
  during that pass — `Vitals LAN (dev test, removable)`, inbound TCP 7331 for
  `vitals-desktop.exe` — which is not created by any script here; remove it
  with `Remove-NetFirewallRule -DisplayName 'Vitals LAN (dev test, removable)'`
  if the machine is being handed back. Second, `vitals serve` is the worse
  case, because a server started by a scheduled task gets no prompt at all and
  the default is Block (S24-22). Do not read "the pairing URL was printed" as
  "the URL works": check it from another host.

**npm.** `packages/client` is currently `private` and exports TypeScript
source, so the registry refuses it. Making it publishable (a JavaScript
build, `exports` pointing at it, `private` removed) is a separate change;
leave `PUBLISH_NPM` off until then.

### Installing per user, unattended (Windows)

The setup is NSIS with `installMode: "both"` and a manifest that asks for
`highest`, so installing it **per user, from a script, with nobody at the
console** needs two things and not one — the scope switch _and_ the
compatibility layer. The app's own one-click move does both; a script that
copies only half of it stops on a consent prompt that no one will answer:

- **`/CurrentUser` alone is not enough.** The manifest still asks for
  elevation, so the unanswered prompt returns `ERROR_CANCELLED`, and
  `Start-Process` throws `System.InvalidOperationException: This command cannot be run due to the error: The operation was canceled by the user.`
- **Set `__COMPAT_LAYER=RunAsInvoker` in the parent process.** The loader of
  the process being started reads the layer from the environment it inherits,
  so a flag on the setup would do nothing; the layer runs the setup with the
  caller's token and no prompt appears.

`vitals_win::install::run_children_as_invoker()`
(`crates/vitals-win/src/install.rs:361`) sets that variable for everything the
process starts afterwards; the scope switch is `installer_args()`
(`install.rs:132`) and, for the move to per-user, `migration_args()`
(`install.rs:187`), the pair the app uses through `start_migration_setup()`
(`apps/desktop/src-tauri/src/updates.rs:870`). `scripts/ci/update-proof-local.ps1`
is the same recipe written out:

```powershell
$env:__COMPAT_LAYER = 'RunAsInvoker'
try { $p = Start-Process -FilePath .\Vitals_1.3.0_x64-setup.exe -ArgumentList '/S', '/CurrentUser' -PassThru -Wait }
finally { Remove-Item Env:__COMPAT_LAYER -ErrorAction SilentlyContinue }
$p.ExitCode   # 0
```

Then **prove where the files went**, because an exit code and a file size say
nothing about that:

```powershell
Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals' | Select-Object InstallLocation
Test-Path 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals'   # False
Test-Path 'C:\Program Files\Vitals'                                            # False
(Get-Item "$env:LOCALAPPDATA\Programs\Vitals\vitals-desktop.exe").VersionInfo.ProductVersion
```

An `InstallLocation` under `%LOCALAPPDATA%\Programs\Vitals`, no `HKLM` entry and
no Program Files folder is what `install.rs` classifies as
`InstallScope::CurrentUser`. An `HKLM` entry beside the `HKCU` one means the
running copy is still the machine-wide one, so every release will keep asking
for a prompt. **Check the installer's SHA-256 against an independent download**
— `SHA256SUMS.txt` from the release, or a second fetch — rather than trusting
the size, which a truncated or substituted file passes.

**A GUI app started from an `ssh` session shows nothing.** That session has no
desktop, so the process exits and a pid check comes back empty. Put anything
that has to appear on the screen into a scheduled task instead —
`-LogonType Interactive` runs it in the logged-on user's session, with
`-RunLevel Limited` for a standard-user token or `-RunLevel Highest` for an
elevated one:

```powershell
$action = New-ScheduledTaskAction -Execute 'C:\path\to\Vitals_1.3.0_x64-setup.exe' -Argument '/S', '/CurrentUser'
$princ  = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName vitals-install -Action $action -Principal $princ -Force
Start-ScheduledTask -TaskName vitals-install
```

**A per-user install cannot replace Task Manager.** It lives outside Program
Files by definition, and `ensure_hook_target_is_admin_only()`
(`crates/vitals-win/src/actions/taskmgr.rs:262`) refuses to point the hook at
any executable an ordinary account could swap — which is exactly what a
`%LOCALAPPDATA%` copy, a Scoop install or a portable copy is. The setting
refuses with that reason; the fix is a per-machine install, not a retry.

### The real-device pass, scripted (Windows)

The checks no runner can make — a filtered token, an elevated app reached by an
unelevated CLI, the sensors helper's log cap, a pairing exchange over the LAN —
are one command against a machine you can reach over `ssh`:

```powershell
pwsh -NoProfile -File scripts/vivobook-pass.ps1
pwsh -NoProfile -File scripts/vivobook-pass.ps1 -SkipSensors -SkipPairing
pwsh -NoProfile -File scripts/vivobook-pass.ps1 -Computer <host> -CliPath <vitals.exe>
```

It runs the unelevated halves as `-RunLevel Limited -LogonType Interactive`
tasks (the ssh session there is elevated, and `Start-Process` from it cannot
show a GUI at all), prints what each check found, and **asserts** the pairing
exchange — 200 with a 43-character token, then 403 for a replay, 403 for a
wrong code, and 403 for control from a read token. Two things it changes on
purpose and leaves behind, because they _are_ the state under test: PawnIO plus
the `vitals-sensors` service, and its `install.log` inside
`Program Files\Vitals Sensors`. Everything it registers is removed in a
`finally`, including a temporary inbound firewall rule for the CLI, and the app
is relaunched through the machine's own `VitalsLaunch` task so the box is left
as it was found.

### Microsoft Store (MSIX, signed by Microsoft)

The Store package is an **MSIX** uploaded unsigned; Microsoft signs it, so
this route needs no code-signing certificate. It is built by hand after the
release build, from the same executables as the installer:

1. Build both architectures and lay out both packages:

```powershell
pwsh -NoProfile -File packaging/msix/build-and-upload.ps1 -Version 1.3.0.0
```

x64 is the host build; ARM64 is built with
`--target aarch64-pc-windows-msvc`, so each architecture has its own
release directory and its own `AppxManifest` processor architecture. One
MSIX per architecture is required because the manifest declares a single
one: an x64-only submission is what leaves an ARM64 machine running the
x64 build under emulation. To make the packages on a machine you own,
build each architecture (`tauri build --no-bundle [--target <triple>]`)
and then
`pwsh -NoProfile -File packaging/msix/build-msix.ps1 -Version 1.3.0.0 -Architecture x64, arm64`.
The fourth version part must be `0`; identity name and publisher come
from Partner Center > Product identity and are parameters of the script. 2. Partner Center > the app > new submission > Packages: upload
`target/msix/Vitals_<version>_x64.msix` **and**
`target/msix/Vitals_<version>_arm64.msix` in the one submission. 3. Listing text, screenshots and the privacy URL: `packaging/msix/listing/`. 4. Submit for certification. Updates are new submissions with a new package.

What differs in the Store build is decided at run time from package identity
(`distribution.rs`): no self-update, no Task Manager replacement, no sensors
service, and logon start through `windows.startupTask`. The `runFullTrust`
restricted capability needs a one-line justification at submission:
"Vitals is a Win32 system monitor; it reads process and performance data and
ends processes at the user's request."
