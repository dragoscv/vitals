<#
.SYNOPSIS
    Runs every quality gate CI runs, locally, as fast as the machine allows.

.DESCRIPTION
    Exists so "it passes on my machine" means the same thing as "it passes in
    CI". Each gate reports pass/fail and the script exits non-zero if any
    failed, so it is usable from a pre-push hook.

    Three lanes run at once, because they share nothing:

      * checks - formatting and contract drift; seconds, and they fail
        fastest, so their result is printed first.
      * rust   - clippy, tests, bindings, perf budget. Sequential inside the
        lane: every cargo command takes the same target-directory lock, so
        running two at once only makes the second wait.
      * ts     - one `turbo run typecheck lint test`, which runs the three
        tasks across all packages in parallel and replays any whose inputs
        have not changed, then the bundle and its size budget.

    Measured 2026-09-28 on the 32-thread dev machine: typecheck 16.9 s cold
    and 3.4 s cached; lint 83.8 s cold and 1.5 s cached. Running them one
    after the other through `pnpm typecheck; pnpm lint; pnpm test` paid the
    cold price for every task that had not changed.

.EXAMPLE
    pwsh -NoProfile -File scripts/verify.ps1
    pwsh -NoProfile -File scripts/verify.ps1 -SkipBuild -SkipPerf
    pwsh -NoProfile -File scripts/verify.ps1 -Force   # ignore the turbo cache
#>
[CmdletBinding()]
param(
    # Skip the production bundle build, which is the slowest TS gate.
    [switch]$SkipBuild,

    # Skip the release-build performance budget, which needs an optimised
    # compile and so is slow from cold.
    [switch]$SkipPerf,

    # Re-run every turbo task instead of replaying cached results. For when
    # you suspect the cache, not for everyday use: turbo's key already covers
    # the package's files, its dependencies' files and the root configs.
    [switch]$Force
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# A gate is a name and a script block; a lane runs its gates in order and
# returns one record per gate. The lanes run as thread jobs, so a record
# carries the gate's output back rather than printing into an interleaved
# console.
$runLane = {
    param([string]$Root, [object[]]$Gates)
    Set-Location $Root
    foreach ($gate in $Gates) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $global:LASTEXITCODE = 0
        $output = & ([scriptblock]::Create($gate.Script)) 2>&1 | ForEach-Object { "$_" }
        $ok = $LASTEXITCODE -eq 0
        $sw.Stop()
        [pscustomobject]@{
            Gate    = $gate.Name
            Passed  = $ok
            Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1)
            Output  = $output
        }
    }
}

# Script blocks as text: a thread job cannot share the caller's scope, and
# rebuilding them from text keeps each gate's variables inside the job.
$forceFlag = if ($Force) { '--force' } else { '' }

$checks = @(
    @{ Name = 'rust: format'; Script = 'cargo fmt --all -- --check' }
    @{ Name = 'contracts: no drift'; Script = 'pwsh -NoProfile -File scripts/check-drift.ps1' }
    @{ Name = 'ts: format'; Script = 'pnpm format:check' }
)

$rust = @(
    @{ Name = 'rust: clippy'; Script = 'cargo clippy --workspace --all-targets -- -D warnings' }
    @{ Name = 'rust: tests'; Script = 'cargo test --workspace' }
    @{ Name = 'bindings: no drift'; Script = 'cargo test -p vitals-core --features ts --quiet | Out-Null; if ($LASTEXITCODE -eq 0) { git diff --exit-code -- packages/protocol/src/generated }' }
)
if (-not $SkipPerf) {
    # Must be a release build: debug is several times slower and would
    # measure the compiler's lack of optimisation rather than our code.
    $rust += @{ Name = 'rust: perf budget'; Script = 'cargo test -p vitals-win --release --test overhead -- --nocapture' }
}

$ts = @(
    @{ Name = 'ts: typecheck + lint + tests'; Script = "pnpm exec turbo run typecheck lint test --output-logs=errors-only $forceFlag" }
)
if (-not $SkipBuild) {
    $ts += @{ Name = 'build: frontend bundle'; Script = 'pnpm --filter @vitals/desktop build:vite' }
    # Only meaningful after a build, and the installer half is skipped here:
    # a full `tauri build` is minutes, whereas the asset budgets catch the
    # regression that actually happens day to day - an eager import.
    $ts += @{ Name = 'build: size budget'; Script = 'pwsh -NoProfile -File scripts/check-size.ps1 -SkipInstaller' }
}

$total = [System.Diagnostics.Stopwatch]::StartNew()
$jobs = [ordered]@{
    checks = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $checks
    rust   = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $rust
    ts     = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $ts
}

$results = [System.Collections.Generic.List[object]]::new()
$pending = [System.Collections.Generic.List[object]]::new()
$jobs.Values | ForEach-Object { $pending.Add($_) }

# Print each lane as it finishes, so a formatting failure is on screen in
# seconds rather than after the four-minute Rust lane.
while ($pending.Count -gt 0) {
    $done = Wait-Job -Job $pending -Any
    $lane = ($jobs.GetEnumerator() | Where-Object { $_.Value.Id -eq $done.Id }).Key
    foreach ($r in @(Receive-Job -Job $done)) {
        $colour = if ($r.Passed) { 'Green' } else { 'Red' }
        $status = if ($r.Passed) { 'PASS' } else { 'FAIL' }
        Write-Host ('  {0}  {1,-30} {2,6:N1}s   [{3}]' -f $status, $r.Gate, $r.Seconds, $lane) -ForegroundColor $colour
        if (-not $r.Passed) {
            $r.Output | Select-Object -Last 30 | ForEach-Object { Write-Host "      $_" -ForegroundColor Red }
        }
        if ($r.Gate -eq 'rust: tests' -and $r.Passed) {
            $passed = ($r.Output | Select-String -Pattern '^test result: ok\. (\d+) passed' |
                    ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
            Write-Host "        $passed Rust tests passed" -ForegroundColor DarkGray
        }
        if ($r.Gate -eq 'rust: perf budget') {
            $r.Output | Select-String -Pattern 'median' | ForEach-Object { Write-Host "        $($_.Line.Trim())" -ForegroundColor DarkGray }
        }
        if ($r.Gate -like 'ts: typecheck*') {
            $r.Output | Select-String -Pattern '^\s*(Tasks|Cached):' | ForEach-Object { Write-Host "        $($_.Line.Trim())" -ForegroundColor DarkGray }
        }
        $results.Add($r)
    }
    Remove-Job -Job $done
    [void]$pending.Remove($done)
}
$total.Stop()

Write-Host ''
Write-Host ('=' * 64) -ForegroundColor DarkGray
$results | Select-Object Gate, Passed, Seconds | Format-Table -AutoSize

$failed = @($results | Where-Object { -not $_.Passed })
$wall = [math]::Round($total.Elapsed.TotalSeconds, 1)
$serial = [math]::Round(($results | Measure-Object Seconds -Sum).Sum, 1)
if ($failed.Count -gt 0) {
    Write-Host "$($failed.Count) gate(s) FAILED: $($failed.Gate -join ', ')  (wall ${wall}s, serial ${serial}s)" -ForegroundColor Red
    exit 1
}

Write-Host "All $($results.Count) gates passed in ${wall}s (${serial}s if run one after another)." -ForegroundColor Green
exit 0
