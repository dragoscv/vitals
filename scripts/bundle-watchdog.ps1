<#
.SYNOPSIS
    Builds the lag watchdog and stages it as a Tauri resource.

.DESCRIPTION
    `apps/desktop/src-tauri/tauri.conf.json` bundles `watchdog/` as a
    resource, so the installer puts it at `$INSTDIR\watchdog\` and the app's
    first launch registers that path at logon (ADR-0032). Runs in
    `beforeBuildCommand`, so the installer carries the watchdog built from
    the same commit as the app.

    Honours CARGO_BUILD_TARGET like bundle-sensors.ps1.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $root 'apps/desktop/src-tauri/watchdog'

Push-Location $root
try {
    cargo build -p vitals-watchdog --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build -p vitals-watchdog failed ($LASTEXITCODE)" }

    $target = if ($env:CARGO_BUILD_TARGET) { "target/$($env:CARGO_BUILD_TARGET)/release" } else { 'target/release' }
    $exe = Join-Path $root "$target/vitals-watchdog.exe"
    if (-not (Test-Path -LiteralPath $exe)) { throw "watchdog not found at $exe" }

    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'vitals-watchdog.exe') -Force
    $size = [math]::Round((Get-Item (Join-Path $stage 'vitals-watchdog.exe')).Length / 1KB)
    Write-Host "staged vitals-watchdog.exe ($size KB) in $stage"
}
finally {
    Pop-Location
}
