<#
.SYNOPSIS
  Reads, sets or checks the one version Vitals ships under.

.DESCRIPTION
  The version lives in four files that build independently: the Cargo
  workspace, the root and desktop package.json, and tauri.conf.json. The
  updater compares the installed version with latest.json, so a release
  whose files disagree with its tag either offers itself forever or never.

  -Set 0.9.0-beta.1   writes it everywhere (and every packages/* manifest)
  -Check v0.9.0-beta.1  fails unless every file says exactly that
  (no args)           prints what each file says
#>
param(
  [string]$Set,
  [string]$Check
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

$cargo = Join-Path $root 'Cargo.toml'
$tauri = Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json'
$jsonFiles = @(Join-Path $root 'package.json') +
  @(Get-ChildItem -Path (Join-Path $root 'apps'), (Join-Path $root 'packages') -Filter package.json -Depth 1 -File |
    Where-Object { $_.FullName -notmatch 'node_modules' } | ForEach-Object FullName)

$semver = '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$'

function Get-Versions {
  $out = [ordered]@{}
  $m = [regex]::Match([IO.File]::ReadAllText($cargo), '(?m)^\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"')
  $out['Cargo.toml'] = $m.Groups[1].Value
  $out['tauri.conf.json'] = ([IO.File]::ReadAllText($tauri) | ConvertFrom-Json).version
  foreach ($f in $jsonFiles) {
    $rel = [IO.Path]::GetRelativePath($root, $f)
    $out[$rel] = ([IO.File]::ReadAllText($f) | ConvertFrom-Json).version
  }
  $out
}

function Set-JsonVersion([string]$path, [string]$version) {
  $text = [IO.File]::ReadAllText($path)
  $new = [regex]::Replace($text, '("version"\s*:\s*")[^"]*(")', "`${1}$version`${2}", 1)
  [IO.File]::WriteAllText($path, $new)
}

if ($Set) {
  if ($Set -notmatch $semver) { throw "not a SemVer version: $Set" }
  $text = [IO.File]::ReadAllText($cargo)
  $text = [regex]::Replace($text, '(?m)(^\[workspace\.package\][\s\S]*?^version\s*=\s*")[^"]+(")', "`${1}$Set`${2}", 1)
  [IO.File]::WriteAllText($cargo, $text)
  Set-JsonVersion $tauri $Set
  foreach ($f in $jsonFiles) { Set-JsonVersion $f $Set }
  Get-Versions | Format-Table -AutoSize | Out-String | Write-Host
  Write-Host "Now run: cargo update --workspace (refreshes Cargo.lock)"
  exit 0
}

$versions = Get-Versions
if ($Check) {
  $want = $Check -replace '^v', ''
  $bad = @($versions.GetEnumerator() | Where-Object { $_.Value -ne $want })
  if ($bad.Count -gt 0) {
    $bad | ForEach-Object { Write-Host "  $($_.Key) says $($_.Value), tag says $want" }
    throw "version drift: $($bad.Count) file(s) disagree with $Check"
  }
  Write-Host "every manifest says $want"
  exit 0
}
$versions | Format-Table -AutoSize
