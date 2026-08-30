<#
.SYNOPSIS
    Runs every quality gate CI runs, locally.

.DESCRIPTION
    Exists so "it passes on my machine" means the same thing as "it passes in
    CI". Each gate reports pass/fail and the script exits non-zero if any
    failed, so it is usable from a pre-push hook.

.EXAMPLE
    pwsh -NoProfile -File scripts/verify.ps1
    pwsh -NoProfile -File scripts/verify.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
    # Skip the production bundle build, which is the slowest gate.
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$results = [System.Collections.Generic.List[object]]::new()

function Invoke-Gate {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][scriptblock]$Command
    )

    Write-Host ''
    Write-Host "── $Name " -NoNewline -ForegroundColor Cyan
    Write-Host ('─' * [Math]::Max(1, 60 - $Name.Length)) -ForegroundColor DarkGray

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $output = & $Command 2>&1
    $ok = $LASTEXITCODE -eq 0
    $sw.Stop()

    if (-not $ok) {
        $output | Select-Object -Last 30 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    }

    $status = if ($ok) { 'PASS' } else { 'FAIL' }
    $colour = if ($ok) { 'Green' } else { 'Red' }
    Write-Host ("  {0}  ({1:N1}s)" -f $status, $sw.Elapsed.TotalSeconds) -ForegroundColor $colour

    $results.Add([pscustomobject]@{
            Gate    = $Name
            Passed  = $ok
            Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1)
        })

    return $output
}

Invoke-Gate 'rust: format' { cargo fmt --all -- --check } | Out-Null
Invoke-Gate 'rust: clippy' { cargo clippy --workspace --all-targets -- -D warnings } | Out-Null

$testOutput = Invoke-Gate 'rust: tests' { cargo test --workspace }
$passed = ($testOutput |
        Select-String -Pattern '^test result: ok\. (\d+) passed' |
        ForEach-Object { [int]$_.Matches[0].Groups[1].Value } |
        Measure-Object -Sum).Sum
if ($passed) { Write-Host "  $passed Rust tests passed" -ForegroundColor DarkGray }

Invoke-Gate 'bindings: no drift' {
    cargo test -p vitals-core --features ts --quiet | Out-Null
    git diff --exit-code -- packages/protocol/src/generated
} | Out-Null

Invoke-Gate 'ts: typecheck' { pnpm typecheck } | Out-Null
Invoke-Gate 'ts: lint' { pnpm lint } | Out-Null
Invoke-Gate 'ts: tests' { pnpm test } | Out-Null
Invoke-Gate 'ts: format' { pnpm format:check } | Out-Null

if (-not $SkipBuild) {
    Invoke-Gate 'build: frontend bundle' {
        Push-Location apps/desktop
        pnpm build:vite
        Pop-Location
    } | Out-Null
}

Write-Host ''
Write-Host ('═' * 64) -ForegroundColor DarkGray
$results | Format-Table -AutoSize

$failed = @($results | Where-Object { -not $_.Passed })
if ($failed.Count -gt 0) {
    Write-Host "$($failed.Count) gate(s) FAILED: $($failed.Gate -join ', ')" -ForegroundColor Red
    exit 1
}

Write-Host "All $($results.Count) gates passed." -ForegroundColor Green
exit 0
