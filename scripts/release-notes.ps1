<#
.SYNOPSIS
  Writes the body of a GitHub release from CHANGELOG.md.

.DESCRIPTION
  CHANGELOG.md is hand-curated and written for people, so it — not the
  commit log — is what a release page shows. This takes the section for the
  version (`## [0.9.0-beta.1]`), falls back to `## [Unreleased]` when the
  version has not been cut yet, and appends how to verify the download.

  The fallback exists because a beta is often tagged before anyone renames
  the Unreleased heading; an empty release page is worse than one that says
  "Unreleased". It prints a warning so the omission is visible in the log.

  GitHub's generated notes (categorised by .github/release.yml) are appended
  by the release action after this body, so the page carries both the
  curated story and the list of pull requests.

.EXAMPLE
  pwsh -NoProfile -File scripts/release-notes.ps1 -Version 0.9.0-beta.1 -Tag v0.9.0-beta.1 -Out notes.md
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Version,
  [string]$Tag = "v$Version",
  [string]$Repository = 'dragoscv/vitals',
  [string]$Changelog = (Join-Path (Split-Path -Parent $PSScriptRoot) 'CHANGELOG.md'),
  [Parameter(Mandatory)][string]$Out,
  # A nightly gets a warning banner and the Unreleased section.
  [switch]$Nightly
)
$ErrorActionPreference = 'Stop'
$Version = $Version -replace '^v', ''

function Get-Section([string[]]$lines, [string]$heading) {
  # Level-2 headings only: the Unreleased section's dated entries are
  # level 3 and belong inside it.
  $pattern = '^##\s+\[' + [regex]::Escape($heading) + '\]'
  $start = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match $pattern) { $start = $i + 1; break }
  }
  if ($start -lt 0) { return $null }
  $end = $lines.Count
  for ($i = $start; $i -lt $lines.Count; $i++) {
    # The next version, or the link-reference block Keep a Changelog puts
    # at the bottom (`[0.9.0]: https://...`).
    if ($lines[$i] -match '^##\s+\[' -or $lines[$i] -match '^\[[^\]]+\]:\s+https?://') { $end = $i; break }
  }
  ($lines[$start..($end - 1)] -join "`n").Trim()
}

$lines = [IO.File]::ReadAllLines($Changelog)
$body = $null
if (-not $Nightly) { $body = Get-Section $lines $Version }
if ([string]::IsNullOrWhiteSpace($body)) {
  if (-not $Nightly) {
    Write-Warning "CHANGELOG.md has no '## [$Version]' section; using [Unreleased]. Rename the heading before the next tag."
  }
  $body = Get-Section $lines 'Unreleased'
}
if ([string]::IsNullOrWhiteSpace($body)) {
  throw "CHANGELOG.md has neither a [$Version] nor an [Unreleased] section with content"
}

$banner = if ($Nightly) {
  @'
> **Nightly build.** Built from `main` every night something lands. Unreleased,
> and it may be broken. The installed app does not update itself from nightlies.

'@
} elseif ($Version -match '-') {
  @'
> **Public beta.** Feature-complete for this release and tested, but still
> collecting reports before 1.0. The installed app updates itself to newer
> betas and to the final release.

'@
} else { '' }

$verify = @"

---

### Downloads

| Windows | Installer |
| ------- | --------- |
| Intel / AMD (x64) | [Vitals_x64-setup.exe](https://github.com/$Repository/releases/download/$Tag/Vitals_x64-setup.exe) |
| ARM64 (Snapdragon) | [Vitals_arm64-setup.exe](https://github.com/$Repository/releases/download/$Tag/Vitals_arm64-setup.exe) |

### Verifying this download

Windows may show a SmartScreen warning on first run (_Windows protected your
PC_) because the installer is not yet signed with a commercial certificate.
Choose **More info → Run anyway**. That is expected — see
[docs/distribution.md](https://github.com/$Repository/blob/main/docs/distribution.md).

You do not have to take our word for what is in the binary. With the
[GitHub CLI](https://cli.github.com):

``````powershell
gh attestation verify .\Vitals_x64-setup.exe --repo $Repository
``````

This proves the file was built by this repository's release workflow from the
tagged commit, and nowhere else. Checksums for every file are in
``SHA256SUMS.txt``:

``````powershell
(Get-FileHash .\Vitals_x64-setup.exe -Algorithm SHA256).Hash.ToLower()
``````

Software bills of materials (CycloneDX) for the Rust and JavaScript
dependencies are attached as ``sbom-rust.cdx.json`` and ``sbom-js.cdx.json``.
"@

if ($Nightly) {
  # Nightly assets are only the versioned names; the stable copies are made
  # for tagged releases.
  $verify = $verify -replace '(?s)### Downloads.*?(?=### Verifying)', ''
}

$text = (@($banner.Trim(), $body, $verify.Trim()) | Where-Object { $_ }) -join "`n`n"
$text = $text.Trim() + "`n"
[IO.File]::WriteAllText($Out, $text, [Text.UTF8Encoding]::new($false))
Write-Host "wrote $Out ($($text.Length) chars)"
