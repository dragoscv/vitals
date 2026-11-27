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
        8. A docs/tracker.csv row with the wrong number of columns, a
            duplicated id, or a status outside the vocabulary. Shipped: two
            rows pasted together with a duplicated status field, which a CSV
            reader turned into garbage while every reader of the table saw a
            plausible row.
        9. An Android manifest that declares a hardware feature REQUIRED.
            Android marks a feature required when a permission implies it
            (ACCESS_WIFI_STATE -> android.hardware.wifi, CAMERA ->
            android.hardware.camera), and Play then hides the app from the
            devices that lack it. Shipped: the phone build required Wi-Fi and a
            camera after 1.3.0, which hid it from Ethernet-only phones and
            camera-less Chromebooks.
               10. A version literal copied into a packaging script, a workflow or a
                script's usage example that no longer matches the current version
                — the number a human pastes into a build. Shipped twice: an MSIX
                  example left at 1.2.0.0 after the bump to 1.3.0, and the
                  workflow_dispatch command in msix.yml left at the previous
                  version after the bump to 1.3.1.
             11. A shipped executable that Windows draws with its generic application
                        icon because nothing embedded one. It compiles, every test passes,
                        and the mark is missing only in a dialog that draws it — Explorer,
                        Task Manager, and the firewall prompt `vitals serve` raises.
                        Shipped: 1.5.0's CLI, watchdog and sensors helper, all three.

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
#
# Two spellings call a command: `invoke('name')`, and the typed wrapper the
# agents feature uses, `call<Type>('name')`. The generic argument is required
# by the second pattern on purpose — without it an unrelated `call(...)` could
# be read as a command name and the forward check below would invent a
# failure. Missing the wrapper made five real commands (the whole Kodwen
# sign-in flow) read as unreachable, and a warning that is always there is a
# warning everybody learns to skip.
$invoked = @(
    Get-Rg @('-o', '--no-filename', "invoke(<[^>]*>)?\('([a-z_]+)'", 'apps/desktop/src', '--glob', '!*.test.*', '-r', '$2')
    Get-Rg @('-o', '--no-filename', "call<[^>]*>\('([a-z_]+)'", 'apps/desktop/src', '--glob', '!*.test.*', '-r', '$1')
) | Sort-Object -Unique

# Commands are whatever generate_handler! lists; that is what is reachable.
# Matching `(\w+)::(\w+)` misses the entries written bare (no module path),
# so take every identifier in the block instead and drop the macro's own.
# Anchored to `])`, not to the first `]`: the list contains `#[cfg(windows)]`
# attributes, and a lazy match stops inside the first of those — which is how
# this check silently saw 5 commands out of 66 on its first run.
$handlerBlock = (Get-Rg @('-U', '--no-filename', 'generate_handler!\[[\s\S]*?\n\s*\]', 'apps/desktop/src-tauri/src/lib.rs')) -join "`n"
# Strip comments first: the block is heavily commented, and prose ending in a
# comma would otherwise be read as a command name ("seconds,", "changing,").
# Attributes too: `#[cfg(any(windows, target_os = "macos"))]` holds a comma.
$handlerCode = ($handlerBlock -split "`n" | ForEach-Object { ($_ -replace '//.*$', '' -replace '#\[[^\]]*\]', '') }) -join "`n"
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
    # Gated to platforms that each define it (windows, or windows + macOS
    # with a macOS body re-exported into the same module) - not portable.
    if ($prev -match '^#\[cfg\((windows|any\(windows, target_os = "macos"\))\)\]$') { continue }
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

# ── 4b. Tizen locale parity ────────────────────────────────────────────

# The Samsung TV app keeps its own two locale files under apps/tizen, which
# the check above does not reach (it reads packages/i18n). The same guarantee
# holds there: a key present in one language and absent in the other renders
# the raw key on the TV, and no compiler notices. CLDR plural suffixes are
# exempt, exactly as above. The app also has a vitest parity test; this gate
# catches the drift in two seconds instead of at the next build.
$tizenLocaleDir = 'apps/tizen/src/locales'
$tizenLocales = @(Get-ChildItem $tizenLocaleDir -Filter '*.json' -ErrorAction SilentlyContinue)
if ($tizenLocales.Count -ge 2) {
    $tizenSets = @{}
    foreach ($l in $tizenLocales) {
        $tizenSets[$l.BaseName] = @(Get-LeafKeys (Get-Content $l.FullName -Raw | ConvertFrom-Json) '') | Sort-Object -Unique
    }
    foreach ($k in @($tizenSets.Keys)) {
        $tizenSets[$k] = @($tizenSets[$k] | ForEach-Object { $_ -replace $pluralSuffix, '' }) | Sort-Object -Unique
    }
    $names = @($tizenSets.Keys | Sort-Object)
    $reference = $tizenSets[$names[0]]
    foreach ($n in $names[1..($names.Count-1)]) {
        $missing = Compare-Object $reference $tizenSets[$n]
        foreach ($m in $missing) {
            $where = if ($m.SideIndicator -eq '<=') { "missing from $n" } else { "missing from $($names[0])" }
            $failures.Add("tizen i18n key '$($m.InputObject)' $where")
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
# One Rust writer, one copy per native client, one decoding test each.
$contractClients = @(
    @{ Name = 'android'; Dir = 'apps/android/core/src/test/resources/contract'; Test = 'apps/android/core/src/test/kotlin/app/vitals/core/ContractTest.kt' },
    @{ Name = 'ios'; Dir = 'apps/ios/VitalsCore/Tests/VitalsCoreTests/Contract'; Test = 'apps/ios/VitalsCore/Tests/VitalsCoreTests/VitalsCoreTests.swift' }
)
if ($Regenerate -and (Test-Path $contractTest)) {
    & cargo test -p vitals-server --test android_contract --quiet
    if ($LASTEXITCODE -ne 0) {
        $failures.Add("cargo test android_contract exited $LASTEXITCODE; the fixtures were not regenerated")
    }
}
foreach ($client in $contractClients) {
$contractDir = $client.Dir
$kotlinTest = $client.Test
$who = $client.Name
if ((Test-Path $contractTest) -and (Test-Path $contractDir)) {
    if ($Regenerate) {
        $changed = @(& git status --porcelain -- $contractDir)
        foreach ($line in $changed) {
            $failures.Add("$who contract fixture changed when regenerated (commit it, and check $kotlinTest still decodes it): $line")
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
        if ($committed -notcontains $name) { $failures.Add("$who contract: $contractTest writes $name but $contractDir has no committed copy") }
        if ($decoded -notcontains $name) { $failures.Add("$who contract: $name is written by Rust but $kotlinTest never decodes it") }
    }
    foreach ($name in $committed) {
        if ($written -notcontains $name) { $failures.Add("$who contract: $contractDir/$name is committed but no longer written by $contractTest (stale shape)") }
    }
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
    'set_tray_metric',
    'set_alert_rules',
    'set_alert_channels',
    'alert_channel_status',
    'get_launch_options', 'plan_terminate_process', 'plan_suspend_process', 'list_benchmarks',
    'get_distribution', 'get_app_history',
    'set_retention_days', 'get_pending_update', 'set_auto_update', 'log_webview', 'diagnose',
    'get_update_status',
    'get_lan_status', 'create_pairing', 'create_pairing_code', 'cancel_pairing_code',
    'pairing_code_status', 'revoke_pairing', 'revoke_all_pairings',
    # Flip a cancellation flag the worker polls.
    'cancel_dev_cleanup_scan', 'cancel_storage_scan', 'cancel_cleanup_search',
    'cancel_drive_cleanup_analysis',
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

# ── iPhone strings: generated catalog current, every key resolvable ────
# apps/ios/Vitals/Localizable.xcstrings is generated from the Android
# strings.xml (EN+RO) plus apps/ios/strings-ios.json, so the phone apps say
# the same thing. A stale catalog, or an L("key") the catalog lacks, renders
# the raw key on screen and no compiler notices.

$iosCatalog = Join-Path $root 'apps/ios/Vitals/Localizable.xcstrings'
if (Test-Path $iosCatalog) {
    $out = & node (Join-Path $root 'scripts/ios-strings.mjs') --check 2>&1
    if ($LASTEXITCODE -ne 0) { $failures.Add("ios: $out") }
    $keys = (Get-Content $iosCatalog -Raw | ConvertFrom-Json -AsHashtable).strings.Keys
    $used = rg -o --no-filename -r '$1' 'L\("([A-Za-z0-9_.]+)"' (Join-Path $root 'apps/ios/Vitals') | Sort-Object -Unique
    foreach ($k in $used) {
        if ($keys -notcontains $k) { $failures.Add("ios: L(`"$k`") has no entry in Localizable.xcstrings (add it to apps/ios/strings-ios.json)") }
    }
}

# ── 8. tracker.csv shape ───────────────────────────────────────────────

# The tracker is the only record of what shipped and what is left, and it is
# hand-edited a few rows at a time. S23-06 and S23-10 were each pasted
# together with two status fields, so a CSV reader produced a row whose
# columns shifted by one and nothing noticed: the slice table and the file
# disagreed. Column count is the cheapest invariant that catches it, because
# Import-Csv renames a surplus field H8 (and leaves a missing one absent), so
# a row of the wrong arity is visible here instead of only to whoever opens
# the file next.
$tracker = Join-Path $root 'docs/tracker.csv'
if (Test-Path $tracker) {
    $trackerColumns = 8   # id,slice,category,title,detail,decision,status,verification
    $trackerStatuses = @('todo', 'in-progress', 'done', 'blocked', 'dropped')
    $trackerRows = @(Import-Csv $tracker)
    if ($trackerRows.Count -eq 0) { $failures.Add('docs/tracker.csv has a header but no rows') }

    # Field counts come from a real reader, not from Import-Csv: a row with
    # too FEW fields is padded to the header's width and looks correct, which
    # is exactly how a pasted-together row survived (the mutation that this
    # check exists for is a short row, not a long one). Quotes are honoured,
    # because the `detail` field holds commas and newlines.
    $fieldCounts = [System.Collections.Generic.List[int]]::new()
    $fields = 1
    $inQuotes = $false
    $text = Get-Content -LiteralPath $tracker -Raw
    for ($i = 0; $i -lt $text.Length; $i++) {
        $c = $text[$i]
        if ($inQuotes) {
            if ($c -eq '"') {
                if ($i + 1 -lt $text.Length -and $text[$i + 1] -eq '"') { $i = $i + 1 } else { $inQuotes = $false }
            }
            continue
        }
        if ($c -eq '"') { $inQuotes = $true }
        elseif ($c -eq ',') { $fields = $fields + 1 }
        elseif ($c -eq "`n") { $fieldCounts.Add($fields); $fields = 1 }
    }
    if ($fieldCounts.Count -eq 0 -or $fields -gt 1) { $fieldCounts.Add($fields) }

    # Record 0 is the header; row N of Import-Csv is record N+1.
    for ($r = 0; $r -lt $fieldCounts.Count; $r++) {
        if ($fieldCounts[$r] -eq $trackerColumns) { continue }
        $label = if ($r -eq 0) { 'header' } else { "row '$($trackerRows[$r - 1].id)'" }
        $failures.Add("docs/tracker.csv: $label has $($fieldCounts[$r]) columns, expected $trackerColumns — a field is missing or pasted twice")
    }

    $trackerIds = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($row in $trackerRows) {
        if (-not $trackerIds.Add($row.id)) {
            $failures.Add("docs/tracker.csv: id '$($row.id)' appears more than once")
        }
        if ($trackerStatuses -notcontains $row.status) {
            $failures.Add("docs/tracker.csv: row '$($row.id)' has status '$($row.status)', not one of $($trackerStatuses -join '/')")
        }
    }
}

# ── 9. Android required features in merged manifests ───────────────────

# Android marks a hardware feature REQUIRED when a permission implies it, and
# Play then hides the app from every device that lacks it. A manifest that
# declares <uses-feature> without android:required="false" is therefore an
# availability bug that no compiler sees. The packaged proof is
# scripts/check-android-features.ps1, which reads the built APK — the implied
# features are added at packaging and appear in no manifest at all. This is the
# cheap half: it reads any merged manifest already on disk and fails when the
# DECLARED required set leaves the allowlist, so an explicit required="true"
# (or a declaration that forgot the attribute) is caught in two seconds with no
# build. A module that has never been built is skipped, not failed: verify.ps1
# must pass on a checkout that has not assembled Android.

$androidAllowFile = Join-Path $root 'scripts/android-features.json'
if (Test-Path $androidAllowFile) {
    $androidAllow = (Get-Content $androidAllowFile -Raw | ConvertFrom-Json).modules

    # Read the attributes by local name so this does not depend on how
    # PowerShell's XML adapter exposes the android: prefix.
    function Get-AndroidAttribute($node, $local) {
        foreach ($a in $node.Attributes) { if ($a.LocalName -eq $local) { return $a.Value } }
        return $null
    }

    foreach ($module in @($androidAllow.PSObject.Properties.Name)) {
        $mergedRoots = @(
            "apps/android/$module/build/intermediates/merged_manifests"
            "apps/android/$module/build/intermediates/merged_manifest"
        )
        $manifests = @(foreach ($r in $mergedRoots) {
            if (Test-Path $r) { Get-ChildItem $r -Recurse -Filter 'AndroidManifest.xml' -ErrorAction SilentlyContinue }
        })
        if ($manifests.Count -eq 0) { continue }

        $allowed = @($androidAllow.$module)
        foreach ($m in $manifests) {
            [xml]$merged = Get-Content -LiteralPath $m.FullName -Raw
            $relative = $m.FullName.Substring($root.Length).TrimStart('\', '/')
            $required = @($merged.manifest.'uses-feature' |
                Where-Object { $_ } |
                ForEach-Object {
                    $feature = Get-AndroidAttribute $_ 'name'
                    $needed = Get-AndroidAttribute $_ 'required'
                    if ($feature -and $needed -ne 'false') { $feature }
                } | Sort-Object -Unique)
            foreach ($f in $required) {
                if ($allowed -notcontains $f) {
                    $failures.Add("${relative}: declares '$f' as required, which is not on the allowlist for module '$module' (scripts/android-features.json); declare it android:required=""false"" if the app can run without it")
                }
            }
        }
    }
}

# ── 10. the product version in packaging and CI examples ───────────────

# The version is copied by hand into usage examples and command snippets, and
# a release that bumps it tends to miss one: the literal still builds, still
# passes every test, and is read only by the person who copy-pastes it into a
# package that then carries the old number. It has happened twice. An MSIX
# example in packaging/msix/build-and-upload.ps1 sat at 1.2.0.0 through the
# bump to 1.3.0, and the workflow_dispatch command in
# .github/workflows/msix.yml survived the bump to 1.3.1 — the v1.3.1 release
# commit updated three of the five four-part examples and missed the other
# two. No compiler and no test sees a number that is only ever pasted by a
# human.
#
# Source of truth: the `[workspace.package] version` in Cargo.toml — the
# first value scripts/version.ps1 reads, the value a release tag is cut from,
# and the value version.ps1 -Check then enforces in every manifest. It is
# read here directly rather than by running version.ps1, because this gate
# runs in the pre-commit hook and must stay in seconds.
#
# Scope, chosen so the check does not cry wolf (a guard that fires on healthy
# text is one people learn to ignore):
#
#   * Only a FOUR-part literal is a candidate. The Store version is the one
#     number that must equal the product version exactly — an MSIX version
#     has four parts, the last always 0 — so it is the one worth pinning.
#     The three-part literals in the release tooling (scripts/release-notes.ps1
#     -Version 0.9.0-beta.1, the winget README's `$v = '0.9.0-beta.1'`) are
#     that tool's own subject: it accepts any version, the repo's placeholder
#     example is deliberately a prerelease, and the Store rule does not apply
#     to them.
#   * Only a literal BOUND to a version slot counts: the value of a `-Version`
#     argument or of a `version=` input (the msix.yml workflow_dispatch). That
#     is what keeps this off everything that merely looks like a version,
#     deliberately and by name:
#       - a dependency or tool pin: `toolchain: 1.99.0`, `npm@11.20.0`,
#         `KOMAC_VERSION: 2.16.0`, third-party-notices.ps1's `Version =
#         '0.2.11'` — they name OTHER software, which may differ in every part.
#       - a URL: PawnIO's `.../releases/download/2.2.0/PawnIO_setup.exe`.
#       - a platform version: AppxManifest.template.xml's
#         MinVersion=10.0.19041.0 and MaxVersionTested=10.0.26100.0,
#         `makepri /pv 10.0.0`, the Chrome `140.0.0.0` in the store-listing
#         user agent — Windows' and Chrome's, not ours.
#       - an action SHA or an `@vN` ref in a workflow.
#       - a historical note: "New in 1.3.0" in packaging/msix/listing, the
#         `-From 1.1.0 -To 1.1.1` of the update proof — those record what
#         shipped, so they must NOT move with a bump.
#       - a format illustration: `e.g. 0.9.0.0` in build-msix.ps1's parameter
#         help and `(e.g. 1.3.0.0)` in the msix.yml input description show the
#         SHAPE, not the current value, so neither is read as one.
#   * A four-part literal must end in `.0`. The current Store version is
#     derived as `<core>.0`, so a wrong last part is also an agreement
#     failure; the rule is stated here because it is the Store's own
#     constraint and reads clearly in the failure message.

$cargoToml = Join-Path $root 'Cargo.toml'
$currentVersion = [regex]::Match([IO.File]::ReadAllText($cargoToml), '(?m)^\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $currentVersion) {
    $failures.Add('Cargo.toml: no [workspace.package] version to check packaging and CI examples against')
} else {
    # The four-part Store form: the core version with any prerelease/build
    # suffix dropped, plus the trailing .0 the Store requires.
    $storeVersion = (($currentVersion -split '[-+]')[0]) + '.0'

    # `-Version <v>` (a CLI parameter) and `version=<v>` (the workflow input);
    # the lookbehind keeps `toolVersion=` and `-VersionInfo` out. Four parts
    # only, by the scope note above.
    $boundVersion = [regex]'(?<![A-Za-z0-9_])(?:-Version\s+|version=)(\d+\.\d+\.\d+\.\d+)'

    # An extension allowlist, not a denylist: packaging/ carries PNGs, and a
    # guess at which files are text would eventually read a binary as one.
    $versionExtensions = @('.ps1', '.psm1', '.psd1', '.yml', '.yaml', '.json', '.xml', '.nuspec', '.md', '.mjs', '.js', '.sh')
    $versionRoots = @('packaging', 'scripts', '.github/workflows')
    $versionFiles = @(Get-ChildItem -Path ($versionRoots | ForEach-Object { Join-Path $root $_ }) -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $versionExtensions -contains $_.Extension })
    foreach ($file in $versionFiles) {
        $relative = ([IO.Path]::GetRelativePath($root, $file.FullName)) -replace '\\', '/'
        $lines = Get-Content -LiteralPath $file.FullName
        for ($i = 0; $i -lt $lines.Count; $i++) {
            foreach ($m in $boundVersion.Matches($lines[$i])) {
                $found = $m.Groups[1].Value
                if ($found -ne $storeVersion) {
                    $failures.Add("${relative}:$($i+1) version $found is not the current Store version $storeVersion — update it with the version bump")
                }
            }
        }
    }
}

# ── 11. the shipped console binaries carry the brand icon ──────────────

# rustc writes no resource section into a binary, so a program that never runs
# a resource compiler is drawn with Windows' generic application icon. The
# desktop app the user double-clicks is covered by tauri-build; the executable
# below it is not, and neither are the two helpers the installer ships beside
# it. Nothing in the source says so — the crate compiles, every test passes, and
# the defect is visible only to a user who opens a dialog that draws the icon.
# 1.5.0 shipped all three of them, and the first report was the firewall prompt
# `vitals serve` raises when it binds a socket.
#
# The three build scripts are byte-identical on purpose. Each copy carries two
# facts — the icon path and the product name — and a copy that drifts is a
# binary that shows no mark or names the wrong product, silently. Comparing
# three hashes here is cheaper than noticing that in a release.
#
# The name is checked against bundle.productName in tauri.conf.json, the same
# string the desktop binary carries, and the icon against bundle.icon, so all
# four executables are pinned to one artefact and one name. The publisher and
# the copyright are held the same way, against bundle.publisher and
# bundle.copyright: tauri-build fills both into the desktop binary from that
# file, and a console binary has no config of its own to read, so the words are
# written into the resource by each build script and compared here. An empty
# publisher is what the firewall prompt shows as a blank name.

$iconRelative = 'apps/desktop/src-tauri/icons/icon.ico'
$iconFile = Join-Path $root ($iconRelative -replace '/', [IO.Path]::DirectorySeparatorChar)
$iconConfFile = Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json'
# tauri.conf.json lists the file relative to src-tauri (bundle.icon), so the
# leaf is what both forms have in common.
$iconLeaf = [IO.Path]::GetFileName($iconRelative)

if (-not (Test-Path -LiteralPath $iconFile)) {
    $failures.Add("${iconRelative}: the brand icon the shipped executables embed is missing — brand/scripts/build.mjs regenerates it")
}

$productName = ''
$publisher = ''
$copyright = ''
if (Test-Path -LiteralPath $iconConfFile) {
    $confText = [IO.File]::ReadAllText($iconConfFile)
    $productName = [regex]::Match($confText, '"productName"\s*:\s*"([^"]+)"').Groups[1].Value
    if (-not $productName) {
        $failures.Add('tauri.conf.json: no bundle productName, so the name the firewall prompt shows cannot be checked')
    }
    $publisher = [regex]::Match($confText, '"publisher"\s*:\s*"([^"]+)"').Groups[1].Value
    if (-not $publisher) {
        $failures.Add('tauri.conf.json: no bundle publisher, so the name the firewall prompt and the file Properties dialog show cannot be checked')
    }
    $copyright = [regex]::Match($confText, '"copyright"\s*:\s*"([^"]+)"').Groups[1].Value
    if (-not $copyright) {
        $failures.Add('tauri.conf.json: no bundle copyright, so the copyright the shipped binaries carry cannot be checked')
    }
    if ($confText -cnotmatch ('"[^"]*' + [regex]::Escape($iconLeaf) + '"')) {
        $failures.Add("tauri.conf.json: bundle.icon does not list $iconLeaf, so $iconRelative is not the artefact the desktop embeds")
    }
} else {
    $failures.Add('tauri.conf.json: missing, so the product name, the publisher and the icon the desktop embeds cannot be checked')
}

$iconBinaries = @('apps/cli', 'apps/watchdog', 'crates/vitals-sensors')
$iconBuildScriptHashes = [ordered]@{}
foreach ($iconPackage in $iconBinaries) {
    $iconBuildScript = Join-Path $root "$iconPackage/build.rs"
    if (-not (Test-Path -LiteralPath $iconBuildScript)) {
        $failures.Add("${iconPackage}/build.rs: missing, so this executable ships with Windows' generic application icon (copy the body from crates/vitals-sensors/build.rs)")
        continue
    }
    $iconBuildSource = [IO.File]::ReadAllText($iconBuildScript)
    if ($iconBuildSource -cnotmatch [regex]::Escape($iconRelative)) {
        $failures.Add("${iconPackage}/build.rs: does not embed $iconRelative")
    }
    if ($productName -and ($iconBuildSource -cnotmatch [regex]::Escape('"' + $productName + '"'))) {
        $failures.Add("${iconPackage}/build.rs: does not name the product $productName, so the firewall prompt would show this crate's own name")
    }
    if ($publisher -and ($iconBuildSource -cnotmatch [regex]::Escape('"' + $publisher + '"'))) {
        $failures.Add("${iconPackage}/build.rs: does not set CompanyName to $publisher, so the firewall prompt shows no publisher at all")
    }
    if ($copyright -and ($iconBuildSource -cnotmatch [regex]::Escape('"' + $copyright + '"'))) {
        $failures.Add("${iconPackage}/build.rs: does not set LegalCopyright to '$copyright'")
    }
    $iconManifest = Join-Path $root "$iconPackage/Cargo.toml"
    if (Test-Path -LiteralPath $iconManifest) {
        if ([IO.File]::ReadAllText($iconManifest) -cnotmatch '(?ms)^\[build-dependencies\][\s\S]*?tauri-winres') {
            $failures.Add("${iconPackage}/Cargo.toml: [build-dependencies] does not declare tauri-winres, so build.rs cannot compile")
        }
    }
    $iconBuildScriptHashes[$iconPackage] = (Get-FileHash -LiteralPath $iconBuildScript -Algorithm SHA256).Hash
}
if (@($iconBuildScriptHashes.Values | Select-Object -Unique).Count -gt 1) {
    $failures.Add("the icon build scripts have drifted apart ($($iconBuildScriptHashes.Keys -join ', ')) — keep them byte-identical, they carry one fact each")
}

# ── Report ─────────────────────────────────────────────────────────────

foreach ($w in $warnings) { Write-Host "warn  $w" -ForegroundColor Yellow }
foreach ($f in $failures) { Write-Host "FAIL  $f" -ForegroundColor Red }

Write-Host ''
Write-Host ("drift: {0} failure(s), {1} warning(s) — checked {2} ts_rs files, {3} invokes, {4} commands, {5} locales, {6} shipped icon binaries" -f
    $failures.Count, $warnings.Count, $tsFiles.Count, $invoked.Count, $registered.Count, $locales.Count, $iconBinaries.Count)

exit $failures.Count
