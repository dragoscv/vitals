<#
.SYNOPSIS
  Copies the versioned Windows installers to names that never change.

.DESCRIPTION
  Tauri names an installer after its version (Vitals_0.9.0-beta.1_x64-setup.exe),
  which is right for the release itself and wrong for a website: a link to it
  goes stale with every release. GitHub resolves
  /releases/latest/download/<name> to the newest release that has <name>, so
  an asset with a fixed name gives a download link that is correct forever:

    https://github.com/dragoscv/vitals/releases/latest/download/Vitals_x64-setup.exe

  Copies, not renames: the updater manifest, winget and Chocolatey all point
  at the versioned files, and a rename would break them.

  The copies carry no .sig. The updater never fetches them, and a signature
  file per copy would only be two more assets to explain.

.EXAMPLE
  pwsh -NoProfile -File scripts/stable-assets.ps1 -Directory publish -Version 0.9.0-beta.1
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Directory,
  [Parameter(Mandatory)][string]$Version
)
$ErrorActionPreference = 'Stop'

$Version = $Version -replace '^v', ''
$pairs = [ordered]@{
  "Vitals_${Version}_x64-setup.exe"   = 'Vitals_x64-setup.exe'
  "Vitals_${Version}_arm64-setup.exe" = 'Vitals_arm64-setup.exe'
}

# Missing is fatal: a release without one of these makes the "latest" link
# for that architecture fall back to an older release, silently serving an
# out-of-date installer. Checked for all before copying any, so a failure
# never leaves half the copies behind.
$missing = @($pairs.Keys | Where-Object { -not (Test-Path -LiteralPath (Join-Path $Directory $_)) })
if ($missing.Count -gt 0) {
  throw "missing $($missing -join ', ') in $Directory; the stable link would serve an older release"
}

foreach ($source in $pairs.Keys) {
  Copy-Item -LiteralPath (Join-Path $Directory $source) -Destination (Join-Path $Directory $pairs[$source]) -Force
  Write-Host "$source -> $($pairs[$source])"
}
