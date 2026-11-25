<#
.SYNOPSIS
    Builds the Windows NSIS installer, and uploads it when running as a cloud job.

.DESCRIPTION
    The release workflow builds installers on a runner. This script exists for the
    other case: someone on this machine needs the installer for the current tree —
    to reinstall, to try a fix, to measure a size budget — and the workstation is
    shared, so the build belongs on the devcloud like every other heavy gate.

    It is the same command either way. The only difference is what happens to the
    result: inside a job (`DEVCLOUD_JOB_ID` and `DEVCLOUD_BUCKET` are set by the
    runner) the `.exe`, its SHA-256 and a one-line manifest are uploaded under
    `jobs/<id>/artefacts/`, because the job's filesystem dies with the instance;
    outside one they stay where the bundler put them and the script prints the
    path.

    `createUpdaterArtifacts` is turned off for this build. It is on in
    `tauri.conf.json` because a release signs its updater artefacts, and signing
    needs `TAURI_SIGNING_PRIVATE_KEY` — a key that lives in the release
    environment and not on a workstation or a build workspace. Without the
    override the bundler stops with "a public key has been found, but no private
    key", which reads like a configuration error rather than a missing secret.

.EXAMPLE
    # here, for the tree in front of you
    pwsh -NoProfile -File scripts/ci/build-installer.ps1

.EXAMPLE
    # on the devcloud, and bring the installer back
    pwsh -NoProfile -File "$env:USERPROFILE\.copilot\bin\agentq-cloud.ps1" -Ws vitals-win `
      -Purpose 'installer for 1.4.0' -Repo . -TimeoutMin 45 `
      -- pwsh -NoProfile -File scripts/ci/build-installer.ps1
#>
[CmdletBinding()]
param(
    # Which bundle to build. NSIS is what this repository ships on Windows.
    [string]$Bundles = 'nsis',

    # Skip the frontend and Rust build when an installer for the current tree is
    # already there, for a re-upload.
    [switch]$Reuse
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

$bundleDir = Join-Path $root 'target/release/bundle'
$pattern = Join-Path $bundleDir "$Bundles/*.exe"

function Get-Installer {
    @(Get-ChildItem -Path $pattern -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
}

$installer = if ($Reuse) { Get-Installer } else { @() }
if ($installer.Count -eq 0) {
    # The version the bundle will carry, so the log says which one was built
    # rather than leaving the reader to read it off the file name.
    $conf = Get-Content (Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
    Write-Host "building $($conf.version) $Bundles installer in $root" -ForegroundColor Cyan

    # The CLI is invoked through node, from the application directory, rather than
    # through `pnpm --filter … tauri`: the override is a single argument containing
    # JSON, and one layer of argument parsing between the script and the CLI is one
    # place for it to be split or eaten. `--config` as one argument keeps the
    # override next to the reason for it instead of in a config file nobody finds.
    $cli = Join-Path $root 'apps/desktop/node_modules/@tauri-apps/cli/tauri.js'
    if (-not (Test-Path $cli)) { throw "the Tauri CLI is not installed at $cli (run pnpm install)" }
    $config = '{"bundle":{"createUpdaterArtifacts":false}}'
    # The CLI derives its `--ci` from the environment and accepts only `true` or
    # `false`; CI systems — and this repository's devcloud runner — set `CI=1`, so
    # the build died in argument parsing with
    # `error: invalid value '1' for '--ci' / possible values: true, false` before it
    # compiled anything (job 20261125-041845-51cb82). A release on a GitHub runner
    # never sees this because Actions sets `CI=true`.
    $env:CI = 'true'
    Push-Location (Join-Path $root 'apps/desktop')
    try {
        & node $cli build --bundles $Bundles --config $config
    } finally {
        Pop-Location
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "tauri build exited $LASTEXITCODE" -ForegroundColor Red
        exit $LASTEXITCODE
    }
    $installer = Get-Installer
}

if ($installer.Count -eq 0) {
    # Not a warning: a build that produced nothing is a build that failed, and a
    # caller that treats the absence as "nothing to do" would install an older
    # installer without noticing.
    Write-Host "no installer in $bundleDir/$Bundles" -ForegroundColor Red
    exit 1
}

$exe = $installer[0]
$hash = (Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
$size = (Get-Item -LiteralPath $exe.FullName).Length
Write-Host ("installer {0} ({1:N0} bytes, sha256 {2})" -f $exe.Name, $size, $hash.Substring(0, 16))

# The size budget is a gate, not a report: `scripts/check-size.ps1` measures the
# same file, and a build that is over it should say so here rather than four steps
# later in a different script.
& pwsh -NoProfile -File (Join-Path $root 'scripts/check-size.ps1')
if ($LASTEXITCODE -ne 0) {
    Write-Host "check-size.ps1 exited $LASTEXITCODE" -ForegroundColor Red
    exit $LASTEXITCODE
}

if (-not ($env:DEVCLOUD_JOB_ID -and $env:DEVCLOUD_BUCKET)) {
    Write-Host "not a cloud job: the installer stays at $($exe.FullName)"
    exit 0
}

# Inside a job. Upload the installer itself, then its checksum and a manifest, so
# the download side can prove it got what the build produced rather than what
# happened to be on its disk.
. (Join-Path $PSScriptRoot '../lib/devcloud-progress.ps1')
$manifest = Join-Path ([IO.Path]::GetTempPath()) 'installer.json'
@{
    version = (Get-Content (Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
    file    = $exe.Name
    bytes   = $size
    sha256  = $hash
    builtAt = (Get-Date).ToUniversalTime().ToString('o')
} | ConvertTo-Json | Set-Content -LiteralPath $manifest -Encoding utf8NoBOM

foreach ($pair in @(@{ path = $exe.FullName; name = 'artefacts/installer.exe' },
        @{ path = $manifest; name = 'artefacts/installer.json' })) {
    Write-DevcloudStage "uploading $($pair.name)"
    Send-DevcloudProgress -Path $pair.path -ObjectName $pair.name -TimeoutSec 600
}
Write-DevcloudStage 'installer uploaded'
Write-Host "uploaded jobs/$($env:DEVCLOUD_JOB_ID)/artefacts/installer.exe"
exit 0
