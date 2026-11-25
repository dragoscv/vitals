<#
.SYNOPSIS
    Installs the toolchains a Windows build workspace needs and a bare image lacks.

.DESCRIPTION
    The workstation is shared with other agents, so the Vitals gates run on the
    devcloud Windows workspace instead (docs/devcloud.md). That workspace has
    Rust and Node, but two gates insist on more:

      * `deps: licences + sources` needs cargo-deny.
      * the `android` artefact lane needs a JDK and the Android SDK — its
        -Build path answers "nothing to check" without them, which is why
        verify.ps1 passes -RequireAndroid and this script is what makes that
        pass honestly.

    Everything lands under -Root, outside the repository, because the cloud job
    cleans the working tree (`git clean -ffdx`) before every run: a toolchain
    inside the repo would be deleted by the next job. It also writes
    `$Root\env.ps1`, which verify.ps1 sources when the variables are missing —
    a process that already exists does not see a new environment variable, and
    the job's shell is one of those.

    Idempotent: each step is skipped when its result is already there, so a
    second run costs seconds. CI and a human run the same command.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/provision-cloud-workspace.ps1
    pwsh -NoProfile -File scripts/ci/provision-cloud-workspace.ps1 -Check
#>
[CmdletBinding()]
param(
    # Where the JDK and the Android SDK go. Outside the repo on purpose.
    [string]$Root = 'C:\devcloud-tools',

    # Report what is missing, install nothing, and exit 1 if anything is.
    [switch]$Check
)

$ErrorActionPreference = 'Stop'

$jdk = Join-Path $Root 'jdk-21'
$sdk = Join-Path $Root 'android-sdk'
$sdkManager = Join-Path $sdk 'cmdline-tools\latest\bin\sdkmanager.bat'
$envFile = Join-Path $Root 'env.ps1'
$missing = [System.Collections.Generic.List[string]]::new()

# The platform the modules compile against, and the build tools whose aapt the
# artefact lane reads the built APK with. Both are named in
# apps/android/*/build.gradle.kts (compileSdk = 37).
#
# `/`, not `;`: the command-line tools are now a shim over the new `android`
# CLI, whose help documents "build-tools/23.0.0" and "platforms/android-23".
# The older `platforms;android-37` still arrives — as two names, so the tool
# answers "Package platforms not found / Package android-37 not found", which
# reads like a bad index rather than a separator that changed.
#
# `37.0`, not `37`: API 37 is published as minor platform releases (the index
# lists platforms/android-37.0, -37.1, -37.2) and there is no bare
# platforms/android-37 to ask for.
$platform = 'platforms/android-37.0'
$buildTools = 'build-tools/37.0.0'

# The install directory the SDK names for $platform is the package's last
# segment, and Gradle is the judge of whether it is the one it wanted — which
# is why the lane is proved with a real assemble rather than by this script.
function Test-Platform37 {
    $dir = Join-Path $sdk 'platforms'
    (Test-Path $dir) -and (@(Get-ChildItem $dir -Directory -Filter 'android-37*').Count -gt 0)
}

function Install-Zip([string]$Url, [string]$Destination, [string]$Inner) {
    $zip = Join-Path $env:TEMP ([IO.Path]::GetFileName($Url))
    $staging = "$Destination.staging"
    Write-Host "  downloading $Url" -ForegroundColor DarkGray
    Invoke-WebRequest -Uri $Url -OutFile $zip -MaximumRedirection 10
    if (Test-Path $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
    Expand-Archive -LiteralPath $zip -DestinationPath $staging
    # -Inner names the directory to keep; without it the archive's own contents
    # are the answer and the move below takes the staging directory itself,
    # which is why the cleanup has to re-check rather than assume it is there.
    $source = if ($Inner) { Join-Path $staging $Inner } else { $staging }
    if (-not (Test-Path $source)) { throw "$Url did not contain $Inner" }
    # Temurin wraps its payload in jdk-21.0.x+y/; one directory with no bin/ of
    # its own is a layout rather than content, so descend instead of making the
    # version number something a caller has to know. (The check below would
    # otherwise never find java.exe and would re-download on every run.)
    $entries = @(Get-ChildItem -LiteralPath $source)
    if ((-not (Test-Path (Join-Path $source 'bin'))) -and $entries.Count -eq 1 -and $entries[0].PSIsContainer) {
        $source = $entries[0].FullName
    }
    if (Test-Path $Destination) { Remove-Item -LiteralPath $Destination -Recurse -Force }
    Move-Item -LiteralPath $source -Destination $Destination
    if (Test-Path $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
    Remove-Item -LiteralPath $zip -Force
}

function Invoke-SdkManager([string[]]$Packages, [switch]$Licenses) {
    if ($Licenses) {
        # The current tools answer "the --licenses option is no longer needed",
        # but they still write $sdk\licenses, which is what Gradle checks for.
        1..40 | ForEach-Object { 'y' } | & $sdkManager --sdk_root="$sdk" --licenses | Out-Null
    } else {
        # Out-Host, not the pipeline: the tool prints failures on stdout, and a
        # function that leaks them returns "Package x not found 1" as its exit
        # code, which then reads as a tool that failed for no stated reason.
        & $sdkManager --sdk_root="$sdk" @Packages | Out-Host
    }
    return $LASTEXITCODE
}

# ── cargo-deny ────────────────────────────────────────────────────────

$deny = Get-Command cargo-deny -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $deny) {
    $missing.Add('cargo-deny')
    if (-not $Check) {
        Write-Host 'installing cargo-deny (compiles, about two minutes)' -ForegroundColor Cyan
        cargo install cargo-deny --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo install cargo-deny exited $LASTEXITCODE" }
    }
}

# ── JDK 21 ────────────────────────────────────────────────────────────

if (-not (Test-Path (Join-Path $jdk 'bin/java.exe'))) {
    $missing.Add("JDK 21 ($jdk)")
    if (-not $Check) {
        Write-Host 'installing Temurin JDK 21' -ForegroundColor Cyan
        New-Item -ItemType Directory -Force -Path $Root | Out-Null
        # The archive holds one jdk-21.0.x+y directory; -Inner strips it.
        Install-Zip -Url 'https://api.adoptium.net/v3/binary/latest/21/ga/windows/x64/jdk/hotspot/normal/eclipse' -Destination $jdk -Inner ''
    }
}

# ── Android command-line tools ────────────────────────────────────────

if (-not (Test-Path $sdkManager)) {
    $missing.Add('Android command-line tools')
    if (-not $Check) {
        Write-Host 'installing the Android command-line tools' -ForegroundColor Cyan
        # The build number is read from Google's own index rather than written
        # here: it changes, and a hardcoded URL rots silently into a 404.
        $index = (Invoke-WebRequest -Uri 'https://dl.google.com/android/repository/repository2-3.xml' -UseBasicParsing).Content
        $builds = [regex]::Matches([string]$index, 'commandlinetools-win-(\d+)_latest\.zip') |
            ForEach-Object { [int]$_.Groups[1].Value } | Sort-Object -Unique
        if (-not $builds) { throw 'the Android repository index listed no command-line tools' }
        $url = "https://dl.google.com/android/repository/commandlinetools-win-$($builds[-1])_latest.zip"
        New-Item -ItemType Directory -Force -Path (Join-Path $sdk 'cmdline-tools') | Out-Null
        # sdkmanager only accepts the archive's own cmdline-tools/ directory
        # under a name of its choosing when it is not the SDK root, so it moves
        # into cmdline-tools/latest.
        Install-Zip -Url $url -Destination (Join-Path $sdk 'cmdline-tools/latest') -Inner 'cmdline-tools'
    }
}

# ── The platform and the build tools ──────────────────────────────────

$installed = Test-Platform37
if (-not $installed) {
    $missing.Add("$platform + $buildTools")
    if (-not $Check) {
        if (-not (Test-Path $sdkManager)) { throw 'the command-line tools are missing, so nothing can be installed' }
        $env:JAVA_HOME = $jdk
        $env:ANDROID_HOME = $sdk
        Write-Host 'accepting the SDK licences, then installing the platform (a few minutes)' -ForegroundColor Cyan
        # The current command-line tools say --licenses is no longer needed, but
        # Gradle looks for the accepted-licence files it writes, so it is still
        # worth asking; a failure there is not fatal, the install below is.
        [void](Invoke-SdkManager -Licenses)
        $code = Invoke-SdkManager -Packages @('platform-tools', $platform, $buildTools)
        if ($code -ne 0 -or -not (Test-Platform37)) {
            Write-Host "sdkmanager exited $code; what the index offers:" -ForegroundColor Red
            & $sdkManager --sdk_root="$sdk" --list | Select-String -Pattern 'build-tools.37|platforms.android-37' |
                ForEach-Object { Write-Host "    $($_.Line.Trim())" -ForegroundColor Red }
            throw "sdkmanager could not install $platform / $buildTools"
        }
    }
}

if ($Check) {
    if ($missing.Count) {
        Write-Host "missing: $($missing -join '; ')" -ForegroundColor Red
        exit 1
    }
    Write-Host 'everything the gates need is present' -ForegroundColor Green
    exit 0
}

# ── Make it findable ──────────────────────────────────────────────────
#
# A new process inherits its parent's environment block, not the registry, so a
# shell started before this script ran sees nothing. The file is the answer for
# those; the registry variables are the answer for everything started later.

$lines = @(
    '# Written by scripts/ci/provision-cloud-workspace.ps1.',
    '# The toolchains this workspace keeps outside the repository.',
    "`$env:JAVA_HOME = '$jdk'",
    "`$env:ANDROID_HOME = '$sdk'",
    "`$env:ANDROID_SDK_ROOT = '$sdk'",
    "`$env:PATH = '$jdk\bin;$sdk\platform-tools;$sdk\cmdline-tools\latest\bin;' + `$env:PATH"
)
Set-Content -LiteralPath $envFile -Value $lines -Encoding ascii

$placed = @()
foreach ($pair in @(@('JAVA_HOME', $jdk), @('ANDROID_HOME', $sdk), @('ANDROID_SDK_ROOT', $sdk))) {
    foreach ($scope in 'Machine', 'User') {
        try {
            [Environment]::SetEnvironmentVariable($pair[0], $pair[1], $scope)
            $placed += "$($pair[0]) ($scope)"
            break
        } catch {
            # Machine needs elevation; the user scope does not. Either is
            # enough for a later process, and neither is enough for this one.
        }
    }
}

# ── Report, with the versions, so a green run says what it proved ──────

$env:JAVA_HOME = $jdk
$env:ANDROID_HOME = $sdk

# The report is the only place a green run says what it proved, so it must not
# be able to fail the run: a -version call on a missing binary used to abort the
# script and hide why anything else was wrong.
function Get-Line([scriptblock]$Probe) {
    try { @(& $Probe 2>&1 | Select-Object -First 1) -join '' } catch { "unavailable: $($_.Exception.Message)" }
}
$buildToolsDir = Get-ChildItem (Join-Path $sdk 'build-tools') -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
Write-Host ''
Write-Host "root          $Root"
Write-Host "cargo-deny    $(Get-Line { & cargo-deny --version })"
Write-Host "java          $(Get-Line { & (Join-Path $jdk 'bin/java.exe') -version })"
Write-Host "sdk platform  $((Get-ChildItem (Join-Path $sdk 'platforms') -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name) -join ', ')"
Write-Host "build tools   $($buildToolsDir.Name)"
Write-Host "env file      $envFile"
Write-Host "registry      $($placed -join ', ')"
