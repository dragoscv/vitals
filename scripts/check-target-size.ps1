<#
.SYNOPSIS
    Reports how big `target/` is and why, and fails over budget.

.DESCRIPTION
    `target/debug` reached 90.8 GB in the main clone and 29 GB in a worktree
    a day old (measured 2026-09-29). Two causes, both invisible until the
    drive filled:

            - Incremental units are never collected. Each `<crate>-<hash>` folder
                is one compilation unit (lib, a test target, an example, the clippy
                variant...); rustc collects old sessions inside a unit, but when the
                hash changes — a new rustc, a dependency bump, a feature set — the
                old unit is simply abandoned. A busy crate had 24 of them. A fresh
                build has at most 8 per crate (measured 2026-09-29, vitals_sensors).
      - The desktop crate built as `staticlib` and `cdylib` as well as
        `rlib`, linking the whole app twice more per build.

    The second is fixed in `apps/desktop/src-tauri/Cargo.toml`. This script
    is the ratchet for the first and for anything new: it prints the
    breakdown, and with `-Prune` removes units nothing has written to for
    `-StaleDays` days. That is always safe: the worst case is one slower
    rebuild of that unit. Keeping only the newest unit per crate would be
    wrong — the others are live (the test build of a crate is a different
    unit from its clippy build) and would be rebuilt from scratch each run.

    Budgets live in `size-budget.json` (`targetDebugBytes`,
    `incrementalSessionsPerCrate`). Raise them deliberately.

.EXAMPLE
    pwsh -NoProfile -File scripts/check-target-size.ps1
    pwsh -NoProfile -File scripts/check-target-size.ps1 -Prune
#>
[CmdletBinding()]
param(
    # Remove incremental units untouched for -StaleDays first.
    [switch]$Prune,
    [int]$StaleDays = 7,
    # Report only; never fail.
    [switch]$ReportOnly
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$budget = Get-Content (Join-Path $root 'size-budget.json') -Raw | ConvertFrom-Json
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
$debug = Join-Path $targetDir 'debug'

if (-not (Test-Path $debug)) {
    Write-Host "No $debug yet; nothing to measure."
    exit 0
}

function Get-Bytes([string]$Path) {
    $sum = (Get-ChildItem -LiteralPath $Path -Recurse -Force -File -ErrorAction SilentlyContinue |
        Measure-Object Length -Sum).Sum
    if ($null -eq $sum) { 0 } else { [long]$sum }
}

function Format-Size([long]$Bytes) {
    if ($Bytes -ge 1GB) { '{0:N2} GB' -f ($Bytes / 1GB) } else { '{0:N0} MB' -f ($Bytes / 1MB) }
}

# Units are `<crate>-<hash>` folders; rustc touches a unit's folder each
# time it starts a session in it, so its write time is when it was last used.
$incremental = Join-Path $debug 'incremental'
$sessions = @(Get-ChildItem -LiteralPath $incremental -Directory -ErrorAction SilentlyContinue)
$byCrate = $sessions | Group-Object { ($_.Name -split '-')[0] }

if ($Prune) {
    $removed = 0
    $freed = 0L
    $cutoff = (Get-Date).AddDays(-$StaleDays)
    # A new Cargo.lock or toolchain gives every affected crate a new unit, so
    # anything not used since then belongs to the old dependency graph. The
    # first verify after main bumped every dependency (S16-01) held both
    # generations side by side — 14 units for one crate, 16.5 GB — and failed
    # this gate for a week. Deleting a unit never forces a rebuild; the next
    # change to that crate just compiles without the incremental head start.
    foreach ($name in 'Cargo.lock', 'rust-toolchain.toml', 'rust-toolchain') {
        $file = Join-Path $root $name
        if (Test-Path $file) {
            $changed = (Get-Item $file).LastWriteTime
            if ($changed -gt $cutoff) { $cutoff = $changed }
        }
    }
    foreach ($dir in $sessions | Where-Object LastWriteTime -lt $cutoff) {
        $freed += Get-Bytes $dir.FullName
        Remove-Item -LiteralPath $dir.FullName -Recurse -Force
        $removed++
    }
    Write-Host ("Pruned {0} incremental units unused since {1:yyyy-MM-dd HH:mm}, {2}." -f $removed, $cutoff, (Format-Size $freed))
    $sessions = @(Get-ChildItem -LiteralPath $incremental -Directory -ErrorAction SilentlyContinue)
    $byCrate = $sessions | Group-Object { ($_.Name -split '-')[0] }
}

$total = Get-Bytes $debug
Write-Host ("target/debug: {0}" -f (Format-Size $total))
foreach ($name in 'incremental', 'deps', 'build', 'examples') {
    $path = Join-Path $debug $name
    if (Test-Path $path) { Write-Host ("  {0,-12} {1}" -f $name, (Format-Size (Get-Bytes $path))) }
}
$worst = $byCrate | Sort-Object Count -Descending | Select-Object -First 1
$maxSessions = if ($worst) { $worst.Count } else { 0 }
if ($worst) { Write-Host ("  most incremental units: {0} x{1}" -f $worst.Name, $worst.Count) }

# The artefacts the dropped crate types produced. Any reappearing means
# someone added `staticlib`/`cdylib` back.
$linked = @(Get-ChildItem -LiteralPath $debug -File -Filter 'vitals_desktop_lib.lib' -ErrorAction SilentlyContinue)

$failures = @()
if ($total -gt [long]$budget.targetDebugBytes) {
    $failures += "target/debug is $(Format-Size $total), over the $(Format-Size ([long]$budget.targetDebugBytes)) budget. Run with -Prune; if it is still over, find what grew."
}
if ($maxSessions -gt [int]$budget.incrementalSessionsPerCrate) {
    $failures += "$($worst.Name) has $maxSessions incremental units (budget $($budget.incrementalSessionsPerCrate)). Run with -Prune, or -Prune -StaleDays 0 after a toolchain change."
}
if ($linked.Count -gt 0) {
    $failures += 'vitals_desktop_lib.lib exists: the desktop crate is being built as a staticlib again. Keep crate-type = ["rlib"].'
}

if ($failures.Count -eq 0) {
    Write-Host 'Within budget.' -ForegroundColor Green
    exit 0
}
foreach ($f in $failures) { Write-Host $f -ForegroundColor Red }
if ($ReportOnly) { exit 0 }
exit 1
