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
        6. Android contract fixtures that differ from what the Rust test writes.
        7. A synchronous #[tauri::command] that is not on the allowlist of
            handlers known to be cheap. Tauri runs a plain `fn` command on the
            main thread: one slow OS call there freezes every window.

    Exit code is the number of hard failures. Warnings do not fail the gate.

.EXAMPLE
    pwsh -NoProfile -File scripts/check-drift.ps1

.EXAMPLE
    pwsh -NoProfile -File scripts/check-drift.ps1 -Regenerate
    Runs the Rust test that writes the Android contract fixtures first, so
    check 6 compares against freshly written output (a cargo build; not two
    seconds).
#>
[CmdletBinding()]
param(
    [switch]$Regenerate
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$failures = [System.Collections.Generic.List[string]]::new()
$warnings = [System.Collections.Generic.List[string]]::new()

# The real executable, resolved once. Without ripgrep on PATH, `& rg` did not
# fail: it recursed until PowerShell gave up with "call depth overflow" — the
# first CI run on a GitHub runner, which has no ripgrep, showed nothing more.
$rgExe = Get-Command rg -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $rgExe) {
    Write-Host 'check-drift needs ripgrep (rg) on PATH: winget install BurntSushi.ripgrep.MSVC, or apt-get install ripgrep' -ForegroundColor Red
    exit 2
}

function Get-Rg {
    param([string[]]$RgArgs)
    # rg exits 1 for "no matches", which is a legitimate answer here.
    $out = & $rgExe.Source @RgArgs 2>$null
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

# ── 3b. a Windows-only command registered for every platform ───────────

# A `#[cfg(windows)] #[tauri::command] fn x` listed in generate_handler!
# without its own `#[cfg(windows)]` compiles on Windows and fails every other
# target with E0433 `__tauri_command_name_x`. Every local gate runs on
# Windows, so only the Linux CI leg noticed (3f0588a -> cc0a647, 2026-09-29).
# Rule: an ungated handler entry must have a definition that is not
# preceded by `#[cfg(windows)]` (a non-Windows stub counts).
$srcRoot = 'apps/desktop/src-tauri/src'
$handlerLines = $handlerBlock -split "`n"
for ($i = 0; $i -lt $handlerLines.Count; $i++) {
    $line = ($handlerLines[$i] -replace '//.*$', '').Trim()
    if ($line -notmatch '^(?:(\w+)::)?(\w+)\s*,$') { continue }
    $module = $Matches[1]; $fn = $Matches[2]
    $prev = if ($i -gt 0) { ($handlerLines[$i - 1] -replace '//.*$', '').Trim() } else { '' }
    if ($prev -match '^#\[cfg\(windows\)\]$') { continue }
    $file = if ($module) { Join-Path $srcRoot "$module.rs" } else { Join-Path $srcRoot 'lib.rs' }
    if (-not (Test-Path $file)) { $file = Join-Path $srcRoot "$module/mod.rs" }
    if (-not (Test-Path $file)) { continue }
    $src = Get-Content $file
    $defs = @()
    for ($j = 0; $j -lt $src.Count; $j++) {
        if ($src[$j] -match "^\s*(pub(\([^)]*\))?\s+)?(async\s+)?fn\s+$fn\b") { $defs += $j }
    }
    if ($defs.Count -eq 0) { continue }
    $portable = $false
    foreach ($d in $defs) {
        # Walk up through the attribute block above the fn.
        $windowsOnly = $false
        for ($k = $d - 1; $k -ge 0 -and $src[$k].Trim() -match '^(#\[|///|//)'; $k--) {
            if ($src[$k].Trim() -eq '#[cfg(windows)]') { $windowsOnly = $true }
        }
        if (-not $windowsOnly) { $portable = $true }
    }
    if (-not $portable) {
        $failures.Add("'$fn' is #[cfg(windows)] in $file but registered in generate_handler! without #[cfg(windows)]: Linux and macOS fail E0433")
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

# ── 6. Android contract fixtures vs the Rust test that writes them ─────

# The Kotlin models are hand-written (ADR-0033), so the only thing tying
# them to Rust is the JSON `android_contract.rs` writes and ContractTest.kt
# decodes. Three ways that link silently breaks, all checked here without a
# build: a fixture the Rust test writes that is not committed, a committed
# fixture nothing writes any more (Kotlin keeps decoding a stale shape), and
# a fixture the Kotlin test never opens. With -Regenerate the Rust test runs
# first and any byte it changed fails the gate — that is the drift itself.
$contractTest = 'crates/vitals-server/tests/android_contract.rs'
$contractDir = 'apps/android/core/src/test/resources/contract'
$kotlinTest = 'apps/android/core/src/test/kotlin/app/vitals/core/ContractTest.kt'
if ((Test-Path $contractTest) -and (Test-Path $contractDir)) {
    if ($Regenerate) {
        & cargo test -p vitals-server --test android_contract --quiet
        if ($LASTEXITCODE -ne 0) {
            $failures.Add("cargo test android_contract exited $LASTEXITCODE; the fixtures were not regenerated")
        }
        $changed = @(& git status --porcelain -- $contractDir)
        foreach ($line in $changed) {
            $failures.Add("android contract fixture changed when regenerated (commit it, and check ContractTest.kt still decodes it): $line")
        }
    }

    $written = @([regex]::Matches((Get-Content $contractTest -Raw), 'write\(\s*"([\w.-]+\.json)"') |
        ForEach-Object { $_.Groups[1].Value }) | Sort-Object -Unique
    $committed = @(Get-ChildItem $contractDir -Filter '*.json' | ForEach-Object Name) | Sort-Object -Unique
    $decoded = @()
    if (Test-Path $kotlinTest) {
        $decoded = @([regex]::Matches((Get-Content $kotlinTest -Raw), '"([\w.-]+\.json)"') |
            ForEach-Object { $_.Groups[1].Value }) | Sort-Object -Unique
    }
    if ($written.Count -eq 0) {
        $failures.Add("$contractTest writes no fixture this check can see — the write(`"name.json`") pattern changed")
    }
    foreach ($name in $written) {
        if ($committed -notcontains $name) { $failures.Add("android contract: $contractTest writes $name but $contractDir has no committed copy") }
        if ($decoded -notcontains $name) { $failures.Add("android contract: $name is written by Rust but ContractTest.kt never decodes it") }
    }
    foreach ($name in $committed) {
        if ($written -notcontains $name) { $failures.Add("android contract: $contractDir/$name is committed but no longer written by $contractTest (stale shape)") }
    }
}

# ── 7. synchronous Tauri commands must be known-cheap ──────────────────

# Tauri runs a command declared as a plain `fn` on the main thread, the one
# that pumps every window's messages. A sync command that blocks for 200 ms
# freezes the UI for 200 ms; one that waits on a driver or a UAC prompt
# hangs it. `async fn` or `#[tauri::command(async)]` runs on the async
# runtime instead. New sync commands must be added here deliberately, with
# the reason they are cheap; anything else belongs off the main thread.
# Stubs under `#[cfg(not(windows))]` return Unsupported and are exempt.
$syncAllowed = @(
    # Reads or writes in-memory state behind a lock; no I/O.
    'get_capabilities', 'request_keyframe', 'get_alerts', 'set_alert_prefs', 'set_alert_strings',
    'set_close_to_tray', 'set_taskbar_load', 'set_tray_strings', 'set_sample_rate',
    'get_launch_options', 'plan_terminate_process', 'plan_suspend_process', 'list_benchmarks',
    'get_distribution', 'get_app_history', 'clear_app_history', 'set_history_enabled',
    'set_retention_days', 'get_pending_update', 'set_auto_update', 'log_webview', 'diagnose',
    'get_lan_status', 'create_pairing', 'create_pairing_code', 'cancel_pairing_code',
    'pairing_code_status', 'revoke_pairing', 'revoke_all_pairings',
    # Flip a cancellation flag the worker polls.
    'cancel_dev_cleanup_scan', 'cancel_storage_scan', 'cancel_cleanup_search',
    # Read an already-finished scan held in memory.
    'get_storage_children', 'get_storage_map',
    # Window management on the main thread is where it must run anyway.
    'quit_app', 'show_main_window', 'stop_lan_server',
    # One bounded Win32 call against one process or key (microseconds to a
    # few milliseconds); none of them waits on another process.
    'terminate_process', 'suspend_process', 'resume_process', 'set_process_priority',
    'set_process_affinity', 'get_efficiency_mode', 'set_efficiency_mode', 'get_executable_path',
    'bring_process_to_front', 'get_taskmgr_replacement', 'get_host_info',
    # Spawn a process and return without waiting for it.
    'open_file_location', 'show_file_properties', 'uninstall_app'
)
$syncSeen = [System.Collections.Generic.HashSet[string]]::new()
foreach ($file in Get-ChildItem 'apps/desktop/src-tauri/src' -Recurse -Filter '*.rs') {
    $src = Get-Content $file.FullName
    for ($i = 0; $i -lt $src.Count; $i++) {
        if ($src[$i] -notmatch '^\s*#\[tauri::command(\((?<opts>[^)]*)\))?\]') { continue }
        $asyncAttr = $Matches['opts'] -match '\basync\b'
        $stub = $false
        for ($j = $i + 1; $j -lt [math]::Min($i + 15, $src.Count); $j++) {
            if ($src[$j].Trim() -eq '#[cfg(not(windows))]') { $stub = $true }
            if ($src[$j] -match '^\s*(pub(\([^)]*\))?\s+)?(?<async>async\s+)?fn\s+(?<name>\w+)') {
                if (-not $asyncAttr -and -not $Matches['async'] -and -not $stub) {
                    $name = $Matches['name']
                    [void]$syncSeen.Add($name)
                    if ($syncAllowed -notcontains $name) {
                        $failures.Add("$($file.Name):$($j+1) '$name' is a synchronous #[tauri::command] (runs on the UI thread): make it async, or add it to `$syncAllowed in check-drift.ps1 with the reason it is cheap")
                    }
                }
                break
            }
        }
    }
}
foreach ($name in $syncAllowed) {
    if (-not $syncSeen.Contains($name)) {
        $failures.Add("check-drift.ps1 `$syncAllowed lists '$name', which is no longer a synchronous command: remove it so the list stays exact")
    }
}

# ── Rust toolchain: one version everywhere ─────────────────────────────
# rust-toolchain.toml pins the version for local builds; the workflows pass
# the same version to dtolnay/rust-toolchain, which does not read the file.
# A bump in one place only would give CI a different rustc (a cold cache and
# different lints) without anyone noticing.

$toolchainFile = Join-Path $root 'rust-toolchain.toml'
if (Test-Path $toolchainFile) {
    $pinned = [regex]::Match((Get-Content $toolchainFile -Raw), 'channel\s*=\s*"([^"]+)"').Groups[1].Value
    foreach ($wf in Get-ChildItem (Join-Path $root '.github/workflows') -Filter *.yml) {
        $lines = Get-Content $wf.FullName
        for ($i = 0; $i -lt $lines.Count; $i++) {
            if ($lines[$i] -notmatch 'dtolnay/rust-toolchain@') { continue }
            $window = $lines[($i + 1)..([math]::Min($i + 4, $lines.Count - 1))] -join "`n"
            $m = [regex]::Match($window, 'toolchain:\s*(\S+)')
            if (-not $m.Success) {
                $failures.Add("$($wf.Name):$($i+1) rust-toolchain step has no 'toolchain:' input; it would install floating stable, not $pinned")
            } elseif ($m.Groups[1].Value -ne $pinned) {
                $failures.Add("$($wf.Name):$($i+1) toolchain $($m.Groups[1].Value) differs from rust-toolchain.toml ($pinned)")
            }
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
