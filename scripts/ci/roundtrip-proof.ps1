<#
.SYNOPSIS
    Builds the installer and runs the installer round trip and the idle
    budget against it, on a Windows machine you own (the devcloud
    `vitals-win` workspace): what CI's `installer-roundtrip` job does, in one
    command, before a push.

.DESCRIPTION
    1. `tauri build --bundles nsis` with createUpdaterArtifacts off (no key).
    2. The previous release's x64 setup from GitHub (unauthenticated).
    3. scripts/ci/installer-roundtrip.ps1 with -Previous and, inside the
       machine pass, scripts/ci/idle-budget.ps1.

    Must run elevated. On a machine without an interactive desktop (a batch
    logon) WebView2 never starts: the idle numbers then cover the Rust
    process only, which the summary says, and -RequireWebView is not passed.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/roundtrip-proof.ps1
    pwsh -NoProfile -File scripts/ci/roundtrip-proof.ps1 -NoBuild -UpdateBudget
#>
[CmdletBinding()]
param(
    [switch]$NoBuild,
    [switch]$UpdateBudget,
    [string]$Repository = 'dragoscv/vitals',
    [string]$SensorsMode = 'auto'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

if (-not $NoBuild) {
    # The devcloud runner sets CI=1, which the Tauri CLI refuses ("invalid
    # value '1' for '--ci'"); it accepts only true/false.
    if ($env:CI -eq '1') { $env:CI = 'true' }
    $overlay = Join-Path ([IO.Path]::GetTempPath()) 'vitals-proof-tauri.json'
    '{"bundle":{"createUpdaterArtifacts":false}}' | Set-Content -Path $overlay -Encoding utf8NoBOM
    $tauriArgs = @('--filter', '@vitals/desktop', 'tauri', 'build', '--bundles', 'nsis', '--config', $overlay)
    & pnpm @tauriArgs
    if ($LASTEXITCODE -ne 0) { throw "tauri build exited $LASTEXITCODE" }
}
$setup = Get-ChildItem (Join-Path $root 'target/release/bundle/nsis') -Filter '*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { throw 'no installer under target/release/bundle/nsis' }
$current = (($setup.VersionInfo.ProductVersion -split '[^0-9]+' | Where-Object { $_ }) | Select-Object -First 3) -join '.'
Write-Host "installer: $($setup.FullName) ($current)"

$releases = Invoke-RestMethod "https://api.github.com/repos/$Repository/releases?per_page=10" -Headers @{ 'User-Agent' = 'vitals-roundtrip-proof' }
$prev = $releases | Where-Object { -not $_.draft -and -not $_.prerelease -and $_.tag_name -match '^v\d' -and ($_.tag_name -replace '^v', '') -ne $current } | Select-Object -First 1
if (-not $prev) { throw "no published release other than $current" }
$v = $prev.tag_name -replace '^v', ''
$prevSetup = Join-Path ([IO.Path]::GetTempPath()) "Vitals_${v}_x64-setup.exe"
Invoke-WebRequest "https://github.com/$Repository/releases/download/$($prev.tag_name)/Vitals_${v}_x64-setup.exe" -OutFile $prevSetup
Write-Host "previous release: $v"

$idleOut = Join-Path ([IO.Path]::GetTempPath()) 'vitals-idle.json'
$idle = Join-Path ([IO.Path]::GetTempPath()) 'vitals-idle-wrapper.ps1'
$extra = if ($UpdateBudget) { ' -Update' } else { '' }
Set-Content -Path $idle -Encoding utf8NoBOM -Value @(
    'param([string]$Exe)'
    "& '$(Join-Path $root 'scripts/ci/idle-budget.ps1')' -Exe `$Exe -Out '$idleOut'$extra"
    'exit $LASTEXITCODE'
)
& pwsh -NoProfile -File (Join-Path $root 'scripts/ci/installer-roundtrip.ps1') -Setup $setup.FullName -Previous $prevSetup -WhileInstalled $idle -SensorsMode $SensorsMode
$rc = $LASTEXITCODE
if (Test-Path $idleOut) { Write-Host '--- idle numbers'; Get-Content $idleOut }
Write-Host "ROUNDTRIP_EXIT=$rc"
exit $rc
