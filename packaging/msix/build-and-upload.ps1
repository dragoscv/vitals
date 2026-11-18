<#
.SYNOPSIS
  Builds the Store MSIX on a devcloud Windows workspace and copies it to the
  job bucket, so the package is made from a clean checkout on a clean
  machine rather than a developer's tree.

.DESCRIPTION
  Release build of the desktop app without bundles, once per architecture —
  x64 without --target, ARM64 with --target so it lands in its own directory
  — then build-msix.ps1 for both, then the metadata-token upload that
  scripts/screenshots/upload.mjs already uses. Prints the gs:// path of the
  packages.

  Both architectures go into one Partner Center submission: an x64-only
  submission is what leaves an ARM64 machine running the x64 build under
  emulation.

.EXAMPLE
  agentq run -Where cloud -Ws vitals-win -- pwsh -NoProfile -File packaging/msix/build-and-upload.ps1 -Version 1.2.0.0

.EXAMPLE
  pwsh -NoProfile -File packaging/msix/build-and-upload.ps1 -Version 1.3.0.0 -NoUpload
#>
param(
  [Parameter(Mandatory)][string]$Version,
  [string]$Bucket = 'dragos-devcloud-devcloud-jobs',
  # Split on commas, not a ValidateSet: a `pwsh -File` call delivers
  # `-Architecture x64,arm64` as one string (see build-msix.ps1).
  [string[]]$Architecture = @('x64', 'arm64'),
  # Leave the packages on disk instead of uploading them to the devcloud
  # bucket. The hosted CI run (msix.yml) uses this: it publishes them as
  # workflow artefacts, and it is the only place that reliably has the ARM64
  # MSVC toolchain, so it is now the normal route for a Store package.
  [switch]$NoUpload
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $root

$architectures = @($Architecture | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Select-Object -Unique)
$unknown = @($architectures | Where-Object { $_ -notin @('x64', 'arm64') })
if ($unknown.Count) { throw "unknown architecture: $($unknown -join ', ') (x64, arm64)" }
if (-not $architectures.Count) { throw 'no architecture given' }

# The devcloud runner sets CI=1, which the Tauri CLI parses as `--ci 1` and
# rejects (it wants true/false). Same fix as scripts/screenshots/run.ps1.
if ($env:CI) { $env:CI = 'true' }

# x64 is the host build (no --target), because that is where build-msix.ps1
# looks for it; only a cross build gets a --target directory of its own.
$targets = @{ x64 = ''; arm64 = 'aarch64-pc-windows-msvc' }

# The Tauri config overlay goes in a FILE, never on the command line: a JSON
# argument reaching a native command through PowerShell loses its double
# quotes, and tauri then reads `{bundle:{createUpdaterArtifacts:false}}` and
# refuses it ("key must be a string at line 1 column 2" — msix.yml run
# 44320991850). ci.yml's Build step uses the same shape.
$overlay = Join-Path ([IO.Path]::GetTempPath()) 'vitals-msix-tauri-overlay.json'
'{"bundle":{"createUpdaterArtifacts":false}}' | Set-Content -Path $overlay -Encoding utf8NoBOM

foreach ($arch in $architectures) {
  $triple = $targets[$arch]
  $targetArgs = if ($triple) { @('--target', $triple) } else { @() }
  if ($triple) {
    # Cross-compiling needs that target's std; a no-op when already present.
    rustup target add $triple
    if ($LASTEXITCODE -ne 0) { throw "rustup target add $triple failed ($LASTEXITCODE)" }
  }

  pnpm --filter '@vitals/desktop' exec tauri build --no-bundle @targetArgs --config $overlay
  if ($LASTEXITCODE -ne 0) { throw "tauri build ($arch) failed ($LASTEXITCODE)" }
  cargo build --release -p vitals-watchdog @targetArgs
  if ($LASTEXITCODE -ne 0) { throw "watchdog build ($arch) failed ($LASTEXITCODE)" }
}

# `-Architecture x64,arm64` as one argument on purpose: build-msix.ps1 splits
# it, and that shape survives the pwsh -File boundary this script is called
# through (agentq, in the example above).
pwsh -NoProfile -File (Join-Path $PSScriptRoot 'build-msix.ps1') -Version $Version -Architecture ($architectures -join ',')
if ($LASTEXITCODE -ne 0) { throw "build-msix failed ($LASTEXITCODE)" }

$out = Join-Path $root 'target\msix-upload'
New-Item -ItemType Directory -Force -Path $out | Out-Null
Get-ChildItem (Join-Path $root 'target\msix') -Filter *.msix | Copy-Item -Destination $out
Get-ChildItem $out | ForEach-Object { "{0} {1:N0} B {2}" -f $_.Name, $_.Length, (Get-FileHash $_.FullName).Hash }

if ($NoUpload) {
  Write-Host "packages in $out (not uploaded)"
  exit 0
}

node (Join-Path $root 'scripts\screenshots\upload.mjs') $out $Bucket
if ($LASTEXITCODE -ne 0) { throw "upload failed ($LASTEXITCODE)" }
