<#
.SYNOPSIS
    Runs the desktop unit suite while the machine is deliberately saturated.

.DESCRIPTION
    The class of defect this exists for: a test that awaits a debounced host
    push (or a cold lazy chunk) with a default timeout passes on an idle runner
    and fails on a loaded one. The 1.6.0 release failed the Verify gate on
    exactly one such widget test, and nothing in this repo runs the desktop
    suite under CPU pressure, so the next one ships green as well.

    This probe starts `-Workers` CPU saturators (a tight arithmetic loop each),
    MEASURES the load they actually produce, and only then runs the desktop
    suite under that load. A run that saturates nothing is refused: the probe
    fails loudly if the measured aggregate CPU never rises above
    `-MinLoadPercent`, because a green gate whose failure you cannot produce is
    decorative.

    It is a SOAK check, not a speed benchmark. Pinning every core for minutes is
    the point; it is opt-in and pre-tag, never part of scripts/verify.ps1.

.EXAMPLE
    # The pre-tag soak: the whole desktop suite under load.
    pwsh -NoProfile -File scripts/check-suite-under-load.ps1

.EXAMPLE
    # One feature's files, quickly.
    pwsh -NoProfile -File scripts/check-suite-under-load.ps1 -Filter dashboard -DurationSec 60

.EXAMPLE
    # Prove the load detector is not vacuous: an impossible threshold must go red.
    pwsh -NoProfile -File scripts/check-suite-under-load.ps1 -MinLoadPercent 200
#>
[CmdletBinding()]
param(
    # CPU saturators to run. Default max(2, logical cores - 2), so the suite
    # still has a sliver of the machine and the probe finishes.
    [int]$Workers = 0,

    # Hard cap, in seconds, on the suite run. A filtered subset finishes well
    # inside it; the full suite needs a few minutes. Reaching the cap is a
    # failure, not a quiet success.
    [int]$DurationSec = 900,

    # Passed through to vitest as a file filter (a substring of a path).
    [string]$Filter = '',

    # The measured aggregate CPU must reach this, or the probe refuses to report
    # on a test it did not actually load.
    [int]$MinLoadPercent = 50
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$cores = [Environment]::ProcessorCount
if ($Workers -le 0) { $Workers = [Math]::Max(2, $cores - 2) }

$desktop = Join-Path $root 'apps/desktop'
$jobPrefix = 'vitals-load'

# ── Measuring the load, because "we ran it under load" is a claim ───────────
#
# Get-Counter is the honest source. The first sample of \Processor(_Total) is
# always junk — the counter is a delta between two reads — so it is discarded.
# If the counter is unavailable the fallback sums the per-process CPU delta,
# which undercounts a little but never lies upward.
function Get-CpuLoadPercent {
    try {
        $samples = @(Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 1 -MaxSamples 4 -ErrorAction Stop).
            CounterSamples | Select-Object -Skip 1 -ExpandProperty CookedValue
        return [Math]::Round(($samples | Measure-Object -Average).Average, 1)
    } catch {
        $before = (Get-Process -ErrorAction SilentlyContinue | Measure-Object -Property CPU -Sum).Sum
        Start-Sleep -Seconds 2
        $after = (Get-Process -ErrorAction SilentlyContinue | Measure-Object -Property CPU -Sum).Sum
        $used = [Math]::Max(0, $after - $before)
        return [Math]::Round(100 * $used / ($cores * 2), 1)
    }
}

# A tight arithmetic loop per saturator. This is what makes the probe a load
# test rather than an intention; a worker that does nothing is caught below.
$saturate = {
    $acc = 0.0
    while ($true) {
        for ($i = 0; $i -lt 300000; $i++) { $acc += [Math]::Sqrt([double]$i) }
        if ($acc -gt 1e300) { $acc = 0.0 }
    }
}

$jobs = [System.Collections.Generic.List[System.Management.Automation.Job]]::new()
$suiteJob = $null
$failed = $false
$watch = [System.Diagnostics.Stopwatch]::StartNew()

try {
    Write-Host ''
    Write-Host ("load probe: {0} saturator(s) on {1} logical cores" -f $Workers, $cores) -ForegroundColor Cyan

    $baseline = Get-CpuLoadPercent

    for ($n = 0; $n -lt $Workers; $n++) {
        $jobs.Add((Start-Job -Name "$jobPrefix-$n" -ScriptBlock $saturate))
    }

    # Let the workers reach full tilt before judging them.
    Start-Sleep -Seconds 3
    $loaded = Get-CpuLoadPercent

    Write-Host ("  baseline CPU : {0,5:N1} %  (before any saturator)" -f $baseline) -ForegroundColor DarkGray
    Write-Host ("  loaded   CPU : {0,5:N1} %  ({1} saturator(s))" -f $loaded, $Workers)
    Write-Host ("  threshold    : {0,5:N1} %  (-MinLoadPercent)" -f $MinLoadPercent)

    if ($loaded -lt $MinLoadPercent) {
        Write-Host ''
        Write-Host ("LOAD PROBE FAILED: measured {0:N1} % CPU with {1} saturator(s), below the {2} % floor." -f $loaded, $Workers, $MinLoadPercent) -ForegroundColor Red
        Write-Host 'Nothing was loaded, so a green suite here would prove nothing. Check that -Workers is sane, that'
        Write-Host 'the saturators actually ran, and that no policy is throttling this process tree.'
        $failed = $true
    } else {
        $vitestArgs = @('vitest', 'run')
        if ($Filter) { $vitestArgs += $Filter }

        Write-Host ("  running      : npx {0}   (from apps/desktop)" -f ($vitestArgs -join ' ')) -ForegroundColor DarkGray
        Write-Host ''

        $suiteJob = Start-Job -Name "$jobPrefix-suite" -ScriptBlock {
            param($dir, $argv)
            Set-Location $dir
            & npx @argv 2>&1 | ForEach-Object { $_.ToString() }
            "__VITEST_EXIT__=$LASTEXITCODE"
        } -ArgumentList $desktop, $vitestArgs

        $finished = $null -ne (Wait-Job $suiteJob -Timeout $DurationSec)
        $output = @(Receive-Job $suiteJob)

        # The vitest summary: the lines a reader needs, as vitest itself wrote
        # them. Everything else is left in the job and dropped.
        foreach ($line in $output) {
            if ($line -match '^\s*(Test Files|Tests|Duration)\s') { Write-Host "  $($line.Trim())" }
        }

        $exitLine = $output | Where-Object { $_ -match '^__VITEST_EXIT__=\d+$' } | Select-Object -Last 1
        $suiteExit = if ($exitLine) { [int]($exitLine -replace '^__VITEST_EXIT__=', '') } else { -1 }

        if (-not $finished) {
            Write-Host ''
            Write-Host ("LOAD PROBE FAILED: the suite did not finish within {0}s (-DurationSec)." -f $DurationSec) -ForegroundColor Red
            Write-Host 'Under load that is itself a result: something is waiting longer than the cap you set.'
            $failed = $true
        } elseif ($suiteExit -ne 0) {
            Write-Host ''
            Write-Host ("LOAD PROBE FAILED: vitest exited {0} — a test failed under load." -f $suiteExit) -ForegroundColor Red
            Write-Host 'Read the failing test: if it awaits a debounced push or a lazy chunk with a default timeout,'
            Write-Host 'the fix is to await the real condition, not to raise the timeout — see the history in docs/releasing.md.'
            $failed = $true
        }
    }
} finally {
    # Never leave the machine pinned: stop every saturator and the suite job.
    foreach ($job in @($suiteJob) + $jobs) {
        if ($null -ne $job) {
            Stop-Job $job -ErrorAction SilentlyContinue
            Remove-Job $job -Force -ErrorAction SilentlyContinue
        }
    }
}

$leftover = @(Get-Job -Name "$jobPrefix-*" -ErrorAction SilentlyContinue)
$watch.Stop()

Write-Host ''
Write-Host ("duration: {0:N1}s   saturators: {1}   survivors: {2}" -f $watch.Elapsed.TotalSeconds, $Workers, $leftover.Count)

if ($failed) {
    Write-Host 'Suite-under-load: RED.' -ForegroundColor Red
    exit 1
}
if ($leftover.Count -gt 0) {
    Write-Host 'Suite-under-load: could not clean up the saturators.' -ForegroundColor Red
    exit 1
}

Write-Host 'Suite-under-load: OK.' -ForegroundColor Green
exit 0
