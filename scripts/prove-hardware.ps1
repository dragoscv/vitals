<#
.SYNOPSIS
    Proves on a real second machine that the shipped binaries carry their icon and version block.

.DESCRIPTION
    A gate that reads the identity off the artefacts (scripts/check-binary-identity.ps1)
    is still blind to the one thing a user sees first: the icon Windows actually
    draws, pulled out of the file by a machine that has the app installed. That is
    where 1.5.0's defect showed up -- a generic Windows shield in a Defender Firewall
    prompt, on a second machine, found by hand and not by any build.

    So this runs that hand proof as one command. It copies the installer to a
    second Windows machine over ssh, installs it silently for the current user
    (/S /CurrentUser, which is the switch that keeps an installMode:"both"
    installer per-user and therefore unelevated), then reads back, for every
    shipped binary, the icon resource hash, ProductName and FileVersion, and
    relays the pass/fail table.

    It reuses scripts/check-binary-identity.ps1 rather than re-implementing the PE
    resource read: that parsing is subtle, the gate already owns it, and one copy
    is enough. No change to that script was needed -- it is invoked as a child
    process, so its own exit code is the verdict.

    Exit codes:
      0  every shipped binary carries the mark (or, with -SkipIcon at the gate, a
         readable version block naming the product) and this version.
      1  at least one binary failed: no icon resource, no version block, or the
         wrong version.
        2  NOT VERIFIED -- the host could not be reached over ssh, the remote
            command never reported a result, or the host has no PowerShell 7 to run
            the identity gate with.
        3  NOT VERIFIED -- the installer ran but no app binary appeared, so there
            was nothing to read back.

     2 and 3 both mean the proof did not run to a verdict. Neither is a pass, and
     neither is 0: only 0 says the binaries were read and passed.

     The gate is run with pwsh. It is UTF-8 without a BOM and carries em dashes, so
     Windows PowerShell 5.1 decodes one of those dashes into a curly quote that ends
     the string it sits in and the file does not parse there at all.

    The remote's ssh shell is PowerShell (5.1), so the payload travels as an
    encoded command: quoting through whichever shell ssh starts is where scripts
    like this break, and base64 removes the question.

    This is a manual, second-machine proof, not a CI gate: it installs software on
    someone's machine. Run it when a release is cut, once per installer scope
    (per-user with the default, per-machine with an elevated copy), and when the
    icon, the version sources or the resource build scripts change.

.EXAMPLE
    pwsh -NoProfile -File scripts/prove-hardware.ps1 -Installer .copilot-tmp/release/Vitals_1.5.1_x64-setup.exe

.EXAMPLE
    # readback only: the app is already installed on the host (or on this machine
    # with -Local), so nothing is copied or run that changes state.
    pwsh -NoProfile -File scripts/prove-hardware.ps1 -Installer target/release/bundle/nsis/Vitals_1.5.1_x64-setup.exe -SkipInstall

.EXAMPLE
    # exercise the readback half on the build machine, against a local install.
    pwsh -NoProfile -File scripts/prove-hardware.ps1 -Installer <setup> -Local -SkipInstall
#>
[CmdletBinding()]
param(
    # The installer to prove: its version block supplies the version every binary
    # is compared against, and (unless -SkipInstall) it is the file installed on
    # the target.
    [Parameter(Mandatory = $true)]
    [string]$Installer,

    # The ssh alias of the machine to prove on. Ignored when -Local is set.
    #
    # The variable is not called Host: $Host is a read-only automatic variable
    # (the host program), and PowerShell refuses to bind a parameter to it --
    # "Cannot overwrite variable Host because it is read-only or constant",
    # on both 5.1 and 7. The alias keeps the promised -Host spelling working.
    [Alias('Host')]
    [string]$SshHost = 'dragos-vivobook',

    # Read the identity of an existing install instead of installing: nothing is
    # copied and no state changes. Use it to re-read a machine, or -- with -Local --
    # to run the readback half here.
    [switch]$SkipInstall,

    # The install directory on the target. Empty means the per-user install
    # directory of the app, resolved on the target as
    # "$env:LOCALAPPDATA\Programs\Vitals".
    [string]$InstallDir = '',

    # Run the payload on this machine instead of over ssh. This is how the
    # readback half is exercised without a second machine; an install here would
    # change this machine's own copy, so pair it with -SkipInstall.
    [switch]$Local
)

$ErrorActionPreference = 'Stop'

function Get-ResultCode {
    # The remote exit code is not trustworthy: Windows OpenSSH collapses every
    # non-zero exit to 1 (verified: exit 2 and exit 5 both arrive as 1), so the
    # payload prints its real code and this reads that line instead.
    param([string]$Text)
    $m = [regex]::Match($Text, 'PROVE_HARDWARE_EXIT=(\d+)')
    if ($m.Success) { return [int]$m.Groups[1].Value }
    return $null
}

function Invoke-Payload {
    # Runs `powershell -EncodedCommand <b64>` either on the ssh host or here, and
    # keeps stdout and stderr apart so an ssh failure can be shown verbatim.
    param([string]$Encoded, [string]$SshHost)

    $so = [IO.Path]::GetTempFileName()
    $se = [IO.Path]::GetTempFileName()
    try {
        if ($SshHost) {
            $sshArgs = @('-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15', $SshHost, "powershell -NoProfile -EncodedCommand $Encoded")
            $p = Start-Process -FilePath 'ssh.exe' -ArgumentList $sshArgs -NoNewWindow -PassThru -RedirectStandardOutput $so -RedirectStandardError $se -Wait
        } else {
            $p = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-EncodedCommand', $Encoded) -NoNewWindow -PassThru -RedirectStandardOutput $so -RedirectStandardError $se -Wait
        }
        return [pscustomobject]@{
            Code = $p.ExitCode
            Out  = (Get-Content -Raw -LiteralPath $so -ErrorAction SilentlyContinue)
            Err  = (Get-Content -Raw -LiteralPath $se -ErrorAction SilentlyContinue)
        }
    } finally {
        Remove-Item -LiteralPath $so, $se -Force -ErrorAction SilentlyContinue
    }
}

function ConvertTo-Encoded {
    param([string]$Text)
    return [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Text))
}

function Copy-ToHost {
    # scp, with the exit code read through a wrapper: a native call whose output
    # is captured would otherwise leave $LASTEXITCODE readable only by luck.
    param([string]$LocalFile, [string]$RemoteSpec)
    $so = [IO.Path]::GetTempFileName()
    $se = [IO.Path]::GetTempFileName()
    try {
        $p = Start-Process -FilePath 'scp.exe' -ArgumentList @('-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15', $LocalFile, $RemoteSpec) -NoNewWindow -PassThru -RedirectStandardOutput $so -RedirectStandardError $se -Wait
        return [pscustomobject]@{ Code = $p.ExitCode; Err = (Get-Content -Raw -LiteralPath $se -ErrorAction SilentlyContinue) }
    } finally {
        Remove-Item -LiteralPath $so, $se -Force -ErrorAction SilentlyContinue
    }
}

# --- the installer, and the version every binary has to declare -------------

if (-not (Test-Path -LiteralPath $Installer -PathType Leaf)) {
    Write-Output "prove-hardware: -Installer $Installer does not exist"
    exit 1
}
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$installerName = Split-Path -Leaf $installerPath

$version = (Get-Item -LiteralPath $installerPath).VersionInfo.FileVersion
if ([string]::IsNullOrWhiteSpace($version)) {
    # A setup built without a version block would itself be the 1.5.0 defect, so
    # fall back to the name and say where the number came from rather than guess
    # silently.
    $m = [regex]::Match($installerName, '_(\d+\.\d+\.\d+)_')
    if ($m.Success) {
        $version = $m.Groups[1].Value
        Write-Output "prove-hardware: the installer has no version block; taking $version from its file name"
    }
}
if ([string]::IsNullOrWhiteSpace($version)) {
    Write-Output "prove-hardware: cannot tell which version $installerName declares -- pass an installer with a version block"
    exit 1
}

# --- the payload that runs on the target ------------------------------------

$installerForPayload = $installerPath
$gateForPayload = Join-Path $PSScriptRoot 'check-binary-identity.ps1'
$hostLabel = if ($Local) { $env:COMPUTERNAME } else { $SshHost }

if (-not $Local) {
    # A fresh staging directory on the target, named so the next person can tell
    # what it is and why it is there.
    $mk = @'
$d = Join-Path $env:TEMP 'vitals-prove-hardware'
New-Item -ItemType Directory -Force -Path $d | Out-Null
Write-Output $d
'@
    $mkResult = Invoke-Payload -Encoded (ConvertTo-Encoded $mk) -SshHost $SshHost
    if ($mkResult.Code -ne 0 -or [string]::IsNullOrWhiteSpace($mkResult.Out)) {
        Write-Output "NOT VERIFIED: could not reach $SshHost over ssh to stage the proof."
        Write-Output $mkResult.Err
        exit 2
    }
    $remoteDir = ($mkResult.Out.Trim() -split "`r?`n" | Select-Object -Last 1).Trim()
    # scp's host:path splitter stops at the first colon and every Windows tool
    # there accepts forward slashes, so the path travels in that form.
    $remoteDirFwd = $remoteDir -replace '\\', '/'

    $dstInstaller = "$remoteDirFwd/$installerName"
    $cpInstaller = Copy-ToHost -LocalFile $installerPath -RemoteSpec "${SshHost}:$dstInstaller"
    if ($cpInstaller.Code -ne 0) {
        Write-Output "NOT VERIFIED: could not copy the installer to ${SshHost}:$dstInstaller"
        Write-Output $cpInstaller.Err
        exit 2
    }

    # The gate is copied too, so the target runs the same bytes this repo holds
    # rather than a re-implementation that could drift from it.
    $dstGate = "$remoteDirFwd/check-binary-identity.ps1"
    $cpGate = Copy-ToHost -LocalFile $gateForPayload -RemoteSpec "${SshHost}:$dstGate"
    if ($cpGate.Code -ne 0) {
        Write-Output "NOT VERIFIED: could not copy the identity gate to ${SshHost}:$dstGate"
        Write-Output $cpGate.Err
        exit 2
    }

    $installerForPayload = $dstInstaller
    $gateForPayload = $dstGate
}

$skipLiteral = if ($SkipInstall) { '$true' } else { '$false' }
$template = @'
# Continue, not Stop: the gate is a child process whose non-zero exit is the
# verdict, and a missing result must still reach the caller as PROVE_HARDWARE_EXIT
# rather than as an abort. SilentlyContinue kills the first-use progress records,
# which PowerShell serialises into the redirected stderr as a CLIXML blob.
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$installer = '@@INSTALLER@@'
$installDir = '@@INSTALLDIR@@'
$gatePath = '@@GATEPATH@@'
$version = '@@VERSION@@'
$skipInstall = @@SKIPINSTALL@@

# check-binary-identity.ps1 is UTF-8 without a BOM and carries em dashes. Windows
# PowerShell 5.1 reads those bytes as CP1252, where the dash's 0x94 byte becomes a
# curly quote that terminates the string, and the gate does not parse at all
# (measured: "Missing ')' in method call" at its line 132). pwsh reads the file as
# UTF-8. So resolve the interpreter instead of assuming whichever one ssh started
# -- and say so when there is none, rather than report the parse failure as a row
# of FAILED binaries.
$gateExe = Get-Command pwsh -ErrorAction SilentlyContinue
if (-not $gateExe) {
    Write-Output 'NOT-VERIFIED: the identity gate needs PowerShell 7 on this host; it is UTF-8 without a BOM and Windows PowerShell 5.1 cannot parse it'
    Write-Output 'PROVE_HARDWARE_EXIT=2'
    exit 2
}

if ([string]::IsNullOrWhiteSpace($installDir)) {
    $installDir = Join-Path $env:LOCALAPPDATA 'Programs\Vitals'
}

Write-Output ("host={0} user={1}" -f $env:COMPUTERNAME, $env:USERNAME)

if (-not $skipInstall) {
    if (-not (Test-Path -LiteralPath $installer)) {
        Write-Output "NOT-VERIFIED: the installer is not present at $installer"
        Write-Output 'PROVE_HARDWARE_EXIT=3'
        exit 3
    }
    Write-Output ("installing {0} silently, per user (/S /CurrentUser, no elevation)" -f $installer)
    $proc = Start-Process -FilePath $installer -ArgumentList @('/S', '/CurrentUser') -Wait -PassThru
    Write-Output ("installer exit {0}" -f $proc.ExitCode)
    # The installer returns before NSIS has finished writing every file; wait for
    # the first binary a user would run rather than sleeping a guessed interval.
    $desktop = Join-Path $installDir 'vitals-desktop.exe'
    $deadline = (Get-Date).AddSeconds(90)
    while ((Get-Date) -lt $deadline -and -not (Test-Path -LiteralPath $desktop)) { Start-Sleep -Seconds 2 }
}

if (-not (Test-Path -LiteralPath $installDir)) {
    Write-Output "NOT-VERIFIED: nothing is installed under $installDir, so there is no binary to read"
    Write-Output 'PROVE_HARDWARE_EXIT=3'
    exit 3
}

Write-Output ("reading the identity of every binary under {0} (expected version {1})" -f $installDir, $version)
$gateOut = & $gateExe.Source -NoProfile -ExecutionPolicy Bypass -File $gatePath -Path $installDir -Version $version 2>&1
$gateCode = $LASTEXITCODE
if ($null -eq $gateCode) { $gateCode = 1 }
$gateOut | ForEach-Object { "$_" }
Write-Output ("PROVE_HARDWARE_EXIT={0}" -f $gateCode)
exit $gateCode
'@

$payload = $template.
    Replace('@@INSTALLER@@', $installerForPayload.Replace("'", "''")).
    Replace('@@INSTALLDIR@@', $InstallDir.Replace("'", "''")).
    Replace('@@GATEPATH@@', $gateForPayload.Replace("'", "''")).
    Replace('@@VERSION@@', $version.Replace("'", "''")).
    Replace('@@SKIPINSTALL@@', $skipLiteral)

Write-Output "prove-hardware: proving version $version on $hostLabel"
$sshHostArg = $null
if (-not $Local) { $sshHostArg = $SshHost }
$result = Invoke-Payload -Encoded (ConvertTo-Encoded $payload) -SshHost $sshHostArg

if ($result.Out) { Write-Output $result.Out.TrimEnd() }

$code = Get-ResultCode $result.Out
if ($null -eq $code) {
    # No result line means the payload never finished. On an ssh attempt that is
    # almost always transport; either way it is not a pass.
    Write-Output ''
    if (-not $Local -and $result.Code -eq 255) {
        Write-Output "NOT VERIFIED: could not reach $SshHost over ssh (exit 255)."
    } else {
        Write-Output "NOT VERIFIED: the command on $hostLabel did not report a result (exit $($result.Code))."
    }
    if ($result.Err) { Write-Output $result.Err.TrimEnd() }
    exit 2
}

Write-Output ''
switch ($code) {
    0 { Write-Output "hardware proof: every shipped binary under the install carries the brand icon and version $version -- VERIFIED on $hostLabel" }
    1 { Write-Output "hardware proof: FAILED -- see the rows above; at least one binary lacks the icon resource or a version block, or declares the wrong version" }
    default { Write-Output "hardware proof: NOT VERIFIED -- the proof did not run to a verdict on $hostLabel; see the reason above (exit $code)" }
}
exit $code
