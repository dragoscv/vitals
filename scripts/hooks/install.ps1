<#
.SYNOPSIS
    Installs the repository's git hooks.

.DESCRIPTION
    Hooks live in `scripts/hooks/` (tracked, reviewable) and are installed
    into `.git/hooks/` (untracked, local). A shim rather than a copy, so
    editing the tracked script takes effect immediately and nobody ends up
    running a stale hook they installed months ago.

    Not done via `core.hooksPath`, because that would silently replace any
    hooks a contributor has configured globally.

    Run once after cloning:
        pwsh -NoProfile -File scripts/hooks/install.ps1
#>
[CmdletBinding()]
param(
    # Remove the hooks instead of installing them.
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$root = (git rev-parse --show-toplevel)
Set-Location $root

$hooksDir = Join-Path (git rev-parse --git-dir) 'hooks'
New-Item -ItemType Directory -Force -Path $hooksDir | Out-Null

$hooks = @('pre-commit', 'pre-push')

foreach ($hook in $hooks) {
    $target = Join-Path $hooksDir $hook

    if ($Uninstall) {
        if (Test-Path $target) {
            Remove-Item $target
            Write-Host "removed $hook" -ForegroundColor Yellow
        }
        continue
    }

    # Git runs hooks through sh even on Windows, so the shim is POSIX and
    # hands off to pwsh. LF endings, no BOM — sh will not run a file with
    # CRLF line endings and reports a baffling error if it tries.
    $shim = @"
#!/bin/sh
exec pwsh -NoProfile -ExecutionPolicy Bypass -File "scripts/hooks/$hook.ps1" "`$@"
"@
    $bytes = [System.Text.Encoding]::ASCII.GetBytes(($shim -replace "`r`n", "`n"))
    [System.IO.File]::WriteAllBytes($target, $bytes)
    Write-Host "installed $hook" -ForegroundColor Green
}

if (-not $Uninstall) {
    Write-Host ''
    Write-Host 'pre-commit runs in seconds: staged-file formatting, contract drift,' -ForegroundColor DarkGray
    Write-Host 'secrets, and debug leftovers. Full gates: scripts/verify.ps1' -ForegroundColor DarkGray
    Write-Host 'pre-push: drift + cfg parity, rustfmt, notices, cargo-deny (under a minute).' -ForegroundColor DarkGray
}
