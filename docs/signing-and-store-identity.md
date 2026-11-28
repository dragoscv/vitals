# Signing and store identity

Two sheets that must not drift: the application pack for a free code-signing
service, and the identity facts every surface has to agree on.

- Why Vitals is unsigned and what the options cost: [distribution.md](distribution.md).
- The short form of the SignPath application: [repository-security.md](repository-security.md) §4.
- How a release is cut, and the Authenticode switch when a certificate exists: [releasing.md](releasing.md).

This file is the complete, pasteable version of the first and the only place
that lists the second. Everything marked **verified** below was read out of
this repository with the command in [Evidence](#evidence). Anything the owner
must supply — a legal name, a contact address, an organisation id — is left as
a blank, never guessed.

## Part 1 — SignPath Foundation application pack

### 1.1 What the programme requires

The programme's own wording is not in this repository, so the requirement list
below is the one this repository already recorded when it was first scoped:
[repository-security.md](repository-security.md) §4 says they require _"an OSI
licence (MIT, yes), builds from a public CI (yes), a code of conduct and
security policy (both in the repo), and that the signing request comes from CI,
never a workstation"_. Treat that list as **expected**, not as read here. The
right-hand column is what this repository can prove.

| Requirement                                            | Where this repository meets it                                                                                                                                                                                                                             |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| An OSI-approved licence                                | MIT — `LICENSE:1` (`MIT License`), `Cargo.toml:26` (`license = "MIT"`), the README badge (`README.md:8`)                                                                                                                                                   |
| A public repository                                    | `https://github.com/dragoscv/vitals` — `Cargo.toml:27` (`repository`), `PRIVACY.md:47`, and every download link in the README points at its releases                                                                                                       |
| A reproducible, publicly verifiable build              | Built in public GitHub Actions from a **tagged commit** (`release.yml:29-30`); every action is pinned to a full commit SHA; artefacts are hashed into `SHA256SUMS.txt` and a SLSA provenance attestation links each one to its commit, workflow and runner |
| A project description                                  | §1.3                                                                                                                                                                                                                                                       |
| The signer's identity                                  | **blank** — §1.5                                                                                                                                                                                                                                           |
| Which artefacts are signed                             | §1.3 (the two Windows NSIS installers, and the executables inside them)                                                                                                                                                                                    |
| A code of conduct and a security policy                | `CODE_OF_CONDUCT.md`, `SECURITY.md`, plus `GOVERNANCE.md`, `CONTRIBUTING.md` (DCO sign-off) and `SUPPORT.md`                                                                                                                                               |
| The signing request comes from CI, never a workstation | The signing step would live in the `publish` job of `release.yml`, which runs only on a tag and only after the owner approves the `release` environment                                                                                                    |

### 1.2 The facts this repository supplies

| Fact                                  | Value                                                                                                                                 | Source                                                                                                                                      |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Licence                               | MIT (OSI-approved)                                                                                                                    | `LICENSE:1`; `Cargo.toml:26`; `README.md:8`                                                                                                 |
| Project name                          | Vitals                                                                                                                                | `apps/desktop/src-tauri/tauri.conf.json:3` (`"productName": "Vitals"`)                                                                      |
| Repository                            | `https://github.com/dragoscv/vitals`                                                                                                  | `Cargo.toml:27`; `PRIVACY.md:47`                                                                                                            |
| Maintainer / copyright holder         | `Dragos Catalin Vladulescu (natural person, maintainer of Vitals)`, Romania                                                           | `PRIVACY.md:44`; `CONTRIBUTORS.md:5`                                                                                                        |
| Contact                               | `dragoscv12@gmail.com`                                                                                                                | `PRIVACY.md:46`; `SUPPORT.md:14`                                                                                                            |
| Website                               | `https://vitals.dragoscatalin.ro`                                                                                                     | `tauri.conf.json:49`; `PRIVACY.md:48`                                                                                                       |
| Build workflow                        | `.github/workflows/release.yml`                                                                                                       | `release.yml:29-30` (trigger), `release.yml:846` (`publish`)                                                                                |
| What it builds                        | the two NSIS installers, x64 and arm64, currently unsigned                                                                            | `release.yml:353` (`bundles: nsis`), `release.yml:470` (`::notice::No WINDOWS_CERTIFICATE: the installer is not Authenticode-signed.`)      |
| Artefacts to sign                     | `Vitals_<version>_x64-setup.exe`, `Vitals_<version>_arm64-setup.exe`, and the `vitals-sensors.exe` / `vitals-watchdog.exe` they carry | `release.yml:1823-1827` and `release.yml:612` (promised and published asset names); `repository-security.md:78-81`; `tauri.conf.json:40-45` |
| Hashing                               | `SHA256SUMS.txt` is written over every published file                                                                                 | `release.yml:1054` (`Checksums`)                                                                                                            |
| Provenance                            | SLSA build provenance, free, via `actions/attest-build-provenance`                                                                    | `release.yml:1067`; `README.md:257`                                                                                                         |
| SBOMs                                 | CycloneDX for the Rust desktop crate, the CLI crate and the JavaScript tree, each attested with `actions/attest-sbom`                 | `release.yml:626` (the `sbom` job); `release.yml:1072-1087` (the attestations)                                                              |
| Supply-chain policy                   | Every action pinned to a full commit SHA; the pinned list is enforced by the repository                                               | `repository-security.md:92-95`                                                                                                              |
| Signing is not exposed anywhere today | `vitals-sensors.exe` runs as SYSTEM unsigned, and the installers are unsigned                                                         | `repository-security.md:72-75`; `release.yml:470`                                                                                           |

### 1.3 The application text (paste this)

```text
Project: Vitals — https://github.com/dragoscv/vitals

What Vitals is
Vitals is a free, open-source (MIT) system monitor and task manager for
Windows 10 and 11, with companion apps for Android, Wear OS, Google TV, iOS
and Samsung TVs. It shows a person what their computer is actually doing and
why it is slow: processes with real names and icons, live processor, memory,
graphics and network readings, drive health, temperatures, storage clean-up and
a lag watchdog that names the program behind a frozen window. It is written in
Rust (a Tauri 2 desktop app with a React webview) and TypeScript, and built and
tested entirely in public CI.

Why signing matters for a system monitor
An unsigned installer makes Windows Defender SmartScreen warn on every first
run ("Windows protected your PC"), and most people close the dialog instead of
finding "More info → Run anyway". That is worse than an inconvenience for this
class of software: Vitals is a program whose whole job is to be trusted with a
process list, temperatures and drive health, and a security warning on first
launch teaches exactly the wrong reflex. The project has documented the warning
honestly rather than telling people to click through it, and the Microsoft
Store route is the only
free way to a warning-free install today — but the Store version is a separate
package, and the direct download, the CLI and the optional sensors service stay
unsigned. A free signing service for open-source projects is the only route
that removes the warning from the binaries people actually download.

How it is built, and how that can be checked
Every release is produced by GitHub Actions from an annotated tag (the workflow
is .github/workflows/release.yml and is in the repository). The run:
  - only builds on a tag (refs/tags/v*), and re-proves the gates on that tag
    before anything is packaged;
  - pins every action, first- and third-party, to a full commit SHA;
  - refuses to publish a partial release: the assemble step fails if any
    required artefact is missing;
  - writes SHA256SUMS.txt over every published file, and attaches a SLSA build
    provenance attestation (actions/attest-build-provenance) that links each
    file to the exact commit, workflow and runner that produced it — verifiable
    with `gh attestation verify <file> --repo dragoscv/vitals`;
  - attaches CycloneDX SBOMs for the Rust, CLI and JavaScript dependency trees;
  - signs the update packages with minisign, so an installed copy verifies them
    before applying; that key already exists and is unrelated to Windows
    Authenticode;
  - reads the code-signing certificate only inside the `publish` job, which runs
    after the owner approves the protected `release` environment, so no build
    leg ever holds a key.

Artefacts to sign
  - Vitals_<version>_x64-setup.exe        (NSIS installer, x64)
  - Vitals_<version>_arm64-setup.exe      (NSIS installer, arm64)
  - the executables those installers carry: vitals-desktop.exe,
    vitals-watchdog.exe and vitals-sensors.exe
  - optionally vitals.exe from the CLI zip (vitals-cli_<version>_<arch>.zip)

What the project already does for its users
  - MIT licence; a privacy policy, terms of use, security policy, code of
    conduct, governance and contribution guide are all in the repository;
  - no telemetry, analytics, advertising or account, and the developer receives
    no data;
  - the maintainer is the only human contributor and signs their commits; every
    pull request carries a Developer Certificate of Origin sign-off;
  - the signing request would come from CI, never from a workstation.

Contact: <OWNER: public contact address>
Signer / legal identity: <OWNER: the name that should appear on the certificate>
```

### 1.4 What would need to change in CI to sign

Four changes, all inside `release.yml`'s `publish` job, in this order. The
first two are how the repository already treats the minisign and Android keys,
so they are a pattern to follow rather than new design.

1. **A verification-only build.** Already true: the `build` legs compile from a
   tag, hold no signing key, and refuse to produce a `.sig`
   (`release.yml:583`, _"a build leg produced a .sig; the updater key must not
   reach this job"_). The signing key would be read only in `publish`, which
   already runs in the protected `release` environment after the owner's
   approval (`release.yml:846-870`).
2. **A signing step after the build.** Add a step in `publish` that submits the
   installers to the service. The service's GitHub action
   (`signpath/github-action-submit-signing-request`) is pinned to a full commit
   SHA and its path added to the repository's allowed-actions list first, or the
   job fails at setup with _"is not allowed"_ (`repository-security.md:97-98`).
3. **Hash the signed output, not the input.** Move the step to run **before**
   `Stable download names` and `Checksums` (`release.yml:1046-1060`), because
   Authenticode changes the installer's bytes and `SHA256SUMS.txt` must describe
   what a user downloads. It must also run **before** `Sign updater bundles`
   (`release.yml:977`), because the minisign signature is computed over the
   final bytes; signing afterwards would invalidate it. _(Reasoned from the step
   order in `release.yml`, not measured.)_
4. **Publish the signed files.** No change: the assemble and publish steps move
   whatever is in `publish/`, so a signed installer that replaces the unsigned
   one on disk is published as-is.

The Authenticode branch already present for a purchased certificate
(`release.yml:470`, `releasing.md:635`) is a second, independent path; the
two are not combined.

### 1.5 Blanks the owner must fill

| Blank                                                      | Why it cannot come from the repository                                                                                |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Legal name of the signer                                   | A natural person's name as it should appear on a certificate; the repository names the maintainer's public alias only |
| Public contact address for the application                 | `dragoscv12@gmail.com` is the project's published contact, but the applicant may prefer another                       |
| Organisation / project id and API token issued on approval | Only SignPath issues these; they do not exist until the application is accepted                                       |
| Country / VAT or legal-entity details, if asked            | `PRIVACY.md:45` states Romania; anything more is the owner's to supply                                                |
| Which account holds the repository on the service          | Account-level, not in the repository                                                                                  |

## Part 2 — Store identity sheet

Every fact below must stay in agreement across the GitHub release, winget,
Scoop, Chocolatey, the Microsoft Store and Google Play. A fact that changes in
one place and not another is a defect, and this table is where to look.

### 2.1 The facts

| Fact                                                 | Value                                                                                                                                                      | Where it comes from                                                                                                                                                      |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Product name (desktop app)                           | `Vitals`                                                                                                                                                   | `apps/desktop/src-tauri/tauri.conf.json:3`                                                                                                                               |
| Product name (Store listing)                         | `Vitals: System Monitor` (`Vitals` alone is taken in the Store)                                                                                            | `packaging/msix/listing/en-us.md:11`; `README.md:216`                                                                                                                    |
| Publisher (installer / winget)                       | `Dragos Catalin Vladulescu`                                                                                                                                | `tauri.conf.json:47`; `packaging/winget/manifests/v/Vitals/Vitals/1.5.0/Vitals.Vitals.installer.yaml:32`                                                                 |
| Publisher display name (MSIX)                        | `Dragos Catalin` **as the default in the script**; Partner Center's value is authoritative                                                                 | `packaging/msix/build-msix.ps1:40`                                                                                                                                       |
| MSIX identity name                                   | `DragosCatalin.VitalsSystemMonitor`                                                                                                                        | `packaging/msix/build-msix.ps1:38`                                                                                                                                       |
| MSIX publisher (the `CN=`)                           | `CN=FE8B5839-E0F9-495F-854C-53165710EB07`                                                                                                                  | `packaging/msix/build-msix.ps1:39`                                                                                                                                       |
| Package family name                                  | `DragosCatalin.VitalsSystemMonitor_2n7wyqrzmk6fw`                                                                                                          | Derived from the PackageFullName recorded in `docs/TRACKER.md:1367`. The `_2n7wyqrzmk6fw` suffix is a hash of the publisher, so it does not change when the version does |
| Store product id (Microsoft Store)                   | `9P95RK5VMGPJ` — https://apps.microsoft.com/detail/9P95RK5VMGPJ                                                                                            | `packaging/msix/listing/en-us.md:13`; `README.md:14`                                                                                                                     |
| Store product id (Google Play)                       | package name `app.vitals`                                                                                                                                  | `.github/workflows/release.yml:1476` (and the other four uploads in the `play` job)                                                                                      |
| Store product id (App Store, iOS)                    | `id6827931747` — https://apps.apple.com/app/id6827931747                                                                                                   | `README.md:28`                                                                                                                                                           |
| Windows app identifier (Tauri)                       | `app.vitals.desktop`                                                                                                                                       | `tauri.conf.json:5`                                                                                                                                                      |
| four-part MSIX version                               | `1.6.0.0` for the current tree — the core version plus `.0`                                                                                                | Core version `Cargo.toml:23` (`version = "1.6.0"`); the derivation is `scripts/check-drift.ps1:638` (`$storeVersion = (($currentVersion -split '[-+]')[0]) + '.0'`)      |
| Where the four-part version is written and validated | Passed by hand as the `version` input to `msix.yml`; validated by `build-msix.ps1`; literals bound to `-Version` are checked by `check-drift.ps1` check 10 | `.github/workflows/msix.yml:20-21`; `packaging/msix/build-msix.ps1:54` (`Version must be N.N.N.0 for the Store`); `scripts/check-drift.ps1:638-658`                      |
| Installer scope (NSIS)                               | `currentUser` — one setup, for the current user only, no UAC prompt                                                                                        | `tauri.conf.json:58`; `docs/adr/0053-nsis-per-user-default.md`                                                                                                           |
| Installer scope (winget manifest)                    | `Scope: user`, `Custom: /CurrentUser`, `%LocalAppData%\Programs\Vitals`                                                                                    | `.../Vitals.Vitals.installer.yaml:21,27,36`; the rewrite is `scripts/ci/winget-user-scope.ps1`                                                                           |
| Silent install switch                                | `/S` (NSIS silent); the app's updater runs the installer `passive`                                                                                         | `packaging/winget/README.md:70`; `packaging/chocolatey/tools/chocolateyinstall.ps1:21-24`; `tauri.conf.json:72` (`"installMode": "passive"`)                             |
| winget package identifier                            | `Vitals.Vitals`                                                                                                                                            | `packaging/winget/manifests/v/Vitals/Vitals/1.5.0/Vitals.Vitals.yaml:16`; `README.md:225`                                                                                |
| winget Contributor fork                              | `dragoscv/winget-pkgs`                                                                                                                                     | `scripts/release-doctor.mjs:799`                                                                                                                                         |

### 2.2 What could not be read, and how to read it

| Unknown                                               | Why                                                                                     | How to read it                                                                                                           |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| The version currently **live** in the Microsoft Store | A live listing is not in the repository. The last submission recorded here is `1.5.0.0` | `docs/tracker.csv:349` records submission 8; the live value is Partner Center → the app → Submissions, or the Store page |
| The Partner Center `PublisherDisplayName` in force    | Only the script's default is in the repository                                          | Partner Center → the app → Product identity                                                                              |
| The version currently in `microsoft/winget-pkgs`      | Lives in another repository                                                             | `winget show Vitals.Vitals`, or `packaging/winget/manifests/…` once an update PR merges                                  |
| Whether the current `WINGET_TOKEN` can push           | The answer is a property of a secret, not of the tree                                   | `WINGET_TOKEN=<token> node scripts/release-doctor.mjs --winget-token` (§2.3)                                             |
| Any SignPath organisation id, project slug or token   | Issued only on approval; none exists yet                                                | §1.5                                                                                                                     |
| The Store listing text in force                       | The console holds a copy; these files are the declared source of truth                  | `docs/tracker.csv:349` (per-section digests) and Partner Center → Store listings                                         |

### 2.3 winget: identifier, permission, rotation

The release job updates an existing package only — `komac update Vitals.Vitals`
writes the manifests to a folder, `scripts/ci/winget-user-scope.ps1` rewrites
the installer manifest, and `komac submit` opens the pull request
(`release.yml:1617-1625`). The submission is made **from the contributor's own
fork**, so the token behind `WINGET_TOKEN` must be able to create a ref on
`dragoscv/winget-pkgs`.

- **Exact permission:** `push` on the fork (the ability to create a ref), as
  probed by `scripts/release-doctor.mjs:836` (`WINGET_TOKEN can create a ref on
dragoscv/winget-pkgs`).
- **What to grant**, in the project's own words: _"a classic PAT with the
  `public_repo` scope, or a fine-grained PAT with Contents read+write and Pull
  requests read+write on the fork"_ (`scripts/release-doctor.mjs:800-801`).
- **Failure symptom:** the release dies with `dragoscv does not have the correct
permissions to execute 'CreateRef'` after every gate has passed
  (`release.yml:130`). Since 2026-11-28 the token is probed **before**
  anything is built, by the `Winget token (the fork accepts a ref)` step
  (`release.yml:139`), so the whole release is no longer lost to it.
- **Rotation:** the probe names it and the message is the instruction —
  _"rotate WINGET_TOKEN: a classic PAT with the public_repo scope, or a
  fine-grained PAT with Contents read+write and Pull requests read+write on the
  fork"_. Replace the repository secret `WINGET_TOKEN`, then re-run the
  probe.

### 2.4 Installer scope, and what it means for a silent upgrade

The NSIS setup is built `currentUser` (`tauri.conf.json:58`, ADR 0053): it
installs for the current user, under `%LOCALAPPDATA%\Programs\Vitals`, and asks
for no elevation. Releases up to 1.5.x were `both` — the same file installed for the
current user or for everyone, chosen at install time — and copies that came
from those still behave as the next two bullets describe. What the current
scope implies for an unattended upgrade:

- **A per-user copy updates silently.** The app runs unelevated, downloads and
  minisign-verifies the next installer, and applies it on quit with
  `/CurrentUser` under `RunAsInvoker`, so no UAC prompt appears
  (`docs/adr/0038-nsis-both-scopes-and-unprompted-updates.md:39`; `README.md:246-248`).
- **A per-machine copy asks for an administrator.** Its update needs
  `Program Files`, so Windows raises a UAC prompt on a desktop the app has just
  left; left unanswered, the next launch is the old version again
  (`docs/adr/0038-nsis-both-scopes-and-unprompted-updates.md:11-14`). Such a copy is told to use **Settings → About →
  Restart to update** instead (`README.md:247-248`).
- **winget installs the per-user copy.** The manifests say `Scope: user`, so the
  copy winget installs is the one that updates itself on exit with no UAC prompt
  (`packaging/winget/README.md:69`;
  `packaging/chocolatey/tools/chocolateyinstall.ps1:21-24`).

A signing change does not alter any of this: Authenticode adds a signature to
the same installer and does not change the scope it installs.

## Evidence

| Fact                                              | Command                                                                                                                        | Matching output                                                                                                                                                                                                                                                                               |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Licence is MIT                                    | `rg -n 'MIT License' LICENSE`                                                                                                  | `1:MIT License`                                                                                                                                                                                                                                                                               |
| Release builds from a tag                         | `rg -n 'tags:' -A1 .github/workflows/release.yml`                                                                              | `29:    tags:` / `30-      - 'v*'`                                                                                                                                                                                                                                                            |
| SBOM job and provenance attestation               | `rg -n '^  sbom:\|^  publish:\|attest-build-provenance\|CreateRef' .github/workflows/release.yml`                              | `626:  sbom:` / `846:  publish:` / `1067:        uses: actions/attest-build-provenance@…` / `130:      # not have the correct permissions to execute CreateRef`                                                                                                                               |
| Store product id                                  | `rg -n '9P95RK5VMGPJ' packaging/msix/listing/en-us.md`                                                                         | `13:(Store ID 9P95RK5VMGPJ — …)`                                                                                                                                                                                                                                                              |
| Package family name                               | `rg -n 'VitalsSystemMonitor_' docs/tracker.csv docs/TRACKER.md`                                                                | `docs/TRACKER.md:1367:… PackageFullName DragosCatalin.VitalsSystemMonitor_1.0.4.0_x64__2n7wyqrzmk6fw`                                                                                                                                                                                         |
| MSIX identity / publisher                         | `rg -n 'IdentityName\|Publisher =\|PublisherDisplayName =' packaging/msix/build-msix.ps1`                                      | `38:… 'DragosCatalin.VitalsSystemMonitor'`, `39:… 'CN=FE8B5839-…'`, `40:… 'Dragos Catalin'`                                                                                                                                                                                                   |
| winget identifier                                 | `rg -n 'PackageIdentifier: Vitals.Vitals' packaging/winget/manifests/…/Vitals.Vitals.yaml`                                     | `16:PackageIdentifier: Vitals.Vitals`                                                                                                                                                                                                                                                         |
| Play package name                                 | `rg -n 'packageName: app.vitals' .github/workflows/release.yml`                                                                | `1476:          packageName: app.vitals`                                                                                                                                                                                                                                                      |
| Four-part version derivation                      | `rg -n 'notmatch .\^\\d+\\.\\d+\\.\\d+\\.0.' packaging/msix/build-msix.ps1`                                                    | `54:if ($Version -notmatch '^\d+\.\d+\.\d+\.0$') { throw "Version must be N.N.N.0 for the Store, got '$Version'" }`                                                                                                                                                                           |
| Store version = core + `.0`                       | `read scripts/check-drift.ps1` (line 638)                                                                                      | `$storeVersion = (($currentVersion -split '[-+]')[0]) + '.0'`                                                                                                                                                                                                                                 |
| Installer scope                                   | `rg -n 'installMode' apps/desktop/src-tauri/tauri.conf.json`                                                                   | `58:        "installMode": "currentUser",` / `72:      "windows": { "installMode": "passive" }`                                                                                                                                                                                               |
| winget scope and switches                         | `rg -n 'Scope: user\|DefaultInstallLocation\|ElevationRequirement\|Custom: /CurrentUser' …/1.5.0/Vitals.Vitals.installer.yaml` | `21:Scope: user` / `27:  Custom: /CurrentUser` / `34:ElevationRequirement: elevatesSelf` / `36:  DefaultInstallLocation: '%LocalAppData%\Programs\Vitals'` — that manifest is a published release's; the location the current releases carry is written by `scripts/ci/winget-user-scope.ps1` |
| winget token permission and rotation              | `rg -n 'WINGET_FORK\|WINGET_ROTATION' scripts/release-doctor.mjs`                                                              | `799:const WINGET_FORK = 'dragoscv/winget-pkgs';` / `800-801: const WINGET_ROTATION = '  rotate WINGET_TOKEN: a classic PAT with the public_repo scope, or a fine-grained PAT with Contents read+write and Pull requests read+write on the fork';`                                            |
| Package-manager token scopes                      | `rg -n 'public_repo\|Contents: read and write' packaging/winget/README.md`                                                     | `90:Both tools need a GitHub token with public_repo scope…` / `110:**Contents: read and write** on that one repository.`                                                                                                                                                                      |
| Per-user updates silently                         | `rg -n 'updates silently' README.md`                                                                                           | `246:per-user install updates silently; one installed for everyone asks an`                                                                                                                                                                                                                   |
| Installer is unsigned today                       | `read .github/workflows/release.yml` (line 470)                                                                                | `Write-Host '::notice::No WINDOWS_CERTIFICATE: the installer is not Authenticode-signed.'`                                                                                                                                                                                                    |
| Cargo.toml package facts                          | `rg -n '^version = \|^license = \|^repository = ' Cargo.toml`                                                                  | `23:version = "1.6.0"` / `26:license = "MIT"` / `27:repository = "https://github.com/dragoscv/vitals"`                                                                                                                                                                                        |
| NSIS bundle and the .sig refusal                  | `rg -n 'bundles: nsis\|produced a .sig' .github/workflows/release.yml`                                                         | `353:            bundles: nsis` / `583:          if (Get-ChildItem $out -Filter '*.sig' -File) { throw 'a build leg produced a .sig; the updater key must not reach this job' }`                                                                                                              |
| Published asset names                             | `rg -n 'Vitals_x64-setup.exe\|vitals-cli_\|vitals-desktop.exe' .github/workflows/release.yml`                                  | `1825:'Vitals_x64-setup.exe', 'Vitals_arm64-setup.exe',` / `1826:"vitals-cli_$($env:VERSION)_x64.zip", …` / `612:            -Reference "target/$env:TARGET/release/vitals-desktop.exe"`                                                                                                      |
| What the installers carry                         | `rg -n 'sensors/.\|watchdog/.' apps/desktop/src-tauri/tauri.conf.json`                                                         | `41:      "sensors/": "sensors/",` / `42:      "watchdog/": "watchdog/",`                                                                                                                                                                                                                     |
| MSIX version input                                | `rg -n 'version:' .github/workflows/msix.yml`                                                                                  | `20:      version:` / `21:        description: 'Four-part Store version; the last part must be 0 (e.g. 1.3.0.0)'`                                                                                                                                                                             |
| Store listing product name                        | `rg -n 'Vitals: System Monitor' packaging/msix/listing/en-us.md`                                                               | `11:Vitals: System Monitor`                                                                                                                                                                                                                                                                   |
| winget manifest publisher                         | `rg -n 'Publisher:' packaging/winget/manifests/v/Vitals/Vitals/1.5.0/Vitals.Vitals.installer.yaml`                             | `32:  - Publisher: Dragos Catalin Vladulescu`                                                                                                                                                                                                                                                 |
| winget silent switch                              | `rg -n 'InstallerSwitches' packaging/winget/README.md`                                                                         | `70:\| \`InstallerSwitches.Silent\` \| \`/S\` \|`                                                                                                                                                                                                                                             |
| ADR 0038 update rows                              | `rg -n 'RunAsInvoker\|Restart to update' docs/adr/0038-nsis-both-scopes-and-unprompted-updates.md`                             | `39:\| per-user (\`HKCU\` entry names its folder) \| any \| \`/CurrentUser\`, under \`RunAsInvoker\` \|`/`41:· \| not on quit: "Restart to update" (prompt) \|`                                                                                                                               |
| winget installs the per-user copy                 | `rg -n 'per-user copy updates itself' packaging/chocolatey/tools/chocolateyinstall.ps1`                                        | `23:  # (ADR 0038): a per-user copy updates itself on quit without a UAC`                                                                                                                                                                                                                     |
| SignPath short form in the repo                   | `rg -n 'SignPath\|signpath\|is not allowed' docs/repository-security.md`                                                       | `72:### 4. Authenticode through SignPath Foundation (free for open source)` / `87:   \`signpath/github-action-submit-signing-request\`, pinned by SHA like every`/`98:added to that list first, or the job fails at setup with "is not allowed".`                                             |
| Publish waits on approval; winget is probed first | `rg -n 'environment: release\|Winget token' .github/workflows/release.yml`                                                     | `805:    environment: release` / `870:    environment: release` / `139:      - name: Winget token (the fork accepts a ref)`                                                                                                                                                                   |
| The Authenticode branch                           | `rg -n '### Authenticode' docs/releasing.md`                                                                                   | `635:### Authenticode, when a certificate exists`                                                                                                                                                                                                                                             |
| DCO sign-off required                             | `rg -n 'Developer Certificate of Origin\|Signed-off-by' CONTRIBUTING.md`                                                       | `172:## Developer Certificate of Origin` / `185:This adds a \`Signed-off-by: Your Name <you@example.com>\` line matching your`                                                                                                                                                                |
| One human contributor                             | `rg -n 'only human contributor' CONTRIBUTORS.md`                                                                               | `15:listed. At the time of writing, the maintainer is the only human contributor.`                                                                                                                                                                                                            |
| SBOMs are attested, not just written              | `rg -n 'attest-sbom' .github/workflows/release.yml`                                                                            | `1072:        uses: actions/attest-sbom@c604332985a26aa8cf1bdc465b92731239ec6b9e # v4` / `1078:` and `1084:` for the JavaScript and CLI SBOMs                                                                                                                                                 |
| Project name and Tauri identifier                 | `rg -n '"productName"\|"identifier"' apps/desktop/src-tauri/tauri.conf.json`                                                   | `3:  "productName": "Vitals",` / `5:  "identifier": "app.vitals.desktop",`                                                                                                                                                                                                                    |
| Maintainer, contact, website, repository          | `rg -n 'Dragos Catalin Vladulescu\|dragoscv12@gmail.com\|vitals.dragoscatalin.ro\|github.com/dragoscv/vitals' PRIVACY.md`      | `44:\| Name \| Dragos Catalin Vladulescu (natural person, maintainer of Vitals) \|` / `46:\| Contact \| dragoscv12@gmail.com` / `47:\| Source code \| github.com/dragoscv/vitals` / `48:\| Website \| vitals.dragoscatalin.ro`                                                                |
| Published contact address                         | `rg -n 'dragoscv12@gmail.com' SUPPORT.md`                                                                                      | `14:\| A question about privacy or your data \| dragoscv12@gmail.com. See PRIVACY.md. \|`                                                                                                                                                                                                     |
| The maintainer is one person                      | `rg -n 'creator and maintainer\|only human contributor' CONTRIBUTORS.md`                                                       | `6:  creator and maintainer` / `15:listed. At the time of writing, the maintainer is the only human contributor.`                                                                                                                                                                             |
| Licence badge in the README                       | `rg -n 'Licence: MIT' README.md`                                                                                               | `8:[![Licence: MIT](https://img.shields.io/badge/licence-MIT-blue)](LICENSE)`                                                                                                                                                                                                                 |
| Store and App Store links                         | `rg -n 'apps.microsoft.com/detail\|apps.apple.com/app' README.md`                                                              | `14:<a href="https://apps.microsoft.com/detail/9P95RK5VMGPJ">` / `28:\| iPhone (iOS 18+) \| [Vitals: System Monitor](https://apps.apple.com/app/id6827931747) on the App Store \|`                                                                                                            |
| Every action must be pinned to a full SHA         | `rg -n 'requires every action to be pinned\|full-length commit SHA' docs/repository-security.md`                               | `94:list of third-party ones, and **requires every action to be pinned to a**` / `95:full-length commit SHA**. Check with`                                                                                                                                                                    |
| Signing must run before the hashes                | `rg -n 'Sign updater bundles\|Stable download names\|Checksums' .github/workflows/release.yml`                                 | `977:      - name: Sign updater bundles` / `1046:      - name: Stable download names` / `1054:      - name: Checksums`                                                                                                                                                                        |
| winget: update, rewrite, submit                   | `rg -n 'komac update\|winget-user-scope\|komac submit' .github/workflows/release.yml`                                          | `1617:& $komac update Vitals.Vitals --version …--dry-run` / `1621:& pwsh -NoProfile -File scripts/ci/winget-user-scope.ps1 -Path $installer.FullName` / `1624:& $komac submit $installer.DirectoryName --yes`                                                                                 |
| winget token push probe                           | `rg -n 'can create a ref' scripts/release-doctor.mjs`                                                                          | `836:WINGET_TOKEN can create a ref on ${WINGET_FORK}`                                                                                                                                                                                                                                         |
| ADR 0038 decision                                 | `rg -n 'installMode: "both"' docs/adr/0038-nsis-both-scopes-and-unprompted-updates.md`                                         | `34:installMode: "both", and the app decides per copy how an update runs`                                                                                                                                                                                                                     |
| The live Store version is not in the tree         | `rg -n 'Submission 8' docs/tracker.csv`                                                                                        | `349:"S26-29",…the 1.5.0 Store submission is hand work and is recorded here…Submission 8 (id 1152921505702529570)…`                                                                                                                                                                           |

## Deliberately not done

- **No value is invented and no blank is guessed.** Anything that lives outside
  the repository is in §1.5 or §2.2, with how to read it.
- **No figure from a live store or a secret is stated as fact.** The live Store
  version, the live winget version and the token's ability to push are all
  unknown here (§2.2).
- **`docs/repository-security.md` §4 was not edited.** It already carries the
  short form of this application; this file is the complete version, and the
  two are meant to be read together rather than duplicated.
