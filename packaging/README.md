# Packaging channels

How Vitals reaches winget, Scoop and Chocolatey. The release workflow
(`.github/workflows/release.yml`) keeps each one current **after the first
listing exists** — its `winget` job runs `komac update`, which only updates a
package already in `microsoft/winget-pkgs`; `scoop` renders
`packaging/scoop/vitals.json` and pushes it to the bucket; `chocolatey` packs
`packaging/chocolatey` and pushes to the community feed. Each job is off until
its repository variable is `true` and its secret is set.

> **The three channels already exist** (`dragoscv/winget-pkgs` fork → PRs to
> `microsoft/winget-pkgs`; `dragoscv/scoop-vitals`; community.chocolatey.org).
> They were last updated at 1.3.x; **1.4.0 was never pushed**, so 1.5.0 is a
> catch-up, not a first submission. The per-channel detail is in
> [winget/README.md](winget/README.md).

Everything points at the **versioned** installers on the GitHub release
(`Vitals_<version>_x64-setup.exe`, `Vitals_<version>_arm64-setup.exe`), never at
the stable-named copies: a package manager needs a URL whose bytes never change.

## Hashes

The release for a version has no assets until its `publish` job runs (a tag
waits on the owner's approval of the `release` environment). No manifest here
carries a guessed hash — winget's `InstallerSha256` is the token
`__SHA256_X64__` / `__SHA256_ARM64__`, and the Scoop/Chocolatey templates are
filled by the workflow. To fill the winget set by hand, download the release's
assets and run:

```powershell
pwsh -NoProfile -File packaging/fill-hashes.ps1 -Assets .copilot-tmp/v1.5.0 -Version 1.5.0
```

It reads the release's `SHA256SUMS.txt` (verifying any installer that is present
against it), writes a fully-filled copy of each channel under
`.copilot-tmp/package-submit/1.5.0/`, and never edits the templates. `-WhatIf`
prints what it would write.

## The exact commands, by channel

Replace `1.5.0` with the version. All three need the release **published**
first, so its `SHA256SUMS.txt` and installers exist.

**winget** — the package is already in `microsoft/winget-pkgs`, so update it:

```powershell
# With Komac (the tool the release job uses; also what generates the manifest):
komac update Vitals.Vitals --version 1.5.0 `
  --urls "https://github.com/dragoscv/vitals/releases/download/v1.5.0/Vitals_1.5.0_x64-setup.exe" `
         "https://github.com/dragoscv/vitals/releases/download/v1.5.0/Vitals_1.5.0_arm64-setup.exe" `
  --release-notes-url https://github.com/dragoscv/vitals/releases/tag/v1.5.0 --submit

# Or, without Komac, with wingetcreate (installed on this machine):
wingetcreate update Vitals.Vitals --version 1.5.0 `
  --urls "https://github.com/dragoscv/vitals/releases/download/v1.5.0/Vitals_1.5.0_x64-setup.exe" `
         "https://github.com/dragoscv/vitals/releases/download/v1.5.0/Vitals_1.5.0_arm64-setup.exe" `
  --submit
```

Either tool pushes to the fork and opens a pull request into
`microsoft/winget-pkgs`. The hand-prepared set in
`packaging/winget/manifests/v/Vitals/Vitals/1.5.0/` exists as a fallback and as
the reference for the exact field shape; `fill-hashes.ps1` completes it.

**Scoop** — render the manifest and push it to the bucket:

```powershell
pwsh -NoProfile -File packaging/fill-hashes.ps1 -Assets .copilot-tmp/v1.5.0 -Version 1.5.0
# copy .copilot-tmp/package-submit/1.5.0/scoop/vitals.json over
#   dragoscv/scoop-vitals: bucket/vitals.json, commit and push.
```

**Chocolatey** — pack the filled copy and push it (administrator shell):

```powershell
pwsh -NoProfile -File packaging/fill-hashes.ps1 -Assets .copilot-tmp/v1.5.0 -Version 1.5.0
choco pack .copilot-tmp/package-submit/1.5.0/chocolatey/vitals.nuspec --version 1.5.0 --outputdirectory out
choco push out/vitals.1.5.0.nupkg --source https://push.chocolatey.org/ --api-key <your key>
```

A pre-release (`0.9.0-beta.1`) packs as `0.9.0-beta1` — Chocolatey versions may
not carry a dot in the pre-release label.

## What to add to `docs/releasing.md`

`docs/releasing.md` documents the channels but not this catch-up path. It should
gain a short subsection under **Channels**: that the channels are updated by the
gated `winget`/`scoop`/`chocolatey` jobs, that a version can be carried forward
by hand with the three command blocks above, and that `fill-hashes.ps1` is the
one step that turns the templates plus a published `SHA256SUMS.txt` into
submittable artefacts. (This file was left unedited on purpose.)
