# Package-manager channels

The release workflow (`.github/workflows/release.yml`) keeps winget, Scoop and
Chocolatey current after the **first** listing exists. Each channel is off
until its repository variable is `true` and its secret is set; a missing
secret skips the job with a notice rather than failing the release. The full
list of secrets and variables is in [docs/releasing.md](../../docs/releasing.md).

All three point at the **versioned** installers on the GitHub release
(`Vitals_<version>_x64-setup.exe`, `Vitals_<version>_arm64-setup.exe`), never
at the stable-named copies (`Vitals_x64-setup.exe`): a package manager needs a
URL whose bytes never change, and the stable name's bytes change with every
release.

## winget

### First submission (by hand, once)

`vedantmgoyal9/winget-releaser` only _updates_ a package that is already in
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). The first
manifest is submitted by hand, after the first tagged release is published.

With [Komac](https://github.com/russellbanks/Komac) (`winget install komac`):

```powershell
$v = '0.9.0-beta.1'
$base = "https://github.com/dragoscv/vitals/releases/download/v$v"
komac new Vitals.Vitals --version $v `
  --urls "$base/Vitals_${v}_x64-setup.exe" "$base/Vitals_${v}_arm64-setup.exe" `
  --package-locale en-US `
  --publisher 'Dragos Catalin Vladulescu' `
  --package-name Vitals `
  --license MIT `
  --short-description 'A modern, fast system monitor and task manager.' `
  --publisher-url https://github.com/dragoscv `
  --package-url https://vitals.dragoscatalin.ro `
  --license-url https://github.com/dragoscv/vitals/blob/main/LICENSE `
  --release-notes-url "https://github.com/dragoscv/vitals/releases/tag/v$v" `
  --submit
```

Or with `wingetcreate` (`winget install wingetcreate`), which asks for each
field interactively:

```powershell
wingetcreate new "$base/Vitals_${v}_x64-setup.exe" "$base/Vitals_${v}_arm64-setup.exe"
```

Komac and wingetcreate both detect most installer fields from the file. Check
that the generated manifests say:

| Field                                  | Value                                                    |
| -------------------------------------- | -------------------------------------------------------- |
| `PackageIdentifier`                    | `Vitals.Vitals`                                          |
| `Publisher`                            | `Dragos Catalin Vladulescu`                              |
| `PackageName`                          | `Vitals`                                                 |
| `License`                              | `MIT`                                                    |
| `InstallerType`                        | `nullsoft`                                               |
| `Scope`                                | `machine` (the installer is per-machine, ADR 0025)       |
| `InstallerSwitches.Silent`             | `/S`                                                     |
| `InstallerSwitches.SilentWithProgress` | `/S`                                                     |
| `Architecture`                         | `x64` and `arm64`, one installer each                    |
| `ReleaseNotesUrl`                      | `https://github.com/dragoscv/vitals/releases/tag/v<ver>` |
| `UpgradeBehavior`                      | `install` (NSIS upgrades in place)                       |

Both tools need a GitHub token with `public_repo` scope to open the pull
request; they prompt for one or read `GITHUB_TOKEN`. Review in winget-pkgs
takes from hours to a few days. The validation pipeline runs the installer in
a sandbox, so SmartScreen does not block it — an unsigned installer is
accepted, though it may be flagged for manual review.

### Every release afterwards

Set repository variable `PUBLISH_WINGET=true` and secret `WINGET_TOKEN` (a
classic personal access token with `public_repo`, from the account that owns
a fork of `microsoft/winget-pkgs` — the action pushes to that fork and opens
the pull request from it).

## Scoop

The workflow renders `packaging/scoop/vitals.json` with the version, URLs and
hashes and pushes it to `bucket/vitals.json` in
[dragoscv/scoop-vitals](https://github.com/dragoscv/scoop-vitals). Create that
repository once (public, empty, with a `bucket/` directory), then set
`PUBLISH_SCOOP=true` and `SCOOP_BUCKET_TOKEN` (a fine-grained token with
**Contents: read and write** on that one repository).

Users then run:

```powershell
scoop bucket add vitals https://github.com/dragoscv/scoop-vitals
scoop install vitals
```

### What Scoop actually installs, and what is uncertain

Scoop never runs installers; it unpacks them. The manifest appends `#/dl.7z`
to the installer URL, which tells Scoop to treat the NSIS `.exe` as an archive
and extract it with 7-Zip.

**Verified** (2026-09-28, 7-Zip 24 against a local `Vitals_*_x64-setup.exe`):
7-Zip reads it as `Type = Nsis`, `SubType = NSIS-3 Unicode`, and lists
`vitals-desktop.exe`, `uninstall.exe` and `$PLUGINSDIR\*`. The manifest's
`pre_install` deletes the last two, leaving the application.

**Not verified** — no end-to-end `scoop install` has been run:

- **The per-machine hooks do not run.** Extraction skips
  `windows/hooks.nsh`, so there is no "close the running copy" step on
  upgrade. Scoop's own upgrade fails if `vitals-desktop.exe` is running,
  which is the safe failure. The install also has no Program Files entry,
  Start-menu entry beyond Scoop's shortcut, or uninstaller registration.
- **WebView2 is not bootstrapped.** The installer downloads it if absent;
  extraction does not. Windows 11 and current Windows 10 ship it, so this
  matters only on stripped-down machines.
- **The app's own updater** may offer the next version and run the full
  per-machine installer over a Scoop copy, producing two installations. The
  manifest's `notes` tell users to update through Scoop. A real fix is for
  the app to detect it runs from a `scoop\apps` path and leave updates to
  Scoop; that is a code change, outside packaging.
- Whether a future signed installer, or a Tauri NSIS template change,
  remains 7-Zip-extractable. If it stops being extractable the Scoop job
  still publishes a manifest, and installs fail with a 7-Zip error. The
  first thing to check after any Tauri upgrade is `7z l <setup.exe>`.

If any of these proves unacceptable, the alternative is a portable `.zip`
release asset (the bare `vitals-desktop.exe` plus resources) built in the
workflow, which Scoop handles natively.

## Chocolatey

`packaging/chocolatey/` is a template: `vitals.nuspec` plus
`tools/chocolateyinstall.ps1` (downloads the versioned installer, checks its
SHA-256 from `SHA256SUMS.txt`, runs it with `/S`; picks the ARM64 build on
ARM64) and `tools/chocolateyuninstall.ps1`.

Chocolatey versions cannot contain a dot in the pre-release label, so
`0.9.0-beta.1` is packed as `0.9.0-beta1`.

Create an account at <https://community.chocolatey.org/account/Register>,
copy the API key from <https://community.chocolatey.org/account>, and set it
as secret `CHOCO_API_KEY` with `PUBLISH_CHOCO=true`. Every push, including
the first, goes through Chocolatey's moderation queue (automated checks, then
a human for new packages); expect days for the first version.

To test a package locally before enabling the job (administrator shell):

```powershell
choco pack packaging/chocolatey/vitals.nuspec --version 0.9.0-beta1 --outputdirectory .copilot-tmp
# Fill the placeholders in a copy first, or the checksum check fails.
choco install vitals --source .copilot-tmp -y
```
