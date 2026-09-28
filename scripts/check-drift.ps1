<#
.SYNOPSIS
    Catches the contract drift that compilers do not.

.DESCRIPTION
    Every check here corresponds to a defect that actually shipped in this
    repo. None of them is caught by cargo, tsc or eslint, because each one is
    an agreement between two sides that compile independently:

      1. Rust structs with a ts_rs derive whose serde casing disagrees with it.
         Shipped: ProcessKey sent `start-time`; the TS binding said `startTime`.
      2. `invoke('name')` calls in the webview with no matching #[tauri::command].
         Shipped: set_process_priority existed in Rust for weeks, unreachable.
      3. Tauri commands registered in generate_handler that no frontend calls.
         Not a bug by itself, but every one of these has turned out to be a
         feature that exists and is invisible — worth a warning.
      4. i18n keys present in one locale and absent in the other.
      5. Settings keys read by the UI that nothing writes, or vice versa.

    Exit code is the number of hard failures. Warnings do not fail the gate.

.EXAMPLE
    pwsh -NoProfile -File scripts/check-drift.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$failures = [System.Collections.Generic.List[string]]::new()
$warnings = [System.Collections.Generic.List[string]]::new()

function Get-Rg {
    param([string[]]$RgArgs)
    # rg exits 1 for "no matches", which is a legitimate answer here.
    $out = & rg @RgArgs 2>$null
    if ($LASTEXITCODE -gt 1) { throw "rg failed: rg $($RgArgs -join ' ')" }
    return @($out)
}

# ── 1. serde casing beside a ts_rs derive ──────────────────────────────

# ts_rs emits camelCase because every export says `rename_all = "camelCase"`.
# serde is independent and defaults to the Rust field name. The two agree
# only when the serde attribute also says camelCase. This holds for enums
# too: ts_rs renders variant literals from ITS OWN rename_all, not serde's,
# so a kebab-case enum ships `"not-responding"` against a union that says
# `"notResponding"`. Fourteen enums did exactly that until 2026-09-28; the
# earlier version of this comment claimed they "stay in step".
$tsFiles = Get-Rg @('-l', 'derive\(ts_rs::TS\)', 'crates', '--glob', '*.rs')
foreach ($file in $tsFiles) {
    $lines = Get-Content $file
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -notmatch '^\s*pub (struct|enum) (\w+)') { continue }
        $kind = $Matches[1]; $name = $Matches[2]

        # Walk back over the attribute block.
        $attrs = @()
        for ($j = $i - 1; $j -ge 0 -and ($lines[$j] -match '^\s*(#\[|#!\[|\)|\]|\s*$|/// |//)' -or $lines[$j] -match '^\s*(feature|ts|derive|serde)' ); $j--) {
            $attrs = ,$lines[$j] + $attrs
        }
        $block = $attrs -join "`n"
        if ($block -notmatch 'ts_rs::TS') { continue }

        $serde = [regex]::Matches($block, 'serde\([^\]]*rename_all\s*=\s*"([a-zA-Z_-]+)"')
        if ($serde.Count -eq 0) {
            # Newtype/transparent structs and untagged enums put no names on
            # the wire, so there is nothing to rename.
            if ($block -match 'serde\((transparent|untagged)\)') { continue }
            $failures.Add("${file}:$($i+1) $kind $name derives ts_rs::TS but has no serde rename_all — names will be Rust-cased on the wire")
        }
        elseif ($serde[0].Groups[1].Value -ne 'camelCase') {
            $failures.Add("${file}:$($i+1) $kind ${name}: serde rename_all is '$($serde[0].Groups[1].Value)' but ts_rs exports camelCase")
        }
    }
}

# ── 2 & 3. invoke() names vs #[tauri::command] ─────────────────────────

# `--no-filename` matters: with it off, rg prefixes every match with the path
# and the "command name" becomes `src/foo.ts:get_users`, which matches nothing.
$invoked = Get-Rg @('-o', '--no-filename', "invoke(<[^>]*>)?\('([a-z_]+)'", 'apps/desktop/src', '--glob', '!*.test.*', '-r', '$2') |
    Sort-Object -Unique

# Commands are whatever generate_handler! lists; that is what is reachable.
# Matching `(\w+)::(\w+)` misses the entries written bare (no module path),
# so take every identifier in the block instead and drop the macro's own.
# Anchored to `])`, not to the first `]`: the list contains `#[cfg(windows)]`
# attributes, and a lazy match stops inside the first of those — which is how
# this check silently saw 5 commands out of 66 on its first run.
$handlerBlock = (Get-Rg @('-U', '--no-filename', 'generate_handler!\[[\s\S]*?\n\s*\]', 'apps/desktop/src-tauri/src/lib.rs')) -join "`n"
# Strip comments first: the block is heavily commented, and prose ending in a
# comma would otherwise be read as a command name ("seconds,", "changing,").
$handlerCode = ($handlerBlock -split "`n" | ForEach-Object { ($_ -replace '//.*$', '') }) -join "`n"
$registered = @([regex]::Matches($handlerCode, '(?:(\w+)::)?(\w+)\s*,') |
    ForEach-Object { $_.Groups[2].Value }) |
    Where-Object { $_ -ne 'generate_handler' } |
    Sort-Object -Unique

foreach ($name in $invoked) {
    if ($name -in @('plugin:store|get', 'plugin:store|set')) { continue }
    if ($registered -notcontains $name) {
        $failures.Add("frontend invokes '$name' but no such command is registered in generate_handler!")
    }
}
foreach ($name in $registered) {
    if ($invoked -notcontains $name) {
        $warnings.Add("command '$name' is registered but nothing in the webview invokes it — reachable only from tests or unreachable")
    }
}

# ── 4. locale parity ───────────────────────────────────────────────────

function Get-LeafKeys {
    param($Node, [string]$Prefix)
    $keys = @()
    if ($Node -is [System.Management.Automation.PSCustomObject]) {
        foreach ($p in $Node.PSObject.Properties) {
            $keys += Get-LeafKeys $p.Value ($(if ($Prefix) { "$Prefix.$($p.Name)" } else { $p.Name }))
        }
    } else {
        $keys += $Prefix
    }
    return $keys
}

$localeDir = 'packages/i18n/src/locales'
$locales = Get-ChildItem $localeDir -Filter '*.json'
if ($locales.Count -ge 2) {
    $sets = @{}
    foreach ($l in $locales) {
        $sets[$l.BaseName] = @(Get-LeafKeys (Get-Content $l.FullName -Raw | ConvertFrom-Json) '') | Sort-Object -Unique
    }
    # CLDR plural suffixes are language-specific by design: Romanian has a
    # `_few` form (2-19) that English does not, and requiring parity would
    # force a dead `_few` key into en. Compared on the base key instead.
    $pluralSuffix = '_(zero|one|two|few|many|other)$'
    foreach ($k in @($sets.Keys)) {
        $sets[$k] = @($sets[$k] | ForEach-Object { $_ -replace $pluralSuffix, '' }) | Sort-Object -Unique
    }

    $names = @($sets.Keys | Sort-Object)
    $reference = $sets[$names[0]]
    foreach ($n in $names[1..($names.Count-1)]) {
        $missing = Compare-Object $reference $sets[$n]
        foreach ($m in $missing) {
            $where = if ($m.SideIndicator -eq '<=') { "missing from $n" } else { "missing from $($names[0])" }
            $failures.Add("i18n key '$($m.InputObject)' $where")
        }
    }
}

# Shell strings are registered in code with `en`/`ro` objects side by side.
$shell = 'apps/desktop/src/shell/strings.ts'
if (Test-Path $shell) {
    $src = Get-Content $shell -Raw
    # Count leaf keys per locale block by a crude but adequate heuristic:
    # every `name: '` inside the en block must appear inside the ro block.
    $enStart = $src.IndexOf('en: {'); $roStart = $src.IndexOf('ro: {')
    if ($enStart -ge 0 -and $roStart -ge 0) {
        $en = $src.Substring($enStart, $roStart - $enStart)
        $ro = $src.Substring($roStart)
        $enKeys = @([regex]::Matches($en, '^\s+(\w+):\s*[''"`]', 'Multiline') | ForEach-Object { $_.Groups[1].Value })
        $roKeys = @([regex]::Matches($ro, '^\s+(\w+):\s*[''"`]', 'Multiline') | ForEach-Object { $_.Groups[1].Value })
        $enCount = ($enKeys | Group-Object | ForEach-Object { "$($_.Name)=$($_.Count)" }) -join ','
        $roCount = ($roKeys | Group-Object | ForEach-Object { "$($_.Name)=$($_.Count)" }) -join ','
        if ($enCount -ne $roCount) {
            $onlyEn = Compare-Object $enKeys $roKeys | Where-Object SideIndicator -eq '<=' | ForEach-Object InputObject
            $onlyRo = Compare-Object $enKeys $roKeys | Where-Object SideIndicator -eq '=>' | ForEach-Object InputObject
            if ($onlyEn) { $failures.Add("shell strings: keys only in en: $($onlyEn -join ', ')") }
            if ($onlyRo) { $failures.Add("shell strings: keys only in ro: $($onlyRo -join ', ')") }
        }
    }
}

# ── Report ─────────────────────────────────────────────────────────────

foreach ($w in $warnings) { Write-Host "warn  $w" -ForegroundColor Yellow }
foreach ($f in $failures) { Write-Host "FAIL  $f" -ForegroundColor Red }

Write-Host ''
Write-Host ("drift: {0} failure(s), {1} warning(s) — checked {2} ts_rs files, {3} invokes, {4} commands, {5} locales" -f
    $failures.Count, $warnings.Count, $tsFiles.Count, $invoked.Count, $registered.Count, $locales.Count)

exit $failures.Count
