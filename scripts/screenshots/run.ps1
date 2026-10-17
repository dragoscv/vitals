<#
.SYNOPSIS
    Builds the release exe of the current checkout and captures the Store and
    website screenshots from it.

.DESCRIPTION
    Meant for a clean Windows machine (the devcloud workspace ws-vitals-win):
    a neutral user and no personal folders, so nothing private appears in a
    capture. Output goes to target/screenshots. With -Upload the PNGs and the
    hero frames are also copied to the devcloud job bucket under
    shots/<stamp>/, because a cloud job returns only its log.

    The exe is built with `tauri build --no-bundle` so the frontend is embedded
    exactly as in the installer; the sensors and watchdog helpers are skipped
    because no screen captured here uses them. The one difference from the
    release exe is the main window's `additionalBrowserArgs`, which opens the
    WebView2 debugging port: WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS is ignored
    once Tauri sets its own browser arguments (checked 2026-10-17, WebView2
    155: the browser process ran without the port).
#>
[CmdletBinding()]
param(
    [switch]$Upload,
    [string]$Bucket = 'dragos-devcloud-devcloud-jobs',
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root
$out = Join-Path $root 'target/screenshots'
if (Test-Path $out) { Remove-Item -Recurse -Force $out }

if (-not $NoBuild) {
    # The devcloud runner sets CI=1, which the Tauri CLI parses as `--ci 1`
    # and rejects; it only accepts true/false.
    if ($env:CI) { $env:CI = 'true' }
    $conf = Get-Content -Raw 'apps/desktop/src-tauri/tauri.conf.json' | ConvertFrom-Json
    $win = $conf.app.windows[0]
    # Tauri's default arguments must be kept: setting the field replaces them.
    $win | Add-Member -NotePropertyName additionalBrowserArgs -Force -NotePropertyValue (
        '--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port=9333 ' +
        '--disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling')
    $overlay = @{
        build  = @{ beforeBuildCommand = 'pnpm build:vite' }
        bundle = @{ createUpdaterArtifacts = $false }
        app    = @{ windows = @($win) }
    }
    $cfg = Join-Path $env:TEMP 'vitals-shots.conf.json'
    $overlay | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $cfg -Encoding utf8NoBOM
    pnpm --filter '@vitals/desktop' exec tauri build --no-bundle --config $cfg
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
}

# The single-instance plugin hands a second launch to the first and exits,
# so a leftover instance would swallow this run.
Get-Process vitals-desktop, msedgewebview2 -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
# An earlier version of the capture toggled "Start with Windows" by mistake;
# the capture machine must not launch Vitals at sign-in.
Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Vitals' -ErrorAction SilentlyContinue

$exe = Join-Path $root 'target/release/vitals-desktop.exe'
if (-not (Test-Path $exe)) { throw "no exe at $exe" }
"exe $((Get-Item $exe).LastWriteTime.ToString('o'))"

node scripts/screenshots/capture.mjs $exe $out
$rc = $LASTEXITCODE
Get-ChildItem $out -Filter '*.png' -ErrorAction SilentlyContinue | ForEach-Object { '{0} {1}' -f $_.Name, $_.Length }
if ($Upload -and (Test-Path $out)) {
    node scripts/screenshots/upload.mjs $out $Bucket
    if ($LASTEXITCODE -ne 0) { Write-Warning "upload failed ($LASTEXITCODE)" }
}
if ($rc -ne 0) {
    Get-CimInstance Win32_Process |
        Where-Object { $_.Name -match 'webview|vitals' } |
        ForEach-Object { 'PROC ' + $_.ProcessId + ' ' + $_.Name + ' :: ' + "$($_.CommandLine)".Substring(0, [Math]::Min(600, "$($_.CommandLine)".Length)) }
    Get-ChildItem -Path $env:LOCALAPPDATA -Recurse -Filter DevToolsActivePort -ErrorAction SilentlyContinue |
        ForEach-Object { 'PORTFILE ' + $_.FullName + ' = ' + ((Get-Content $_.FullName) -join ' ') }
    Get-Process vitals-desktop, msedgewebview2 -ErrorAction SilentlyContinue | Stop-Process -Force
    throw "capture failed ($rc)"
}
exit 0
