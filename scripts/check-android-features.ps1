<#
.SYNOPSIS
    Fails when a built Android APK requires a hardware feature Vitals can live
    without.

.DESCRIPTION
    Android marks a hardware feature REQUIRED when a permission implies it
    (ACCESS_WIFI_STATE -> android.hardware.wifi, CAMERA ->
    android.hardware.camera) unless the manifest declares that feature with
    android:required="false". Play then hides the app from every device that
    lacks it, which is how Vitals quietly lost Ethernet-only phones, camera-less
    Chromebooks and Android XR (2026-11-18).

    Neither the source manifest nor the merged manifest can be checked for this,
    because the implied features are added at packaging: a merged manifest
    contains no android.hardware.camera at all. The only honest evidence is the
    built APK, so this reads `aapt dump badging` and asserts two things:

      1. Consistency with the source. Every feature the source declares
         android:required="false" must appear as `uses-feature-not-required`.
      2. No unapproved required feature. Every REQUIRED feature must be on the
         allowlist in scripts/android-features.json, so a permission that
         implies a feature nobody considered is caught rather than shipped.

    Both are content checks, so the answer never depends on a file timestamp:
    an artefact whose manifest predates the source fails on the disagreement
    (and the fix is to rebuild it), and an artefact that agrees is checked
    whether it was written a minute or a month ago. Deciding by mtime was tried
    and removed — a Gradle UP-TO-DATE build leaves an old timestamp on current
    content, and an edit that is reverted leaves a new one on unchanged content,
    so the verdict was wrong in both directions.

    The only skip left is having no APK at all, which is the normal state on a
    Linux or macOS checkout. It is printed, never silent, and -RequireBuilt
    (or -Strict) turns it into a failure where a build has just run.

    -Build removes even that skip where it can: it assembles the three debug
    APKs first (through run-build.ps1 and the agentq queue, so two agents never
    compile at once) and only then reads them. A machine with no Android SDK
    says so and skips, which is the honest answer rather than a false pass.

.EXAMPLE
    pwsh -NoProfile -File scripts/check-android-features.ps1
    pwsh -NoProfile -File scripts/check-android-features.ps1 -RequireBuilt
#>
[CmdletBinding()]
param(
    # Path to aapt. Defaults to ANDROID_HOME/ANDROID_SDK_ROOT build-tools
    # (newest first), then aapt on PATH.
    [string]$Aapt,

    # No built APK is a failure instead of a skip, so a run that checked
    # nothing cannot look green where a build is guaranteed.
    [switch]$RequireBuilt,

    # Same refusal, kept as a separate name because CI reads it as "prove it".
    [switch]$Strict,

    # Assemble the debug APKs first, so a local run asserts instead of
    # skipping. The build goes through run-build.ps1 (the agentq queue) where
    # it exists, because two Gradle builds in one repo corrupt each other's
    # incremental state.
    [switch]$Build
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$failures = [System.Collections.Generic.List[string]]::new()

$allowFile = Join-Path $PSScriptRoot 'android-features.json'
if (-not (Test-Path $allowFile)) {
    Write-Host "check-android-features: $allowFile is missing" -ForegroundColor Red
    exit 1
}
$allow = (Get-Content $allowFile -Raw | ConvertFrom-Json).modules

# The build redirects its output to a file, and a devcloud job's stdout is
# published only when one of its children speaks — so without this, a working
# assemble and a hung one look identical from outside. Measured 2026-11-25: after
# the redirect the only external signal left was the instance's CPU, and eleven
# minutes of a hung build were inferred from it. `scripts/lib` owns the channel
# now, because the lane above a gate needs it for the same reason the gate does:
# one implementation, one timeline under `jobs/<id>/stage`.
. (Join-Path $PSScriptRoot 'lib/devcloud-progress.ps1')

# A growing file uploaded as it grows, for the assemble's own progress: this one
# answers "is the build working", and Write-DevcloudStage answers "where is the
# gate now" once the build is over.
function Send-ProgressFile {
    param([string]$Path, [string]$ObjectName = 'progress')
    Send-DevcloudProgress -Path $Path -ObjectName $ObjectName
}

function Write-Stage {
    param([string]$Text)
    Write-DevcloudStage $Text
}

# ── Build the artefacts first, when asked ─────────────────────────────

if ($Build) {
    # Only the Windows wrapper: Invoke-Expression cannot run the shell script,
    # and this is a local convenience rather than something CI needs (the
    # android job has just assembled what it verifies).
    $wrapper = Join-Path $root 'apps/android/gradlew.bat'
    $sdk = @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not (Test-Path $wrapper) -or -not $sdk) {
        # -Strict is the caller saying this machine has to be able to answer,
        # so the skip this gate allows elsewhere would be a false pass here.
        # A devcloud lane passes it: a workspace without the SDK must be a red
        # gate that names the missing toolchain, not a green one that built
        # nothing.
        if ($Strict) {
            Write-Host 'check-android-features: -Strict was set, but there is no Android SDK (or gradlew.bat) here to build the APK this gate verifies' -ForegroundColor Red
            exit 1
        }
        Write-Host 'check-android-features: -Build asked for an APK, but there is no Android SDK and gradlew.bat here — nothing to check' -ForegroundColor Yellow
        exit 0
    }
    $assemble = 'apps\android\gradlew.bat -p apps\android :app:assembleDebug :tv:assembleDebug :wear:assembleDebug --console=plain'
    $runBuild = Join-Path $env:USERPROFILE '.copilot\hooks\run-build.ps1'
    # A build that never returns has to become a red gate in bounded time. Twenty
    # minutes is generous for three debug APKs built from a warm daemon, and it is
    # finite: measured 2026-11-25, a wedged assemble left the job silent while the
    # caller waited out its own 60-minute timeout.
    $assembleTimeoutMin = 20
    Write-Host 'check-android-features: assembling the debug APKs first' -ForegroundColor Cyan
    Write-Stage "assemble: starting (deadline $assembleTimeoutMin min)"
    # A build that cold-starts a daemon hands the daemon its own handles: Gradle's
    # client exits and the daemon keeps the pipe open, so whoever reads this
    # process's stdout never sees EOF. Inside a devcloud job that reader is the job
    # runner, which waits for the stream to close before it publishes an exit code
    # — the lane printed PASS at 63.3 s and the job still had to be killed by its
    # own timeout (2026-11-25, jobs 20261125-020039-1c1093 and
    # 20261125-021346-637c46, neither of which ever wrote result.json). So the
    # build must never inherit our stdio: it writes to a file and we wait on the
    # process, which is what run-build.ps1 does for the same reason (titi,
    # 2026-09-29: it hung 3 h after a 103 s gate).
    $queued = [bool]$env:DEVCLOUD_JOB_ID -or ("$env:AGENTQ_HELD" -match '(^|,)build:')
    if (-not $queued -and (Test-Path $runBuild)) {
        # The queue matters locally: two Gradle builds in one repo corrupt each
        # other's incremental state. Inside a queued build — and a devcloud job is
        # one — we already hold that ticket, so asking for it again would wait on
        # ourselves; and run-build's copy on a VM may predate its file redirect.
        Write-Host 'check-android-features: assembling through run-build.ps1 (agentq queue)' -ForegroundColor DarkGray
        & pwsh -NoProfile -File $runBuild -Wait -Purpose 'android artefact gate' -Command $assemble
        $built = $LASTEXITCODE
    } else {
        $tmp = Join-Path $root '.copilot-tmp'
        if (-not (Test-Path $tmp)) { $null = New-Item -ItemType Directory -Path $tmp }
        $outFile = Join-Path $tmp 'android-assemble.log'
        $stdinEmpty = Join-Path $tmp 'android-assemble.stdin'
        if (-not (Test-Path -LiteralPath $stdinEmpty)) { [IO.File]::WriteAllText($stdinEmpty, '') }
        Write-Host "check-android-features: assembling directly, already queued (log: $outFile)" -ForegroundColor DarkGray
        # All three streams go to files. Probing this on the devcloud (2026-11-25):
        # a grandchild that inherits the caller's stdio keeps the job open for the
        # grandchild's whole life (95 s for a 90 s ping), and redirecting only
        # stdout still left it held (92 s) because cmd and its children kept the
        # inherited stderr handle.
        $errFile = Join-Path $tmp 'android-assemble.err.log'
        $proc = Start-Process -FilePath $env:ComSpec -ArgumentList '/d', '/s', '/c', "$assemble 2>&1" -WindowStyle Hidden -PassThru -RedirectStandardOutput $outFile -RedirectStandardError $errFile -RedirectStandardInput $stdinEmpty -WorkingDirectory $root
        # Follow the log from inside this process while the build runs. A devcloud job
        # publishes its output only when a child of the job speaks, and the redirect
        # above means Gradle never speaks to us — measured 2026-11-25: three minutes
        # into the android lane the job log was empty, which is indistinguishable from
        # the hang this redirection exists to prevent.
        $seen = 0
        $drain = {
            $lines = @(Get-Content -LiteralPath $outFile -ErrorAction SilentlyContinue)
            if ($lines.Count -gt $seen) {
                $lines[$seen..($lines.Count - 1)] | ForEach-Object { Write-Host "  $_" }
                $seen = $lines.Count
            }
        }
        # Dot-sourced, not called: a scriptblock called with & gets its own scope,
        # where assigning $seen leaves the outer copy at 0 and reprints from the top
        # every five seconds.
        $deadline = (Get-Date).AddMinutes($assembleTimeoutMin)
        $lastPush = [datetime]::MinValue
        $timedOut = $false
        $swAssemble = [System.Diagnostics.Stopwatch]::StartNew()
        while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
            . $drain
            if ($progress -and ((Get-Date) - $lastPush).TotalSeconds -ge 15) {
                $lastPush = Get-Date
                # The header is what makes this readable from outside: the log alone
                # cannot say whether a silent build is waiting or working, and that
                # is the question. A JVM with 0 s of CPU and 0 output is a client
                # waiting on something; a JVM burning CPU with no output is a quiet
                # build; no JVM at all means the failure is before the daemon.
                $jvms = @(Get-Process -Name java -ErrorAction SilentlyContinue)
                $header = '# {0}Z elapsed={1}s java={2} javaCpu={3}s logLines={4}' -f `
                    (Get-Date).ToUniversalTime().ToString('HH:mm:ss'), [math]::Round($swAssemble.Elapsed.TotalSeconds, 0), $jvms.Count, `
                    [math]::Round((($jvms | Measure-Object CPU -Sum).Sum), 1), @(Get-Content -LiteralPath $outFile -ErrorAction SilentlyContinue).Count
                $push = Join-Path (Split-Path -Parent $outFile) 'android-progress.txt'
                $body = @($header) + @(Get-Content -LiteralPath $outFile -ErrorAction SilentlyContinue)
                [IO.File]::WriteAllText($push, ($body -join "`r`n"))
                Send-ProgressFile -Path $push
            }
            Start-Sleep -Seconds 5
        }
        if (-not $proc.HasExited) {
            $timedOut = $true
            Write-Host "check-android-features: the assemble is still running after $assembleTimeoutMin minutes — killing it" -ForegroundColor Red
            $null = & taskkill /T /F /PID $proc.Id 2>&1
            Start-Sleep -Seconds 2
        }
        $proc.WaitForExit()
        . $drain
        $built = if ($timedOut) { 124 } else { $proc.ExitCode }
        Write-Stage "assemble: finished exit=$built after $([math]::Round($swAssemble.Elapsed.TotalSeconds, 0))s"
        if ($built -ne 0) {
            Write-Host 'check-android-features: the assemble failed; the tail of its log:' -ForegroundColor Red
            Get-Content $outFile -Tail 40 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $_" }
            Get-Content $errFile -Tail 20 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $_" }
        }
    }
    if ($built -ne 0) {
        Write-Host "check-android-features: the assemble failed (exit $built) — the gate cannot answer for an artefact that was not built" -ForegroundColor Red
        exit 1
    }
}

# ── Find the artefacts ────────────────────────────────────────────────

# Only the packaging output counts: a copy under intermediates/ or a test
# fixture carries an older manifest and would give a false answer.
$apkPattern = '[\\/]apps[\\/]android[\\/](?<module>[^\\/]+)[\\/]build[\\/]outputs[\\/]apk[\\/]'
Write-Stage 'apk-scan: starting'
$apks = @(Get-ChildItem (Join-Path $root 'apps/android') -Recurse -Filter '*.apk' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match $apkPattern } |
    Sort-Object FullName)
Write-Stage "apk-scan: $($apks.Count) apk(s)"

if ($apks.Count -eq 0) {
    if ($RequireBuilt -or $Strict) {
        Write-Host 'check-android-features: no built APK found, and -RequireBuilt/-Strict was set' -ForegroundColor Red
        exit 1
    }
    Write-Host 'check-android-features: no built APK yet — nothing to check (build :app/:wear/:tv first)' -ForegroundColor Yellow
    exit 0
}

# ── Find aapt ─────────────────────────────────────────────────────────

$candidates = [System.Collections.Generic.List[string]]::new()
if ($Aapt) { $candidates.Add($Aapt) }
foreach ($sdk in @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT)) {
    if ([string]::IsNullOrWhiteSpace($sdk)) { continue }
    $buildTools = Join-Path $sdk 'build-tools'
    if (-not (Test-Path $buildTools)) { continue }
    $versions = Get-ChildItem $buildTools -Directory | Sort-Object Name -Descending
    foreach ($v in $versions) {
        $candidates.Add((Join-Path $v.FullName 'aapt.exe'))
        $candidates.Add((Join-Path $v.FullName 'aapt'))
    }
}
$aaptOnPath = Get-Command aapt -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -ne $aaptOnPath) { $candidates.Add($aaptOnPath.Source) }

$aapt = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $aapt) {
    # An APK exists, so this is a real failure: a gate that cannot run against a
    # present artefact must not pass silently.
    Write-Host 'check-android-features: aapt not found (install Android SDK build-tools, or set ANDROID_HOME)' -ForegroundColor Red
    exit 1
}

# ── What the source asks for ──────────────────────────────────────────

# Every app module merges the shared libraries, so a shared manifest that
# declares a feature optional changes the answer for each APK.
$sharedModules = @('core', 'device', 'shared-ui')

function Get-OptionalFeatures([string[]]$modules) {
    $names = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($module in $modules) {
        $src = Join-Path $root "apps/android/$module/src"
        if (-not (Test-Path $src)) { continue }
        foreach ($manifest in Get-ChildItem $src -Recurse -Filter 'AndroidManifest.xml' -ErrorAction SilentlyContinue) {
            [xml]$xml = Get-Content -LiteralPath $manifest.FullName -Raw
            foreach ($node in @($xml.manifest.'uses-feature') | Where-Object { $_ }) {
                # Read attributes by local name so this does not depend on how
                # PowerShell's XML adapter exposes the android: prefix.
                $name = $null; $required = $null
                foreach ($a in $node.Attributes) {
                    if ($a.LocalName -eq 'name') { $name = $a.Value }
                    elseif ($a.LocalName -eq 'required') { $required = $a.Value }
                }
                if ($name -and $required -eq 'false') { $null = $names.Add($name) }
            }
        }
    }
    return @($names)
}

$sharedOptional = Get-OptionalFeatures $sharedModules
Write-Stage "features: $($sharedOptional.Count) optional feature(s) declared in the shared modules"

# ── Check each artefact ───────────────────────────────────────────────

# `uses-feature:` (required) is a different line from
# `uses-feature-not-required:`, and the literal `uses-feature:` does not occur
# inside the latter, so this pattern matches only the required ones.
$requiredPattern = "uses-feature:\s*name='(?<name>[^']+)'"
$optionalPattern = "uses-feature-not-required:\s*name='(?<name>[^']+)'"

foreach ($apk in $apks) {
    $module = [regex]::Match($apk.FullName, $apkPattern).Groups['module'].Value

    Write-Stage "aapt: reading $($apk.Name)"
    $badging = & $aapt dump badging $apk.FullName 2>&1
    $required = @([regex]::Matches(($badging -join "`n"), $requiredPattern) | ForEach-Object { $_.Groups['name'].Value }) | Sort-Object -Unique
    $optional = @([regex]::Matches(($badging -join "`n"), $optionalPattern) | ForEach-Object { $_.Groups['name'].Value }) | Sort-Object -Unique

    Write-Host ("{0}  [{1}]" -f $apk.Name, $module) -ForegroundColor Cyan
    foreach ($o in $optional) { Write-Host "  not required  $o" }
    foreach ($f in $required) { Write-Host "  REQUIRED      $f" -ForegroundColor Yellow }

    # 1. The source says these are optional; the artefact must agree.
    $declaredOptional = @($sharedOptional + (Get-OptionalFeatures @($module))) | Sort-Object -Unique
    foreach ($f in $declaredOptional) {
        if ($required -contains $f) {
            $failures.Add("$($apk.Name): the manifest declares '$f' as android:required=""false"", but this APK still requires it — rebuild this artefact; if it still requires it afterwards, the fix does not work")
        }
    }

    # 2. Nothing required that nobody approved.
    if ($allow.PSObject.Properties.Name -contains $module) {
        foreach ($f in $required) {
            if (@($allow.$module) -notcontains $f) {
                $failures.Add("$($apk.Name): requires '$f', which is not on the allowlist for module '$module' (declare it android:required=""false"" if the app can run without it)")
            }
        }
    } else {
        $failures.Add("$($apk.Name): module '$module' is not in scripts/android-features.json — add it with the features it truly needs")
    }
}

# ── Report ────────────────────────────────────────────────────────────

foreach ($f in $failures) { Write-Host "FAIL  $f" -ForegroundColor Red }
Write-Host ''
Write-Host ("android-features: {0} failure(s) across {1} APK(s)" -f $failures.Count, $apks.Count)
Write-Stage "verdict: $($failures.Count) failure(s) across $($apks.Count) apk(s)"

exit $failures.Count
