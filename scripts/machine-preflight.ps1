<#
.SYNOPSIS
    Reports machine and queue load before a release or a gate run.

.DESCRIPTION
    Decides nothing and fails nothing. It prints the numbers behind the
    "run this gate here, or on the cloud workspace" choice, which until now
    was made from memory or from an ad-hoc probe — on 2026-11-20 that cost
    two wasted gate runs: one was started against a saturated machine, and
    the local verify.ps1 was given up on without a number behind either
    decision.

    Deliberately NOT part of scripts/verify.ps1. A gate that fails because
    the machine is busy is exactly the problem this describes.

    Two of the numbers here are the ones that have actually broken builds in
    this repo, and neither is interchangeable with its obvious look-alike:

      * COMMIT CHARGE, not free RAM. A build died with "memory allocation
        failed" while 50 GB of RAM looked free, because the commit limit was
        exhausted by vmmemWSL (2026-09-30). Both are printed, separately.
      * Free space on C: and E:, because "disk full" on this machine has
        always been build output (2026-10-24: E: down to 191 GB free).

    agentq is the authority on queue and admission state, and this reports it
    rather than reimplementing it. One read-only `agentq status -All -Json`
    call supplies the start probe, the task-lease count and the ticket queue;
    a second copy of the probe here would give two answers to one question,
    and they would start disagreeing the first time a threshold moved.

.EXAMPLE
    pwsh -NoProfile -File scripts/machine-preflight.ps1

.EXAMPLE
    pwsh -NoProfile -File scripts/machine-preflight.ps1 -Json | ConvertFrom-Json
    The machine form: one JSON object, with unmeasurable values omitted
    rather than zeroed, because a zero reading asserts something.

.NOTES
    Always exits 0 — this is advice, not a gate. Read the VERDICT line.
#>
#Requires -Version 7.0
[CmdletBinding()]
param(
    # Machine-readable output: one JSON object on stdout, and no block.
    [switch]$Json,

    # agentq status only reads the cached load sample, so it returns in about
    # two seconds; the bound exists so a wedged machine cannot hang a preflight.
    [int]$AgentqTimeoutSec = 30
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# The floors. The commit one is the number agentq's own admission gate compares
# against, so it is read from agentq's config below and cannot drift from it.
$thresholds = [ordered]@{
    diskFloorGB    = 20
    ramFloorGB     = 6     # agentq run's -MinFreeGB default
    commitFloorGB  = 12    # overridden by agentq-config.json minFreeCommitGB
    cpuBusyPercent = 85
}

# ── measurement ────────────────────────────────────────────────────────

# DriveInfo, not Get-PSDrive or WMI: it answers in well under a millisecond,
# and a drive that is not there comes back as a not-ready object rather than an
# error — which is the "absent, not zero" answer we want.
function Get-DiskSample([string]$Drive) {
    try {
        $d = [IO.DriveInfo]::new($Drive)
        if (-not $d.IsReady) { return $null }
        [ordered]@{ freeGB = [math]::Round($d.AvailableFreeSpace / 1GB, 1); totalGB = [math]::Round($d.TotalSize / 1GB, 1) }
    } catch { $null }
}

# One Win32_OperatingSystem query for RAM and commit. FreeVirtualMemory is the
# commit headroom (commit limit minus committed) and TotalVirtualMemorySize is
# the commit limit; agentq's admission gate divides the same two fields by the
# same 1MB, so the two cannot disagree about whether there is room.
function Get-MemorySample {
    try {
        $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop `
            -Property FreePhysicalMemory, TotalVisibleMemorySize, FreeVirtualMemory, TotalVirtualMemorySize
        [ordered]@{
            ramFreeGB     = [math]::Round($os.FreePhysicalMemory / 1MB, 1)
            ramTotalGB    = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1)
            commitFreeGB  = [math]::Round($os.FreeVirtualMemory / 1MB, 1)
            commitLimitGB = [math]::Round($os.TotalVirtualMemorySize / 1MB, 1)
        }
    } catch { $null }
}

# LoadPercentage is what a person means by "CPU load", and it is reported but
# never binding on its own: at 100 % CPU agentq measured a trivial process
# still starting in about 100 ms (2026-11-11), so the start probe — not this —
# is the saturation signal.
function Get-CpuPercent {
    try {
        $avg = (@(Get-CimInstance Win32_Processor -Property LoadPercentage -ErrorAction Stop) |
            Measure-Object LoadPercentage -Average).Average
        if ($null -eq $avg) { $null } else { [int][math]::Round([double]$avg) }
    } catch { $null }
}

# ── agentq, reused rather than reinvented ──────────────────────────────

# The installed entry point is the `agentq` the PATH resolves, or the copy the
# coordination rules name at ~/.copilot/bin. E:\gh\agentq\ps is the source that
# copy is installed from, so it is not a second candidate: two agents invoking
# two different copies of the queue is the failure this looks like it prevents.
function Resolve-AgentqScript {
    $cmd = Get-Command agentq -CommandType Application, ExternalScript -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($cmd -and (Test-Path -LiteralPath $cmd.Source -PathType Leaf)) { return $cmd.Source }
    $installed = Join-Path (Join-Path $HOME '.copilot') 'bin\agentq.ps1'
    if (Test-Path -LiteralPath $installed -PathType Leaf) { return $installed }
    $null
}

# Read-only: `status` reads the shared sample and never takes one, so a
# saturated machine does not make this slow or make it queue behind itself.
function Get-AgentqStatus([string]$Script, [int]$TimeoutSec) {
    $exe = $null
    try { $exe = (Get-Process -Id $PID).Path } catch { }
    if (-not $exe) {
        $cmd = Get-Command pwsh -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($cmd) { $exe = $cmd.Source }
    }
    if (-not $exe) { return @{ ok = $false; why = 'pwsh could not be resolved to run agentq' } }

    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $exe
    foreach ($arg in @('-NoProfile', '-File', $Script, 'status', '-All', '-Json')) { $psi.ArgumentList.Add($arg) }
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true

    $proc = $null
    try {
        $proc = [Diagnostics.Process]::Start($psi)
        # Both pipes are drained asynchronously. Reading stdout to the end
        # before waiting for exit deadlocks the moment stderr fills its buffer.
        $outTask = $proc.StandardOutput.ReadToEndAsync()
        $errTask = $proc.StandardError.ReadToEndAsync()
        if (-not $proc.WaitForExit($TimeoutSec * 1000)) {
            try { $proc.Kill($true) } catch { }
            return @{ ok = $false; why = "agentq status did not finish within ${TimeoutSec}s" }
        }
        $text = $outTask.GetAwaiter().GetResult()
        [void]$errTask.GetAwaiter().GetResult()
        if (-not $text) { return @{ ok = $false; why = "agentq status exited $($proc.ExitCode) with no output" } }
        # Belt and braces: a module-load warning ahead of the JSON would
        # otherwise make a parse failure look like an absent agentq.
        $first = $text.IndexOf('{'); $last = $text.LastIndexOf('}')
        if ($first -lt 0 -or $last -le $first) { return @{ ok = $false; why = 'agentq status printed no JSON object' } }
        try { $parsed = $text.Substring($first, $last - $first + 1) | ConvertFrom-Json -ErrorAction Stop }
        catch { return @{ ok = $false; why = "agentq status JSON unreadable: $($_.Exception.Message)" } }
        @{ ok = $true; json = $parsed }
    } catch {
        @{ ok = $false; why = "agentq status failed: $($_.Exception.Message)" }
    } finally {
        if ($proc) { $proc.Dispose() }
    }
}

# ── gather ─────────────────────────────────────────────────────────────

$report = [ordered]@{
    at   = [DateTime]::UtcNow.ToString('o')
    repo = $root
}

$report.disk = [ordered]@{}
foreach ($drive in 'C', 'E') {
    $sample = Get-DiskSample $drive
    if ($sample) { $report.disk[$drive.ToLowerInvariant()] = $sample }
}

$cpu = Get-CpuPercent
if ($null -ne $cpu) { $report.cpu = [ordered]@{ percent = $cpu } }

$memory = Get-MemorySample
if ($memory) { $report.memory = $memory }

$agentqScript = Resolve-AgentqScript
if ($agentqScript) {
    $agentq = [ordered]@{ available = $true; path = $agentqScript }
    $status = Get-AgentqStatus $agentqScript $AgentqTimeoutSec
    if ($status.ok) {
        $j = $status.json
        $agentq.status = 'ok'
        # role, not state: `state` is about staleness (ok/STALE/WEDGED), while
        # role is HOLD / HOLD(idle) / wait#n — i.e. whether it is running or queued.
        $holders = @($j.tickets | Where-Object { "$($_.role)" -like 'HOLD*' })
        $waiting = @($j.tickets | Where-Object { "$($_.role)" -like 'wait*' })
        $agentq.queue = [ordered]@{
            holding = @($holders | ForEach-Object { [ordered]@{ resource = "$($_.resource)"; role = "$($_.role)"; session = "$($_.session)"; purpose = "$($_.purpose)" } })
            waiting = @($waiting | ForEach-Object { [ordered]@{ resource = "$($_.resource)"; role = "$($_.role)"; session = "$($_.session)"; purpose = "$($_.purpose)" } })
            stale   = @($j.tickets | Where-Object { "$($_.state)" -ne 'ok' } | ForEach-Object { "$($_.resource): $($_.state)" })
        }
        if ($null -ne $j.agents -and $null -ne $j.agents.limit) {
            $agentq.slots = [ordered]@{ leased = [int]$j.agents.leased; total = [int]$j.agents.limit; state = "$($j.agents.state)"; why = "$($j.agents.why)" }
        }
        if ($null -ne $j.load) {
            $load = [ordered]@{}
            foreach ($k in 'probeMs', 'medianMs', 'thresholdMs', 'samples', 'ageSec', 'src', 'saturated', 'enabled') {
                if ($null -ne $j.load.$k) { $load[$k] = $j.load.$k }
            }
            $agentq.load = $load
        }
        # Which cloud workspace this repo would go to, so "use CI" names a place
        # rather than a direction. Keyed by the repo directory name, which is
        # what agentq's config uses.
        $config = Join-Path (Split-Path -Parent $agentqScript) 'agentq-config.json'
        if (Test-Path -LiteralPath $config -PathType Leaf) {
            try {
                $cfg = Get-Content -LiteralPath $config -Raw | ConvertFrom-Json
                if ($null -ne $cfg.minFreeCommitGB) { $thresholds.commitFloorGB = [double]$cfg.minFreeCommitGB }
                $leaf = Split-Path -Leaf $root
                if ($null -ne $cfg.cloudWorkspaces -and $null -ne $cfg.cloudWorkspaces.$leaf) {
                    $agentq.cloudWorkspace = "$($cfg.cloudWorkspaces.$leaf)"
                }
            } catch { }
        }
    } else {
        # agentq is installed but would not answer: do not guess its queue.
        $agentq.status = 'unreadable'
        $agentq.why = $status.why
    }
    $report.agentq = $agentq
} else {
    $report.agentq = [ordered]@{ available = $false; status = 'missing' }
}

$report.thresholds = $thresholds

# ── verdict ────────────────────────────────────────────────────────────

# One recommendation, and the single constraint that produced it. Ordered by
# severity, first match wins: a full disk or an exhausted commit charge will
# break a build no matter how idle the queue is.
function Get-PreflightVerdict {
    $disk = $report.disk
    foreach ($drive in 'c', 'e') {
        if ($disk[$drive] -and $disk[$drive].freeGB -lt $thresholds.diskFloorGB) {
            return @{ recommendation = 'ci'; reason = "$($drive.ToUpperInvariant()): has $($disk[$drive].freeGB) GB free, below the $($thresholds.diskFloorGB) GB floor" }
        }
    }
    if ($report.memory -and $report.memory.commitFreeGB -lt $thresholds.commitFloorGB) {
        return @{ recommendation = 'ci'; reason = "commit charge has $($report.memory.commitFreeGB) GB free, below agentq's $($thresholds.commitFloorGB) GB floor" }
    }
    if ($report.memory -and $report.memory.ramFreeGB -lt $thresholds.ramFloorGB) {
        return @{ recommendation = 'ci'; reason = "RAM has $($report.memory.ramFreeGB) GB free, below the $($thresholds.ramFloorGB) GB floor" }
    }
    if (-not $report.agentq.available) {
        return @{ recommendation = 'unknown'; reason = 'agentq not found, so queue load could not be measured' }
    }
    if ($report.agentq.status -ne 'ok') {
        return @{ recommendation = 'unknown'; reason = "agentq would not report its state ($($report.agentq.why))" }
    }
    # The start probe decides saturation, on its own number. A median over the
    # limit IS a slow machine for this tool's purpose, whatever agentq's own
    # `saturated` flag says: on 2026-11-20 that flag read false while the median
    # was 7411 ms against a 400 ms limit, and a verdict that had followed the
    # flag printed "process starts are not slow" beside its own 18x-over number.
    # A tool whose job is to make "is this machine usable for a build" a question
    # of evidence cannot contradict the evidence, so the number wins and any
    # disagreement with the flag is stated outright — an unflagged slow machine
    # is exactly the state the reader is trying to detect.
    $load = $report.agentq.load
    $median = if ($load) { $load.medianMs } else { $null }
    $limit = if ($load) { $load.thresholdMs } else { $null }
    if ($null -ne $median -and $null -ne $limit -and $median -gt $limit) {
        $flag = if ($load.saturated) {
            'agentq agrees (saturated)'
        } else {
            "agentq's saturated flag reads false, so the two disagree"
        }
        return @{ recommendation = 'ci'; reason = "start probe median $median ms against the $limit ms limit ($flag)" }
    }
    if ($load -and $load.saturated) {
        # The other direction: the flag is set although the number is under the
        # limit. Still "ci", and still stated as a disagreement rather than
        # silently trusting whichever side happens to be quieter.
        $probe = if ($null -ne $median -and $null -ne $limit) {
            " (its probe median $median ms is within the $limit ms limit, so the flag and the number disagree)"
        } else {
            ''
        }
        return @{ recommendation = 'ci'; reason = "agentq reports saturated$probe" }
    }
    $slots = $report.agentq.slots
    if ($slots -and $slots.total -gt 0 -and $slots.leased -ge $slots.total) {
        return @{ recommendation = 'ci'; reason = "all $($slots.total) task slots are leased" }
    }
    $waiting = @($report.agentq.queue.waiting).Count
    if ($waiting -gt 0) {
        return @{ recommendation = 'ci'; reason = "$waiting ticket(s) already waiting in the build queue" }
    }

    $parts = @()
    $parts += if (@($report.agentq.queue.holding).Count) { "$(@($report.agentq.queue.holding).Count) holding, none waiting" } else { 'queue idle' }
    if ($slots) { $parts += "$($slots.leased)/$($slots.total) task slots leased" }
    if ($report.memory) { $parts += "commit $($report.memory.commitFreeGB) GB free" }
    if ($load -and $null -ne $load.medianMs) {
        $parts += "probe $($load.medianMs) ms against the $($load.thresholdMs) ms limit"
    } else {
        $parts += 'no load sample yet'
    }
    # Reached only when the probe is under the limit (or absent), so this says
    # what the run actually measured rather than asserting starts are fast.
    if ($null -ne $cpu -and $cpu -ge $thresholds.cpuBusyPercent) {
        if ($load -and $null -ne $load.medianMs) {
            $parts += "CPU busy at $cpu % but starts are within the $($load.thresholdMs) ms limit"
        } else {
            $parts += "CPU busy at $cpu % (no start sample to weigh it)"
        }
    }
    @{ recommendation = 'local'; reason = ($parts -join ', ') }
}

$verdict = Get-PreflightVerdict
$report.verdict = $verdict

# ── output ─────────────────────────────────────────────────────────────

if ($Json) {
    $report | ConvertTo-Json -Depth 6
    exit 0
}

function Format-Gb($v) {
    if ($null -eq $v) { return 'UNKNOWN' }
    # Invariant, not the current culture. `-f` would render 1862.2 as
    # "1.862,2 GB" on this machine's locale, and the block is read by agents
    # and pasted into reports where a decimal comma is a different number.
    [string]::Format([Globalization.CultureInfo]::InvariantCulture, '{0:N1} GB', [double]$v)
}

Write-Host ('preflight  {0}  {1:yyyy-MM-dd HH:mm:ss} local' -f $root, (Get-Date))
$c = $report.disk['c']; $e = $report.disk['e']
Write-Host ('  disk    C: {0} free of {1}    E: {2} free of {3}' -f (Format-Gb $c.freeGB), (Format-Gb $c.totalGB), (Format-Gb $e.freeGB), (Format-Gb $e.totalGB))
if ($report.memory) {
    Write-Host ('  cpu     {0}   ram {1} free of {2}   commit {3} free of {4}' -f `
        $(if ($null -eq $cpu) { 'UNKNOWN' } else { "$cpu %" }), (Format-Gb $report.memory.ramFreeGB), (Format-Gb $report.memory.ramTotalGB), (Format-Gb $report.memory.commitFreeGB), (Format-Gb $report.memory.commitLimitGB))
} else {
    Write-Host '  cpu     UNKNOWN   ram UNKNOWN   commit UNKNOWN'
}
$aq = $report.agentq
if (-not $aq.available) {
    Write-Host '  agentq  UNKNOWN - not found on PATH or at ~/.copilot/bin/agentq.ps1'
} elseif ($aq.status -ne 'ok') {
    Write-Host "  agentq  UNKNOWN - $($aq.why)"
} else {
    $slots = $aq.slots; $load = $aq.load
    $slotTxt = if ($slots) { "$($slots.leased)/$($slots.total) task slots leased" } else { 'slots UNKNOWN' }
    $queueTxt = "queue idle"
    if (@($aq.queue.holding).Count -or @($aq.queue.waiting).Count) {
        $queueTxt = "queue busy: $(@($aq.queue.holding).Count) holding, $(@($aq.queue.waiting).Count) waiting"
    }
    $loadTxt = if ($load -and $null -ne $load.medianMs) {
        "load $($load.medianMs) ms vs $($load.thresholdMs) ms limit ($($load.samples) samples, $($load.ageSec) s old)"
    } else { 'load UNKNOWN (no sample)' }
    Write-Host ('  agentq  {0}   {1}   {2}' -f $slotTxt, $queueTxt, $loadTxt)
    foreach ($t in @($aq.queue.holding) + @($aq.queue.waiting) | Select-Object -First 4) {
        Write-Host ('            {0,-16} {1,-6} {2}  {3}' -f $t.resource, $t.role, $t.session, $t.purpose)
    }
    foreach ($s in $aq.queue.stale) { Write-Host "            stale: $s" }
}

$label = switch ($verdict.recommendation) {
    'ci' { 'use CI' }
    'unknown' { 'UNKNOWN' }
    default { 'local build is reasonable' }
}
$colour = switch ($verdict.recommendation) {
    'ci' { 'Yellow' }
    'unknown' { 'DarkGray' }
    default { 'Green' }
}
# A plain hyphen, not an em dash: a child process writes its piped stdout
# through the OEM console encoding, where U+2014 is not representable, so a
# captured run would silently show a different line from a displayed one.
Write-Host "VERDICT: $label - $($verdict.reason)" -ForegroundColor $colour
