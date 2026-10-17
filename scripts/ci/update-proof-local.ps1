<#
.SYNOPSIS
    The DEV-3 half of update-proof.ps1: an UNELEVATED Vitals installs a
    downloaded update on quit without a UAC prompt. Called by
    update-proof.ps1 -Local; see that script for the contract.

.DESCRIPTION
    Checks, in order:
      1. -Dir has both installers and a latest.json naming -To.
      2. A local HTTP server answers on 127.0.0.1:8765 (the endpoint baked
         into those two builds).
      3. -Scope user: -From installs per user (/S /CurrentUser) with no
         elevation and registers under HKCU. -Scope legacy: a per-machine
         copy must already exist (installed elevated beforehand, as 1.0.0-
         1.0.3 were); it is what gets updated.
      4. The installed app finds -To and verifies the throwaway signature.
      5. Quitting through the app's own command starts the setup, and
         consent.exe never appears while it runs (a UAC prompt would).
      6. The per-user executable then reports -To. For legacy, the copy now
         runs from %LOCALAPPDATA%\Programs\Vitals, is registered under HKCU,
         and Start-with-Windows (if set) names it.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Dir,
    [Parameter(Mandatory)][string]$From,
    [Parameter(Mandatory)][string]$To,
    [ValidateSet('user', 'legacy')][string]$Scope = 'user',
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
        Start-Sleep -Seconds 1
    }
    return [bool](& $Test)
}
function Get-Reg([string]$Hive) {
    Get-ItemProperty -Path "${Hive}:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals" -ErrorAction SilentlyContinue
}

$Dir = [IO.Path]::GetFullPath($Dir)
$userExe = Join-Path $env:LOCALAPPDATA 'Programs\Vitals\vitals-desktop.exe'
$machineExe = Join-Path $env:ProgramFiles 'Vitals\vitals-desktop.exe'
$log = Join-Path $env:LOCALAPPDATA 'Vitals\logs\vitals.log'

# ── 1. Artefacts ─────────────────────────────────────────────────────────
$fromSetup = Join-Path $Dir "Vitals_${From}_x64-setup.exe"
$toSetup = Join-Path $Dir "Vitals_${To}_x64-setup.exe"
Assert-True ((Test-Path $fromSetup) -and (Test-Path $toSetup)) "both installers are in $Dir"
$manifest = Get-Content (Join-Path $Dir 'latest.json') -Raw | ConvertFrom-Json
Assert-True ($manifest.version -eq $To) "latest.json names $To"
if ($failures.Count) { exit $failures.Count }

# ── 2. Local endpoint ────────────────────────────────────────────────────
# A background job, not a thread: HttpListener on 127.0.0.1 needs no URL ACL
# for the current user, and the job dies with this script.
$server = Start-Job -ArgumentList $Dir -ScriptBlock {
    param($root)
    $l = [System.Net.HttpListener]::new()
    $l.Prefixes.Add('http://127.0.0.1:8765/')
    $l.Start()
    while ($l.IsListening) {
        $c = $l.GetContext()
        $path = Join-Path $root ([Uri]::UnescapeDataString($c.Request.Url.AbsolutePath.TrimStart('/')))
        if (Test-Path $path -PathType Leaf) {
            $bytes = [IO.File]::ReadAllBytes($path)
            $c.Response.ContentLength64 = $bytes.Length
            $c.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        } else { $c.Response.StatusCode = 404 }
        $c.Response.Close()
    }
}
$served = Wait-Until { try { (Invoke-RestMethod 'http://127.0.0.1:8765/latest.json').version -eq $To } catch { $false } } 20
Assert-True $served 'the local update endpoint serves latest.json'

try {
    # ── 3. The copy to be updated ────────────────────────────────────────
    Get-Process vitals-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
    if ($Scope -eq 'user') {
        $p = Start-Process -FilePath $fromSetup -ArgumentList '/S', '/CurrentUser' -PassThru -Wait
        Assert-True ($p.ExitCode -eq 0) "per-user installer for $From exited 0 (got $($p.ExitCode))"
        Assert-True ($null -ne (Get-Reg HKCU)) 'registered under HKCU'
        $launch = $userExe
    } else {
        Assert-True (Test-Path $machineExe) "a per-machine copy exists at $machineExe (install $From elevated first)"
        $m = Get-Reg HKLM
        Assert-True ($null -ne $m -and $null -eq $m.PSObject.Properties['VitalsScopeChosen']) 'it is a legacy registration (no VitalsScopeChosen value)'
        $launch = $machineExe
    }
    $v = if (Test-Path $launch) { (Get-Item $launch).VersionInfo.ProductVersion } else { '(missing)' }
    Assert-True ($v -like "$From*") "the copy to update reports $v, expected $From"

    # ── 4. Launch and wait for the verified download ─────────────────────
    if (Test-Path $log) { Remove-Item $log -Force }
    $port = 9339
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$port"
    $app = Start-Process -FilePath $launch -PassThru
    Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
    $found = Wait-Until { (Test-Path $log) -and ((Get-Content $log -Raw) -match 'update verified') } $TimeoutSeconds
    $tail = if (Test-Path $log) { (Get-Content $log | Select-String 'update|up to date' | ForEach-Object Line) -join "`n" } else { '(no log)' }
    Write-Host $tail
    Assert-True $found "$From found $To and verified its signature"

    # ── 5. Quit, and watch for a UAC prompt ──────────────────────────────
    $consentSeen = $false
    if ($found) {
        $targets = $null
        $up = Wait-Until { try { $script:targets = Invoke-RestMethod "http://127.0.0.1:$port/json"; $true } catch { $false } } 30
        Assert-True $up 'the webview debugging port answered (needs an interactive session)'
        if ($up) {
            $page = $targets | Where-Object { $_.type -eq 'page' } | Select-Object -First 1
            $ws = [System.Net.WebSockets.ClientWebSocket]::new()
            $ws.ConnectAsync([uri]$page.webSocketDebuggerUrl, [Threading.CancellationToken]::None).Wait()
            $body = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = "window.__TAURI_INTERNALS__.invoke('quit_app')" } } | ConvertTo-Json -Compress
            $bytes = [Text.Encoding]::UTF8.GetBytes($body)
            $ws.SendAsync([ArraySegment[byte]]::new($bytes), 'Text', $true, [Threading.CancellationToken]::None).Wait()
            Assert-True (Wait-Until { $app.HasExited } 30) 'the app quit when asked'
            # consent.exe is the UAC prompt. Polled for the whole install.
            $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
            # Done = the setup relaunched the app from the per-user folder
            # (/R, its last step, after the post-install hook). Waiting on
            # the executable's version alone races the hook: the file is
            # replaced first, the shortcuts come after. The updater's temp
            # copy of the setup has a random name, so it cannot be waited on.
            while ((Get-Date) -lt $deadline) {
                if (Get-Process consent -ErrorAction SilentlyContinue) { $consentSeen = $true; break }
                $relaunched = Get-Process vitals-desktop -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $userExe }
                if ($relaunched -and ((Get-Item $userExe).VersionInfo.ProductVersion -like "$To*")) { break }
                Start-Sleep -Milliseconds 250
            }
            $back = Get-Process vitals-desktop -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $userExe }
            Assert-True ($null -ne $back) 'the setup restarted Vitals from the per-user folder'
        }
    }
    Assert-True (-not $consentSeen) 'no UAC prompt (consent.exe) appeared during the update'

    # ── 6. The result ────────────────────────────────────────────────────
    $now = if (Test-Path $userExe) { (Get-Item $userExe).VersionInfo.ProductVersion } else { '(missing)' }
    Assert-True ($now -like "$To*") "the per-user executable reports $now, expected $To"
    Assert-True ($null -ne (Get-Reg HKCU)) 'the updated copy is registered under HKCU'
    if ($Scope -eq 'legacy') {
        Assert-True (Test-Path (Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Vitals.lnk')) 'the per-user copy has a Start menu shortcut'
        $run = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Vitals
        if ($run) {
            Start-Sleep -Seconds 5
            $run = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run').Vitals
            Assert-True ($run -like "$userExe*") "Start with Windows names the per-user copy ($run)"
        }
    }
    $attempt = Join-Path $env:LOCALAPPDATA 'Vitals\update-attempt.json'
    if (Test-Path $attempt) { Write-Host "update marker: $(Get-Content $attempt -Raw)" }
} finally {
    Stop-Job $server -ErrorAction SilentlyContinue; Remove-Job $server -Force -ErrorAction SilentlyContinue
    if ($Uninstall) {
        Get-Process vitals-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
        $un = Join-Path $env:LOCALAPPDATA 'Programs\Vitals\uninstall.exe'
        if (Test-Path $un) { Start-Process -FilePath $un -ArgumentList '/S' -Wait }
    }
}

Write-Host ''
if ($failures.Count -eq 0) { Write-Host "update proof passed (unelevated, $Scope): $From -> $To" }
else { Write-Host "$($failures.Count) check(s) failed" -ForegroundColor Red }
exit $failures.Count
