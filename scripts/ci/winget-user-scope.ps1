<#
.SYNOPSIS
    Rewrites a Komac-generated winget installer manifest for a per-user install.

.DESCRIPTION
    `komac update` carries installer fields forward from the previous
    manifest, and 1.0.0-1.0.3 were published as `Scope: machine` with
    `ElevationRequirement: elevatesSelf` and a Program Files location. Since
    ADR 0038 the installer is `installMode: both`, and the copy winget should
    install is the per-user one: it updates itself on quit without a UAC
    prompt. Left alone, Komac would publish the machine scope forever, so the
    release job writes the manifests to a folder, runs this, then submits.

    Idempotent: a manifest that already says `Scope: user` comes out unchanged.
    Exit code 1 when the file does not look like a winget installer manifest,
    so a Komac format change fails the job instead of submitting nonsense.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/winget-user-scope.ps1 -Path out/manifests/v/Vitals/Vitals/1.0.4/Vitals.Vitals.installer.yaml
#>
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Path)
$ErrorActionPreference = 'Stop'

$text = [IO.File]::ReadAllText((Resolve-Path $Path))
if ($text -notmatch '(?m)^ManifestType: installer\s*$') { Write-Error "$Path is not a winget installer manifest"; exit 1 }
$nl = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
$lines = [Collections.Generic.List[string]]::new([string[]]($text -split '\r?\n'))

# Top-level keys only (no indentation): an installer entry may carry its own.
function Find-Key([string]$key) {
    for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i] -match "^$([regex]::Escape($key)):") { return $i } }
    return -1
}
# Removes a top-level key and its indented children.
function Remove-Key([string]$key) {
    $i = Find-Key $key
    if ($i -lt 0) { return }
    $lines.RemoveAt($i)
    while ($i -lt $lines.Count -and $lines[$i] -match '^\s+\S') { $lines.RemoveAt($i) }
}

$scope = Find-Key 'Scope'
if ($scope -ge 0) { $lines[$scope] = 'Scope: user' }
else {
    $at = Find-Key 'InstallerType'
    if ($at -lt 0) { Write-Error "$Path has no InstallerType"; exit 1 }
    $lines.Insert($at + 1, 'Scope: user')
}

# ElevationRequirement stays `elevatesSelf`: the `both` setup requests
# `highest`, so an administrator still sees one UAC prompt even with
# /CurrentUser (a standard user sees none). Dropping the key also fails the
# winget validator's consistency check against earlier versions (PR #454976).
Remove-Key 'InstallerSwitches'
$at = Find-Key 'Scope'
$lines.Insert($at + 1, 'InstallerSwitches:')
$lines.Insert($at + 2, '  Custom: /CurrentUser')

Remove-Key 'InstallationMetadata'
$at = Find-Key 'InstallerSwitches'
$lines.Insert($at + 2, 'InstallationMetadata:')
$lines.Insert($at + 3, "  DefaultInstallLocation: '%LocalAppData%\Programs\Vitals'")

[IO.File]::WriteAllText((Resolve-Path $Path), ($lines -join $nl))
Write-Host "winget manifest set to Scope: user, Custom: /CurrentUser ($Path)"
