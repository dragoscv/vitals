<#
.SYNOPSIS
    Builds two signed installers of the working tree, versions -From and
    -To, plus a latest.json naming -To, for update-proof.ps1 -Local (DEV-3).

.DESCRIPTION
    The real release key never leaves CI, and a proof of the per-user update
    path cannot wait for a release to exist. So the tree is built twice with a
    throwaway minisign key and a local update endpoint, both supplied through
    a config overlay (`tauri build --config`); nothing in the repository
    changes. The overlay also allows plain HTTP, which only these two builds
    ever see.

    Output in -Out: Vitals_<From>_x64-setup.exe, Vitals_<To>_x64-setup.exe,
    the signed -To updater artefact and latest.json. With -Upload, the folder
    is copied to that gs:// prefix as well (a devcloud job returns only its
    log).

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/update-proof-build.ps1 -KeyFile k.key -KeyPassword p -Out out
#>
[CmdletBinding()]
param(
    [string]$From = '1.99.0',
    [string]$To = '1.99.1',
    [Parameter(Mandatory)][string]$KeyFile,
    [Parameter(Mandatory)][string]$KeyPassword,
    [string]$Endpoint = 'http://127.0.0.1:8765/latest.json',
    [Parameter(Mandatory)][string]$Out,
    [string]$Upload
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$app = Join-Path $root 'apps/desktop'
$Out = [IO.Path]::GetFullPath($Out)
New-Item -ItemType Directory -Force -Path $Out | Out-Null

$pub = (Get-Content "$KeyFile.pub" -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $KeyFile -Raw).Trim()
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $KeyPassword
# The Tauri CLI reads CI as a boolean flag and rejects the `1` many runners
# (and the devcloud agent) set.
if ($env:CI) { $env:CI = 'true' }

$bundle = Join-Path $root 'target/release/bundle/nsis'
foreach ($version in $From, $To) {
    $overlay = Join-Path $Out "overlay-$version.json"
    @{
        version = $version
        plugins = @{
            updater = @{
                pubkey = $pub
                endpoints = @($Endpoint)
                dangerousInsecureTransportProtocol = $true
            }
        }
    } | ConvertTo-Json -Depth 6 | Set-Content -Path $overlay -Encoding utf8NoBOM
    Push-Location $app
    try {
        pnpm tauri build --bundles nsis --config $overlay
        if ($LASTEXITCODE) { throw "tauri build $version exited $LASTEXITCODE" }
    } finally { Pop-Location }
    $setup = Join-Path $bundle "Vitals_${version}_x64-setup.exe"
    if (-not (Test-Path $setup)) { throw "no installer at $setup" }
    Copy-Item $setup $Out -Force
    if ($version -eq $To) { Copy-Item "$setup.sig" $Out -Force }
}

$sig = (Get-Content (Join-Path $Out "Vitals_${To}_x64-setup.exe.sig") -Raw).Trim()
$base = $Endpoint.Substring(0, $Endpoint.LastIndexOf('/') + 1)
@{
    version = $To
    notes = 'update-proof'
    pub_date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = @{ 'windows-x86_64' = @{ signature = $sig; url = "${base}Vitals_${To}_x64-setup.exe" } }
} | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $Out 'latest.json') -Encoding utf8NoBOM

Get-ChildItem $Out | ForEach-Object { '{0,12:N0}  {1}' -f $_.Length, $_.Name }
if ($Upload) {
    gcloud storage cp (Join-Path $Out '*') "$Upload/"
    if ($LASTEXITCODE) { throw "upload to $Upload failed" }
    "uploaded to $Upload"
}
