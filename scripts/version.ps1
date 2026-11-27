<#
.SYNOPSIS
  Reads, sets or checks the one version Vitals ships under.

.DESCRIPTION
  The version lives in six independent places: the Cargo workspace, the root
  and desktop package.json, tauri.conf.json, the XcodeGen spec
  (apps/ios/project.yml, MARKETING_VERSION) and the Tizen widget manifest
  (apps/tizen/public/config.xml). The updater compares the installed version
  with latest.json, so a release whose files disagree with its tag either
  offers itself forever or never — and iOS or Tizen silently shipping last
  month's version is the same failure, one store at a time.

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
$iosSpec = Join-Path $root 'apps/ios/project.yml'
$tizenXml = Join-Path $root 'apps/tizen/public/config.xml'
$jsonFiles = @(Join-Path $root 'package.json') +
  @(Get-ChildItem -Path (Join-Path $root 'apps'), (Join-Path $root 'packages') -Filter package.json -Depth 1 -File |
    Where-Object { $_.FullName -notmatch 'node_modules' } | ForEach-Object FullName)

$semver = '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$'

# XcodeGen YAML. One value sits in settings.base and every target inherits
# it; the quotes are optional in YAML, so both are matched and the write puts
# them back. The pattern stops at the line end, so CURRENT_PROJECT_VERSION
# and the deployment target are never touched.
$iosPattern = '(?m)^([ \t]*MARKETING_VERSION:[ \t]*)''?([^''\r\n#]+?)''?[ \t]*$'
# The widget element of the Tizen manifest. Anchored to `<widget`, so
# required_version on <tizen:application> and any other attribute named
# version elsewhere in the file stay as they are.
$tizenPattern = '(?s)(<widget\b[^>]*?\bversion=")([^"]*)(")'

function Get-Versions {
  $out = [ordered]@{}
  $m = [regex]::Match([IO.File]::ReadAllText($cargo), '(?m)^\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"')
  $out['Cargo.toml'] = $m.Groups[1].Value
  $out['tauri.conf.json'] = ([IO.File]::ReadAllText($tauri) | ConvertFrom-Json).version
  $out['apps/ios/project.yml'] = (Get-IosVersions) -join ' '
  $out['apps/tizen/public/config.xml'] = Get-TizenVersion
  foreach ($f in $jsonFiles) {
    $rel = [IO.Path]::GetRelativePath($root, $f)
    $out[$rel] = ([IO.File]::ReadAllText($f) | ConvertFrom-Json).version
  }
  $out
}

# Every MARKETING_VERSION value in the spec, so a second one added later is
# reported (and fails the check) rather than overwritten invisibly.
function Get-IosVersions {
  $matches = [regex]::Matches([IO.File]::ReadAllText($iosSpec), $iosPattern)
  if ($matches.Count -eq 0) { throw "$iosSpec has no MARKETING_VERSION line" }
  @($matches | ForEach-Object { $_.Groups[2].Value.Trim() } | Select-Object -Unique)
}

function Get-TizenVersion {
  $m = [regex]::Match([IO.File]::ReadAllText($tizenXml), $tizenPattern)
  if (-not $m.Success) { throw "$tizenXml has no <widget ... version=`"...`"> to read" }
  $m.Groups[2].Value
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

  $text = [IO.File]::ReadAllText($iosSpec)
  if ([regex]::Matches($text, $iosPattern).Count -eq 0) { throw "$iosSpec has no MARKETING_VERSION line to set" }
  # Single quotes around the value, as the file has always written it. The
  # replacement is concatenated rather than interpolated: in a double-quoted
  # string PowerShell would expand `$Set` and eat the `$1` group reference
  # unless it were backtick-escaped, which is the kind of line that breaks
  # silently later.
  [IO.File]::WriteAllText($iosSpec, [regex]::Replace($text, $iosPattern, ('$1' + "'" + $Set + "'")))

  $text = [IO.File]::ReadAllText($tizenXml)
  if (-not [regex]::IsMatch($text, $tizenPattern)) { throw "$tizenXml has no <widget ... version=...> to set" }
  [IO.File]::WriteAllText($tizenXml, [regex]::Replace($text, $tizenPattern, ('${1}' + $Set + '${3}'), 1))

  # The Store version is a FOUR-part literal - the product version with a
  # trailing .0 - and it is only ever pasted by a human into an MSIX build, so
  # no compiler and no test sees it. scripts/check-drift.ps1 pins every literal
  # bound to a `-Version` argument or a `version=` workflow input against it,
  # and that gate has fired twice: the 1.3.0 bump missed one example and the
  # 1.3.1 release commit updated three of the five and missed the other two.
  # Rewriting them here is what makes the bump unable to leave one behind; the
  # gate stays as the check that it did not, and it deliberately still reads
  # Cargo.toml directly so it never depends on this having run.
  $storeVersion = (($Set -split '[-+]')[0]) + '.0'
  # Same shape as the gate's: the binding prefix is captured so that only the
  # number moves, and `e.g. 0.9.0.0` or `(e.g. 1.3.0.0)` - illustrations of the
  # form, with no `-Version` in front of them - are left alone.
  $boundVersion = '(?<![A-Za-z0-9_])((?:-Version\s+|version=))\d+\.\d+\.\d+\.\d+'
  $boundRoots = @('packaging', 'scripts', '.github/workflows')
  $boundExtensions = @('.ps1', '.psm1', '.psd1', '.yml', '.yaml', '.json', '.xml', '.nuspec', '.md', '.mjs', '.js', '.sh')
  $rewritten = 0
  $rewrittenFiles = @()
  foreach ($file in @(Get-ChildItem -Path ($boundRoots | ForEach-Object { Join-Path $root $_ }) -Recurse -File -ErrorAction SilentlyContinue |
    Where-Object { $boundExtensions -contains $_.Extension })) {
    $text = [IO.File]::ReadAllText($file.FullName)
    if (-not [regex]::IsMatch($text, $boundVersion)) { continue }
    $new = [regex]::Replace($text, $boundVersion, ('${1}' + $storeVersion))
    if ($new -ne $text) {
      [IO.File]::WriteAllText($file.FullName, $new)
      $rewritten++
      $rewrittenFiles += ([IO.Path]::GetRelativePath($root, $file.FullName) -replace '\\', '/')
    }
  }
  Write-Host ("Store version {0} written into {1} bound example(s): {2}" -f $storeVersion, $rewritten, ($rewrittenFiles -join ', '))

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
