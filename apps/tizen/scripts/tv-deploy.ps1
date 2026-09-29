<#
.SYNOPSIS
    Packages dist/ as a signed .wgt, installs it on a Samsung TV and launches it.

.DESCRIPTION
    Needs Tizen Studio (CLI + sdb) and a Samsung certificate profile whose
    distributor certificate lists the TV's DUID. Build first:
    `pnpm --filter @vitals/tizen build`.

    Packages from a TEMP copy of dist, because `tizen package` signs in
    place and would leave author/signature XML files inside dist.

    With -DevTools the app is launched in debug mode instead, and Chromium's
    DevTools port is forwarded to localhost:9222, so `cdp.mjs` can read the
    page, press keys and take screenshots.

.EXAMPLE
    pwsh -NoProfile -File apps/tizen/scripts/tv-deploy.ps1 -DevTools
#>
[CmdletBinding()]
param(
    [string]$Device = '192.168.100.135:26101',
    [string]$CertProfile = 'mixai-samsung',
    [string]$AppId = 'VitalsTv01.Vitals',
    [string]$Log = (Join-Path $PSScriptRoot '..\..\..\.copilot-tmp\tizen\deploy.log'),
    [switch]$DevTools,
    [switch]$NoInstall
)
$ErrorActionPreference = 'Continue'
$tz = Join-Path $env:USERPROFILE 'tizen-studio\tools\ide\bin\tizen.bat'
$sdb = Join-Path $env:USERPROFILE 'tizen-studio\tools\sdb.exe'
foreach ($tool in $tz, $sdb) { if (-not (Test-Path $tool)) { throw "missing $tool — install Tizen Studio" } }
$dist = Join-Path $PSScriptRoot '..\dist'
if (-not (Test-Path (Join-Path $dist 'config.xml'))) { throw 'dist/config.xml missing — run the build first' }
$pkgId = $AppId.Split('.')[0]

New-Item -ItemType Directory -Force (Split-Path $Log) | Out-Null
"deploy $(Get-Date -Format o)" | Set-Content $Log
function Say([string]$m) { $m | Add-Content $Log; Write-Host $m }

$stage = Join-Path $env:TEMP ('vitals-tz-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$out = "$stage-out"
New-Item -ItemType Directory -Force $stage, $out | Out-Null
Copy-Item (Join-Path $dist '*') $stage -Recurse

& $sdb connect $Device.Split(':')[0] *>> $Log
& $tz package -t wgt -s $CertProfile -o $out -- $stage *>> $Log
$wgt = Get-ChildItem $out -Filter *.wgt | Select-Object -First 1
if (-not $wgt) { Say 'NO WGT produced — see the log'; exit 1 }
Say "WGT $($wgt.FullName) $($wgt.Length) bytes"
if ($NoInstall) { exit 0 }

# A running instance keeps the old code and ignores a debug launch.
& $sdb -s $Device shell 0 was_kill $pkgId *>> $Log
& $tz install -n $wgt.Name -s $Device -- $out *>> $Log
if (-not (Select-String -Path $Log -Pattern 'installing\[100\]|install completed|Installed the package' -Quiet)) {
    Say 'install did not report success — see the log'
}

if (-not $DevTools) {
    & $tz run -p $AppId -s $Device *>> $Log
    Say "run exit $LASTEXITCODE"
    exit $LASTEXITCODE
}

$dbgOut = Join-Path $env:TEMP 'vitals-tz-debug.out'
$text = ''
# Two tries: a debug launch while the previous instance is still shutting
# down prints nothing at all (seen on the Odyssey, 2026-09-30).
for ($try = 0; $try -lt 2 -and $text -notmatch 'port:\s*\d+'; $try++) {
    & $sdb -s $Device shell 0 was_kill $pkgId *>> $Log
    Start-Sleep -Seconds (2 + 4 * $try)
    Remove-Item $dbgOut -ErrorAction SilentlyContinue
    # `shell 0 debug` keeps its stream open after printing the port, so run it
    # detached and read the file rather than waiting for it to exit.
    $p = Start-Process -FilePath $sdb -ArgumentList '-s', $Device, 'shell', '0', 'debug', $AppId -RedirectStandardOutput $dbgOut -PassThru -WindowStyle Hidden
    for ($i = 0; $i -lt 30 -and $text -notmatch 'port:\s*\d+'; $i++) {
        Start-Sleep -Seconds 1
        # Get-Content -Raw of an empty file is $null, which Match refuses.
        if (Test-Path $dbgOut) { $text = [string](Get-Content $dbgOut -Raw) }
    }
    if (-not $p.HasExited) { $p.Kill() }
}
# Not `$Matches`: a `-notmatch` that succeeds leaves it null, which is how
# the first run forwarded to port ''.
$found = [regex]::Match($text, 'port:\s*(\d+)')
if (-not $found.Success) { Say "no debug port: $text"; exit 2 }
$port = $found.Groups[1].Value
& $sdb -s $Device forward --remove-all *>> $Log
& $sdb -s $Device forward tcp:9222 "tcp:$port" *>> $Log
Say "debug port $port forwarded to localhost:9222"
