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

# ── Build the artefacts first, when asked ─────────────────────────────

if ($Build) {
    # Only the Windows wrapper: Invoke-Expression cannot run the shell script,
    # and this is a local convenience rather than something CI needs (the
    # android job has just assembled what it verifies).
    $wrapper = Join-Path $root 'apps/android/gradlew.bat'
    $sdk = @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not (Test-Path $wrapper) -or -not $sdk) {
        Write-Host 'check-android-features: -Build asked for an APK, but there is no Android SDK and gradlew.bat here — nothing to check' -ForegroundColor Yellow
        exit 0
    }
    $assemble = 'apps\android\gradlew.bat -p apps\android :app:assembleDebug :tv:assembleDebug :wear:assembleDebug --console=plain'
    $runBuild = Join-Path $env:USERPROFILE '.copilot\hooks\run-build.ps1'
    Write-Host 'check-android-features: assembling the debug APKs first' -ForegroundColor Cyan
    if (Test-Path $runBuild) {
        & pwsh -NoProfile -File $runBuild -Wait -Purpose 'android artefact gate' -Command $assemble
        $built = $LASTEXITCODE
    } else {
        Invoke-Expression $assemble
        $built = $LASTEXITCODE
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
$apks = @(Get-ChildItem (Join-Path $root 'apps/android') -Recurse -Filter '*.apk' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match $apkPattern } |
    Sort-Object FullName)

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

# ── Check each artefact ───────────────────────────────────────────────

# `uses-feature:` (required) is a different line from
# `uses-feature-not-required:`, and the literal `uses-feature:` does not occur
# inside the latter, so this pattern matches only the required ones.
$requiredPattern = "uses-feature:\s*name='(?<name>[^']+)'"
$optionalPattern = "uses-feature-not-required:\s*name='(?<name>[^']+)'"

foreach ($apk in $apks) {
    $module = [regex]::Match($apk.FullName, $apkPattern).Groups['module'].Value

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

exit $failures.Count
