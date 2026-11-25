<#
.SYNOPSIS
    Proves the Vite dev server serves the app, browser-free, inside a cloud job.

.DESCRIPTION
    The workstation is shared and the standing rule is that anything browser- or
    load-related runs on the devcloud, never here. scripts/e2e-cloud.ps1 already
    does that for the Playwright suite; this is the same idea for the dev server
    a person would otherwise start on this machine only to look at a screen.

    It starts the Vite dev server, waits for the port in
    apps/desktop/vite.config.ts to answer, fetches the app root and every other
    Vite entry the server exposes, checks each response for the `#root` element
    the app mounts into, then stops the server and its whole process tree, so no
    dev server is ever left running on the workspace. The exit code is the
    verdict: 0 only if every check passed and the port was released.

    It starts `dev:vite`, not `dev`. In this package `dev` is `tauri dev`, which
    runs this same server as its beforeDevCommand and then builds and launches the
    Rust app — a window the devcloud's non-interactive session cannot open, and a
    Rust build this lane has no reason to pay for. The subject here is the web
    server, so it starts exactly that.

    The desktop shell is a single-page app: its screens are chosen from the URL
    fragment (`#processes`, `#storage`, ...), which a browser never sends to the
    server, so every screen is answered with the same index.html. The deep routes
    that are distinct on the wire are the other two Vite entries the app ships —
    `/mobile.html` (the phone surface) and `/hud.html` (the always-on-top
    overlay) — and those are what a "deep route" means here.

.EXAMPLE
    pwsh -NoProfile -File scripts/dev-server-probe.ps1
    pwsh -NoProfile -File scripts/dev-server-probe.ps1 -ReadyTimeoutSec 200

.NOTES
    Runs on the workspace as the remote command of scripts/dev-server-cloud.ps1.
    Prints one `[devcloud] dev-server ...` line per step plus a final verdict
    line, modelled on the runner's own `[devcloud]` lines, so the streamed job log
    reads as one timeline rather than a second dialect.
#>
[CmdletBinding()]
param(
    # The dev-server port; the fixed value in apps/desktop/vite.config.ts.
    [int]$Port = 5273,

    # How long to wait for the port to answer. A cold Vite with Tailwind on a
    # loaded machine has taken over 30 s to start, and the first on-demand
    # transform adds more, so this is generous without being unbounded.
    [int]$ReadyTimeoutSec = 180,

    # The paths to fetch. `/` is the desktop webview; the other two are the
    # remaining Vite entries in the app's build config.
    [string[]]$Route = @('/', '/mobile.html', '/hud.html'),

    # Per-request timeout.
    [int]$RequestTimeoutSec = 30
)

$ErrorActionPreference = 'Stop'

# One line per fact, in the runner's own style, so the streamed job log reads as
# one timeline rather than a second dialect.
function Write-Probe {
    param([string]$Text, [string]$Color = 'Gray')
    Write-Host "[devcloud] dev-server $Text" -ForegroundColor $Color
}

# A listener check that never throws on "nothing is listening": the cmdlet raises
# a non-terminating error for an empty result, which is the normal case here.
function Test-PortListening {
    param([int]$P)
    try {
        $c = @(Get-NetTCPConnection -LocalPort $P -State Listen -ErrorAction Stop)
        return ($c.Count -gt 0)
    } catch {
        return $false
    }
}

# Kill a process and everything it spawned. `pnpm` runs node (Vite), which runs
# its own workers, so killing only the process we started would orphan Vite.
function Stop-Tree {
    param([int]$RootPid)
    if ($RootPid) { & taskkill.exe /PID $RootPid /T /F *> $null }
}

# Last resort: whatever still holds the port, kill its tree.
function Stop-PortHolders {
    param([int]$P)
    try { $conns = @(Get-NetTCPConnection -LocalPort $P -State Listen -ErrorAction SilentlyContinue) } catch { $conns = @() }
    foreach ($c in $conns) {
        if ($c.OwningProcess) { & taskkill.exe /PID $c.OwningProcess /T /F *> $null }
    }
}

$root = Split-Path -Parent $PSScriptRoot
$serverCmd = 'pnpm --filter @vitals/desktop dev:vite'

# Vite binds `localhost` — that is the address it prints and the one
# apps/desktop/playwright.config.ts uses — and on Windows that name can resolve to
# the IPv6 loopback first, so a probe hard-coded to 127.0.0.1 sees "connection
# refused" for a server that is up and serving (measured on ws-vitals-win
# 2026-11-25: Vite reported "ready in 957 ms" while 127.0.0.1 answered nothing).
# Try every loopback spelling and keep the first that answers.
$candidates = @("http://localhost:$Port", "http://127.0.0.1:$Port", "http://[::1]:$Port")

$logDir = Join-Path ([IO.Path]::GetTempPath()) ('vitals-dev-server-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$outLog = Join-Path $logDir 'server.out.log'
$errLog = Join-Path $logDir 'server.err.log'

$proc = $null
$ready = $false
$stoppedClean = $false
$results = @()

try {
    Write-Probe "start: $serverCmd (port $Port)"

    # cmd, not pnpm directly: on Windows `pnpm` is a .cmd, and cmd runs it
    # without Start-Process having to guess at PATHEXT. Everything after /c is one
    # argument, so nothing is re-split — the same reason the caller builds the
    # remote argv as an array.
    $proc = Start-Process -FilePath 'cmd.exe' `
        -ArgumentList @('/d', '/c', $serverCmd) `
        -WorkingDirectory $root -PassThru `
        -RedirectStandardOutput $outLog -RedirectStandardError $errLog

    $started = Get-Date
    $deadline = $started.AddSeconds($ReadyTimeoutSec)
    $baseUrl = $null
    while ((Get-Date) -lt $deadline) {
        if ($proc.HasExited) { break }
        foreach ($candidate in $candidates) {
            try {
                Invoke-WebRequest -Uri "$candidate/" -TimeoutSec 5 | Out-Null
                $baseUrl = $candidate
                break
            } catch {
            }
        }
        if ($baseUrl) { $ready = $true; break }
        Start-Sleep -Milliseconds 400
    }

    if (-not $ready) {
        $why = if ($proc.HasExited) { "the server exited (code $($proc.ExitCode))" } else { "no answer on any loopback within $ReadyTimeoutSec s" }
        Write-Probe "listening: FAIL - $why" 'Red'
        throw "the dev server never answered on port $Port"
    }
    $secs = [Math]::Round(((Get-Date) - $started).TotalSeconds, 1)
    Write-Probe "listening: $baseUrl answered after ${secs}s" 'Green'

    # `$path`, never `$route`: PowerShell variable names are case-insensitive, so
    # a loop over `$route` would overwrite `$Route` itself and leave the list a
    # single string (measured 2026-11-25 — the verdict read "3/1 routes served"
    # after all three had passed).
    foreach ($path in $Route) {
        $target = "$baseUrl$path"
        try {
            $resp = Invoke-WebRequest -Uri $target -TimeoutSec $RequestTimeoutSec
            $body = [string]$resp.Content
            $bytes = if ($null -ne $resp.RawContentLength) { $resp.RawContentLength } else { [Text.Encoding]::UTF8.GetByteCount($body) }
            $hasRoot = $body.Contains('id="root"')
            $ok = ($resp.StatusCode -eq 200) -and $hasRoot
            $rootWord = if ($hasRoot) { 'yes' } else { 'no' }
            $mark = if ($ok) { 'pass' } else { 'FAIL' }
            $color = if ($ok) { 'Green' } else { 'Red' }
            $results += [pscustomobject]@{ Route = $path; Ok = $ok }
            Write-Probe "check $path -> $($resp.StatusCode), $bytes bytes, root=$rootWord [$mark]" $color
        } catch {
            $status = if ($_.Exception.Response) { [int]$_.Exception.Response.StatusCode } else { 'no response' }
            $results += [pscustomobject]@{ Route = $path; Ok = $false }
            Write-Probe "check $path -> $status, root=no [FAIL]" 'Red'
        }
    }
} catch {
    Write-Probe "error: $($_.Exception.Message)" 'Red'
} finally {
    # Stop the server and its children, then confirm the port is actually free:
    # an orphaned Vite on the workspace would make the next lane's strictPort
    # start fail, so releasing the port is part of passing.
    try {
        if ($proc -and -not $proc.HasExited) { Stop-Tree -RootPid $proc.Id }
        for ($i = 0; $i -lt 25; $i++) {
            if (-not (Test-PortListening $Port)) { break }
            Stop-PortHolders -Port $Port
            Start-Sleep -Milliseconds 200
        }
        $stoppedClean = -not (Test-PortListening $Port)
    } catch {
        $stoppedClean = $false
    }
    if ($stoppedClean) {
        Write-Probe "stopped: port $Port free" 'Green'
    } else {
        Write-Probe "stopped: FAIL - something still listens on $Port" 'Red'
    }
}

$checks = $Route.Count
$okCount = @($results | Where-Object { $_.Ok }).Count
$allOk = $ready -and $stoppedClean -and ($results.Count -eq $checks) -and ($okCount -eq $checks)

if ($allOk) {
    Write-Probe "verdict: PASS ($okCount/$checks routes served, root element present, port released)" 'Green'
    exit 0
}

Write-Probe "verdict: FAIL ($okCount/$checks routes served, port released=$stoppedClean)" 'Red'
foreach ($f in @($outLog, $errLog)) {
    if (Test-Path -LiteralPath $f) {
        $tail = @(Get-Content -LiteralPath $f -Tail 20 -ErrorAction SilentlyContinue)
        if ($tail.Count) {
            Write-Probe "log $(Split-Path -Leaf $f) (last $($tail.Count) lines):"
            foreach ($line in $tail) { Write-Host "    $line" -ForegroundColor DarkGray }
        }
    }
}
exit 1
