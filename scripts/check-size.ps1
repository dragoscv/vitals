<#
.SYNOPSIS
    Fails if the shipped bundle or installer has grown past its budget.

.DESCRIPTION
    Bundle size regresses one careless import at a time, and nobody notices
    until it is measured in megabytes. This is the ratchet: the budgets below
    sit just above the current measured sizes, so an accidental regression
    goes red while a deliberate one is a visible, reviewable edit to this
    file.

    Three separate numbers, because they fail for different reasons:

        - INITIAL: what the main window parses before the first screen appears.
            This is the one the user feels as startup latency. It grows when
            something is imported eagerly that should have been lazy.
        - SHIPPED: every JS and CSS asset. Grows when a dependency is added.
    - INSTALLER: what the user downloads. Grows when a non-code asset creeps
      into `dist` — sourcemaps did exactly this, at 3 MB.

        Plus one budget per secondary entry point (`mobile.html`, `hud.html`),
        measured as the transitive closure of that entry's module graph. Without
        these, adding an entry inflates SHIPPED and looks like a regression in
        the desktop app, while a real regression inside the phone app hides in
        the same total. Each entry is a separate download for a separate device
        and deserves its own ratchet.

    Run after a production build. `-Update` rewrites the budgets to the
    current sizes, for when a change is intended.

.EXAMPLE
    pnpm --filter @vitals/desktop build:vite
    pwsh -NoProfile -File scripts/check-size.ps1

.EXAMPLE
    pwsh -NoProfile -File scripts/check-size.ps1 -SkipInstaller
#>
[CmdletBinding()]
param(
    # Skip the installer check, which requires a full `tauri build`.
    [switch]$SkipInstaller,

    # Rewrite the budget file to the sizes measured now.
    [switch]$Update
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$budgetFile = Join-Path $root 'size-budget.json'
$distDir = Join-Path $root 'apps/desktop/dist/assets'

if (-not (Test-Path $distDir)) {
    Write-Host 'No build output found. Run the frontend build first:' -ForegroundColor Red
    Write-Host '  pnpm --filter @vitals/desktop build:vite'
    exit 1
}

# ---------------------------------------------------------------------------
# Measure
# ---------------------------------------------------------------------------

function Get-GzipSize {
    param([Parameter(Mandatory)][string]$Path)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $stream = [System.IO.MemoryStream]::new()
    $gzip = [System.IO.Compression.GZipStream]::new(
        $stream, [System.IO.Compression.CompressionLevel]::Optimal, $true)
    $gzip.Write($bytes, 0, $bytes.Length)
    $gzip.Dispose()
    $size = $stream.Length
    $stream.Dispose()
    $size
}

$assets = Get-ChildItem $distDir -File |
    Where-Object { $_.Extension -in '.js', '.css' } |
    ForEach-Object {
        [pscustomobject]@{
            Name   = $_.Name
            Bytes  = $_.Length
            Gzip   = Get-GzipSize $_.FullName
        }
    }

# The transitive closure of one HTML entry's module graph.
#
# Rolldown's own per-file table understates a secondary entry: it attributes
# a shared chunk to whichever entry it lists first. Walking the graph is the
# only way to answer "what does someone opening hud.html actually download",
# which is the question a budget should be asking.
function Get-EntryClosure {
    param([Parameter(Mandatory)][string]$Html)

    $seen = [System.Collections.Generic.HashSet[string]]::new()
    $queue = [System.Collections.Generic.Queue[string]]::new()

    # `src=`, `href=` — which covers the stylesheet and, crucially, every
    # `<link rel="modulepreload">`. Those are fetched before paint, so they
    # are initial cost by definition; the old name-pattern missed them and
    # under-reported the main window's startup by 26 kB.
    foreach ($m in [regex]::Matches((Get-Content $Html -Raw), '/assets/([^"'']+\.(?:js|css))')) {
        $queue.Enqueue($m.Groups[1].Value)
    }

    while ($queue.Count -gt 0) {
        $name = $queue.Dequeue()
        if (-not $seen.Add($name)) { continue }

        $path = Join-Path $distDir $name
        if (-not (Test-Path $path) -or $name -notlike '*.js') { continue }

        # Static imports only: `import"./x.js"`, `from"./x.js"`. A dynamic
        # `import("./x.js")` is a route chunk the entry does NOT pay for up
        # front, which is the whole point of the split.
        $code = Get-Content $path -Raw
        foreach ($m in [regex]::Matches($code, '(?:^|[^(])\b(?:import|from)\s*["'']\./([^"'']+\.js)["'']')) {
            $queue.Enqueue($m.Groups[1].Value)
        }
    }

    $seen
}

# The entry chunk, the vendor chunk it statically imports, the module runtime
# and its stylesheet: everything the main window must have before it paints.
# Route chunks are deliberately excluded — they are the point of the split.
#
# Derived from index.html rather than from a name pattern: the old pattern
# ended in `|\.css$`, which swept in the phone's and the overlay's
# stylesheets — assets the main window never loads — and reported them as
# startup cost.
$indexClosure = Get-EntryClosure (Join-Path $root 'apps/desktop/dist/index.html')
$eager = $assets | Where-Object { $indexClosure.Contains($_.Name) }

$measured = [ordered]@{
    initialGzipBytes = ($eager | Measure-Object Gzip -Sum).Sum
    shippedGzipBytes = ($assets | Measure-Object Gzip -Sum).Sum
}

# One budget per secondary entry. Named `<entry>GzipBytes` so adding an
# entry adds a budget rather than silently inflating the total.
foreach ($html in Get-ChildItem (Join-Path $root 'apps/desktop/dist') -Filter '*.html') {
    $entry = [System.IO.Path]::GetFileNameWithoutExtension($html.Name)
    if ($entry -eq 'index') { continue }
    $closure = Get-EntryClosure $html.FullName
    $sum = ($assets | Where-Object { $closure.Contains($_.Name) } | Measure-Object Gzip -Sum).Sum
    $measured["${entry}GzipBytes"] = $sum
}

# Source maps must never reach `dist`: Tauri packages that directory whole,
# so anything left there ships inside the installer and inside every update.
$maps = @(Get-ChildItem $distDir -File -Filter '*.map' -ErrorAction SilentlyContinue)

$installer = Get-ChildItem (Join-Path $root 'target/release/bundle/nsis') `
    -Filter '*.exe' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if ($installer) {
    $measured.installerBytes = $installer.Length
}

# ---------------------------------------------------------------------------
# Compare
# ---------------------------------------------------------------------------

if ($Update -or -not (Test-Path $budgetFile)) {
    $budget = [ordered]@{
        comment = 'Budgets in bytes. Raise deliberately, never to make CI pass.'
        initialGzipBytes = [math]::Ceiling($measured.initialGzipBytes * 1.05)
        shippedGzipBytes = [math]::Ceiling($measured.shippedGzipBytes * 1.05)
        installerBytes   = if ($measured.installerBytes) {
            [math]::Ceiling($measured.installerBytes * 1.05)
        } else { 3145728 }
    }

    # Secondary entries, in whatever order the build produced them.
    foreach ($key in $measured.Keys) {
        if ($key -in 'initialGzipBytes', 'shippedGzipBytes', 'installerBytes') { continue }
        $budget[$key] = [math]::Ceiling($measured[$key] * 1.05)
    }

    # Budgets this script does not measure (check-target-size.ps1 owns
    # `targetDebugBytes` and `incrementalSessionsPerCrate`) survive a rewrite.
    if (Test-Path $budgetFile) {
        $existing = Get-Content $budgetFile -Raw | ConvertFrom-Json
        foreach ($prop in $existing.PSObject.Properties) {
            if (-not $budget.Contains($prop.Name)) { $budget[$prop.Name] = $prop.Value }
        }
    }

    $budget | ConvertTo-Json | Set-Content -Encoding utf8 $budgetFile
    Write-Host "Budgets written to $budgetFile (current + 5% headroom)." -ForegroundColor Yellow
    if (-not $Update) { Write-Host 'Re-run without -Update to check against them.' }
    exit 0
}

$budget = Get-Content $budgetFile -Raw | ConvertFrom-Json

$checks = @(
    @{ Label = 'initial load (gzip)'; Key = 'initialGzipBytes' }
    @{ Label = 'all assets  (gzip)'; Key = 'shippedGzipBytes' }
)

# A secondary entry is only checked once it has a budget, so adding an entry
# is a deliberate `-Update` rather than an instant red build.
foreach ($key in $measured.Keys) {
    if ($key -in 'initialGzipBytes', 'shippedGzipBytes', 'installerBytes') { continue }
    if ($null -eq $budget.$key) {
        Write-Host "  note: entry '$key' has no budget yet; run with -Update." -ForegroundColor Yellow
        continue
    }
    $name = $key -replace 'GzipBytes$', ''
    $checks += @{ Label = ('{0,-10} (gzip)' -f $name); Key = $key }
}

if (-not $SkipInstaller) {
    $checks += @{ Label = 'installer         '; Key = 'installerBytes' }
}

$failed = $false

Write-Host ''
foreach ($check in $checks) {
    $key = $check.Key
    $actual = $measured[$key]
    $limit = $budget.$key

    if ($null -eq $actual) {
        Write-Host ('  {0}  not built, skipped' -f $check.Label) -ForegroundColor DarkGray
        continue
    }

    $pct = if ($limit -gt 0) { 100 * $actual / $limit } else { 0 }
    $over = $actual -gt $limit

    $line = '  {0}  {1,8:N1} KB  of {2,8:N1} KB budget  ({3,5:N1}%)' -f
        $check.Label, ($actual / 1KB), ($limit / 1KB), $pct

    if ($over) {
        $failed = $true
        Write-Host "$line  OVER" -ForegroundColor Red
    } else {
        Write-Host $line -ForegroundColor Green
    }
}

if ($maps.Count -gt 0) {
    $failed = $true
    Write-Host ''
    Write-Host ("  {0} source map(s) in dist/. Tauri packages that directory whole, so these would ship." -f $maps.Count) -ForegroundColor Red
}

Write-Host ''
if ($failed) {
    Write-Host 'Size budget exceeded.' -ForegroundColor Red
    Write-Host 'If the growth is intended, run with -Update and commit the new budget.'
    exit 1
}

Write-Host 'Within budget.' -ForegroundColor Green
exit 0
