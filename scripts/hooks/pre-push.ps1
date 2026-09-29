<#
.SYNOPSIS
    Checks that must hold before anything reaches GitHub, in well under a
    minute.

.DESCRIPTION
    The failures that CI found and every local gate missed were static, and
    all of them are cheap to see before a push:

      * A `#[cfg(windows)]` Tauri command registered for every platform
        compiled on Windows and failed the Linux leg (3f0588a -> cc0a647).
        check-drift.ps1 now finds it.
      * A new crate without its notice, and a licence nobody had read.
        third-party-notices.ps1 -Check and cargo-deny.
      * An unformatted file on a branch nobody ran rustfmt over.

    Not the test suites: scripts/verify.ps1 takes minutes and queues behind
    other builds, and a hook that slow gets bypassed within a day. CI runs
    everything on every push.

.NOTES
    Installed by scripts/hooks/install.ps1 into .git/hooks/pre-push.
    Bypass with `git push --no-verify` when you genuinely need to.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$root = (git rev-parse --show-toplevel)
Set-Location $root

$failed = [System.Collections.Generic.List[string]]::new()
$clock = [System.Diagnostics.Stopwatch]::StartNew()

function Step([string]$name, [scriptblock]$body) {
    $t = [System.Diagnostics.Stopwatch]::StartNew()
    $out = & $body 2>&1
    $ok = $LASTEXITCODE -eq 0
    Write-Host ('  {0}  {1,-28} {2,5:N1}s' -f ($(if ($ok) { 'ok  ' } else { 'FAIL' })), $name, $t.Elapsed.TotalSeconds) -ForegroundColor $(if ($ok) { 'Green' } else { 'Red' })
    if (-not $ok) {
        $out | Select-Object -Last 15 | ForEach-Object { Write-Host "        $_" }
        $failed.Add($name)
    }
}

Write-Host 'pre-push:' -ForegroundColor Cyan
Step 'contracts + cfg parity' { pwsh -NoProfile -File scripts/check-drift.ps1 }
Step 'rust format' { cargo fmt --all -- --check }
Step 'third-party notices' { pwsh -NoProfile -File scripts/third-party-notices.ps1 -Check }
if (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
    Step 'licences + sources' { cargo deny --all-features check licenses bans sources }
} else {
    Write-Host '  skip  licences + sources          cargo-deny missing: cargo binstall cargo-deny' -ForegroundColor Yellow
}

Write-Host ('pre-push done in {0:N1}s' -f $clock.Elapsed.TotalSeconds) -ForegroundColor DarkGray
if ($failed.Count -gt 0) {
    Write-Host "push refused: $($failed -join ', ')" -ForegroundColor Red
    exit 1
}
exit 0
