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

The names are not arbitrary — the Tauri CLI reads exactly these environment
variables during `tauri build`, and `.github/workflows/release.yml` passes
them through. Get a name wrong and the build still succeeds; it simply
produces no `.sig` files, and the release ships without an update manifest.

The workflow fails loudly on that case rather than publishing a silently
broken release: on a tag, the `verify` job refuses to start without
`TAURI_SIGNING_PRIVATE_KEY`, and both the build legs and the "Update
manifest" step throw if any installer has no signature beside it.

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
   tag's version), contract drift, `cargo fmt`, clippy with `-D warnings`,
   `cargo test --workspace`, the bindings drift check, `pnpm` typecheck,
   lint, format check and tests, and the release-build performance budget. A
   tag can point at a commit CI never saw, so it is re-proved.
3. **build**, in parallel:

   | Leg               | Runner           | Output                                             | Attached to the release |
   | ----------------- | ---------------- | -------------------------------------------------- | ----------------------- |
   | `windows-x64`     | `windows-latest` | `Vitals_<v>_x64-setup.exe` + `.sig`                | always                  |
   | `windows-arm64`   | `windows-11-arm` | `Vitals_<v>_arm64-setup.exe` + `.sig`              | always                  |
   | `macos-universal` | `macos-latest`   | `.dmg`, `Vitals_<v>_universal.app.tar.gz` + `.sig` | only if `PUBLISH_MACOS` |
   | `linux-x64`       | `ubuntu-22.04`   | `.AppImage` + `.sig`, `.deb`, `.rpm`               | only if `PUBLISH_LINUX` |

   macOS and Linux build on every run so the port cannot rot, but their
   samplers are honest stubs (ADR 0007), so by default they stay workflow
   artefacts (downloadable from the run page for 30 days) and are not
   attached to the release. While unpublished, a failure there shows red
   without blocking the Windows release.

   Every leg passes `--target <triple>`, so bundles land in
   `target/<triple>/release/bundle`. Per-run configuration (Authenticode,
   disabling updater artefacts on a keyless nightly) is a JSON merge-patch
   written to the runner's temp directory and passed with `--config`; nothing
   in the tree changes.

4. **sbom** writes CycloneDX SBOMs: `sbom-rust.cdx.json` (`cargo cyclonedx`
   on the desktop crate, on Windows so the Windows-only dependencies are
   included) and `sbom-js.cdx.json` (`pnpm sbom`, desktop app, production
   dependencies only).
5. **publish** gathers every published artefact and only then writes the
   files that must see all of them at once — doing it in a build leg meant
   two runners racing to upload the same `latest.json`:
   - `latest.json` with `windows-x86_64` and `windows-aarch64` (plus
     `darwin-universal`, `darwin-aarch64`, `darwin-x86_64` and
     `linux-x86_64` when those are published). `version` has no leading `v`.
   - The stable names `Vitals_x64-setup.exe` and `Vitals_arm64-setup.exe`
     (`scripts/stable-assets.ps1`), copies of the versioned installers, so
     `https://github.com/dragoscv/vitals/releases/latest/download/Vitals_x64-setup.exe`
     always works.
   - `SHA256SUMS.txt` over every published file.
   - Build-provenance attestations for every file, and SBOM attestations on
     the installers.
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

### Nightlies

At 03:00 UTC, when anything landed in the last 24 hours (or by hand, via
_Run workflow_), the same build runs without `verify` — `main` is gated by
CI on every push — and publishes to the moving `nightly` tag as a
**pre-release**, which is never "latest". The previous nightly is deleted
first so the tag moves. Nightlies are versioned
`<base>-nightly.<yyyymmdd>.g<sha>` and get no `latest.json`: an updater
pointed at a moving target would offer the same "new" build forever.
Running the workflow by hand with _nightly_ unticked builds everything and
publishes nothing.

## Secrets and variables

**Settings → Secrets and variables → Actions.** Secrets are encrypted and
never printed; variables are plain switches. None of these are needed for
CI, only for releases.

| Secret                               | Needed for               | What it is and where to get it                                                                                                                                                                                     |
| ------------------------------------ | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `TAURI_SIGNING_PRIVATE_KEY`          | every tag (**required**) | The updater minisign key; see _The key_ above.                                                                                                                                                                     |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | every tag (**required**) | Its password.                                                                                                                                                                                                      |
| `WINDOWS_CERTIFICATE`                | Authenticode (optional)  | The code-signing certificate as a base64 PFX (`[Convert]::ToBase64String([IO.File]::ReadAllBytes('vitals.pfx'))`), from the certificate vendor (docs/distribution.md). Absent: unsigned, with a notice in the log. |
| `WINDOWS_CERTIFICATE_PASSWORD`       | Authenticode (optional)  | The PFX password.                                                                                                                                                                                                  |
| `NPM_TOKEN`                          | npm                      | npmjs.com → avatar → _Access Tokens_ → _Generate New Token_ → **Granular**, read and write on the `@vitals` scope (create the `vitals` organisation first).                                                        |
| `WINGET_TOKEN`                       | winget                   | A **classic** GitHub token with `public_repo`, from an account that has a fork of `microsoft/winget-pkgs`; the action pushes to the fork and opens the pull request from it.                                       |
| `SCOOP_DEPLOY_KEY`                   | Scoop                    | Private half of an ed25519 deploy key with write access on `dragoscv/scoop-vitals` only (the public half is registered there).                                                                                     |
| `CHOCO_API_KEY`                      | Chocolatey               | community.chocolatey.org → _Account_ → API key.                                                                                                                                                                    |

| Variable         | Effect when `true`                                                                                       |
| ---------------- | -------------------------------------------------------------------------------------------------------- |
| `PUBLISH_MACOS`  | Attach the macOS `.dmg` and updater archive to the release and add the `darwin-*` keys to `latest.json`. |
| `PUBLISH_LINUX`  | Attach the AppImage, `.deb` and `.rpm` and add `linux-x86_64` to `latest.json`.                          |
| `PUBLISH_NPM`    | Publish `packages/client` as `@vitals/client`, with npm provenance.                                      |
| `PUBLISH_WINGET` | Open a winget-pkgs pull request for `Vitals.Vitals`.                                                     |
| `PUBLISH_SCOOP`  | Push `bucket/vitals.json` to `dragoscv/scoop-vitals`.                                                    |
| `PUBLISH_CHOCO`  | Pack and push the `vitals` Chocolatey package.                                                           |

Publishing macOS or Linux turns that leg from advisory into a hard gate:
`latest.json` would otherwise name a file that does not exist. Do it only
once a real sampler exists for that platform.

A channel whose variable is `true` but whose secret is missing skips with a
notice; it never fails the release. The GitHub release is already public by
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
| Microsoft Store | no                          | by hand, and blocked on a certificate — below                 |

Every package manager points at the **versioned** installer URL and the hash
in `SHA256SUMS.txt`, never at the stable-named copy, whose bytes change with
each release.

**npm.** `packages/client` is currently `private` and exports TypeScript
source, so the registry refuses it. Making it publishable (a JavaScript
build, `exports` pointing at it, `private` removed) is a separate change;
leave `PUBLISH_NPM` off until then.

### Microsoft Store (manual, future work)

The Store accepts a plain `.exe` installer ("MSI or EXE app") through
[Partner Center](https://partner.microsoft.com/dashboard), without MSIX
repackaging. It is not automated, and is blocked on two things:

1. **A code-signing certificate.** EXE submissions must be Authenticode
   signed by a certificate that chains to a trusted root; the Store does not
   re-sign them. Nothing proceeds until `WINDOWS_CERTIFICATE` exists.
2. **An offline WebView2 installer variant.** Store certification runs
   offline and rejects installers that download at install time; this build
   uses `webviewInstallMode: downloadBootstrapper`. A Store build needs
   `offlineInstaller` (about +130 MB) or `embedBootstrapper`, set through a
   `--config` override in a separate build leg so the direct download stays
   small.

Then, by hand: register as an individual developer (a one-off fee), create
the app, choose _EXE or MSI app_, give the **versioned, signed** installer
URL from the GitHub release, silent switch `/S`, architectures x64 and ARM64,
and submit for certification. Updates are new submissions with the new URL.
Partner Center has a submission API, so this can be automated once the first
listing exists.
