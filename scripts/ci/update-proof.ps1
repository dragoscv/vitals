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
      4. The app is quit: through its own `quit_app` command when the webview
         debugging port answers, else through the tray menu's Quit. Both run
         the install-on-exit path a user's Quit runs.
      5. The installed executable now reports -To.

    Against a published release (-From/-To on GitHub) it must run elevated:
    1.0.0-1.0.3 installers are per-machine only.

    -Local proves DEV-3 instead, and must NOT run elevated: -Dir holds two
    installers and a latest.json built by update-proof-build.ps1 (throwaway
    key, endpoint http://127.0.0.1:8765). A local HTTP server serves them,
    -From is installed for the current user (-Scope user) or, with -Scope
    legacy, an existing per-machine copy is expected to be updated into a
    per-user one; quitting must install -To with no UAC prompt (consent.exe
    never appears).

    Steps 4-5 need an interactive desktop session: in a service or batch
    logon (the devcloud agent) WebView2 never starts, so there is no page to
    ask to quit and no tray to click. Run it from an interactive scheduled task (/IT, and /RL
    HIGHEST only for the published-release mode). Leaves -To installed unless
    -Uninstall is passed. Exit code = number of failed checks.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/update-proof.ps1 -From 1.0.4 -To 1.0.6 -Uninstall

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/update-proof.ps1 -Local -Dir C:\dev3 -From 1.99.0 -To 1.99.1 -Uninstall
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$From,
    [Parameter(Mandatory)][string]$To,
    [string]$Repository = 'dragoscv/vitals',
    [int]$TimeoutSeconds = 240,
    [switch]$Uninstall,
    [switch]$Local,
    [string]$Dir,
    [ValidateSet('user', 'legacy')][string]$Scope = 'user'
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
$elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($Local) {
    if ($elevated) {
        Write-Host 'update-proof -Local must NOT run elevated: it proves the unelevated path (DEV-3)' -ForegroundColor Red
        exit 2
    }
    & (Join-Path $PSScriptRoot 'update-proof-local.ps1') -Dir $Dir -From $From -To $To -Scope $Scope -TimeoutSeconds $TimeoutSeconds -Uninstall:$Uninstall
    exit $LASTEXITCODE
}
if (-not $elevated) {
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

# ── 4. Quit the way a user does ──────────────────────────────────────────────
# The release executable ignores WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, so the
# debugging port is usually refused (proven on the 1.0.3 run). CDP is tried
# first because it is exact; the fallback is the tray menu, which is what a
# user's Quit runs anyway. The menu exposes no UI Automation items, so Quit is
# reached with Up (it is the last item) and Enter. Needs the interactive
# desktop session: run this from a task with LogonType Interactive.
function Invoke-CdpQuit([int]$Port) {
    try {
        $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json" -TimeoutSec 5
    } catch { return $false }
    $page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
    if ($null -eq $page) { return $false }
    $ws = [System.Net.WebSockets.ClientWebSocket]::new()
    $ws.ConnectAsync([uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait()
    $body = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = "window.__TAURI_INTERNALS__.invoke('quit_app')" } } | ConvertTo-Json -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($body)
    $ws.SendAsync([ArraySegment[byte]]::new($bytes), 'Text', $true, [Threading.CancellationToken]::None).Wait()
    return $true
}
function Invoke-TrayQuit {
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms
    if (-not ('VitalsProof.Mouse' -as [type])) {
        Add-Type -Namespace VitalsProof -Name Mouse -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y); [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, System.UIntPtr e);'
    }
    $uia = [System.Windows.Automation.AutomationElement]
    $root = $uia::RootElement
    $scope = [System.Windows.Automation.TreeScope]::Descendants
    $click = {
        param($element, [uint32]$down, [uint32]$up)
        $r = $element.Current.BoundingRectangle
        [VitalsProof.Mouse]::SetCursorPos([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2)) | Out-Null
        [VitalsProof.Mouse]::mouse_event($down, 0, 0, 0, [UIntPtr]::Zero)
        [VitalsProof.Mouse]::mouse_event($up, 0, 0, 0, [UIntPtr]::Zero)
    }
    $chevron = $root.FindFirst($scope, [System.Windows.Automation.PropertyCondition]::new($uia::NameProperty, 'Show Hidden Icons'))
    if ($chevron) { & $click $chevron 0x02 0x04; Start-Sleep -Seconds 2 }
    $buttons = [System.Windows.Automation.PropertyCondition]::new($uia::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    # The tray tooltip starts with "Vitals"; the taskbar's own window button
    # carries the same name, so it is excluded by class.
    $icon = $root.FindAll($scope, $buttons) |
        Where-Object { $_.Current.Name -like 'Vitals*' -and $_.Current.ClassName -notlike 'Taskbar.*' } |
        Select-Object -First 1
    if ($null -eq $icon) { return $false }
    & $click $icon 0x08 0x10
    Start-Sleep -Seconds 1
    [System.Windows.Forms.SendKeys]::SendWait('{UP}')
    Start-Sleep -Milliseconds 300
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    return $true
}
if ($found) {
    $how = if (Invoke-CdpQuit $port) { 'debugging port' } elseif (Invoke-TrayQuit) { 'tray menu' } else { $null }
    Assert-True ($null -ne $how) "asked the app to quit (via $how)"
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
