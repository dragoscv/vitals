<#
.SYNOPSIS
    Packages the Tizen web app for release: the bundle, then the signed .wgt.

.DESCRIPTION
    The release-time half of the Tizen lane. It builds apps/tizen
    (tsc --noEmit then vite build -> apps/tizen/dist) and signs that bundle
    into a versioned .wgt with the Tizen CLI and a Samsung certificate profile
    -- the same two steps apps/tizen/scripts/tv-deploy.ps1 -NoInstall performs,
    but as a release step that checks its prerequisites instead of trusting
    them.

    It fails loudly. Every prerequisite is checked before anything is built or
    signed; a missing one names exactly what is missing and exits non-zero. It
    never skips a step, never reports a success it did not produce, and refuses
    to hand back a package the Tizen CLI did not actually sign -- an unsigned
    bundle is the one thing a TV will not install.

    -DryRun prints the exact command sequence and the state of every
    prerequisite, builds nothing, writes nothing and exits 0. It is meant to
    run on a machine without Tizen Studio, which is what proves the lane's
    logic without the SDK.

    Signing is not a separate step. `tizen package -s <profile>` produces the
    signed .wgt in the one command, so there is no -Sign switch; the
    certificate and profile requirements are in docs/distribution.md.

    The version is never hard-coded: it is read from scripts/version.ps1 and
    checked against apps/tizen/public/config.xml, so a package that kept last
    release's version fails here instead of shipping. The version.ps1 script
    owns that one number across six manifests, and duplicating the rule here
    would be a seventh copy to keep in step.

.PARAMETER CertProfile
    The Samsung certificate profile to sign with. Default: mixai-samsung.

.PARAMETER TizenHome
    Tizen Studio install root. Default: $env:TIZEN_STUDIO, else
    %USERPROFILE%\tizen-studio.

.PARAMETER DataHome
    Tizen Studio data root holding profiles.xml and the keystore.
    Default: $env:TIZEN_STUDIO_DATA, else %USERPROFILE%\tizen-studio-data.

.PARAMETER OutDir
    Where the .wgt and SHA256SUMS.txt are written. Default: apps/tizen/build,
    which .gitignore already covers (the repository-wide `build/` pattern).

.PARAMETER SkipBuild
    Package an existing apps/tizen/dist instead of rebuilding the bundle.

.PARAMETER DryRun
    Print the plan and the prerequisite state, then exit 0 without building,
    packaging or writing anything.

.EXAMPLE
    pwsh -NoProfile -File scripts/tizen-package.ps1 -DryRun

.EXAMPLE
    pwsh -NoProfile -File scripts/tizen-package.ps1

.NOTES
    Exit codes:
      0  packaged (or a dry run)
      1  a prerequisite is missing (Tizen CLI or signing profile)
      2  the bundle build failed, or dist is incomplete
      3  the Tizen CLI could not produce a package
      4  a package was produced but is not a signed .wgt
      5  the version surfaces disagree
#>
[CmdletBinding()]
param(
    [string]$CertProfile = 'mixai-samsung',
    [string]$TizenHome = $(if ($env:TIZEN_STUDIO) { $env:TIZEN_STUDIO } else { Join-Path $env:USERPROFILE 'tizen-studio' }),
    [string]$DataHome = $(if ($env:TIZEN_STUDIO_DATA) { $env:TIZEN_STUDIO_DATA } else { Join-Path $env:USERPROFILE 'tizen-studio-data' }),
    [string]$OutDir,
    [switch]$SkipBuild,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
# Resolve everything against the repository root, so the script behaves the same
# however and from wherever it is launched.
Set-Location $root

$appDir = Join-Path $root 'apps/tizen'
$distDir = Join-Path $appDir 'dist'
$configXml = Join-Path $appDir 'public/config.xml'
$tizenBat = Join-Path $TizenHome 'tools\ide\bin\tizen.bat'
$sdbExe = Join-Path $TizenHome 'tools\sdb.exe'
$profileXml = Join-Path $DataHome 'profile\profiles.xml'
if (-not $OutDir) { $OutDir = Join-Path $appDir 'build' }

# -- output helpers --------------------------------------------------------
function Say([string]$m) { Write-Host $m }
function Rule([string]$m) { Write-Host ''; Write-Host $m -ForegroundColor Cyan }
function Kv([string]$k, [string]$v) { Write-Host ('  {0,-14} {1}' -f $k, $v) }
function Prereq([bool]$ok, [string]$name, [string]$detail) {
    $tag = if ($ok) { '[ok]     ' } else { '[missing]' }
    $colour = if ($ok) { 'DarkGray' } else { 'Yellow' }
    Write-Host ('  {0} {1,-12} {2}' -f $tag, $name, $detail) -ForegroundColor $colour
}
function Fail([int]$code, [string]$m) {
    Write-Host ''
    Write-Host 'tizen-package: FAILED' -ForegroundColor Red
    Write-Host "  $m" -ForegroundColor Red
    exit $code
}

Write-Host 'tizen-package -- Vitals for Samsung TVs (Tizen web app)' -ForegroundColor White

# -- the version, from the one script that owns it -------------------------
# version.ps1 with no arguments prints a table named by manifest. The Cargo
# row is the canonical value; the Tizen row is the surface this package
# carries, and the two must agree or the .wgt would ship a stale number.
$versionScript = Join-Path $PSScriptRoot 'version.ps1'
$pwshCmd = Get-Command pwsh -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
$hostExe = if ($pwshCmd) { $pwshCmd.Source } else { 'powershell.exe' }
$versionText = (& $hostExe -NoProfile -File $versionScript 2>&1) -join "`n"
$versionMatch = [regex]::Match($versionText, '(?m)^\s*Cargo\.toml\s+([0-9][^\s]*)\s*$')
if (-not $versionMatch.Success) {
    Write-Host $versionText
    Fail 5 'could not read the version from scripts/version.ps1 (its output is above)'
}
$version = $versionMatch.Groups[1].Value

# -- the widget manifest, and the drift check ------------------------------
$xmlText = [IO.File]::ReadAllText($configXml)
$widgetMatch = [regex]::Match($xmlText, '(?s)(<widget\b[^>]*?\bversion=")([^"]*)(")')
if (-not $widgetMatch.Success) { Fail 5 "$configXml has no <widget ... version=...> to read" }
$tizenVersion = $widgetMatch.Groups[2].Value
$appMatch = [regex]::Match($xmlText, '<tizen:application\s+id="([^"]+)"[^>]*?required_version="([^"]+)"')
$appId = if ($appMatch.Success) { $appMatch.Groups[1].Value } else { '(unknown)' }
$requiredVersion = if ($appMatch.Success) { $appMatch.Groups[2].Value } else { '(unknown)' }

if ($tizenVersion -ne $version) {
    Fail 5 ("version drift: scripts/version.ps1 says $version but apps/tizen/public/config.xml says $tizenVersion. " +
            "Run: pwsh -NoProfile -File scripts/version.ps1 -Set $version")
}

# -- prerequisites, evaluated before anything is built or signed -----------
# The CLI is required. sdb is NOT: it is only used to install onto a TV, and a
# release machine can produce a .wgt without one -- so it is reported, not
# required, which keeps a false "cannot package" out of the way.
$haveCli = Test-Path $tizenBat
$haveSdb = Test-Path $sdbExe
$haveProfileFile = Test-Path $profileXml
$profileNames = $false
if ($haveProfileFile) {
    $profileNames = [regex]::IsMatch([IO.File]::ReadAllText($profileXml), 'name\s*=\s*"' + [regex]::Escape($CertProfile) + '"')
}
$distBuilt = Test-Path (Join-Path $distDir 'config.xml')

$problems = [System.Collections.Generic.List[string]]::new()
if (-not $haveCli) { $problems.Add("Tizen CLI not found at $tizenBat (install Tizen Studio, or pass -TizenHome)") }
if (-not $haveProfileFile) { $problems.Add("signing profile file not found at $profileXml (create the profile in Tizen Studio's Certificate Manager)") }
elseif (-not $profileNames) { $problems.Add("signing profile '$CertProfile' is not in $profileXml") }
if ($SkipBuild -and -not $distBuilt) { $problems.Add('apps/tizen/dist has no config.xml and -SkipBuild was given (build it, or drop -SkipBuild)') }

$wgtName = "Vitals_$version.wgt"
$wgtPath = Join-Path $OutDir $wgtName
$outRel = [IO.Path]::GetRelativePath($root, $OutDir)
$tizenRel = [IO.Path]::GetRelativePath($root, $tizenBat)

# -- what this run knows ---------------------------------------------------
Kv 'version' "$version  (scripts/version.ps1; config.xml agrees)"
Kv 'app id' "$appId  (required_version $requiredVersion)"
Kv 'profile' $CertProfile
Kv 'tizen studio' $TizenHome
Kv 'output' "$outRel\$wgtName"

Rule 'prerequisites'
Prereq $haveCli 'tizen CLI' $tizenBat
Prereq $haveSdb 'sdb' "$sdbExe  (only needed to install; packaging does not use it)"
if (-not $haveProfileFile) {
    Prereq $false 'profile' "$CertProfile -- $profileXml not found"
} elseif (-not $profileNames) {
    Prereq $false 'profile' "$CertProfile is not listed in $profileXml"
} else {
    Prereq $true 'profile' "$CertProfile in $profileXml"
}
if ($SkipBuild) { Prereq $distBuilt 'bundle' "apps/tizen/dist  (-SkipBuild)" } else { Prereq $true 'bundle' 'will be built by this script' }

# The exact command sequence, whether or not the SDK is present. A dry run is
# the lane's specification made executable.
Rule 'commands this lane runs'
Say '  1. pnpm --filter @vitals/tizen build'
Say "     (tsc --noEmit then vite build -> apps/tizen/dist)"
Say "  2. copy apps/tizen/dist (minus any *.wgt) into a fresh TEMP staging dir"
Say "  3. $tizenRel package -t wgt -s $CertProfile -o <temp-out> -- <temp-stage>"
Say '     -- one command: it produces the signed .wgt in place, so there is no separate signing step'
Say "  4. copy the produced .wgt to $outRel\$wgtName"
Say '  5. assert the .wgt is a signed package (a zip carrying author-signature.xml / signatureN.xml)'
Say "  6. Get-FileHash -Algorithm SHA256 -> $outRel\SHA256SUMS.txt"

# -- dry run: report and stop ----------------------------------------------
if ($DryRun) {
    Rule 'dry-run verdict'
    if ($problems.Count -eq 0) {
        Say '  the real run would SUCCEED: every prerequisite is present.' -ForegroundColor Green
    } else {
        Say "  the real run would FAIL with $($problems.Count) unmet prerequisite(s):" -ForegroundColor Yellow
        $problems | ForEach-Object { Say "    - $_" -ForegroundColor Yellow }
    }
    Say ''
    Say '  nothing was built, packaged or written.'
    exit 0
}

# -- real run: refuse to start without the prerequisites -------------------
if ($problems.Count -gt 0) {
    $detail = ($problems | ForEach-Object { "    - $_" }) -join "`n"
    Fail 1 ("cannot package the Tizen app:`n$detail`n  Re-run with -DryRun to see the plan, or -TizenHome <path> to point at another Tizen Studio install.")
}

# -- build -----------------------------------------------------------------
$pnpmCmd = Get-Command pnpm -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $SkipBuild) {
    if (-not $pnpmCmd) { Fail 2 'pnpm is not on PATH and the bundle must be built (or pass -SkipBuild)' }
    Rule 'building the bundle'
    & $pnpmCmd.Source --filter @vitals/tizen build
    if ($LASTEXITCODE -ne 0) { Fail 2 "the bundle build failed (pnpm exited $LASTEXITCODE)" }
}
if (-not (Test-Path (Join-Path $distDir 'config.xml'))) {
    Fail 2 'apps/tizen/dist/config.xml is missing: the bundle was not built (drop -SkipBuild)'
}

# -- stage and sign --------------------------------------------------------
# tizen package signs in place, so it works on a TEMP copy of dist and never
# inside it. A .wgt an earlier run left in dist is excluded, or it would be
# zipped into the next package and the file would grow on every run.
$stage = Join-Path $env:TEMP ('vitals-tizen-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$stageOut = "$stage-out"
New-Item -ItemType Directory -Force $stage, $stageOut | Out-Null
Get-ChildItem (Join-Path $distDir '*') -Exclude '*.wgt' | ForEach-Object {
    Copy-Item $_.FullName $stage -Recurse -Force
}

Rule "signing (tizen package -s $CertProfile)"
& $tizenBat package -t wgt -s $CertProfile -o $stageOut -- $stage
$pkgExit = $LASTEXITCODE
if ($pkgExit -ne 0) {
    Fail 3 "the Tizen CLI failed packaging with profile '$CertProfile' (exit $pkgExit). Is the profile's certificate usable and Tizen Studio complete?"
}

# The CLI prints its output path as it finishes and the file can appear a
# moment later; look again briefly rather than report a missing package that is
# on its way (seen on the first workspace run).
$built = $null
for ($i = 0; $i -lt 20 -and $null -eq $built; $i++) {
    $built = Get-ChildItem $stageOut -Filter *.wgt -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $built) { Start-Sleep -Milliseconds 250 }
}
if ($null -eq $built) { Fail 3 "the Tizen CLI exited $pkgExit but produced no .wgt" }

# -- assert the package is signed -----------------------------------------
# A signed .wgt is a zip whose root carries author-signature.xml and/or
# signatureN.xml; an unsigned bundle (what CI falls back to on a hosted runner)
# has neither and a TV refuses it. Deciding on content, not on the exit code,
# is the whole point of this check.
try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
    $zip = [System.IO.Compression.ZipFile]::OpenRead($built.FullName)
    $entryNames = @($zip.Entries | ForEach-Object { $_.FullName })
    $zip.Dispose()
} catch {
    Fail 4 "the produced package could not be opened to check its signature: $($_.Exception.Message)"
}
$signatureEntries = @($entryNames | Where-Object { $_ -match '(^|/)(author-)?signature\d*\.xml$' })
if ($signatureEntries.Count -eq 0) {
    Fail 4 ("the produced .wgt is not signed (no author-signature.xml / signatureN.xml at its root; entries: " +
            ($entryNames -join ', ') + ')')
}

# -- publish the artefact --------------------------------------------------
New-Item -ItemType Directory -Force $OutDir | Out-Null
Copy-Item $built.FullName $wgtPath -Force
$sizeKb = [math]::Round((Get-Item $wgtPath).Length / 1KB, 1)
$hash = (Get-FileHash -Algorithm SHA256 -Path $wgtPath).Hash.ToLower()
"$hash  $wgtName" | Set-Content -Path (Join-Path $OutDir 'SHA256SUMS.txt') -Encoding ascii

Rule 'packaged'
Say "  $outRel\$wgtName"
Kv 'size' "$sizeKb KB"
Kv 'sha256' $hash
Kv 'signed' ($signatureEntries -join ', ')
Say ''
Say 'Install it on a TV with: pwsh -NoProfile -File apps/tizen/scripts/tv-deploy.ps1 (needs the TV in developer mode).'
exit 0
