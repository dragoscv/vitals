<#
.SYNOPSIS
  Regenerates THIRD_PARTY_NOTICES.md: every Rust crate and npm package that
  ships inside the desktop app, the CLI, the sensors helper and the lag
  watchdog, with its licence, followed by the full text of every licence
  named.

.DESCRIPTION
  MIT and Apache-2.0 both require their notices to travel with the binary.
  Built from `cargo metadata` (normal dependencies of the shipped crates, for
  the Windows target) and `pnpm licenses list --prod` for the webview and the
  workspace packages it bundles, so no extra tool has to be installed. The
  file is installed beside the app (tauri.conf.json bundle.resources), which
  is why it carries the licence texts themselves rather than links: a link
  is not a copy of the licence.

  Standard SPDX texts live in `licences/<id>.txt`. A dependency under a
  licence with no text there fails the run, so a new licence is read and
  added on purpose rather than shipped without its text.

  `-Check` fails when the committed file is stale, which is how CI keeps it
  true after a dependency bump.
#>
param([switch]$Check)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root 'THIRD_PARTY_NOTICES.md'
$licenceDir = Join-Path $root 'licences'

# Embedded in vitals-sensors.exe (crates/vitals-sensors/modules), not a
# crate, so no lockfile names it. Keep in step with the module hashes in
# crates/vitals-sensors/src/lib.rs.
$pawnioModules = [pscustomobject]@{
  Name = 'PawnIO.Modules'
  Version = '0.2.11'
  License = 'LGPL-2.1-or-later'
  Url = 'https://github.com/namazso/PawnIO.Modules/tree/0.2.11'
  Copyright = 'Copyright (C) 2025 namazso <admin@namazso.eu>'
  Text = 'crates/vitals-sensors/modules/COPYING'
}

Push-Location $root
try {
  $meta = cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc --locked | ConvertFrom-Json
  $byId = @{}
  foreach ($p in $meta.packages) { $byId[$p.id] = $p }
  $nodes = @{}
  foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }

  # Walk normal (non-dev, non-build) edges from every binary that ships: the
  # desktop app, the CLI, and the two helpers the installer carries.
  $shipped = 'vitals-desktop', 'vitals-cli', 'vitals-sensors', 'vitals-watchdog'
  $roots = $meta.packages | Where-Object { $_.name -in $shipped } | ForEach-Object id
  if (@($roots).Count -ne $shipped.Count) { throw "expected $($shipped.Count) root crates, found $(@($roots).Count)" }
  $seen = [System.Collections.Generic.HashSet[string]]::new()
  $stack = [System.Collections.Generic.Stack[string]]::new()
  foreach ($r in $roots) { $stack.Push($r) }
  while ($stack.Count -gt 0) {
    $id = $stack.Pop()
    if (-not $seen.Add($id)) { continue }
    foreach ($dep in $nodes[$id].deps) {
      $normal = @($dep.dep_kinds | Where-Object { $null -eq $_.kind }).Count -gt 0
      if ($normal) { $stack.Push($dep.pkg) }
    }
  }
  $crates = foreach ($id in $seen) {
    $p = $byId[$id]
    if ($p.source -eq $null) { continue } # workspace members are ours
    [pscustomobject]@{ Name = $p.name; Version = $p.version; License = ($p.license ?? 'see repository'); Url = ($p.repository ?? "https://crates.io/crates/$($p.name)") }
  }
  $crates = $crates | Sort-Object Name, Version

  # `...` takes the workspace packages the webview bundles (@vitals/ui pulls
  # radix-ui, clsx, tailwind-merge, class-variance-authority) with it.
  $npmRaw = pnpm --filter '@vitals/desktop...' licenses list --prod --json 2>$null | Out-String
  $npm = @()
  if ($npmRaw.Trim()) {
    $parsed = $npmRaw | ConvertFrom-Json
    foreach ($lic in $parsed.PSObject.Properties) {
      foreach ($pkg in $lic.Value) {
        foreach ($v in @($pkg.versions)) {
          $npm += [pscustomobject]@{ Name = $pkg.name; Version = $v; License = $lic.Name; Url = ($pkg.homepage ?? "https://www.npmjs.com/package/$($pkg.name)") }
        }
      }
    }
  }
  $npm = $npm | Where-Object { $_.Name -notlike '@vitals/*' } | Sort-Object Name, Version
} finally {
  Pop-Location
}

# Every SPDX id named by any row, so each gets its text once.
function Get-LicenceIds([string]$expr) {
  $expr -replace '/', ' OR ' -split '[\s()]+' |
    Where-Object { $_ -and $_ -notin 'OR', 'AND', 'WITH' } |
    ForEach-Object { $_.Trim() }
}
$ids = @(@($crates) + @($npm) | ForEach-Object { Get-LicenceIds $_.License }) | Sort-Object -Unique
$missing = @($ids | Where-Object { -not (Test-Path -LiteralPath (Join-Path $licenceDir "$_.txt")) })
if ($missing.Count -gt 0) {
  throw "no licence text for: $($missing -join ', '). Add licences/<id>.txt (the SPDX text) after reading it."
}
function Read-Text([string]$path) {
  ([IO.File]::ReadAllText($path).Replace("`r`n", "`n") -split "`n" | ForEach-Object { $_.TrimEnd() }) -join "`n"
}

$sb = [System.Text.StringBuilder]::new()
[void]$sb.AppendLine('# Third-party notices')
[void]$sb.AppendLine()
[void]$sb.AppendLine('Vitals is MIT-licensed (see [LICENSE](LICENSE)). It is built on the open-source')
[void]$sb.AppendLine('components below, each under its own licence. Generated by')
[void]$sb.AppendLine('`scripts/third-party-notices.ps1` from the lockfiles; do not edit by hand.')
[void]$sb.AppendLine()
[void]$sb.AppendLine('The full text of every licence named below is at the end of this file. Each')
[void]$sb.AppendLine('component''s own copyright notice is in its source, linked from its row.')
[void]$sb.AppendLine('Where a component offers a choice of licences ("OR"), Vitals uses it under any')
[void]$sb.AppendLine('one of them; all of their texts are included.')
[void]$sb.AppendLine()
[void]$sb.AppendLine('Microsoft Edge WebView2 is a component of Windows, installed and licensed by')
[void]$sb.AppendLine('Microsoft; Vitals does not redistribute it.')
[void]$sb.AppendLine()
[void]$sb.AppendLine('The optional sensors service downloads PawnIO (`PawnIO_setup.exe`, from')
[void]$sb.AppendLine('[github.com/namazso/PawnIO.Setup](https://github.com/namazso/PawnIO.Setup)) when the')
[void]$sb.AppendLine('user installs it; Vitals does not redistribute the PawnIO driver.')
[void]$sb.AppendLine()
[void]$sb.AppendLine('## Embedded in the sensors helper (vitals-sensors.exe)')
[void]$sb.AppendLine()
[void]$sb.AppendLine('| Component | Version | Licence | Copyright |')
[void]$sb.AppendLine('| --- | --- | --- | --- |')
[void]$sb.AppendLine("| [$($pawnioModules.Name)]($($pawnioModules.Url)) | $($pawnioModules.Version) | $($pawnioModules.License) | $($pawnioModules.Copyright) |")
[void]$sb.AppendLine()
[void]$sb.AppendLine('The signed `IntelMSR`, `AMDFamily17` and `LpcIO` module binaries are embedded')
[void]$sb.AppendLine('unmodified. Their complete source is at the link above; the licence text is')
[void]$sb.AppendLine('below and is also installed as `sensors\PawnIO-Modules-COPYING.txt`.')
[void]$sb.AppendLine()
foreach ($section in @(@{ Title = "Rust crates ($(@($crates).Count))"; Rows = $crates }, @{ Title = "JavaScript packages ($(@($npm).Count))"; Rows = $npm })) {
  [void]$sb.AppendLine("## $($section.Title)")
  [void]$sb.AppendLine()
  [void]$sb.AppendLine('| Component | Version | Licence |')
  [void]$sb.AppendLine('| --- | --- | --- |')
  foreach ($r in $section.Rows) {
    $lic = ($r.License -replace '\|', '\|') -replace '/', ' OR '
    [void]$sb.AppendLine("| [$($r.Name)]($($r.Url)) | $($r.Version) | $lic |")
  }
  [void]$sb.AppendLine()
}
[void]$sb.AppendLine('## Licence texts')
[void]$sb.AppendLine()
foreach ($id in $ids) {
  [void]$sb.AppendLine("### $id")
  [void]$sb.AppendLine()
  [void]$sb.AppendLine('```text')
  [void]$sb.AppendLine((Read-Text (Join-Path $licenceDir "$id.txt")).Trim("`n"))
  [void]$sb.AppendLine('```')
  [void]$sb.AppendLine()
}
[void]$sb.AppendLine("### $($pawnioModules.License) ($($pawnioModules.Name))")
[void]$sb.AppendLine()
[void]$sb.AppendLine('```text')
[void]$sb.AppendLine((Read-Text (Join-Path $root $pawnioModules.Text)).Trim("`n"))
[void]$sb.AppendLine('```')
$text = $sb.ToString().Replace("`r`n", "`n").TrimEnd() + "`n"

if ($Check) {
  $current = if (Test-Path $out) { [IO.File]::ReadAllText($out).Replace("`r`n", "`n") } else { '' }
  if ($current -ne $text) { throw 'THIRD_PARTY_NOTICES.md is stale: run scripts/third-party-notices.ps1' }
  Write-Host 'THIRD_PARTY_NOTICES.md is current'
  exit 0
}
[IO.File]::WriteAllText($out, $text, [Text.UTF8Encoding]::new($false))
Write-Host "wrote $out  ($(@($crates).Count) crates, $(@($npm).Count) npm packages)"
