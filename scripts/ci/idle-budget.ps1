<#
.SYNOPSIS
    Measures what an installed Vitals costs while nobody is looking at it,
    and fails when it costs more than idle-budget.json allows (PERF-1).

.DESCRIPTION
    A system monitor runs all day on someone else's machine, so its idle
    cost is a feature, and like the installer size it regresses one careless
    timer at a time. Unit tests cannot see it: it is the sum of the sampler,
    the tray, the webview and every timer in the frontend.

    Two variants, each a fresh launch of the installed executable:

      - hidden   `vitals-desktop.exe --minimized`, what Start-with-Windows
                 runs: tray only, the window never shown.
      - visible  `vitals-desktop.exe`, the dashboard on screen.

    For each: wait -SettleSeconds (webview start-up and first paint are not
    idle), then -Windows consecutive windows of -WindowSeconds. In every
    window the process tree is the app plus every descendant (the
    msedgewebview2.exe processes, found by walking Win32_Process
    ParentProcessId), and three numbers are taken:

      - CPU      TotalProcessorTime delta over the window, as a percentage of
                 ONE logical core (100 % = one core busy all the time), so the
                 number does not shrink on a bigger machine;
      - private  PrivateMemorySize64 summed at the end of the window (commit
                 the tree holds; what Task Manager calls "memory");
      - working  WorkingSet64 summed at the end of the window.

    The reported value is the median of the windows, so one background
    hiccup on a shared runner does not decide the result. A process that
    exits during a window counts what it used until it exited.

    A Markdown table goes to $env:GITHUB_STEP_SUMMARY when it is set (the CI
    job) and to the console. Exit code = number of budget breaches; 2 when
    the app is not installed or does not stay up.

    -Update rewrites idle-budget.json from this run: measured median plus
    -Headroom, rounded up. Budgets are raised deliberately, never to make CI
    pass.

.PARAMETER Exe
    The installed executable. Defaults to the per-machine install.

.EXAMPLE
    pwsh -NoProfile -File scripts/ci/idle-budget.ps1
    pwsh -NoProfile -File scripts/ci/idle-budget.ps1 -Update -Headroom 0.5
#>
[CmdletBinding()]
param(
    [string]$Exe = (Join-Path $env:ProgramFiles 'Vitals\vitals-desktop.exe'),
    [int]$SettleSeconds = 60,
    [int]$WindowSeconds = 40,
    [int]$Windows = 3,
    [ValidateSet('hidden', 'visible')][string[]]$Variant = @('hidden', 'visible'),
    [string]$Budget = (Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'idle-budget.json'),
    [switch]$Update,
    [double]$Headroom = 0.5,
    # Fail when no msedgewebview2.exe runs under the app. Without an
    # interactive desktop (a service or batch logon, e.g. the devcloud
    # agent) WebView2 never starts, and the numbers would describe only the
    # Rust process. CI's runner has a desktop, so CI passes this.
    [switch]$RequireWebView,
    # Where the measured numbers are written as JSON, for a caller that wants
    # them (the devcloud proof, a README generator).
    [string]$Out
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Exe)) {
    Write-Host "idle-budget: $Exe is not installed" -ForegroundColor Red
    exit 2
}

# The app plus every descendant, from one Win32_Process snapshot. WebView2
# processes are children (and grandchildren) of the app, so a name filter
# alone would also count another program's webview.
function Get-Tree([int]$RootId) {
    $all = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CreationDate)
    $byParent = @{}
    foreach ($p in $all) {
        $key = [int]$p.ParentProcessId
        if (-not $byParent.ContainsKey($key)) { $byParent[$key] = [Collections.Generic.List[object]]::new() }
        $byParent[$key].Add($p)
    }
    $root = $all | Where-Object { $_.ProcessId -eq $RootId } | Select-Object -First 1
    if ($null -eq $root) { return @() }
    $out = [Collections.Generic.List[object]]::new()
    $queue = [Collections.Generic.Queue[object]]::new()
    $queue.Enqueue($root)
    while ($queue.Count -gt 0) {
        $p = $queue.Dequeue()
        $out.Add($p)
        if ($byParent.ContainsKey([int]$p.ProcessId)) {
            # A recycled pid can name a parent that exited before its child
            # started; a child older than its parent is not its child.
            foreach ($c in $byParent[[int]$p.ProcessId]) {
                if ($c.ProcessId -ne $p.ProcessId -and $c.CreationDate -ge $p.CreationDate) { $queue.Enqueue($c) }
            }
        }
    }
    return $out.ToArray()
}

# CPU time per pid, keyed by pid + start time so a recycled pid is a new
# process. A process that has exited keeps the last value read for it.
function Read-Tree([int]$RootId) {
    $snap = @{}
    $private = 0L; $working = 0L; $names = @{}
    foreach ($p in Get-Tree $RootId) {
        try {
            $proc = [Diagnostics.Process]::GetProcessById([int]$p.ProcessId)
            $key = "$($p.ProcessId)@$($proc.StartTime.ToFileTimeUtc())"
            $snap[$key] = $proc.TotalProcessorTime.TotalMilliseconds
            $private += $proc.PrivateMemorySize64
            $working += $proc.WorkingSet64
            $names[$p.Name] = 1 + [int]$names[$p.Name]
        } catch {
            # Exited between the snapshot and the read.
            continue
        }
    }
    [pscustomobject]@{ Cpu = $snap; Private = $private; Working = $working; Names = $names }
}

function Get-Median([double[]]$values) {
    $s = @($values | Sort-Object)
    if ($s.Count -eq 0) { return $null }
    if ($s.Count % 2) { return $s[[int][math]::Floor($s.Count / 2)] }
    return ($s[$s.Count / 2 - 1] + $s[$s.Count / 2]) / 2
}

function Stop-Vitals {
    Get-Process -Name 'vitals-desktop', 'vitals-watchdog' -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}

function Measure-Variant([string]$Name) {
    Stop-Vitals
    $launchArgs = if ($Name -eq 'hidden') { @('--minimized') } else { @() }
    $start = @{ FilePath = $Exe; PassThru = $true }
    if ($launchArgs.Count) { $start.ArgumentList = $launchArgs }
    $app = Start-Process @start
    Write-Host "[$Name] started pid $($app.Id); settling ${SettleSeconds}s"
    Start-Sleep -Seconds $SettleSeconds
    if ($app.HasExited) { throw "[$Name] the app exited during settle (code $($app.ExitCode))" }

    $cpu = @(); $priv = @(); $work = @(); $procs = $null
    for ($w = 1; $w -le $Windows; $w++) {
        $before = Read-Tree $app.Id
        $sw = [Diagnostics.Stopwatch]::StartNew()
        Start-Sleep -Seconds $WindowSeconds
        $after = Read-Tree $app.Id
        $sw.Stop()
        if ($app.HasExited) { throw "[$Name] the app exited during window $w (code $($app.ExitCode))" }
        $deltaMs = 0.0
        foreach ($k in $after.Cpu.Keys) {
            $base = if ($before.Cpu.ContainsKey($k)) { $before.Cpu[$k] } else { 0.0 }
            $deltaMs += [math]::Max(0.0, $after.Cpu[$k] - $base)
        }
        # Exited mid-window: what it used since `before` is unknown, but it
        # was counted up to `before`; nothing more can be said honestly.
        $pct = 100.0 * $deltaMs / $sw.Elapsed.TotalMilliseconds
        $cpu += $pct; $priv += $after.Private; $work += $after.Working; $procs = $after.Names
        Write-Host ("[{0}] window {1}/{2}: cpu {3:N2} % of a core, private {4:N1} MB, working set {5:N1} MB, {6} processes" -f `
                $Name, $w, $Windows, $pct, ($after.Private / 1MB), ($after.Working / 1MB), $after.Cpu.Count)
    }
    Stop-Vitals
    [pscustomobject]@{
        variant      = $Name
        cpuPercent   = [math]::Round((Get-Median $cpu), 2)
        privateMB    = [math]::Round((Get-Median $priv) / 1MB, 1)
        workingSetMB = [math]::Round((Get-Median $work) / 1MB, 1)
        windows      = @($cpu | ForEach-Object { [math]::Round($_, 2) })
        webView      = [bool]($procs -and $procs.ContainsKey('msedgewebview2.exe'))
        processes    = (($procs.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name) x$($_.Value)" }) -join ', ')
    }
}

$results = @()
foreach ($v in $Variant) { $results += Measure-Variant $v }

$version = (Get-Item -LiteralPath $Exe).VersionInfo.ProductVersion
$cores = [Environment]::ProcessorCount
$metrics = @(
    @{ Key = 'cpuPercent'; Label = 'CPU (% of one core)' }
    @{ Key = 'privateMB'; Label = 'Private memory (MB)' }
    @{ Key = 'workingSetMB'; Label = 'Working set (MB)' }
)

if ($Update) {
    $budgetObj = [ordered]@{
        comment  = 'Idle cost of the installed app, median of 3 windows after a 60 s settle (scripts/ci/idle-budget.ps1). Measured on GitHub windows-latest plus headroom; raise deliberately, never to make CI pass.'
        measured = [ordered]@{ version = $version; logicalCores = $cores; date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd') }
    }
    foreach ($r in $results) {
        $entry = [ordered]@{}
        foreach ($m in $metrics) {
            $val = [double]$r.($m.Key)
            # CPU near zero gets an absolute floor: 50 % of 0.05 % is noise.
            $limit = if ($m.Key -eq 'cpuPercent') { [math]::Max(0.5, [math]::Ceiling($val * (1 + $Headroom) * 10) / 10) } else { [math]::Ceiling($val * (1 + $Headroom)) }
            $entry[$m.Key] = $limit
            $entry["$($m.Key)Measured"] = $val
        }
        $budgetObj[$r.variant] = $entry
    }
    $budgetObj | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Budget -Encoding utf8NoBOM
    Write-Host "budgets written to $Budget (median + $([int]($Headroom * 100)) %)" -ForegroundColor Yellow
}

$limits = if (Test-Path -LiteralPath $Budget) { Get-Content -LiteralPath $Budget -Raw | ConvertFrom-Json } else { $null }
$breaches = 0
$lines = [Collections.Generic.List[string]]::new()
$lines.Add("### Idle cost, Vitals $version ($cores logical cores)")
$lines.Add('')
$lines.Add('| Variant | Metric | Median | Windows | Budget | |')
$lines.Add('| --- | --- | ---: | --- | ---: | --- |')
foreach ($r in $results) {
    foreach ($m in $metrics) {
        $val = [double]$r.($m.Key)
        $limit = if ($limits -and $limits.($r.variant)) { $limits.($r.variant).($m.Key) } else { $null }
        $state = if ($null -eq $limit) { 'no budget' } elseif ($val -le [double]$limit) { 'ok' } else { $breaches++; '**over**' }
        $win = if ($m.Key -eq 'cpuPercent') { ($r.windows -join ' / ') } else { '' }
        $lines.Add("| $($r.variant) | $($m.Label) | $val | $win | $(if ($null -ne $limit) { $limit } else { '-' }) | $state |")
    }
}
$lines.Add('')
foreach ($r in $results) {
    $lines.Add("- $($r.variant): $($r.processes)")
    if (-not $r.webView) {
        $lines.Add("  - no WebView2 process under the app: these numbers exclude the webview (no interactive desktop?)")
        if ($RequireWebView) { $breaches++ }
    }
}

$lines | ForEach-Object { Write-Host $_ }
if ($env:GITHUB_STEP_SUMMARY) { $lines | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
if ($Out) { $results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $Out -Encoding utf8NoBOM }

if ($null -eq $limits) {
    Write-Host "idle-budget: no $Budget; run with -Update to create it" -ForegroundColor Yellow
    exit 1
}
Write-Host ("idle budget: {0} breach(es)" -f $breaches)
exit $breaches
