<#
.SYNOPSIS
    Installs the NSIS setup silently, turns on everything the app can leave
    on the machine, uninstalls it, and proves the uninstall removed all of
    it (QA-1, QA-2, QA2-2). Then the same for a per-user install, and an
    upgrade over the previous release.

.DESCRIPTION
    Every assertion here is a defect that a unit test cannot see because it
    lives in the installer script (apps/desktop/src-tauri/windows/hooks.nsh)
    or in what the app writes to the machine at run time.

    Until 2026-10-18 the removal checks were vacuous: nothing turned the
    features on, so "no sensors service after uninstall" passed on a machine
    that never had one (QA2-2). Now, between install and uninstall, each is
    switched on the way the app does it, and its presence is asserted BEFORE
    the uninstall, so a removal check can only pass by removing something:

      - sensors: `<install>\sensors\vitals-sensors.exe install`, which
        downloads the pinned PawnIO installer (the app does the download;
        this script fetches the same pinned URL and hash), installs the
        driver and creates the LocalSystem `vitals-sensors` service. A runner
        that refuses the driver still gets the service registration: the
        helper is then registered by hand with `sc create`, exactly the
        state the uninstaller must clean (see -SensorsMode);
      - Task Manager replacement: `vitals-desktop.exe
        --set-taskmgr-replacement on`, the elevated child the Settings switch
        spawns, which writes the IFEO Debugger;
      - Start with Windows: HKCU Run `Vitals` = `<exe> --minimized`, unquoted,
        as tauri-plugin-autostart (auto-launch 0.5) writes it, plus the
        StartupApproved toggle;
      - lag watchdog: HKCU Run `VitalsWatchdog` = `"<install>\watchdog\
        vitals-watchdog.exe"` and the process running, as the Settings
        switch does (watchdog.rs `enable`). `--install` is NOT used: it
        copies the exe to %LOCALAPPDATA% (a hand install the uninstaller
        deliberately leaves alone) and its child never exits, so
        Start-Process -Wait on it hangs.

    Passes:

      1. machine  `setup.exe /S /AllUsers` -> Program Files, everything on,
                  launch, uninstall, all gone.
      2. user     `setup.exe /S /CurrentUser` -> %LOCALAPPDATA%\Programs\
                  Vitals, HKCU uninstall entry, launch, the sensors service
                  installed from the per-user copy's helper (S21-06),
                  uninstall, gone - the SYSTEM service and Program Files\
                  Vitals Sensors included.
      3. upgrade  -Previous (the last published release) installed per
                  machine, a settings file written, then -Setup over it:
                  the executable reports the new version, the install
                  directory is the same, the settings file is untouched,
                  and an enabled watchdog survives the update.

    Must run elevated (pass 1 and 3 install per machine; GitHub's Windows
    runners are elevated). Exit code is the number of failed assertions.

    Elevation and the per-user pass: the per-user uninstaller is started from
    this elevated script, so it inherits an administrator token and the
    helper's own `uninstall` succeeds at once. The branch a real standard
    user reaches (helper exits 5, hooks.nsh retries it with the "runas" verb,
    one UAC prompt) is therefore NOT driven here: on these runners a UAC
    prompt is approved silently or never shown, so the pass proves the hook
    reaches and runs the removal for a per-user copy, not the prompt itself.
    That branch is held by a guard test on hooks.nsh (updates.rs) and by
    makensis compiling it, not by this script.

.PARAMETER Setup
    Path to the NSIS `*-setup.exe` under test.

.PARAMETER Previous
    Path to the previous release's x64 setup, for the upgrade pass. Omitted:
    the pass is skipped with a note, never silently passed.

.PARAMETER SensorsMode
    `auto` (default) tries the real helper install and falls back to a bare
    service registration when the driver cannot load; `real` fails when the
    helper install fails; `register` skips the download.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/installer-roundtrip.ps1 -Setup target/release/bundle/nsis/Vitals_1.0.5_x64-setup.exe -Previous prev/Vitals_1.0.4_x64-setup.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Setup,
    [string]$Previous,
    [ValidateSet('machine', 'user', 'upgrade')][string[]]$Pass = @('machine', 'user', 'upgrade'),
    [ValidateSet('auto', 'real', 'register')][string]$SensorsMode = 'auto',
    # Seconds the launched app must survive. Long enough for the webview to
    # load and the sampler to start; a crash on start lands well inside it.
    [int]$AliveSeconds = 10,
    # Runs between install and uninstall of the machine pass, with the
    # installed exe path as its only argument (idle-budget.ps1 in CI), so
    # one install serves both jobs.
    [string]$WhileInstalled
)

$ErrorActionPreference = 'Stop'

$failures = [System.Collections.Generic.List[string]]::new()
$notes = [System.Collections.Generic.List[string]]::new()
function Assert-True {
    param([bool]$Condition, [string]$Message)
    if ($Condition) {
        Write-Host "ok    $Message"
    } else {
        Write-Host "FAIL  $Message" -ForegroundColor Red
        $failures.Add($Message)
    }
}
function Note([string]$Message) { Write-Host "note  $Message" -ForegroundColor Yellow; $notes.Add($Message) }

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'installer-roundtrip must run elevated: two passes install per machine' -ForegroundColor Red
    exit 2
}

$setupPath = (Resolve-Path $Setup).Path
$machineDir = Join-Path $env:ProgramFiles 'Vitals'
$userDir = Join-Path $env:LOCALAPPDATA 'Programs\Vitals'
$sensorsDir = Join-Path $env:ProgramFiles 'Vitals Sensors'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$approvedKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
$ifeoKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\taskmgr.exe'
$uninstallSub = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals'
$processNames = @('vitals-desktop', 'vitals-watchdog', 'vitals-sensors')
$sc = Join-Path $env:SystemRoot 'System32\sc.exe'
# Kept in step with crates/vitals-sensors/src/lib.rs (PAWNIO_SETUP_URL/SHA256).
$pawnioUrl = 'https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe'
$pawnioSha = '1F519A22E47187F70A1379A48CA604981C4FCF694F4E65B734AAA74A9FBA3032'

# Polls until $Test returns true or the deadline passes. An NSIS uninstaller
# copies itself to %TEMP% and returns at once, so "the process exited" is not
# "the uninstall finished".
function Wait-Until {
    param([scriptblock]$Test, [int]$Seconds)
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        if (& $Test) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return [bool](& $Test)
}

function Test-Names([object]$Value, [string]$Dir) {
    if ($null -eq $Value) { return $false }
    return ([string]$Value).Trim().Trim('"').StartsWith($Dir, [StringComparison]::OrdinalIgnoreCase)
}
function Get-RunValue([string]$Name) { (Get-ItemProperty -Path $runKey -Name $Name -ErrorAction SilentlyContinue).$Name }
# ProductVersion of the exe and of the NSIS setup differ in form ("1.0.4"
# vs "1.0.4.0"); compare the first three parts.
function ConvertTo-Version([string]$Text) {
    if (-not $Text) { return '' }
    (($Text.Trim() -split '[^0-9]+' | Where-Object { $_ -ne '' } | Select-Object -First 3) -join '.')
}
function Get-Debugger { (Get-ItemProperty -Path $ifeoKey -Name 'Debugger' -ErrorAction SilentlyContinue).Debugger }
function Test-Service {
    & $sc query vitals-sensors | Out-Null
    $code = $LASTEXITCODE
    $global:LASTEXITCODE = 0
    return $code -ne 1060
}
function Stop-Vitals {
    Get-Process -Name $processNames -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

# A clean slate: a previous pass (or a developer's own copy on a devcloud
# workspace) must not satisfy or break this one.
function Reset-Machine {
    Stop-Vitals
    foreach ($dir in $machineDir, $userDir) {
        $u = Join-Path $dir 'uninstall.exe'
        if (Test-Path $u) {
            Start-Process -FilePath $u -ArgumentList '/S' -Wait | Out-Null
            [void](Wait-Until { -not (Test-Path $dir) } 60)
        }
    }
    if (Test-Service) {
        & $sc stop vitals-sensors | Out-Null
        & $sc delete vitals-sensors | Out-Null
        $global:LASTEXITCODE = 0
    }
    foreach ($n in 'Vitals', 'VitalsWatchdog') {
        Remove-ItemProperty -Path $runKey -Name $n -ErrorAction SilentlyContinue
        Remove-ItemProperty -Path $approvedKey -Name $n -ErrorAction SilentlyContinue
    }
    $d = Get-Debugger
    if ($d -and $d -match 'vitals-desktop') { Remove-ItemProperty -Path $ifeoKey -Name 'Debugger' -ErrorAction SilentlyContinue }
}

function Install-Setup([string]$Path, [string]$Mode) {
    $flag = if ($Mode -eq 'user') { '/CurrentUser' } else { '/AllUsers' }
    Write-Host "installing $Path ($flag)"
    $p = Start-Process -FilePath $Path -ArgumentList '/S', $flag -Wait -PassThru
    return $p.ExitCode
}

function Find-Shortcut([string]$Exe) {
    $menus = @(
        (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs'),
        (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs')
    )
    $lnk = $menus | Where-Object { Test-Path $_ } |
        ForEach-Object { Get-ChildItem $_ -Recurse -Filter 'Vitals.lnk' -ErrorAction SilentlyContinue } |
        Where-Object { (New-Object -ComObject WScript.Shell).CreateShortcut($_.FullName).TargetPath -ieq $Exe } |
        Select-Object -First 1
    return $lnk
}

function Test-Launch([string]$Exe) {
    $app = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds $AliveSeconds
    $app.Refresh()
    Assert-True (-not $app.HasExited) "the installed app is still running after $AliveSeconds s (exit code $(if ($app.HasExited) { $app.ExitCode } else { 'n/a' }))"
    Stop-Vitals
}

function Invoke-Uninstall([string]$Dir) {
    $u = Join-Path $Dir 'uninstall.exe'
    if (-not (Test-Path $u)) { $failures.Add("no uninstaller in $Dir"); return }
    Write-Host "uninstalling via $u"
    $p = Start-Process -FilePath $u -ArgumentList '/S' -Wait -PassThru
    Assert-True ($p.ExitCode -eq 0) "silent uninstall exits 0 (got $($p.ExitCode))"
    Assert-True (Wait-Until { -not (Test-Path $Dir) } 120) "the install directory $Dir is gone"
}

# ── Feature switches (what the app turns on) ───────────────────────────────

function Enable-Sensors([string]$Dir) {
    $helper = Join-Path $Dir 'sensors\vitals-sensors.exe'
    Assert-True (Test-Path $helper) "the sensors helper is bundled at $helper"
    if (-not (Test-Path $helper)) { return }
    $real = $false
    if ($SensorsMode -ne 'register') {
        $pawnio = Join-Path $tempDir 'PawnIO_setup.exe'
        try {
            Invoke-WebRequest -Uri $pawnioUrl -OutFile $pawnio -UseBasicParsing
            $hash = (Get-FileHash $pawnio -Algorithm SHA256).Hash
            if ($hash -ne $pawnioSha) { throw "PawnIO setup hash $hash is not the pinned $pawnioSha" }
            # Bounded: a driver install that waits on something nobody can
            # answer must fail this step, not hang the job.
            $outFile = Join-Path $tempDir 'sensors-install.out'
            $errFile = Join-Path $tempDir 'sensors-install.err'
            $p = Start-Process -FilePath $helper -ArgumentList 'install', '--pawnio-setup', "`"$pawnio`"" -PassThru -WindowStyle Hidden -RedirectStandardOutput $outFile -RedirectStandardError $errFile
            if (-not $p.WaitForExit(240000)) { $p.Kill($true); throw 'vitals-sensors install did not finish in 240 s' }
            $code = $p.ExitCode
            Get-Content $outFile, $errFile -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "      $_" }
            # The helper exits non-zero after creating the service when the
            # driver loads but the CPU is unsupported or the first read
            # fails; the service is what the uninstaller must remove.
            $real = Test-Service
            if (-not $real) { throw "vitals-sensors install exited $code and created no service" }
            Write-Host "      vitals-sensors install exited $code"
        } catch {
            if ($SensorsMode -eq 'real') { Assert-True $false "the sensors helper installs the service: $_" ; return }
            Note "sensors: the real install did not complete on this machine ($_); registering the service by hand instead"
        }
    }
    if (-not $real) {
        # The same registration create_and_start makes (install.rs), without
        # the driver: LocalSystem, AutoStart, a copy in Program Files\Vitals
        # Sensors. Not started — it would exit without PawnIO.
        New-Item -ItemType Directory -Force -Path $sensorsDir | Out-Null
        Copy-Item $helper -Destination (Join-Path $sensorsDir 'vitals-sensors.exe') -Force
        $bin = '"' + (Join-Path $sensorsDir 'vitals-sensors.exe') + '" service'
        & $sc create vitals-sensors binPath= $bin start= auto DisplayName= 'Vitals sensors' | Out-Null
        $global:LASTEXITCODE = 0
    }
    Assert-True (Test-Service) 'before uninstall: the vitals-sensors service exists'
    Assert-True (Test-Path $sensorsDir) "before uninstall: $sensorsDir exists"
}

function Enable-TaskManager([string]$Exe) {
    $p = Start-Process -FilePath $Exe -ArgumentList '--set-taskmgr-replacement', 'on' -Wait -PassThru -WindowStyle Hidden
    Assert-True ($p.ExitCode -eq 0) "the elevated Task Manager switch exits 0 (got $($p.ExitCode))"
    $d = Get-Debugger
    Assert-True (Test-Names $d (Split-Path -Parent $Exe)) "before uninstall: the taskmgr IFEO Debugger names the install (got '$d')"
}

function Enable-Autostart([string]$Exe) {
    # auto-launch 0.5 (tauri-plugin-autostart) writes `<path> <args>`
    # unquoted, and enables it in StartupApproved with 02 00 00 00 ...
    New-ItemProperty -Path $runKey -Name 'Vitals' -Value "$Exe --minimized" -PropertyType String -Force | Out-Null
    if (-not (Test-Path $approvedKey)) { New-Item -Path $approvedKey -Force | Out-Null }
    New-ItemProperty -Path $approvedKey -Name 'Vitals' -Value ([byte[]](2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) -PropertyType Binary -Force | Out-Null
    Assert-True (Test-Names (Get-RunValue 'Vitals') (Split-Path -Parent $Exe)) "before uninstall: HKCU Run 'Vitals' names the install"
}

function Enable-Watchdog([string]$Dir) {
    $wd = Join-Path $Dir 'watchdog\vitals-watchdog.exe'
    Assert-True (Test-Path $wd) "the watchdog is bundled at $wd"
    if (-not (Test-Path $wd)) { return }
    New-ItemProperty -Path $runKey -Name 'VitalsWatchdog' -Value ('"' + $wd + '"') -PropertyType String -Force | Out-Null
    # Never -Wait: the watchdog runs until killed.
    Start-Process -FilePath $wd -WindowStyle Hidden | Out-Null
    Assert-True (Wait-Until { @(Get-Process vitals-watchdog -ErrorAction SilentlyContinue).Count -gt 0 } 20) 'before uninstall: the watchdog is running'
    Assert-True (Test-Names (Get-RunValue 'VitalsWatchdog') $Dir) "before uninstall: HKCU Run 'VitalsWatchdog' names the install"
}

function Assert-Gone([string]$Dir) {
    $left = @(Get-Process -Name $processNames -ErrorAction SilentlyContinue)
    Assert-True ($left.Count -eq 0) "no Vitals process is running (found: $(($left | ForEach-Object Name) -join ', '))"
    foreach ($name in 'Vitals', 'VitalsWatchdog') {
        $value = Get-RunValue $name
        Assert-True (-not (Test-Names $value $Dir)) "HKCU Run '$name' does not name the removed install (got '$value')"
    }
    $approved = (Get-ItemProperty -Path $approvedKey -Name 'Vitals' -ErrorAction SilentlyContinue).Vitals
    Assert-True ($null -eq $approved) "StartupApproved\Run 'Vitals' is gone (no dead Task Manager startup entry)"
    $debugger = Get-Debugger
    Assert-True (-not (Test-Names $debugger $Dir)) "the taskmgr IFEO Debugger does not name the removed install (got '$debugger')"
    # 1060 = ERROR_SERVICE_DOES_NOT_EXIST. Any other code means a LocalSystem
    # service outlived the app.
    Assert-True (-not (Test-Service)) 'sc query vitals-sensors reports no such service'
    Assert-True (-not (Test-Path $sensorsDir)) "$sensorsDir is absent"
}

$tempDir = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
# A profile that never had a startup entry (the devcloud agent's account)
# has no Run key at all; the app creates it on demand, so does this script.
foreach ($k in $runKey, $approvedKey) { if (-not (Test-Path $k)) { New-Item -Path $k -Force | Out-Null } }

# ── 1. Machine: install, everything on, uninstall ──────────────────────────

if ('machine' -in $Pass) {
    Write-Host "`n== machine pass"
    Reset-Machine
    $exe = Join-Path $machineDir 'vitals-desktop.exe'
    $code = Install-Setup $setupPath 'machine'
    Assert-True ($code -eq 0) "silent per-machine install exits 0 (got $code)"
    Assert-True (Wait-Until { Test-Path $exe } 60) "the executable exists at $exe"
    Assert-True (Test-Path (Join-Path $machineDir 'uninstall.exe')) 'the uninstaller exists in the install directory'
    Assert-True ($null -ne (Get-ItemProperty "HKLM:\$uninstallSub" -ErrorAction SilentlyContinue)) 'an HKLM uninstall entry exists'
    $lnk = Find-Shortcut $exe
    Assert-True ($null -ne $lnk) 'a Start Menu shortcut named Vitals targets the installed executable'

    if (Test-Path $exe) {
        Test-Launch $exe
        if ($WhileInstalled) {
            & pwsh -NoProfile -File $WhileInstalled -Exe $exe
            Assert-True ($LASTEXITCODE -eq 0) "$(Split-Path -Leaf $WhileInstalled) passes against the installed app (exit $LASTEXITCODE)"
            $global:LASTEXITCODE = 0
        }
        Enable-Sensors $machineDir
        Enable-TaskManager $exe
        Enable-Autostart $exe
        Enable-Watchdog $machineDir
    }
    Invoke-Uninstall $machineDir
    Assert-Gone $machineDir
    Assert-True ($null -eq (Get-ItemProperty "HKLM:\$uninstallSub" -ErrorAction SilentlyContinue)) 'the HKLM uninstall entry is gone'
}

# ── 2. Per user: /CurrentUser lands in LOCALAPPDATA ────────────────────────

if ('user' -in $Pass) {
    Write-Host "`n== per-user pass"
    Reset-Machine
    $exe = Join-Path $userDir 'vitals-desktop.exe'
    $code = Install-Setup $setupPath 'user'
    Assert-True ($code -eq 0) "silent per-user install exits 0 (got $code)"
    Assert-True (Wait-Until { Test-Path $exe } 60) "the executable exists at $exe"
    Assert-True (-not (Test-Path (Join-Path $machineDir 'vitals-desktop.exe'))) 'nothing was installed under Program Files'
    Assert-True ($null -ne (Get-ItemProperty "HKCU:\$uninstallSub" -ErrorAction SilentlyContinue)) 'an HKCU uninstall entry exists'
    Assert-True ($null -ne (Find-Shortcut $exe)) 'a Start Menu shortcut targets the per-user executable'
    if (Test-Path $exe) {
        Test-Launch $exe
        # S21-06: a per-user copy whose owner turned sensors on. Before this
        # pass covered it, its uninstaller could not remove the service.
        Enable-Sensors $userDir
        Enable-Autostart $exe
        Enable-Watchdog $userDir
    }
    Invoke-Uninstall $userDir
    Assert-Gone $userDir
    Assert-True ($null -eq (Get-ItemProperty "HKCU:\$uninstallSub" -ErrorAction SilentlyContinue)) 'the HKCU uninstall entry is gone'
}

# ── 3. Upgrade: the previous release, then this one over it ────────────────

if ('upgrade' -in $Pass) {
    Write-Host "`n== upgrade pass"
    if (-not $Previous -or -not (Test-Path $Previous)) {
        Note 'upgrade: no -Previous installer given; pass skipped'
    } else {
        Reset-Machine
        $exe = Join-Path $machineDir 'vitals-desktop.exe'
        $newVersion = ConvertTo-Version (Get-Item $setupPath).VersionInfo.ProductVersion
        $code = Install-Setup (Resolve-Path $Previous).Path 'machine'
        Assert-True ($code -eq 0) "the previous release installs (got $code)"
        [void](Wait-Until { Test-Path $exe } 60)
        $before = ConvertTo-Version (Get-Item $exe -ErrorAction SilentlyContinue).VersionInfo.ProductVersion

        # User state the update must keep: a settings file in Roaming (where
        # the store and watchdog.json live) and an enabled watchdog. The
        # installer ends the watchdog and its uninstall step drops the Run
        # value; the app restores both at its next launch from watchdog.json
        # (watchdog.rs ensure_default), which is what is asserted.
        $settingsDir = Join-Path $env:APPDATA 'Vitals'
        New-Item -ItemType Directory -Force -Path $settingsDir | Out-Null
        $marker = Join-Path $settingsDir 'roundtrip-marker.json'
        $token = [guid]::NewGuid().ToString()
        Set-Content -LiteralPath $marker -Value "{`"token`":`"$token`"}" -Encoding utf8NoBOM
        $wdConfig = Join-Path $settingsDir 'watchdog.json'
        $wdBackup = if (Test-Path $wdConfig) { Get-Content -LiteralPath $wdConfig -Raw } else { $null }
        Set-Content -LiteralPath $wdConfig -Value '{"enabled":true,"sensitivity":"normal","sound":{"kind":"default"},"volume":80}' -Encoding utf8NoBOM
        Enable-Watchdog $machineDir
        Stop-Vitals

        $code = Install-Setup $setupPath 'machine'
        Assert-True ($code -eq 0) "the new installer over $before exits 0 (got $code)"
        [void](Wait-Until { (ConvertTo-Version (Get-Item $exe -ErrorAction SilentlyContinue).VersionInfo.ProductVersion) -ne $before } 60)
        $after = ConvertTo-Version (Get-Item $exe -ErrorAction SilentlyContinue).VersionInfo.ProductVersion
        Assert-True ($after -eq $newVersion -and $after -ne $before) "the installed executable moved $before -> $after (setup is $newVersion)"
        Assert-True (-not (Test-Path (Join-Path $userDir 'vitals-desktop.exe'))) 'the upgrade stayed per machine (no second per-user copy)'
        Assert-True ((Test-Path $marker) -and (Get-Content $marker -Raw).Contains($token)) 'the settings file survived the upgrade'
        $app = Start-Process -FilePath $exe -ArgumentList '--minimized' -PassThru
        $restored = Wait-Until { (Test-Names (Get-RunValue 'VitalsWatchdog') $machineDir) -and @(Get-Process vitals-watchdog -ErrorAction SilentlyContinue).Count -gt 0 } 30
        Assert-True $restored 'after the upgrade the first launch restores the enabled watchdog (Run value + process)'
        $app.Refresh()
        Assert-True (-not $app.HasExited) 'the upgraded app is running'
        Stop-Vitals
        Remove-Item -LiteralPath $marker -Force -ErrorAction SilentlyContinue
        if ($null -ne $wdBackup) { Set-Content -LiteralPath $wdConfig -Value $wdBackup -NoNewline -Encoding utf8NoBOM } else { Remove-Item -LiteralPath $wdConfig -Force -ErrorAction SilentlyContinue }
        Invoke-Uninstall $machineDir
        Assert-Gone $machineDir
    }
}

Write-Host ''
foreach ($n in $notes) { Write-Host "note: $n" }
if ($env:GITHUB_STEP_SUMMARY) {
    $summary = @("### Installer round trip", '', "- passes: $($Pass -join ', ')", "- failures: $($failures.Count)")
    $summary += $failures | ForEach-Object { "  - FAIL $_" }
    $summary += $notes | ForEach-Object { "- note: $_" }
    $summary | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Encoding utf8
}
Write-Host ("installer round trip: {0} failure(s)" -f $failures.Count)
exit $failures.Count
