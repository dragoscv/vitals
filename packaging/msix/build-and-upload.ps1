<#
.SYNOPSIS
  Builds the Store MSIX on a devcloud Windows workspace and copies it to the
  job bucket, so the package is made from a clean checkout on a clean
  machine rather than a developer's tree.

.DESCRIPTION
  Release build of the desktop app without bundles (the MSIX is laid out by
  build-msix.ps1 from target\release), then build-msix.ps1, then the
  metadata-token upload that scripts/screenshots/upload.mjs already uses.
  Prints the gs:// path of the package.

.EXAMPLE
  agentq run -Where cloud -Ws vitals-win -- pwsh -NoProfile -File packaging/msix/build-and-upload.ps1 -Version 1.1.1.0
#>
param(
  [Parameter(Mandatory)][string]$Version,
  [string]$Bucket = 'dragos-devcloud-devcloud-jobs'
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $root

# The devcloud runner sets CI=1, which the Tauri CLI parses as `--ci 1` and
# rejects (it wants true/false). Same fix as scripts/screenshots/run.ps1.
if ($env:CI) { $env:CI = 'true' }

pnpm --filter '@vitals/desktop' exec tauri build --no-bundle --config '{"bundle":{"createUpdaterArtifacts":false}}'
if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
cargo build --release -p vitals-watchdog
if ($LASTEXITCODE -ne 0) { throw "watchdog build failed ($LASTEXITCODE)" }

pwsh -NoProfile -File (Join-Path $PSScriptRoot 'build-msix.ps1') -Version $Version
if ($LASTEXITCODE -ne 0) { throw "build-msix failed ($LASTEXITCODE)" }

$out = Join-Path $root 'target\msix-upload'
New-Item -ItemType Directory -Force -Path $out | Out-Null
Get-ChildItem (Join-Path $root 'target\msix') -Filter *.msix | Copy-Item -Destination $out
Get-ChildItem $out | ForEach-Object { "{0} {1:N0} B {2}" -f $_.Name, $_.Length, (Get-FileHash $_.FullName).Hash }
node (Join-Path $root 'scripts\screenshots\upload.mjs') $out $Bucket
if ($LASTEXITCODE -ne 0) { throw "upload failed ($LASTEXITCODE)" }
