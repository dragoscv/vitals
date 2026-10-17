<#
.SYNOPSIS
    Proves that an installed older Vitals updates itself to the published
    release: latest.json is signed by the app's key, the old app finds,
    downloads and verifies it, and quitting installs it.

.DESCRIPTION
    The updater signing moved into the approved publish job in 1.0.3 (SEC-5).
    A mistake there produces a release that looks fine on GitHub and that no
    installed copy will ever accept — the failure is silent on both sides.
    This script is the end-to-end check, run against the real GitHub release:

      1. latest.json names -To, and each Windows signature was made by the key
         whose public half is in tauri.conf.json (minisign key id), with -To
         in its signed trusted comment.
      2. -From is installed silently from its GitHub release and launched with
         a debugging port on its webview.
      3. The app's own log says it found -To and verified the download (the
         first check runs 20 s after launch).
      4. The app is quit through its own `quit_app` command, which runs the
         install-on-exit path a user's Quit runs.
      5. The installed executable now reports -To.

    Must run elevated: the installer is perMachine, and the passive update it
    starts on quit inherits our token instead of raising a UAC prompt nobody
    is there to answer (DEV-3 covers the prompt itself). Leaves -To installed
    unless -Uninstall is passed. Exit code = number of failed checks.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/update-proof.ps1 -From 1.0.2 -To 1.0.3 -Uninstall
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$From,
    [Parameter(Mandatory)][string]$To,
    [string]$Repository = 'dragoscv/vitals',
    [int]$TimeoutSeconds = 240,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$failures = [System.Collections.Generic.List[string]]::new()
function Assert-True([bool]$Condition, [string]$Message) {
    if ($Condition) { Write-Host "ok    $Message" }
    else { Write-Host "FAIL  $Message" -ForegroundColor Red; $failures.Add($Message) }
}
function Wait-Until([scriptblock]$Test, [int]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if (& $Test) { return $true }
        Start-Sleep -Seconds 2
    }
    return [bool](& $Test)
}

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'update-proof must run elevated: the installer is perMachine' -ForegroundColor Red
    exit 2
}

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$work = Join-Path ([IO.Path]::GetTempPath()) "vitals-update-proof-$PID"
New-Item -ItemType Directory -Force -Path $work | Out-Null
$exe = Join-Path $env:ProgramFiles 'Vitals\vitals-desktop.exe'
$log = Join-Path $env:LOCALAPPDATA 'Vitals\logs\vitals.log'

# ── 1. The manifest and its signatures ──────────────────────────────────
# A minisign key id is the 8 bytes after the 2-byte algorithm tag, in both
# the public key and every signature it made.
function Get-KeyId([string]$base64Box, [int]$line) {
    $text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($base64Box))
    $raw = [Convert]::FromBase64String(($text -split "`n")[$line].Trim())
    [BitConverter]::ToString($raw[2..9])
}
$conf = Get-Content (Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$appKey = Get-KeyId $conf.plugins.updater.pubkey 1

$manifestUrl = "https://github.com/$Repository/releases/latest/download/latest.json"
$manifest = Invoke-RestMethod -Uri $manifestUrl -Headers @{ 'Cache-Control' = 'no-cache' }
Assert-True ($manifest.version -eq $To) "latest.json says $($manifest.version), expected $To"
foreach ($platform in 'windows-x86_64', 'windows-aarch64') {
    $entry = $manifest.platforms.$platform
    if ($null -eq $entry) { Assert-True $false "latest.json has no $platform entry"; continue }
    $sigText = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($entry.signature))
    Assert-True ((Get-KeyId $entry.signature 1) -eq $appKey) "$platform signed by the app's key $appKey"
    Assert-True ($sigText -match [regex]::Escape($To)) "$platform trusted comment names $To"
    Assert-True ($entry.url -match [regex]::Escape("v$To/")) "$platform URL points at the v$To release"
}

# ── 2. Install the old version ──────────────────────────────────────────
Get-Process vitals-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
$setup = Join-Path $work "Vitals_${From}_x64-setup.exe"
Invoke-WebRequest -Uri "https://github.com/$Repository/releases/download/v$From/Vitals_${From}_x64-setup.exe" -OutFile $setup
$p = Start-Process -FilePath $setup -ArgumentList '/S' -PassThru -Wait
Assert-True ($p.ExitCode -eq 0) "installer for $From exited 0 (got $($p.ExitCode))"
$installed = (Get-Item $exe).VersionInfo.ProductVersion
Assert-True ($installed -like "$From*") "installed executable reports $installed, expected $From"

# ── 3. Launch it and wait for the verified download ─────────────────────
if (Test-Path $log) { Remove-Item $log -Force }
$port = 9339
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$port"
$app = Start-Process -FilePath $exe -PassThru
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$found = Wait-Until { (Test-Path $log) -and ((Get-Content $log -Raw) -match 'update verified') } $TimeoutSeconds
$tail = if (Test-Path $log) { (Get-Content $log | Select-String 'update|up to date' | ForEach-Object Line) -join "`n" } else { '(no log)' }
Write-Host $tail
Assert-True ($found -and $tail -match [regex]::Escape($To)) "$From found $To and verified its signature"

# ── 4. Quit through the app's own command ───────────────────────────────
# CDP over a raw WebSocket: no Node, no Playwright on the machine under test.
if ($found) {
    $targets = Invoke-RestMethod "http://127.0.0.1:$port/json"
    $page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
    $ws = [System.Net.WebSockets.ClientWebSocket]::new()
    $ws.ConnectAsync([uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait()
    $body = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = "window.__TAURI_INTERNALS__.invoke('quit_app')" } } | ConvertTo-Json -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($body)
    $ws.SendAsync([ArraySegment[byte]]::new($bytes), 'Text', $true, [Threading.CancellationToken]::None).Wait()
    Assert-True (Wait-Until { $app.HasExited } 30) 'the app quit when asked'
}

# ── 5. The installed version afterwards ─────────────────────────────────
$updated = Wait-Until {
    -not (Get-Process -Name "Vitals_${To}_x64-setup", 'Un_A' -ErrorAction SilentlyContinue) -and
    (Test-Path $exe) -and ((Get-Item $exe).VersionInfo.ProductVersion -like "$To*")
} $TimeoutSeconds
$now = if (Test-Path $exe) { (Get-Item $exe).VersionInfo.ProductVersion } else { '(missing)' }
Assert-True $updated "after quitting, the installed executable reports $now, expected $To"

if ($Uninstall) {
    Get-Process vitals-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
    $un = Join-Path $env:ProgramFiles 'Vitals\uninstall.exe'
    if (Test-Path $un) { Start-Process -FilePath $un -ArgumentList '/S' -Wait }
}
Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue

Write-Host ''
if ($failures.Count -eq 0) { Write-Host "update proof passed: $From -> $To" }
else { Write-Host "$($failures.Count) check(s) failed" -ForegroundColor Red }
exit $failures.Count
