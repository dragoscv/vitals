<#
.SYNOPSIS
  Fills the SHA-256 hashes the three package channels are still missing, from
  the assets of a published GitHub release.

.DESCRIPTION
  The 1.5.0 manifests are prepared, but the release they point at is waiting on
  the owner's approval, so its installers do not exist yet and no hash can be
  written down. A wrong hash is worse than a missing one, so every hash field
  under packaging/ is a token or empty, and this script fills all of them at
  once from the release's own SHA256SUMS.txt -- the same file the workflow's
  channel jobs trust.

  It fills, writing rendered copies under -OutDir (never editing the templates
  in place):
    * packaging/winget/manifests/v/Vitals/Vitals/<version>/Vitals.Vitals.installer.yaml
      (__SHA256_X64__ and __SHA256_ARM64__), with the version and locale
      manifests copied alongside so the set is complete;
    * packaging/scoop/vitals.json            -> <OutDir>/scoop/vitals.json
    * packaging/chocolatey/tools/chocolateyinstall.ps1
      (__URL64__ / __CHECKSUM64__ / __URLARM64__ / __CHECKSUMARM64__)
                                             -> <OutDir>/chocolatey/tools/...

  If an installer is present in -Assets it is hashed and compared with
  SHA256SUMS.txt before its value is used, so a tampered sums file fails the
  run instead of publishing a hash nobody can verify.

  Use -WhatIf to see what it would write without writing anything.

.EXAMPLE
  # Dry run against whatever assets are already downloaded:
  pwsh -NoProfile -File packaging/fill-hashes.ps1 -Assets .copilot-tmp/v1.5.0 -Version 1.5.0 -WhatIf

.EXAMPLE
  # Real run after the release is approved and its assets downloaded:
  pwsh -NoProfile -File packaging/fill-hashes.ps1 -Assets .copilot-tmp/v1.5.0 -Version 1.5.0
#>
[CmdletBinding(SupportsShouldProcess)]
param(
  [Parameter(Mandatory)][string]$Assets,
  [Parameter(Mandatory)][string]$Version,
  [string]$Repository = 'dragoscv/vitals',
  [string]$OutDir
)
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $repoRoot ".copilot-tmp/package-submit/$Version" }

$Assets = (Resolve-Path -LiteralPath $Assets).Path
$sumsPath = Join-Path $Assets 'SHA256SUMS.txt'
if (-not (Test-Path -LiteralPath $sumsPath)) { throw "no SHA256SUMS.txt in $Assets" }

$sums = @{}
foreach ($line in Get-Content -LiteralPath $sumsPath) {
  if ($line -match '^\s*$') { continue }
  $h, $n = $line -split '\s+', 2
  $sums[$n.Trim()] = $h.ToLower()
}

$tag = "v$Version"
$url = @{}
$sha = @{}
foreach ($a in 'x64', 'arm64') {
  $name = "Vitals_${Version}_${a}-setup.exe"
  if (-not $sums.ContainsKey($name)) { throw "$name is not in SHA256SUMS.txt" }
  # A downloaded installer is the strongest check: hash it and compare.
  $file = Join-Path $Assets $name
  if (Test-Path -LiteralPath $file) {
    $actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $sums[$name]) { throw "$name hashes to $actual but SHA256SUMS.txt says $($sums[$name])" }
  }
  $url[$a] = "https://github.com/$Repository/releases/download/$tag/$name"
  $sha[$a] = $sums[$name]
}
Write-Host "version  $Version"
Write-Host "x64      $($sha.x64)  $($url.x64)"
Write-Host "arm64    $($sha.arm64)  $($url.arm64)"

function Write-File([string]$Path, [string]$Content) {
  if (-not $PSCmdlet.ShouldProcess($Path, 'write')) { return }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
  [IO.File]::WriteAllText($Path, $Content)
  Write-Host "wrote    $Path"
}

# --- winget -----------------------------------------------------------------
$wingetDir = Join-Path $repoRoot "packaging/winget/manifests/v/Vitals/Vitals/$Version"
$installer = Join-Path $wingetDir 'Vitals.Vitals.installer.yaml'
if (-not (Test-Path -LiteralPath $installer)) { throw "winget manifest set missing: $installer" }
$text = [IO.File]::ReadAllText($installer)
if ($text -notmatch '__SHA256_X64__' -or $text -notmatch '__SHA256_ARM64__') {
  throw "$installer has no hash tokens (already filled?)"
}
$text = $text.Replace('__SHA256_X64__', $sha.x64).Replace('__SHA256_ARM64__', $sha.arm64)
$outWinget = Join-Path $OutDir "winget/manifests/v/Vitals/Vitals/$Version"
Write-File (Join-Path $outWinget 'Vitals.Vitals.installer.yaml') $text
foreach ($f in 'Vitals.Vitals.yaml', 'Vitals.Vitals.locale.en-US.yaml') {
  Write-File (Join-Path $outWinget $f) ([IO.File]::ReadAllText((Join-Path $wingetDir $f)))
}

# --- scoop ------------------------------------------------------------------
$scoopTpl = Join-Path $repoRoot 'packaging/scoop/vitals.json'
$m = Get-Content -LiteralPath $scoopTpl -Raw | ConvertFrom-Json
$m.version = $Version
foreach ($pair in @(@('64bit', 'x64'), @('arm64', 'arm64'))) {
  $m.architecture.($pair[0]).url = "$($url[$pair[1]])#/dl.7z"
  $m.architecture.($pair[0]).hash = $sha[$pair[1]]
}
Write-File (Join-Path $OutDir 'scoop/vitals.json') ($m | ConvertTo-Json -Depth 10)

# --- chocolatey -------------------------------------------------------------
$chocoSrc = Join-Path $repoRoot 'packaging/chocolatey'
$chocoDst = Join-Path $OutDir 'chocolatey'
$install = Join-Path $chocoSrc 'tools/chocolateyinstall.ps1'
$choco = [IO.File]::ReadAllText($install)
$choco = $choco.Replace('__URL64__', $url.x64).Replace('__CHECKSUM64__', $sha.x64)
$choco = $choco.Replace('__URLARM64__', $url.arm64).Replace('__CHECKSUMARM64__', $sha.arm64)
Write-File (Join-Path $chocoDst 'tools/chocolateyinstall.ps1') $choco
Write-File (Join-Path $chocoDst 'tools/chocolateyuninstall.ps1') ([IO.File]::ReadAllText((Join-Path $chocoSrc 'tools/chocolateyuninstall.ps1')))
Write-File (Join-Path $chocoDst 'vitals.nuspec') ([IO.File]::ReadAllText((Join-Path $chocoSrc 'vitals.nuspec')))

Write-Host ""
Write-Host "rendered for $Version under $OutDir"
