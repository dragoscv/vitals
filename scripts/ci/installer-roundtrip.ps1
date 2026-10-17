<#
.SYNOPSIS
    Installs the NSIS setup silently, launches the app, uninstalls it, and
    proves the uninstall left nothing of ours behind (QA-1, QA-2).

.DESCRIPTION
    Every assertion here is a defect that a unit test cannot see because it
    lives in the installer script (apps/desktop/src-tauri/windows/hooks.nsh)
    or in what the app writes to the machine at run time:

      - the install lands in Program Files with the executable and a Start
        Menu shortcut;
      - the installed app starts and is still running ten seconds later;
      - after `uninstall.exe /S`: no Vitals process, no install directory,
        no HKCU Run 'Vitals' / 'VitalsWatchdog' value naming it, no Task
        Manager IFEO Debugger naming it, no `vitals-sensors` service
        (`sc query` exits 1060) and no `Program Files\Vitals Sensors`.

    The uninstall audit (DEV-1/QA-2) found exactly that LocalSystem service
    surviving an uninstall. This script is the proof that it no longer does.

    Must run elevated: the installer is perMachine. GitHub's Windows runners
    are. Exit code is the number of failed assertions (0 = clean round trip).

.PARAMETER Setup
    Path to the NSIS `*-setup.exe`.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/installer-roundtrip.ps1 -Setup target/release/bundle/nsis/Vitals_0.9.0_x64-setup.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Setup,
    # Seconds the launched app must survive. Long enough for the webview to
    # load and the sampler to start; a crash on start lands well inside it.
    [int]$AliveSeconds = 10
)

$ErrorActionPreference = 'Stop'

$failures = [System.Collections.Generic.List[string]]::new()
function Assert-True {
    param([bool]$Condition, [string]$Message)
    if ($Condition) {
        Write-Host "ok    $Message"
    } else {
        Write-Host "FAIL  $Message" -ForegroundColor Red
        $failures.Add($Message)
    }
}

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'installer-roundtrip must run elevated: the installer is perMachine' -ForegroundColor Red
    exit 2
}

$setupPath = (Resolve-Path $Setup).Path
$installDir = Join-Path $env:ProgramFiles 'Vitals'
$exe = Join-Path $installDir 'vitals-desktop.exe'
$sensorsDir = Join-Path $env:ProgramFiles 'Vitals Sensors'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$ifeoKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\taskmgr.exe'
$processNames = @('vitals-desktop', 'vitals-watchdog', 'vitals-sensors')

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

function Test-NamesInstallDir {
    param([object]$Value)
    if ($null -eq $Value) { return $false }
    return ([string]$Value).Trim('"').StartsWith($installDir, [StringComparison]::OrdinalIgnoreCase)
}

# ── Install ────────────────────────────────────────────────────────────

Write-Host "installing $setupPath"
$install = Start-Process -FilePath $setupPath -ArgumentList '/S' -Wait -PassThru
Assert-True ($install.ExitCode -eq 0) "silent install exits 0 (got $($install.ExitCode))"
Assert-True (Wait-Until { Test-Path $exe } 60) "the executable exists at $exe"
Assert-True (Test-Path (Join-Path $installDir 'uninstall.exe')) 'the uninstaller exists in the install directory'

# perMachine puts the shortcut under the all-users Start Menu; searched
# recursively so a future start-menu folder setting does not break this.
$startMenus = @(
    (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs'),
    (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs')
)
$shortcut = $startMenus | Where-Object { Test-Path $_ } |
    ForEach-Object { Get-ChildItem $_ -Recurse -Filter 'Vitals.lnk' -ErrorAction SilentlyContinue } |
    Select-Object -First 1
Assert-True ($null -ne $shortcut) 'a Start Menu shortcut named Vitals exists'
if ($shortcut) {
    $target = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut.FullName).TargetPath
    Assert-True ($target -ieq $exe) "the shortcut targets the installed executable (got '$target')"
}

# ── Launch ─────────────────────────────────────────────────────────────

if (Test-Path $exe) {
    $app = Start-Process -FilePath $exe -PassThru
    Start-Sleep -Seconds $AliveSeconds
    $app.Refresh()
    Assert-True (-not $app.HasExited) "the installed app is still running after $AliveSeconds s (exit code $(if ($app.HasExited) { $app.ExitCode } else { 'n/a' }))"
    Get-Process -Name $processNames -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

# ── Uninstall ──────────────────────────────────────────────────────────

$uninstaller = Join-Path $installDir 'uninstall.exe'
if (Test-Path $uninstaller) {
    Write-Host "uninstalling via $uninstaller"
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru
    Assert-True ($uninstall.ExitCode -eq 0) "silent uninstall exits 0 (got $($uninstall.ExitCode))"
    Assert-True (Wait-Until { -not (Test-Path $installDir) } 120) "the install directory $installDir is gone"
} else {
    $failures.Add('no uninstaller to run')
}

$left = @(Get-Process -Name $processNames -ErrorAction SilentlyContinue)
Assert-True ($left.Count -eq 0) "no Vitals process is running (found: $(($left | ForEach-Object Name) -join ', '))"

foreach ($name in 'Vitals', 'VitalsWatchdog') {
    $value = (Get-ItemProperty -Path $runKey -Name $name -ErrorAction SilentlyContinue).$name
    Assert-True (-not (Test-NamesInstallDir $value)) "HKCU Run '$name' does not name the removed install (got '$value')"
}

$debugger = (Get-ItemProperty -Path $ifeoKey -Name 'Debugger' -ErrorAction SilentlyContinue).Debugger
Assert-True (-not (Test-NamesInstallDir $debugger)) "the taskmgr IFEO Debugger does not name the removed install (got '$debugger')"

# 1060 = ERROR_SERVICE_DOES_NOT_EXIST. Any other code, 0 included, means a
# LocalSystem service outlived the app.
& "$env:SystemRoot\System32\sc.exe" query vitals-sensors | Out-Null
$scExit = $LASTEXITCODE
Assert-True ($scExit -eq 1060) "sc query vitals-sensors reports no such service (exit $scExit)"
Assert-True (-not (Test-Path $sensorsDir)) "$sensorsDir is absent"

Write-Host ''
Write-Host ("installer round trip: {0} failure(s)" -f $failures.Count)
exit $failures.Count
