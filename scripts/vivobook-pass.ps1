#Requires -Version 7
<#
    .SYNOPSIS
    Runs the S21-08 real-OS claims that no CI runner can make.

    .DESCRIPTION
    S21-08 is the part of the elevation/security work whose premise is an
    interactive **non-administrator** Windows session: a machine whose user
    token is filtered (Medium integrity), where the desktop app is installed
    per-user and the sensors helper must prove it cannot be talked into
    writing outside Program Files.

    Two mechanisms matter, and both were learned the hard way on 2026-11-19:

    1. The ssh session on the test laptop is **elevated**, so nothing started
       from it is a standard-user process. The unelevated halves therefore run
       as scheduled tasks registered with `-RunLevel Limited -LogonType
       Interactive`, and the evidence is read back out of a log file the task
       writes (a task has no console).
    2. `Start-Process` from the ssh shell cannot show a GUI at all — that
       session has no desktop. Anything that has to appear on screen (the app
       window, the Settings dialog) must be launched by a task with an
       Interactive logon type, which is also how the app is given an
       administrator token for the SEC-7 reverse direction.

    What this proves, and how:

      SEC-2  With PawnIO installed, `vitals-sensors install` writes
             `install.log` inside `Program Files\Vitals Sensors` and nowhere
             else, and truncates it rather than growing it past 1 MiB. The cap
             is exercised for real: the log is pre-grown past the cap first, so
             a passing run shows it shrink rather than merely exist.
      SEC-7  An elevated CLI reaches an unelevated app **over the pipe**, and
             the reverse direction is measured rather than assumed. It is
             refused (`Access is denied`, os error 5) because the pipe's ACL is
             `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GO;;;OW)` and an elevated token's
             default owner is the Administrators group, not the user; the CLI
             then degrades to the loopback API. That is the safe direction and
             the reason the row no longer claims otherwise.

    Left behind on purpose: PawnIO 2.2.0, the `vitals-sensors` service and the
    install log — that *is* the state SEC-2 describes. Every scheduled task
    this script registers is removed in a `finally`, and the app is relaunched
    through the pre-existing unelevated `VitalsLaunch` task so the laptop is
    left as it was found.

    .EXAMPLE
    pwsh -NoProfile -File scripts/vivobook-pass.ps1

    .EXAMPLE
    # Reuse the PawnIO already installed and only re-check the pipe directions
    pwsh -NoProfile -File scripts/vivobook-pass.ps1 -SkipSensors
#>
[CmdletBinding()]
param(
    # The real-OS test laptop. It is an ordinary machine with a filtered-token
    # user, which is the whole point; see docs/TRACKER.md, "the vivobook pass".
    [string]$Computer = 'dragos-vivobook',

    # The CLI built from the tree under test, already copied to the laptop.
    [string]$CliPath = 'C:\vproof\vitals130\vitals.exe',

    # Skip the PawnIO install and the log-cap exercise (SEC-2).
    [switch]$SkipSensors,

    # Skip the LAN pairing round trip.
    [switch]$SkipPairing,

    # Read-only: report what is installed and what the two boundaries would say,
    # without installing PawnIO, registering a task or touching a firewall rule.
    # Use it to re-check a machine you have already run the full pass against.
    [switch]$Check,

    # Keep the two scheduled tasks this registers, for a follow-up run.
    [switch]$KeepTasks
)

$ErrorActionPreference = 'Stop'

$setupUrl = 'https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe'
$setupSha = '1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032'

# A remote script is shipped as UTF-16LE base64 and run with
# `powershell -EncodedCommand`: the laptop's default ssh shell is cmd, and
# quoting a multi-line PowerShell program through two shells is exactly the
# trap that strands a session at the `>>` prompt.
function Invoke-Remote {
    param([Parameter(Mandatory)][string]$Script)

    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Script))
    $output = & ssh -o ConnectTimeout=20 -o BatchMode=yes $Computer "powershell -NoProfile -EncodedCommand $encoded" 2>&1
    $exit = $LASTEXITCODE
    $output | ForEach-Object { "$_" }
    if ($exit -ne 0) { throw "ssh to $Computer exited $exit" }
}

# ── Read-only check ────────────────────────────────────────────────────
#
# The S21-08 claims are about state, so re-checking them does not have to
# install anything: what was installed stays installed until it is removed.
# The two questions worth asking again are whether the product is still the
# version under test and whether the app's LAN listener still answers.

if ($Check) {
    Write-Host '── read-only check ──' -ForegroundColor Cyan
    $probe = @'
$ErrorActionPreference = 'Continue'
function W($m) { $m }
W '--- the install ---'
foreach ($k in 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals') {
  if (Test-Path $k) { $u = Get-ItemProperty $k; W "  $k => $($u.DisplayVersion) @ $($u.InstallLocation)" } else { W "  $k => absent" }
}
foreach ($p in "$env:LOCALAPPDATA\Programs\Vitals\vitals-desktop.exe", "$env:ProgramFiles\Vitals\vitals-desktop.exe") {
  $version = if (Test-Path $p) { (Get-Item $p).VersionInfo.ProductVersion } else { 'absent' }
  W "  $p => $version"
}
W '--- SEC-2 state: the helper and its log ---'
$helper = "$env:LOCALAPPDATA\Programs\Vitals\sensors\vitals-sensors.exe"
$helperVersion = if (Test-Path $helper) { (& $helper version 2>&1 | Select-Object -First 1) } else { 'absent' }
W "  helper => $helperVersion"
$log = Join-Path $env:ProgramFiles 'Vitals Sensors\install.log'
$logSize = if (Test-Path $log) { "$((Get-Item $log).Length) bytes" } else { 'absent' }
W "  install.log => $logSize"
$service = (Get-Service vitals-sensors -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Status)/$($_.StartType)" }) -join ' '
W "  service => $service"
if (Test-Path $log) { W "  last line: $((Get-Content $log -Tail 1) -join '')" }
W '--- SEC-9 state: where the app listens ---'
Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.LocalPort -in 7330, 7331 } | ForEach-Object { W "  $($_.LocalAddress):$($_.LocalPort)" }
W '--- S24-22 state: does the firewall admit the CLI? ---'
$cli = '__CLI__'
$key = 'HKLM:\SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\FirewallRules'
$all = Get-ItemProperty $key
$naming = 0
foreach ($name in (Get-Item $key).Property) {
  if ("$($all.$name)" -match [regex]::Escape($cli)) { $naming++ }
}
W "  rules naming the CLI path: $naming (0 means vitals serve would warn)"
'CHECK-DONE'
'@
    $probe = $probe.Replace('__CLI__', $CliPath)
    (Invoke-Remote -Script $probe) | ForEach-Object { "  $_" }
    Write-Host 'read-only check finished — nothing on the machine was changed.' -ForegroundColor Green
    return
}

# ── SEC-2: the sensors helper's log ────────────────────────────────────

if (-not $SkipSensors) {
    Write-Host '── SEC-2: sensors log location and the 1 MiB cap ──' -ForegroundColor Cyan
    $remote = @"
`$ErrorActionPreference = 'Continue'
`$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
"elevated: `$admin (SEC-2 is an elevated operation)"
`$install = "`$env:LOCALAPPDATA\Programs\Vitals"
Get-ChildItem "`$install\sensors" -Force | Select-Object Name, Length | Format-Table -AutoSize | Out-String
`$helper = Get-ChildItem "`$install\sensors" -Filter '*sensors*.exe' | Select-Object -First 1
if (-not `$helper) { throw 'no sensors helper in the per-user install' }
"helper: `$(`$helper.FullName)"
"version: `$((& `$helper.FullName version 2>&1 | Out-String).Trim())"

`$setup = Join-Path `$env:TEMP 'PawnIO_setup.exe'
if (-not (Test-Path `$setup)) {
  Invoke-WebRequest -Uri '$setupUrl' -OutFile `$setup -UseBasicParsing -TimeoutSec 180
}
`$got = (Get-FileHash `$setup -Algorithm SHA256).Hash
if (`$got -ne '$setupSha') { throw "PawnIO setup sha256 `$got is not the pinned $setupSha" }
"PawnIO setup sha256 matches the pin"

# Exercise the cap rather than trusting it: grow the log past 1 MiB first, so
# a pass means it shrank. Each install appends a dozen lines, so waiting for a
# real install to reach the cap would take years of runs.
`$logDir = Join-Path `$env:ProgramFiles 'Vitals Sensors'
New-Item -ItemType Directory -Force -Path `$logDir | Out-Null
`$logPath = Join-Path `$logDir 'install.log'
"before: `$((Get-Item `$logPath -ErrorAction SilentlyContinue).Length) bytes"
`$sw = [IO.StreamWriter]::new(`$logPath, `$false)
for (`$i = 0; `$i -lt 1200; `$i++) { `$sw.WriteLine("x" * 1024) }
`$sw.Close()
"pre-grown to: `$((Get-Item `$logPath).Length) bytes (cap is 1048576)"

& `$helper.FullName install --pawnio-setup `$setup 2>&1 | Out-String
"EXIT=`$LASTEXITCODE"
"after: `$((Get-Item `$logPath).Length) bytes"

`$hits = @()
`$hits += Get-ChildItem `$env:TEMP -Recurse -Force -Filter 'install.log' -ErrorAction SilentlyContinue
`$hits += Get-ChildItem `$install -Recurse -Force -Filter 'install.log' -ErrorAction SilentlyContinue
`$hits += Get-ChildItem "`$env:LOCALAPPDATA\Vitals" -Recurse -Force -Filter 'install.log' -ErrorAction SilentlyContinue
if (`$hits.Count -gt 0) { `$hits | ForEach-Object { "HIT `$(`$_.FullName)" } } else { 'no install.log outside Program Files (good)' }

Get-Service vitals-sensors -ErrorAction SilentlyContinue | Select-Object Name, Status, StartType | Format-Table -AutoSize | Out-String
"read: `$((& `$helper.FullName read 2>&1 | Out-String).Trim())"
'SEC2-DONE'
"@
    Invoke-Remote -Script $remote
}

# ── SEC-7: which way the pipe is reachable ─────────────────────────────

Write-Host '── SEC-7: the pipe, in both directions ──' -ForegroundColor Cyan

# The app has to be started by a task with an Interactive logon type and
# `-RunLevel Highest`: that is the only way to get an elevated app with a
# desktop, and the elevation is the entire subject of the second half.
$appScript = @'
$ErrorActionPreference = 'Continue'
$log = 'C:\Users\vladu\vb-elev-app.log'
Remove-Item $log -Force -ErrorAction SilentlyContinue
function W($m) { $m | Out-File -FilePath $log -Append -Encoding utf8 }
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
W "task shell elevated=$admin (the app inherits this token)"
Get-Process vitals-desktop -ErrorAction SilentlyContinue | ForEach-Object { W "stopping pid=$($_.Id)"; $_ | Stop-Process -Force }
Start-Sleep -Seconds 6
Start-Process -FilePath "$env:LOCALAPPDATA\Programs\Vitals\vitals-desktop.exe"
for ($i = 0; $i -lt 30; $i++) { Start-Sleep -Seconds 2; if (Get-Process vitals-desktop -ErrorAction SilentlyContinue) { break } }
$p = Get-Process vitals-desktop -ErrorAction SilentlyContinue | Select-Object -First 1
if ($p) { W "app pid=$($p.Id)" } else { W 'APP NOT RUNNING' }
W 'ELEV-APP-DONE'
'@

$cliScript = @'
$ErrorActionPreference = 'Continue'
$log = Join-Path $env:TEMP 'vb-sec7-lim.log'
Remove-Item $log -Force -ErrorAction SilentlyContinue
function W($m) { $m | Out-File -FilePath $log -Append -Encoding utf8 }
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
W "task shell elevated=$admin (must be False: this is the standard-user half)"
W ("whoami=" + (whoami))
Get-Process vitals-desktop -ErrorAction SilentlyContinue | ForEach-Object { W "app visible to this token: pid=$($_.Id)" }
$cli = "$env:LOCALAPPDATA\vproof\vitals.exe"
W '--- auto: the CLI is free to pick a transport ---'
$auto = & $cli ps --top 3 2>&1 | Out-String
W $auto
# `--source app` is pipe-only and fails rather than falling back, so it is what
# turns "it worked" into "it worked *over the pipe*".
W '--- pipe only: --source app must not fall back ---'
$pipe = & $cli --source app ps --top 3 2>&1 | Out-String
W $pipe
W ("PIPE-ONLY-EXIT=" + $LASTEXITCODE)
W 'SEC7-DONE'
'@

$driver = @'
$ErrorActionPreference = 'Continue'
function W($m) { $m }
$user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$utf8 = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText('C:\Users\vladu\vb-elev-app.ps1', [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__APP__')), $utf8)
[IO.File]::WriteAllText('C:\Users\vladu\vb-sec7-lim.ps1', [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__CLI__')), $utf8)

# A Medium-integrity token cannot read the elevated tooling directory, so the
# CLI is copied somewhere the standard user owns before the Limited task runs.
New-Item -ItemType Directory -Force -Path "$env:LOCALAPPDATA\vproof" | Out-Null
Copy-Item '__CLIPATH__' "$env:LOCALAPPDATA\vproof\vitals.exe" -Force
"cli copied: $((Get-Item "$env:LOCALAPPDATA\vproof\vitals.exe").Length) bytes"

try {
  $appAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\Users\vladu\vb-elev-app.ps1'
  $appPrincipal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest
  Register-ScheduledTask -TaskName 'VbElevApp' -Action $appAction -Principal $appPrincipal -Force | Out-Null

  $cliAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\Users\vladu\vb-sec7-lim.ps1'
  $cliPrincipal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
  Register-ScheduledTask -TaskName 'VbSec7Pipe' -Action $cliAction -Principal $cliPrincipal -Force | Out-Null
  "tasks registered for $user"

  Start-ScheduledTask -TaskName 'VbElevApp'
  Start-Sleep -Seconds 50
  '--- elevated app ---'
  Get-Content 'C:\Users\vladu\vb-elev-app.log' -ErrorAction SilentlyContinue
  Get-Process vitals-desktop -ErrorAction SilentlyContinue | ForEach-Object { "app pid=$($_.Id)" }

  Start-ScheduledTask -TaskName 'VbSec7Pipe'
  Start-Sleep -Seconds 35
  '--- standard-user CLI ---'
  Get-Content (Join-Path $env:TEMP 'vb-sec7-lim.log') -ErrorAction SilentlyContinue
}
finally {
  __CLEANUP__
}
Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -like 'Vb*' } | ForEach-Object { "LEFTOVER TASK $($_.TaskName)" }
'@

$cleanup = if ($KeepTasks) {
  "'tasks kept: VbElevApp, VbSec7Pipe'"
} else {
    @'
Unregister-ScheduledTask -TaskName 'VbElevApp' -Confirm:$false -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName 'VbSec7Pipe' -Confirm:$false -ErrorAction SilentlyContinue
if (-not (Get-Process vitals-desktop -ErrorAction SilentlyContinue)) { Start-ScheduledTask -TaskName 'VitalsLaunch' -ErrorAction SilentlyContinue }
"tasks removed; the app is relaunched unelevated by the pre-existing VitalsLaunch task"
'@
}

$appB64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($appScript))
$cliB64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($cliScript))
$driver = $driver.Replace('__APP__', $appB64).Replace('__CLI__', $cliB64).Replace('__CLIPATH__', $CliPath).Replace('__CLEANUP__', $cleanup)

Invoke-Remote -Script $driver

# ── The LAN pairing round trip ─────────────────────────────────────────
#
# The pairing leg of S21-08/S24-16 without a phone or a TV: the CLI's headless
# server prints a one-shot six-digit code at start-up, so the exchange can be
# driven from this machine against the LAN address.
#
# The CLI has no inbound firewall rule (S24-22), so the same request answers
# `HTTP 000` until one exists. A temporary rule is created for the duration and
# removed in the `finally` — an unreachable listener is the finding, not a
# reason to skip the check.

if (-not $SkipPairing) {
  Write-Host '── Pairing over the LAN, with no phone ──' -ForegroundColor Cyan

  $serveScript = @'
$ErrorActionPreference = 'Continue'
Get-Process vitals-desktop -ErrorAction SilentlyContinue | ForEach-Object { $_ | Stop-Process -Force }
Start-Sleep -Seconds 5
& 'C:\vproof\vitals130\vitals.exe' serve *> 'C:\Users\vladu\vb-serve-pass.log'
'@
  $pairDriver = @'
$ErrorActionPreference = 'Continue'
$utf8 = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText('C:\Users\vladu\vb-serve-pass.ps1', [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__SERVE__')), $utf8)
Remove-Item 'C:\Users\vladu\vb-serve-pass.log' -Force -ErrorAction SilentlyContinue
Remove-NetFirewallRule -DisplayName 'VbServe CLI (temp)' -ErrorAction SilentlyContinue
New-NetFirewallRule -DisplayName 'VbServe CLI (temp)' -Direction Inbound -Action Allow -Program '__CLI__' -Profile Any | Out-Null
'temporary inbound rule created for the CLI'
$u = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\Users\vladu\vb-serve-pass.ps1'
$p = New-ScheduledTaskPrincipal -UserId $u -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName 'VbServePass' -Action $a -Principal $p -Force | Out-Null
Start-ScheduledTask -TaskName 'VbServePass'
Start-Sleep -Seconds 15
Get-Content 'C:\Users\vladu\vb-serve-pass.log' -ErrorAction SilentlyContinue
'PAIR-SETUP-DONE'
'@
  $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($serveScript))
  $pairDriver = $pairDriver.Replace('__SERVE__', $b64).Replace('__CLI__', $CliPath)
  $setup = (Invoke-Remote -Script $pairDriver) -join "`n"
  $setup -split "`n" | Where-Object { $_.Trim() -ne '' } | ForEach-Object { "  $_" }

  $codeMatch = [regex]::Match($setup, 'TV pairing code[^:]*:\s*(\d{3})\s*(\d{3})')
  $baseMatch = [regex]::Match($setup, '(http://\d+\.\d+\.\d+\.\d+:\d+)/mobile\.html')
  if (-not $codeMatch.Success -or -not $baseMatch.Success) {
    Write-Host '  the server printed no pairing code — the exchange was not run' -ForegroundColor Yellow
  } else {
    $code = $codeMatch.Groups[1].Value + $codeMatch.Groups[2].Value
    $base = $baseMatch.Groups[1].Value
    Write-Host "  server $base, code $code"

    $failures = @()
    $pairBody = '{"code":"' + $code + '"}'
    $first = & curl.exe -s -w ' HTTP=%{http_code}' -X POST "$base/api/v1/pair" -H 'Content-Type: application/json' -d $pairBody
    Write-Host "  pair: $first"
    $payload = ($first -split ' HTTP=')[0]
    if (($first -split ' HTTP=')[-1] -ne '200') { $failures += 'the pairing exchange did not answer 200' }
    $token = ''
    try { $token = ($payload | ConvertFrom-Json).token } catch { }
    if ($token.Length -ne 43) { $failures += "the minted token is $($token.Length) characters, not 43" }
    else {
      Write-Host "  token: 43 characters (prefix $($token.Substring(0, 6))…)"
      $summary = & curl.exe -s -o NUL -w '%{http_code}' -H "Authorization: Bearer $token" "$base/api/v1/summary"
      Write-Host "  /api/v1/summary with the minted token: $summary"
      if ($summary -ne '200') { $failures += "the minted token could not read /summary ($summary)" }
      $control = & curl.exe -s -o NUL -w '%{http_code}' -X POST "$base/api/v1/control" -H "Authorization: Bearer $token" -H 'Content-Type: application/json' -d '{"action":"terminate","key":{"pid":1,"startTime":1}}'
      Write-Host "  /api/v1/control with the read token: $control"
      if ($control -ne '403') { $failures += "a read token reached control ($control)" }
    }
    $replay = & curl.exe -s -w ' HTTP=%{http_code}' -X POST "$base/api/v1/pair" -H 'Content-Type: application/json' -d $pairBody
    Write-Host "  replay: $replay"
    if (($replay -split ' HTTP=')[-1] -ne '403') { $failures += 'a spent code minted a second token' }
    $wrong = & curl.exe -s -w ' HTTP=%{http_code}' -X POST "$base/api/v1/pair" -H 'Content-Type: application/json' -d '{"code":"000000"}'
    Write-Host "  wrong code: $wrong"
    if ((($wrong -split ' HTTP=')[0]) -ne (($replay -split ' HTTP=')[0])) { $failures += 'a wrong code is distinguishable from a spent one' }

    if ($failures.Count -eq 0) { Write-Host '  pairing: all five assertions held' -ForegroundColor Green }
    else { $failures | ForEach-Object { Write-Host "  pairing FAILED: $_" -ForegroundColor Red } }
  }

  $teardown = @'
$ErrorActionPreference = 'Continue'
Stop-ScheduledTask -TaskName 'VbServePass' -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName 'VbServePass' -Confirm:$false -ErrorAction SilentlyContinue
Get-Process vitals -ErrorAction SilentlyContinue | ForEach-Object { $_ | Stop-Process -Force }
Start-Sleep -Seconds 3
Remove-NetFirewallRule -DisplayName 'VbServe CLI (temp)' -ErrorAction SilentlyContinue
if (Get-NetFirewallRule -DisplayName 'VbServe CLI (temp)' -ErrorAction SilentlyContinue) { 'TEMPORARY RULE STILL PRESENT' } else { 'temporary inbound rule removed' }
Start-ScheduledTask -TaskName 'VitalsLaunch' -ErrorAction SilentlyContinue
'PAIR-TEARDOWN-DONE'
'@
  (Invoke-Remote -Script $teardown) | ForEach-Object { "  $_" }
}

Write-Host 'vivobook pass finished — read the blocks above; nothing is asserted for you.' -ForegroundColor Green
