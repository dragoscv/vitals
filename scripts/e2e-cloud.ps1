<#
.SYNOPSIS
    Runs the Playwright end-to-end suite on the devcloud warm workspace.

.DESCRIPTION
    Browser tests must never run on the workstation: the suite drives a real
    Chromium, this machine is shared with other agents, and a local run
    contends for the CPU every other build needs. This submits the suite as one
    agentq cloud job to the warm Windows workspace `ws-vitals-win`, which
    already has the repository checked out with node_modules, and streams the
    remote log back.

    It runs the same command the repository runs locally —
    `pnpm --filter @vitals/desktop exec playwright test` — so the suite is
    defined once in `apps/desktop/playwright.config.ts` and nothing about it
    changes here. The suite talks to the no-host fallback (src/shell/host.ts),
    so the job needs no Tauri binary and no window: it is Vite plus Chromium.

    Chromium is not necessarily installed on the workspace, so the first run
    needs `-InstallBrowsers`, which submits `playwright install chromium` as
    its own step before the suite. A run without it on a workspace that has no
    browser fails with Playwright's own "Executable doesn't exist" message.

    The exit code is the remote exit code: 0 means the suite passed. On any
    other code the failure names itself in the streamed log, and the HTML
    report is left on the workspace under the checkout's
    `apps/desktop/e2e-report/`.

.EXAMPLE
    pwsh -NoProfile -File scripts/e2e-cloud.ps1 -InstallBrowsers   # first run
    pwsh -NoProfile -File scripts/e2e-cloud.ps1                    # the suite
    pwsh -NoProfile -File scripts/e2e-cloud.ps1 -Spec e2e/a11y.spec.ts
    pwsh -NoProfile -File scripts/e2e-cloud.ps1 -DryRun            # print the argv

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

    # A spec file or Playwright test filter passed through as a positional
    # argument, e.g. `e2e/a11y.spec.ts` or a `-g` style substring.
    [string]$Spec,

    # Restrict to one Playwright project (`--project <name>`). The suite has a
    # single chromium project, so this is only for one that is added.
    [string]$Project,

    # How long this caller waits for the job: the queue, the workspace sync and
    # the suite. Playwright's own per-test timeout is 60 s and the suite is
    # fully parallel, so a cold run is a few minutes; the default leaves room
    # for a job queued behind another lane.
    [int]$TimeoutMin = 90,

    # Install Chromium on the workspace first, as a separate step in its own
    # job. Needed once per workspace: the browser download is cached outside the
    # repository, so it survives the job's `git clean` and a later run skips it.
    [switch]$InstallBrowsers,

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
# re-split by whatever quotes it — which is how a spec pattern containing a
# space becomes two arguments.
$browserArgs = @('pnpm', '--filter', '@vitals/desktop', 'exec', 'playwright', 'install', 'chromium')
$testArgs = @('pnpm', '--filter', '@vitals/desktop', 'exec', 'playwright', 'test')
if ($Project) { $testArgs += @('--project', $Project) }
if ($Spec) { $testArgs += $Spec }

if ($DryRun) {
    Write-Host "repo:       $root"
    Write-Host "workspace:  $Ws"
    if ($InstallBrowsers) {
        Write-Host 'remote argv (browser install):'
        Write-Host ('  ' + ($browserArgs -join ' '))
    }
    Write-Host 'remote argv (suite):'
    Write-Host ('  ' + ($testArgs -join ' '))
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

# The suite is one job. The browser install, when asked for, is its own job
# first: the runner takes one command, and chaining two programs into a single
# `pwsh -Command` string would reintroduce the quoting this script exists to
# avoid. A failed install stops here rather than letting the suite fail later
# with Playwright's "Executable doesn't exist".
if ($InstallBrowsers) {
    $code = Invoke-CloudJob -Purpose 'install Chromium for the e2e suite' -Remote $browserArgs
    if ($code -ne 0) {
        Write-Host "Chromium install on $Ws failed (exit $code)" -ForegroundColor Red
        Write-Host '  the streamed log above names the cause'
        exit $code
    }
}

$code = Invoke-CloudJob -Purpose 'run the Playwright e2e suite' -Remote $testArgs
if ($code -ne 0) {
    Write-Host "e2e suite failed on $Ws (exit $code)" -ForegroundColor Red
    Write-Host '  the failing test is named in the streamed log above'
    Write-Host '  HTML report on the workspace, in the checkout: apps/desktop/e2e-report/'
} else {
    Write-Host "e2e suite passed on $Ws" -ForegroundColor Green
}
exit $code
