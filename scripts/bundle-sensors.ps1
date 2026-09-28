<#
.SYNOPSIS
    Builds the CPU sensors helper and stages it as a Tauri resource.

.DESCRIPTION
    `apps/desktop/src-tauri/tauri.conf.json` bundles `sensors/` as a
    resource, and the app looks for `resources\sensors\vitals-sensors.exe`
    when the user asks to install the service (ADR-0034). This runs as part
    of `beforeBuildCommand`, so a release installer always carries the helper
    built from the same commit as the app — a helper staged by hand could be
    from any commit, and it runs as SYSTEM.

    The PawnIO modules are embedded in the exe, so it is the whole payload.
    The licence text for the embedded modules is copied beside it.

    Honours CARGO_BUILD_TARGET, so the ARM64 release leg stages an ARM64
    helper (which reports "unsupported architecture" rather than crashing).
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# The sensors helper reads Windows-only interfaces; the macOS and Linux
# release legs share this beforeBuildCommand and ship an empty resource dir.
if (-not $IsWindows) {
    Write-Host 'not Windows: nothing to stage'
    exit 0
}
$root = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $root 'apps/desktop/src-tauri/sensors'

Push-Location $root
try {
    cargo build -p vitals-sensors --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build -p vitals-sensors failed ($LASTEXITCODE)" }

    $target = if ($env:CARGO_BUILD_TARGET) { "target/$($env:CARGO_BUILD_TARGET)/release" } else { 'target/release' }
    $exe = Join-Path $root "$target/vitals-sensors.exe"
    if (-not (Test-Path -LiteralPath $exe)) { throw "helper not found at $exe" }

    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'vitals-sensors.exe') -Force
    Copy-Item -LiteralPath (Join-Path $root 'crates/vitals-sensors/modules/COPYING') `
        -Destination (Join-Path $stage 'PawnIO-Modules-COPYING.txt') -Force
    $size = [math]::Round((Get-Item (Join-Path $stage 'vitals-sensors.exe')).Length / 1KB)
    Write-Host "staged vitals-sensors.exe ($size KB) in $stage"
}
finally {
    Pop-Location
}
