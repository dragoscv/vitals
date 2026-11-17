<#
.SYNOPSIS
  Builds the Microsoft Store package (MSIX) from a release build.

.DESCRIPTION
  Tauri has no MSIX bundler, so this lays the package out by hand from the
  release executables and the generated brand assets, indexes the scaled
  assets with makepri and packs with makeappx. The result is UNSIGNED on
  purpose: the Store signs Store packages with its own certificate. Pass
  -TestCertificate to sign a copy for a local install test.

  Store policy differences are runtime checks in the same binary
  (distribution.rs), so nothing is recompiled here. The sensors service and
  driver are simply not laid out.

.PARAMETER Version
  Four-part numeric version, e.g. 0.9.0.0 (the Store requires the last part 0).

.PARAMETER Architecture
  Which MSIX packages to lay out. One package per architecture goes in the
  same Partner Center submission: a submission with only an x64 package
  leaves every ARM64 machine running the x64 build under emulation. Each
  architecture needs its own release build —
  `tauri build --no-bundle --target <triple>` — and is read from that
  target's release directory, unless -ReleaseDir names one (single
  architecture only).

.PARAMETER IdentityName / Publisher / PublisherDisplayName
  From Partner Center > the app > Product identity. Defaults are the reserved
  product 9P95RK5VMGPJ ("Vitals: System Monitor"; "Vitals" alone was taken).

.EXAMPLE
  pwsh -NoProfile -File packaging/msix/build-msix.ps1 -Version 1.3.0.0
  pwsh -NoProfile -File packaging/msix/build-msix.ps1 -Version 1.3.0.0 -Architecture x64,arm64
#>
param(
  [Parameter(Mandatory)][string]$Version,
  [string]$IdentityName = 'DragosCatalin.VitalsSystemMonitor',
  [string]$Publisher = 'CN=FE8B5839-E0F9-495F-854C-53165710EB07',
  [string]$PublisherDisplayName = 'Dragos Catalin',
  # x64, arm64, or both. Split on commas rather than declared as a
  # ValidateSet array: a `pwsh -File` call delivers `-Architecture x64,arm64`
  # as ONE string, and a ValidateSet refuses it before any code runs
  # (measured 2026-11-17 — the same call through `pwsh -Command` is an array
  # and always was). Both shapes are accepted, and a typo still fails loudly.
  [string[]]$Architecture = @('x64'),
  [string]$ReleaseDir,
  [string]$OutDir,
  [string]$TestCertificate
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $OutDir) { $OutDir = Join-Path $root 'target\msix' }
if ($Version -notmatch '^\d+\.\d+\.\d+\.0$') { throw "Version must be N.N.N.0 for the Store, got '$Version'" }
$architectures = @($Architecture | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Select-Object -Unique)
$unknown = @($architectures | Where-Object { $_ -notin @('x64', 'arm64') })
if ($unknown.Count) { throw "unknown architecture: $($unknown -join ', ') (x64, arm64)" }
if (-not $architectures.Count) { throw 'no architecture given' }
# A release directory belongs to one architecture, so one -ReleaseDir cannot
# serve two packages; make that a stated error rather than a silent
# mispackage (both .msix files would carry the same binaries).
if ($ReleaseDir -and $architectures.Count -gt 1) { throw '-ReleaseDir names one architecture; run once per architecture instead' }

$sdk = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName 'x64\makeappx.exe') } |
  Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { throw 'Windows SDK with makeappx.exe not found' }
$makeappx = Join-Path $sdk.FullName 'x64\makeappx.exe'
$makepri = Join-Path $sdk.FullName 'x64\makepri.exe'
$signtool = Join-Path $sdk.FullName 'x64\signtool.exe'

# Where a `tauri build --target <triple>` leaves the executables. The x64
# entry is the host directory: an x64 build without --target, which is what
# build-and-upload.ps1 does for this architecture.
$releaseDirs = @{
  x64   = 'target\release'
  arm64 = 'target\aarch64-pc-windows-msvc\release'
}

foreach ($arch in $architectures) {
  $dir = if ($ReleaseDir) { $ReleaseDir } else { Join-Path $root $releaseDirs[$arch] }
  $app = Join-Path $dir 'vitals-desktop.exe'
  $watchdog = Join-Path $dir 'vitals-watchdog.exe'
  foreach ($f in $app, $watchdog) { if (-not (Test-Path $f)) { throw "missing $f — build the $arch release first" } }

  # One layout per architecture: makeappx reads a directory, so two packages
  # cannot share one.
  $layout = Join-Path $OutDir "layout-$arch"
  if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
  New-Item -ItemType Directory -Force (Join-Path $layout 'Assets'), (Join-Path $layout 'watchdog') | Out-Null

  Copy-Item $app $layout
  Copy-Item $watchdog (Join-Path $layout 'watchdog')
  Copy-Item (Join-Path $root 'LICENSE') $layout
  Copy-Item (Join-Path $root 'THIRD_PARTY_NOTICES.md') $layout

  # Assets: the base names the manifest references, with MRT qualifiers
  # (scale-N, targetsize-N, altform-*) that makepri indexes.
  $assets = Join-Path $root 'brand\icons\msix'
  Get-ChildItem $assets -Filter *.png | ForEach-Object { Copy-Item $_.FullName (Join-Path $layout 'Assets') }

  $manifest = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'AppxManifest.template.xml'))
  $manifest = $manifest.Replace('{{IDENTITY_NAME}}', $IdentityName).Replace('{{PUBLISHER}}', $Publisher).
    Replace('{{PUBLISHER_DISPLAY_NAME}}', $PublisherDisplayName).Replace('{{VERSION}}', $Version).
    Replace('{{PROCESSOR_ARCHITECTURE}}', $arch)
  # The template's own comment says "{{...}} placeholders", so match a real
  # one: CAPS inside the braces.
  if ($manifest -match '\{\{[A-Z_]+\}\}') { throw "an AppxManifest placeholder was not filled in: $($Matches[0])" }
  [IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest)

  $priConfig = Join-Path $OutDir "priconfig-$arch.xml"
  & $makepri createconfig /cf $priConfig /dq en-US /pv 10.0.0 /o | Out-Null
  if ($LASTEXITCODE) { throw "makepri createconfig failed ($LASTEXITCODE)" }
  & $makepri new /pr $layout /cf $priConfig /mn (Join-Path $layout 'AppxManifest.xml') /of (Join-Path $layout 'resources.pri') /o | Out-Null
  if ($LASTEXITCODE) { throw "makepri new failed ($LASTEXITCODE)" }

  $package = Join-Path $OutDir "Vitals_$($Version)_$arch.msix"
  & $makeappx pack /d $layout /p $package /o | Out-Null
  if ($LASTEXITCODE) { throw "makeappx pack failed ($LASTEXITCODE)" }
  Write-Host "package: $package ($([math]::Round((Get-Item $package).Length / 1MB, 1)) MB)"

  if ($TestCertificate) {
    $signed = $package -replace '\.msix$', '.test-signed.msix'
    Copy-Item $package $signed -Force
    & $signtool sign /fd SHA256 /a /f $TestCertificate $signed | Out-Null
    if ($LASTEXITCODE) { throw "signtool failed ($LASTEXITCODE)" }
    Write-Host "test-signed: $signed"
  }
}
