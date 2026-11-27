<#
.SYNOPSIS
    Runs every quality gate CI runs, locally, as fast as the machine allows.

.DESCRIPTION
    Exists so "it passes on my machine" means the same thing as "it passes in
    CI". Each gate reports pass/fail and the script exits non-zero if any
    failed, so it is usable from a pre-push hook.

    Three lanes run at once, because they share nothing:

      * checks - formatting and contract drift; seconds, and they fail
        fastest, so their result is printed first.
      * rust   - clippy, tests, bindings, perf budget. Sequential inside the
        lane: every cargo command takes the same target-directory lock, so
        running two at once only makes the second wait.
      * ts     - one `turbo run typecheck lint test`, which runs the three
        tasks across all packages in parallel and replays any whose inputs
        have not changed, then the bundle and its size budget.

    Measured 2026-09-28 on the 32-thread dev machine: typecheck 16.9 s cold
    and 3.4 s cached; lint 83.8 s cold and 1.5 s cached. Running them one
    after the other through `pnpm typecheck; pnpm lint; pnpm test` paid the
    cold price for every task that had not changed.

    The Windows lanes can run on the devcloud workspace rather than here (see
    docs/devcloud.md). `-Where auto`, the default, submits each one as its own
    agentq cloud job and runs it here only when that submission is refused;
    `AGENTQ_WHERE=local` is the opt-out. A cloud lane is this same script
    invoked inside the job as `-Lane <name> -Where local`, so every gate is
    still defined exactly once, and it prints one `LANE_RESULT` line per gate
    for the summary table, with the workspace that answered as its last field -
    the same gate runs here and on the devcloud, and a pass from a workspace
    whose toolchain regressed must not read like a pass from this machine. The
    workstation is shared with other agents, so the
    default spends its CPU elsewhere; measured 2026-11-25: the four Windows
    lanes cost this machine nothing and finish in about the time the Rust lane
    alone took locally.

.EXAMPLE
    pwsh -NoProfile -File scripts/verify.ps1
    pwsh -NoProfile -File scripts/verify.ps1 -SkipBuild -SkipPerf
    pwsh -NoProfile -File scripts/verify.ps1 -Force   # ignore the turbo cache
    pwsh -NoProfile -File scripts/verify.ps1 -Where local            # everything here
    pwsh -NoProfile -File scripts/verify.ps1 -Lane rust              # re-run one red lane
#>
[CmdletBinding()]
param(
    # Skip the production bundle build, which is the slowest TS gate.
    [switch]$SkipBuild,

    # Skip the release-build performance budget, which needs an optimised
    # compile and so is slow from cold.
    [switch]$SkipPerf,

    # Re-run every turbo task instead of replaying cached results. For when
    # you suspect the cache, not for everyday use: turbo's key already covers
    # the package's files, its dependencies' files and the root configs.
    [switch]$Force,

    # Linux clippy, CI's exact arguments, on the devcloud Linux workspace
    # (agentq run -Where cloud -Ws codai). `auto` (default) runs it when
    # agentq is installed and reports a skip otherwise; `on` fails when it
    # cannot run; `off` skips it. CI's Linux leg caught Windows-only dead
    # code three times that no local Windows gate could see (QA2-4).
    [ValidateSet('auto', 'on', 'off')][string]$LinuxClippy = 'auto',

    # Which lanes to run. `all` runs every lane and lets each Windows-capable
    # one go to the devcloud; naming one lane is what a cloud job does to
    # itself, and what you want when a single lane is red.
    [ValidateSet('all', 'checks', 'rust', 'ts', 'linux', 'android')][string[]]$Lane = @('all'),

    # Where the lanes run. `cloud` submits them to the devcloud workspace and
    # runs a lane here only when agentq refuses the submission; `local` is the
    # old behaviour; `auto` (default) prefers the cloud when agentq can reach
    # it and this process does not already hold the build ticket.
    [ValidateSet('local', 'cloud', 'auto')][string]$Where = 'auto',

    # The devcloud workspace for the Windows lanes (docs/devcloud.md).
    [string]$Ws = 'vitals-win',

    # Whether the Android artefact lane may run on the devcloud. On, because the
    # workspace now has the JDK 21 and SDK 37 the lane needs (provisioned by
    # scripts/ci/provision-cloud-workspace.ps1) and the lane was measured there on
    # 2026-11-25: assemble 56 s, five APKs read, zero failures. `off` is the
    # escape hatch for a workspace that has not been provisioned: without the SDK
    # the lane's -Build path would answer "nothing to check", and a gate that
    # checked nothing must not pass.
    [ValidateSet('auto', 'on', 'off')][string]$AndroidCloud = 'on',

    # Passed by a caller that knows the artefact lane must be able to answer
    # here: a missing SDK or a missing APK is then a failure rather than the
    # skip this script allows on a machine that has no Android tooling. The
    # devcloud submission sets it; a human never needs to.
    [switch]$RequireAndroid
)

$ErrorActionPreference = 'Continue'

# A devcloud job narrates itself from inside. The runner publishes a job's output
# only when one of its children speaks, and a lane captures its gate's output into
# a record instead of printing it — so from outside, a gate that is working and a
# gate that is wedged look identical: no lines, low CPU, no network. No-op outside
# a job (the helper checks DEVCLOUD_JOB_ID itself), so local runs are unaffected.
. (Join-Path $PSScriptRoot 'lib/devcloud-progress.ps1')
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# The repository declares what each lane needs in `.devcloud/profile.json`
# (schema and validator: E:\gh\vmui\infra\devcloud\profile) so that placement does
# not have to know this repository. Placement reads the toolchains and the machine
# class; this script is a lane's only executor, so it reads the one field that
# changes what happens here — how long the lane may take. The gate deadline used to
# be the constant 30 minutes below, which is under what two of the five lanes
# declare (rust 60, android 40): a lane killed by a number nobody chose reports
# nothing about the gate that was still working.
$laneProfile = $null
$profileFile = Join-Path $root '.devcloud/profile.json'
if (Test-Path $profileFile) {
    try {
        $laneProfile = (Get-Content $profileFile -Raw | ConvertFrom-Json).lanes
    } catch {
        Write-Host "  note: .devcloud/profile.json ignored ($($_.Exception.Message)); lanes use their fallback budget" -ForegroundColor Yellow
    }
}
function Get-LaneBudget {
    param([string]$Name, [int]$Fallback = 30)
    if ($laneProfile) {
        $mine = $laneProfile.PSObject.Properties[$Name]
        if ($mine -and $mine.Value.timeoutMin) { return [int]$mine.Value.timeoutMin }
    }
    return $Fallback
}

# A cloud workspace keeps its toolchains outside the repository — the job's own
# shell predates them, and a process does not see a variable added after it
# started. scripts/ci/provision-cloud-workspace.ps1 writes this file for that
# case; on a machine with its own JDK and SDK it is simply absent.
if (-not $env:ANDROID_HOME) {
    $toolEnv = if ($env:DEVCLOUD_ENV) { $env:DEVCLOUD_ENV } else { 'C:\devcloud-tools\env.ps1' }
    if (Test-Path $toolEnv) { . $toolEnv }
}

$runEveryLane = 'all' -in $Lane

# Which lanes this invocation runs. A cloud job calls itself with one lane, so
# every gate below is defined once and the two placements cannot drift.
function Test-Lane([string]$Name) { $runEveryLane -or ($Name -in $Lane) }

# One line per gate, printed by a lane running on its own: the caller rebuilds
# the summary table from these, so a gate that failed remotely arrives with its
# name and its seconds rather than a single opaque failure.
#
# Invariant, because the two ends are two machines and not necessarily two
# cultures: the cloud workspace formats seconds as `57.1` and this workstation as
# `146,7`, and a caller parsing the line with its own culture either throws or
# silently reads 146. Format and parse are the same contract, so both name the
# culture instead of inheriting one.
$invariant = [System.Globalization.CultureInfo]::InvariantCulture

# Where a gate ran travels with its result. The same gate runs here and on the
# devcloud, so a "pass" from a workspace whose toolchain silently regressed must
# not read like a "pass" from this machine - before this field, "which machine
# answered?" was only answerable by reading the log around the line (S26-13).
# A gate that reached a workspace of its own says so in $Record.Ws (the Linux
# clippy gate goes to codai from a lane that runs here); otherwise the job's own
# environment decides, and the caller passes -Ws down to the child lane so the
# child cannot report a workspace it was not given.
function Get-Placement($Record) {
    if ($Record.PSObject.Properties['Ws'] -and $Record.Ws) { return "$($Record.Ws)" }
    if ($env:DEVCLOUD_JOB_ID) { return "cloud:$Ws" }
    'local'
}
function Format-LaneResult($Record, [string]$LaneName) {
    'LANE_RESULT|{0}|{1}|{2}|{3}|{4}' -f $LaneName, $Record.Gate, $(if ($Record.Passed) { 'pass' } else { 'fail' }), $Record.Seconds.ToString('0.0', $invariant), (Get-Placement $Record)
}

# A gate is a name and a line of PowerShell; a lane runs its gates in order and
# returns one record per gate. The lanes run as thread jobs, so a record carries
# the gate's output back rather than printing into an interleaved console.
#
# Each gate runs as its own process with its output in files, and the lane waits
# for that process — never for its pipes. A pipeline of a native command ends on
# EOF, and a toolchain daemon outlives the gate that started it while keeping the
# handles it inherited from its ancestors (a Gradle daemon is the one that taught
# this lesson), so the reader waits for the daemon instead of the gate. Measured
# here 2026-11-25: a gate that finished in 0.2 s left a 40 s grandchild behind and
# `& <scriptblock> | ForEach-Object` returned after 41 s. On the devcloud that is
# a job that never reports: the runner waits for this process, this process waits
# for a daemon that is doing nothing, and the caller sits at "busy" with low CPU
# and no log lines. The gate's own build already writes to a file for the same
# reason; this is the same rule one level up.
#
# The epilogue keeps the old `$LASTEXITCODE` semantics: a script ending in a
# native command must report that command's code, and a gate that ends in `exit 1`
# must report 1 — which inside a thread job used to mean "end the runspace".
$runLane = {
    param([string]$Root, [object[]]$Gates, [int]$GateTimeoutMin = 30)
    Set-Location $Root
    . (Join-Path $Root 'scripts/lib/devcloud-progress.ps1')
    $logs = Join-Path ([IO.Path]::GetTempPath()) 'vitals-lane-gates'
    if (-not (Test-Path $logs)) { $null = New-Item -ItemType Directory -Force -Path $logs | Out-Null }
    foreach ($gate in $Gates) {
        Write-DevcloudStage "gate '$($gate.Name)': starting"
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $slug = $gate.Name -replace '[^\w]', '_'
        $scriptFile = Join-Path $logs "gate-$slug.ps1"
        $outFile = Join-Path $logs "gate-$slug.out"
        $errFile = Join-Path $logs "gate-$slug.err"
        $body = $gate.Script + "`n" + 'if ($LASTEXITCODE -is [int] -and $LASTEXITCODE -ne 0) { exit $LASTEXITCODE }' + "`n" + 'exit 0'
        [IO.File]::WriteAllText($scriptFile, $body, [Text.UTF8Encoding]::new($false))
        $p = Start-Process pwsh -ArgumentList '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', ('"{0}"' -f $scriptFile) `
            -WorkingDirectory $Root -WindowStyle Hidden -PassThru -RedirectStandardOutput $outFile -RedirectStandardError $errFile
        # A gate that never returns has to become a red gate in bounded time, and
        # it has to stop holding the lane. The assemble inside the android gate
        # has its own deadline; this one only catches a gate that is stuck before
        # it gets that far.
        $timedOut = -not $p.WaitForExit($GateTimeoutMin * 60 * 1000)
        if ($timedOut) {
            $null = & taskkill /T /F /PID $p.Id 2>&1
            Start-Sleep -Seconds 1
        }
        $ok = (-not $timedOut) -and ($p.ExitCode -eq 0)
        $output = @()
        foreach ($f in $outFile, $errFile) {
            if (Test-Path -LiteralPath $f) { $output += @(Get-Content -LiteralPath $f) }
        }
        $sw.Stop()
        $secs = [math]::Round($sw.Elapsed.TotalSeconds, 1)
        Write-DevcloudStage "gate '$($gate.Name)': $(if ($timedOut) { "timed out after ${secs}s" } else { "exit $($p.ExitCode) in ${secs}s" })"
        [pscustomobject]@{
            Gate    = $gate.Name
            Passed  = $ok
            Seconds = $secs
            Output  = $output
            Ws      = $(if ($gate.ContainsKey('Ws')) { $gate.Ws } else { $null })
        }
    }
}

# Script blocks as text: a thread job cannot share the caller's scope, and
# rebuilding them from text keeps each gate's variables inside the job.
$forceFlag = if ($Force) { '--force' } else { '' }

$checks = @(
    @{ Name = 'rust: format'; Script = 'cargo fmt --all -- --check' }
    @{ Name = 'contracts: no drift'; Script = 'pwsh -NoProfile -File scripts/check-drift.ps1' }
    # check-drift.ps1 owns the CSV's own shape (arity, duplicate id, status
    # vocabulary); this owns the agreement between the two tracker files, which
    # nothing else reads. Both are file reads, so the fast lane still fails fast.
    @{ Name = 'tracker: conventions'; Script = 'pwsh -NoProfile -File scripts/check-tracker.ps1' }
    # The Apple listings are held one version behind on purpose; a ripple pass
    # that moves them would advertise a version App Store Connect is not
    # reviewing. Pure file reads, so it costs the fast lane nothing.
    @{ Name = 'apple: listing held'; Script = 'node scripts/check-apple-hold.mjs' }
    @{ Name = 'ts: format'; Script = 'pnpm format:check' }
    # CI's supply-chain job runs this; without it here a new crate passed
    # every local gate and failed on GitHub (S12-36, 2026-09-28).
    @{ Name = 'notices: current'; Script = 'pwsh -NoProfile -File scripts/third-party-notices.ps1 -Check' }
    # Same licence/source policy CI enforces (deny.toml). A missing tool is a
    # failure with the install command, never a silent pass.
    @{ Name = 'deps: licences + sources'; Script = 'if (-not (Get-Command cargo-deny -ErrorAction SilentlyContinue)) { Write-Host "cargo-deny missing: cargo binstall cargo-deny"; exit 1 }; cargo deny --all-features check licenses bans sources' }
)

$rust = @(
    @{ Name = 'rust: clippy'; Script = 'cargo clippy --workspace --all-targets -- -D warnings' }
    @{ Name = 'rust: tests'; Script = 'cargo test --workspace' }
    @{ Name = 'bindings: no drift'; Script = 'cargo test -p vitals-core --features ts --quiet | Out-Null; if ($LASTEXITCODE -eq 0) { git diff --exit-code -- packages/protocol/src/generated }' }
)
if (-not $SkipPerf) {
    # Must be a release build: debug is several times slower and would
    # measure the compiler's lack of optimisation rather than our code.
    $rust += @{ Name = 'rust: perf budget'; Script = 'cargo test -p vitals-win --release --test overhead -- --nocapture' }
}
# Last in the lane, after every cargo command has written what it will.
# Pruning here is what keeps target/ from growing without bound; the gate
# then fails only on what pruning cannot fix (a real regression).
$rust += @{ Name = 'rust: target size budget'; Script = 'pwsh -NoProfile -File scripts/check-target-size.ps1 -Prune' }

$ts = @(
    @{ Name = 'ts: typecheck + lint + tests'; Script = "pnpm exec turbo run typecheck lint test --output-logs=errors-only $forceFlag" }
)
if (-not $SkipBuild) {
    $ts += @{ Name = 'build: frontend bundle'; Script = 'pnpm --filter @vitals/desktop build:vite' }
    # Only meaningful after a build. The installer half needs a `tauri
    # build` (minutes), so it is checked only when an installer newer than
    # the last change to the desktop sources exists: a stale installer would
    # measure yesterday's code. CI's build job always measures it.
    $installerFresh = 'pwsh -NoProfile -File scripts/check-size.ps1 -SkipInstaller'
    $nsis = Get-ChildItem (Join-Path $root 'target/release/bundle/nsis') -Filter '*-setup.exe' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    $lastChange = git log -1 --format=%cI -- apps/desktop crates Cargo.lock 2>$null
    if ($nsis -and $lastChange -and $nsis.LastWriteTime -gt [datetime]$lastChange) {
        $installerFresh = 'pwsh -NoProfile -File scripts/check-size.ps1'
        if (Test-Lane 'ts') { Write-Host "  note: measuring the installer too ($($nsis.Name), built $($nsis.LastWriteTime))" -ForegroundColor DarkGray }
    } else {
        if (Test-Lane 'ts') { Write-Host '  note: no installer newer than the last desktop change; installer size is checked by CI (build a fresh one with tauri build to check it here)' -ForegroundColor DarkGray }
    }
    $ts += @{ Name = 'build: size budget'; Script = $installerFresh }
}

# CI's Linux clippy leg, on the devcloud Linux workspace. Its own lane: it
# waits on the network and a remote compile, not on this machine.
$linux = @()
$agentq = Join-Path $HOME '.copilot\bin\agentq.ps1'
if ((Test-Lane 'linux') -and ($LinuxClippy -ne 'off')) {
    if (Test-Path $agentq) {
        # Single-quoted inner command: bash receives CI's argv unchanged, and
        # agentq cannot strip a `--` that is inside one argument.
        $cmd = "`$env:AGENTQ_CLOUD_STRICT='1'; & '$agentq' run -Resource build -Where cloud -Ws codai -Purpose 'verify.ps1 linux clippy' '--' bash -lc 'cargo clippy --workspace --lib --bins --tests --benches --exclude vitals-win -- -D warnings'"
        $linux += @{ Name = 'rust: clippy (linux, devcloud)'; Script = $cmd; Ws = 'cloud:codai' }
    } elseif ($LinuxClippy -eq 'on') {
        $linux += @{ Name = 'rust: clippy (linux, devcloud)'; Script = "Write-Host 'agentq is not installed: cannot reach the devcloud Linux workspace'; exit 1"; Ws = 'cloud:codai' }
    } else {
        Write-Host '  note: Linux clippy skipped (no agentq); CI runs it' -ForegroundColor DarkGray
    }
}

$total = [System.Diagnostics.Stopwatch]::StartNew()
# Android availability: Play hides the app from any device that lacks a
# feature the manifest declares REQUIRED, and a permission implies one. The
# answer exists only in a built APK, so this lane assembles the three debug
# APKs and then reads them. Its own lane on purpose: it builds, and the checks
# lane exists to fail in seconds.
$androidLane = @()
if (Test-Lane 'android') {
    if (Test-Path (Join-Path $root 'apps/android/gradlew.bat')) {
        $androidGate = 'pwsh -NoProfile -File scripts/check-android-features.ps1 -Build -RequireBuilt'
        if ($RequireAndroid) { $androidGate += ' -Strict' }
        $androidLane += @{ Name = 'android: features'; Script = $androidGate }
        # Reads the APKs the gate above just assembled: the debug artefacts
        # carry the same native libraries as the Play bundles, and a 64-bit one
        # that is not 16 KB aligned cannot be loaded on a 16 KB-page device.
        # -RequireBuilt is right here for the same reason it is above — a build
        # has just run, so "found nothing" has to be red rather than green.
        $android16k = 'pwsh -NoProfile -File scripts/check-android-16k.ps1 -RequireBuilt'
        if ($RequireAndroid) { $android16k += ' -Strict' }
        $androidLane += @{ Name = 'android: 16 KB alignment'; Script = $android16k }
    } else {
        Write-Host '  note: no gradlew.bat here (no Android SDK); the artefact gate is skipped, and CI runs it' -ForegroundColor DarkGray
    }
}

# ── Where the lanes run ───────────────────────────────────────────────
#
# This machine is shared with other agents, so the Windows lanes prefer the
# devcloud. Each one is submitted as its own agentq job, which means the
# resource queue serialises it against whatever else is building this repo,
# and agentq's own fallback (exit 4) runs it here when the cloud refuses.
# `AGENTQ_WHERE=local` turns the whole thing off. A lane inside the job calls
# this script with -Lane <name> -Where local, so the gate definitions above
# remain the only ones.
$cloudReachable = Test-Path $agentq
if ($Where -eq 'local' -or $env:AGENTQ_WHERE -eq 'local') { $cloudReachable = $false }
# A job holds its own build ticket; re-entering the queue for the same resource
# would wait on itself for ever.
if ($env:AGENTQ_HELD -split ',' -contains 'build:vitals') { $cloudReachable = $false }
# The same reasoning, without depending on how the job was submitted: the agent
# runs one job at a time, so a cloud job that submits another cloud job waits for
# a slot that cannot free until it finishes. A job is identified by its own
# environment, and this is the only signal that cannot be missing.
if ($env:DEVCLOUD_JOB_ID) { $cloudReachable = $false }
if (($Where -eq 'cloud') -and -not $cloudReachable) {
    Write-Host '  note: -Where cloud asked for, but this process holds the build ticket or agentq is missing; running here' -ForegroundColor Yellow
}

$cloudLanes = @()
if ($cloudReachable) {
    foreach ($name in 'checks', 'rust', 'ts', 'android') {
        if (-not (Test-Lane $name)) { continue }
        if (($name -eq 'android') -and ($AndroidCloud -eq 'off')) {
                    Write-Host '  note: the android lane runs here — -AndroidCloud off was asked for (the cloud workspace has the SDK; see docs/devcloud.md)' -ForegroundColor DarkGray
            continue
        }
        $cloudLanes += $name
    }
}

$results = [System.Collections.Generic.List[object]]::new()

foreach ($name in $cloudLanes) {
    $budget = Get-LaneBudget $name
    # -Ws is passed down so the child lane reports the workspace it actually ran on rather than its
    # own default, which would be right only while every lane shared one workspace.
    $remote = @('pwsh', '-NoProfile', '-File', 'scripts/verify.ps1', '-Lane', $name, '-Where', 'local', '-Ws', $Ws, '-LinuxClippy', 'off')
    if ($SkipBuild) { $remote += '-SkipBuild' }
    if ($SkipPerf) { $remote += '-SkipPerf' }
    if ($Force) { $remote += '-Force' }
    if ($name -eq 'android') { $remote += '-RequireAndroid' }
    # How long this caller waits for the job: the lane's own budget plus the queue
    # and the workspace sync around it. It has to exceed the lane budget, or the
    # wait dies first and the caller reports an opaque `(devcloud exit 3)` row for a
    # lane whose own deadline never had the chance to name the gate that was slow.
    # Measured 2026-11-25 on this queue: a claim took 2.7 minutes for one lane.
    $call = @('run', '-Resource', 'build', '-Where', 'cloud', '-Ws', $Ws, '-TimeoutMin', ($budget + 20), '-Purpose', "verify.ps1 $name lane on the devcloud", '--') + $remote
    # The deadline is printed because it comes from the repository's own declaration
    # now: a log that shows the budget is the difference between a lane that stopped
    # and a lane that this number stopped.
    Write-Host "  → $name on $Ws (budget $budget min)" -ForegroundColor Cyan
    Write-DevcloudStage "cloud lane '$name': submitted"
    $stream = [System.Collections.Generic.List[string]]::new()
    # A child process, not `& $agentq`, and for a measured reason: the cloud
    # client narrates through [Console]::Out.WriteLine, which is the process's
    # own stdout and not a PowerShell stream. Called in-process, its log never
    # reaches this pipeline — and neither did any LANE_RESULT line, so all four
    # cloud lanes collapsed into "(devcloud exit N)" rows on the first all-cloud
    # run (2026-11-25: five rows, four of them opaque, while the log held six
    # LANE_RESULT lines for the checks lane alone). Spawning the shell is what
    # makes it capturable.
    $shell = (Get-Process -Id $PID).Path
    if (-not $shell) { $shell = 'pwsh' }
    & $shell -NoProfile -NoLogo -File $agentq @call 2>&1 | ForEach-Object { $stream.Add("$_"); Write-Host "    $_" }
    $code = $LASTEXITCODE
    Write-DevcloudStage "cloud lane '$name': exit $code"
    $reported = 0
    # The stream is split before it is matched, because what arrives is chunks
    # and not lines: the client prints a job's log as it grows, and the last
    # chunk is often the whole log in one string. Matched line by line against
    # whole chunks, `^LANE_RESULT` finds nothing and every cloud lane collapses
    # into one opaque row — which is the failure this table exists to prevent,
    # measured 2026-11-25 on the first all-cloud run: five rows, four of them
    # "checks (devcloud exit 0)", with the six gates of that lane nowhere.
    foreach ($line in @($stream | ForEach-Object { $_ -split "`r?`n" } | Where-Object { $_ -match '^LANE_RESULT\|' })) {
        $f = $line.Split('|')
        $passed = $f[3] -eq 'pass'
        $colour = if ($passed) { 'Green' } else { 'Red' }
        $seconds = [double]::Parse($f[4], $invariant)
        # Field 5 is the placement. A stream from a lane that predates the field has four of them and
        # is reported as the cloud lane it came from rather than as unknown.
        $place = if ($f.Count -gt 5) { $f[5] } else { "cloud:$Ws" }
        # Formatted, not left to `-f`'s own culture: the remote lane prints its
        # own table with a dot, and on a Romanian machine this line rendered the
        # same number as `1,1s` beside it. Two renderings of one measurement in
        # one log is how a number stops being greppable.
        $shown = $seconds.ToString('0.0', $invariant)
        Write-Host ('  {0}  {1,-30} {2,6}s   [{3} {4}]' -f $f[3].ToUpperInvariant(), $f[2], $shown, $name, $place) -ForegroundColor $colour
        # The whole remote log rather than a tail: the summary below reads the
        # annotations out of it, and a failing lane was already streamed here.
        $results.Add([pscustomobject]@{ Gate = $f[2]; Passed = $passed; Seconds = $seconds; Output = @($stream); Streamed = $true; Ws = $place })
        $reported++
    }
    if ($reported -eq 0) {
        # The lane never reached a gate: a failed sync, a missing toolchain, a
        # timeout. One row saying so is honest; no row for a lane that was
        # asked for is not.
        $results.Add([pscustomobject]@{
                Gate     = "$name (devcloud exit $code)"
                Passed   = $code -eq 0
                Seconds  = 0
                Output   = @($stream)
                Streamed = $true
            })
    }
}

$pending = [System.Collections.Generic.List[object]]::new()
$jobs = [ordered]@{}
if ((Test-Lane 'checks') -and ('checks' -notin $cloudLanes)) { $jobs['checks'] = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $checks, (Get-LaneBudget 'checks') }
if ((Test-Lane 'rust') -and ('rust' -notin $cloudLanes)) { $jobs['rust'] = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $rust, (Get-LaneBudget 'rust') }
if ((Test-Lane 'ts') -and ('ts' -notin $cloudLanes)) { $jobs['ts'] = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $ts, (Get-LaneBudget 'ts') }
if ($linux.Count) { $jobs['linux'] = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $linux, (Get-LaneBudget 'linux') }
if ($androidLane.Count -and ('android' -notin $cloudLanes)) { $jobs['android'] = Start-ThreadJob -ScriptBlock $runLane -ArgumentList $root, $androidLane, (Get-LaneBudget 'android') }
$jobs.Values | ForEach-Object { $pending.Add($_) }
if ($jobs.Count) {
    Write-Host "  running here: $(($jobs.Keys | ForEach-Object { "$_ $(Get-LaneBudget $_)m" }) -join ', ')" -ForegroundColor DarkGray
}

# Print each lane as it finishes, so a formatting failure is on screen in
# seconds rather than after the four-minute Rust lane.
while ($pending.Count -gt 0) {
    $done = Wait-Job -Job $pending -Any
    # Not `$lane`: PowerShell is case-insensitive, so that name IS the [string[]]
    # parameter and the key would be wrapped in an array and print as
    # `System.String[]` in the line below.
    [string]$laneName = ($jobs.GetEnumerator() | Where-Object { $_.Value.Id -eq $done.Id }).Key
    foreach ($r in @(Receive-Job -Job $done)) {
        $colour = if ($r.Passed) { 'Green' } else { 'Red' }
        $status = if ($r.Passed) { 'PASS' } else { 'FAIL' }
        $gatePlace = Get-Placement $r
        Write-Host ('  {0}  {1,-30} {2,6:N1}s   [{3} {4}]' -f $status, $r.Gate, $r.Seconds, $laneName, $gatePlace) -ForegroundColor $colour
        # A lane that ran somewhere else says what it found in a line the
        # caller can read back; this is that line, and the summary above is
        # still the answer for a human.
        if (-not $runEveryLane) { Write-Host (Format-LaneResult $r $laneName) }
        # A streamed lane (the devcloud) has already printed its output here;
        # a local one kept it in the record and this is where it appears.
        if ((-not $r.Passed) -and (-not $r.Streamed)) {
            $r.Output | Select-Object -Last 30 | ForEach-Object { Write-Host "      $_" -ForegroundColor Red }
        }
        if ($r.Gate -eq 'rust: tests' -and $r.Passed) {
            $passed = ($r.Output | Select-String -Pattern '^test result: ok\. (\d+) passed' |
                    ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
            Write-Host "        $passed Rust tests passed" -ForegroundColor DarkGray
        }
        if ($r.Gate -eq 'rust: perf budget') {
            $r.Output | Select-String -Pattern 'median' | ForEach-Object { Write-Host "        $($_.Line.Trim())" -ForegroundColor DarkGray }
        }
        if ($r.Gate -like 'ts: typecheck*') {
            $r.Output | Select-String -Pattern '^\s*(Tasks|Cached):' | ForEach-Object { Write-Host "        $($_.Line.Trim())" -ForegroundColor DarkGray }
        }
        # A failing ts gate printed only the last thirty lines, and those are the
        # vitest summary: the one line a reader needs — the name of the failing
        # test and the assertion it made — sits just above them and was cut off.
        # The release run of 2026-11-27 failed this gate and the log could not say
        # which of the 1519 tests had failed, which cost a whole investigation
        # cycle; this is that answer, printed where the failure is reported.
        if ((-not $r.Passed) -and ($r.Gate -like 'ts: typecheck*')) {
            $r.Output |
                Select-String -Pattern 'FAIL |AssertionError|expected\b.*\bto be\b|\.test\.tsx?:\d+' |
                Select-Object -First 12 |
                ForEach-Object { Write-Host "        $($_.Line.Trim())" -ForegroundColor Red }
        }
        # The artefact gate's own one-line result, so whether it had any APK to
        # check is visible in the run and not only inside the gate.
        if ($r.Gate -eq 'android: features') {
            # Anchored on the script's own prefix, not on '^android-features:':
            # every line it prints starts with 'check-android-features:', so the
            # old pattern matched nothing and the summary showed the verdict
            # without any of the reasons behind it.
            $r.Output | Select-String -Pattern 'check-android-features:' | ForEach-Object { Write-Host "        $($_.Line.Trim())" -ForegroundColor DarkGray }
        }
        if ($r.Gate -eq 'android: 16 KB alignment') {
            $r.Output | Select-String -Pattern 'check-android-16k:' | ForEach-Object { Write-Host "        $($_.Line.Trim())" -ForegroundColor DarkGray }
        }
        $results.Add($r)
    }
    Remove-Job -Job $done
    [void]$pending.Remove($done)
}
$total.Stop()
Write-DevcloudStage "wrapper: every lane finished, $($results.Count) gate result(s)"

Write-Host ''
Write-Host ('=' * 64) -ForegroundColor DarkGray
# The workspace is a column rather than a footnote: with the Windows lanes on
# the devcloud and the Linux clippy lane on a different workspace again, "it
# passed" is only half the answer and the machine is the other half.
$results | Select-Object Gate, Passed, Seconds, @{ Name = 'Where'; Expression = { Get-Placement $_ } } | Format-Table -AutoSize

$failed = @($results | Where-Object { -not $_.Passed })
$wall = [math]::Round($total.Elapsed.TotalSeconds, 1)
$serial = [math]::Round(($results | Measure-Object Seconds -Sum).Sum, 1)
if ($failed.Count -gt 0) {
    Write-Host "$($failed.Count) gate(s) FAILED: $($failed.Gate -join ', ')  (wall ${wall}s, serial ${serial}s)" -ForegroundColor Red
    Write-DevcloudStage "wrapper: exiting 1"
    exit 1
}

Write-Host "All $($results.Count) gates passed in ${wall}s (${serial}s if run one after another)." -ForegroundColor Green
Write-DevcloudStage "wrapper: exiting 0"
exit 0
