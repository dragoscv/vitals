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

.PARAMETER IdentityName / Publisher / PublisherDisplayName
  From Partner Center > the app > Product identity.
#>
param(
  [Parameter(Mandatory)][string]$Version,
  [string]$IdentityName = 'DragosCatalinVladulescu.Vitals',
  [string]$Publisher = 'CN=Dragos Catalin Vladulescu',
  [string]$PublisherDisplayName = 'Dragos Catalin Vladulescu',
  [string]$ReleaseDir,
  [string]$OutDir,
  [string]$TestCertificate
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $ReleaseDir) { $ReleaseDir = Join-Path $root 'target\release' }
if (-not $OutDir) { $OutDir = Join-Path $root 'target\msix' }
if ($Version -notmatch '^\d+\.\d+\.\d+\.0$') { throw "Version must be N.N.N.0 for the Store, got '$Version'" }

$sdk = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName 'x64\makeappx.exe') } |
  Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { throw 'Windows SDK with makeappx.exe not found' }
$makeappx = Join-Path $sdk.FullName 'x64\makeappx.exe'
$makepri = Join-Path $sdk.FullName 'x64\makepri.exe'
$signtool = Join-Path $sdk.FullName 'x64\signtool.exe'

$app = Join-Path $ReleaseDir 'vitals-desktop.exe'
$watchdog = Join-Path $ReleaseDir 'vitals-watchdog.exe'
foreach ($f in $app, $watchdog) { if (-not (Test-Path $f)) { throw "missing $f — run the release build first" } }

$layout = Join-Path $OutDir 'layout'
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
  Replace('{{PUBLISHER_DISPLAY_NAME}}', $PublisherDisplayName).Replace('{{VERSION}}', $Version)
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest)

$priConfig = Join-Path $OutDir 'priconfig.xml'
& $makepri createconfig /cf $priConfig /dq en-US /pv 10.0.0 /o | Out-Null
if ($LASTEXITCODE) { throw "makepri createconfig failed ($LASTEXITCODE)" }
& $makepri new /pr $layout /cf $priConfig /mn (Join-Path $layout 'AppxManifest.xml') /of (Join-Path $layout 'resources.pri') /o | Out-Null
if ($LASTEXITCODE) { throw "makepri new failed ($LASTEXITCODE)" }

$package = Join-Path $OutDir "Vitals_$($Version)_x64.msix"
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
