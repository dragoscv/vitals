<#
.SYNOPSIS
    Drives the browser-free dev-server probe on the devcloud warm workspace.

.DESCRIPTION
    The workstation is shared and the standing rule is that anything browser- or
    load-related runs on the devcloud, never here. scripts/e2e-cloud.ps1 already
    does that for the Playwright suite; this is the same shape for the Vite dev
    server, so nobody has to start a dev server on the shared machine just to
    check that a screen is served.

    It submits exactly one job to the warm Windows workspace `ws-vitals-win`:

        pwsh -NoProfile -File scripts/dev-server-probe.ps1

    and streams the remote log back, so the exit code is the probe's. The probe
    itself (scripts/dev-server-probe.ps1) starts the Vite dev server, checks that
    the app root and the app's other Vite entries answer with the `#root` element
    present, then stops the server and its whole process tree before it exits. No
    browser is involved and no Tauri binary is needed — the screens render against
    the no-host fallback (apps/desktop/src/shell/host.ts).

    The remote command is a `.ps1` file, not an inline `pwsh -Command` string:
    that is how the quoting a dev-server command line would otherwise attract
    (paths, `--filter`, the port) stays in PowerShell instead of being re-split by
    a second shell.

.EXAMPLE
    pwsh -NoProfile -File scripts/dev-server-cloud.ps1
    pwsh -NoProfile -File scripts/dev-server-cloud.ps1 -DryRun      # print the argv
    pwsh -NoProfile -File scripts/dev-server-cloud.ps1 -TimeoutMin 30

.NOTES
    The cloud runner is ~/.copilot/bin/agentq-cloud.ps1 (DC-037). Everything
    after `--` is executed in the workspace checkout with PowerShell argv, not
    bash quoting; see docs/devcloud.md.
#>
[CmdletBinding()]
param(
    # The devcloud warm workspace (docs/devcloud.md). vitals-win is the Windows
    # workspace the repository's other heavy lanes already use.
    [string]$Ws = 'vitals-win',

    # How long this caller waits for the job: the queue, the workspace sync, a
    # cold Vite start and the checks. The dev server has no browser and no Rust
    # build behind it, so the job is short; the default leaves room for a job
    # queued behind another lane.
    [int]$TimeoutMin = 20,

    # Print the exact remote argv and exit 0 without submitting anything. For a
    # review, for CI that only wants the plan, and for checking a change to this
    # script without spending a cloud job.
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# The runner is installed user-wide, not vendored, so it is resolved from the
# same place this repository's other scripts find the agent harness. It is
# checked here rather than letting the child process answer "the term
# 'agentq-cloud.ps1' is not recognised", which reads like a typo rather than a
# missing install.
$cloud = Join-Path $HOME '.copilot\bin\agentq-cloud.ps1'
if (-not (Test-Path -LiteralPath $cloud -PathType Leaf)) {
    throw "agentq-cloud runner not found at $cloud (it is installed with the agent harness)"
}

# The remote argv is built as an array, never a joined string: agentq-cloud
# passes it through to the workspace as argv, and a single string would be
# re-split by whatever quotes it. One element per argument, always.
$probeArgs = @('pwsh', '-NoProfile', '-File', 'scripts/dev-server-probe.ps1')

if ($DryRun) {
    Write-Host "repo:       $root"
    Write-Host "workspace:  $Ws"
    Write-Host 'remote argv (probe):'
    Write-Host ('  ' + ($probeArgs -join ' '))
    Write-Host 'not submitted (-DryRun)'
    exit 0
}

# A child process, not `& $cloud`, and for the reason verify.ps1 records: the
# cloud client narrates through [Console]::Out.WriteLine, which is the process's
# own stdout and not a PowerShell stream, so called in-process its log never
# reaches this pipeline — and its own `exit N` would end this script. Spawning
# the shell is what makes the remote log capturable and the exit code ours.
function Invoke-CloudJob {
    param([string]$Purpose, [string[]]$Remote)
    $argv = @('-Ws', $Ws, '-Purpose', $Purpose, '-Lane', 'gate', '-TimeoutMin', $TimeoutMin, '-Repo', $root, '--') + $Remote
    Write-Host "  -> $Purpose on $Ws" -ForegroundColor Cyan
    Write-Host ('     ' + ($argv -join ' ')) -ForegroundColor DarkGray
    $shell = (Get-Process -Id $PID).Path
    if (-not $shell) { $shell = 'pwsh' }
    & $shell -NoProfile -NoLogo -File $cloud @argv 2>&1 | ForEach-Object { Write-Host "     $_" }
    $LASTEXITCODE
}

$code = Invoke-CloudJob -Purpose 'prove the Vite dev server serves the app' -Remote $probeArgs
if ($code -ne 0) {
    Write-Host "dev-server probe failed on $Ws (exit $code)" -ForegroundColor Red
    Write-Host '  the failing check is named in the streamed log above'
} else {
    Write-Host "dev-server probe passed on $Ws" -ForegroundColor Green
}
exit $code
