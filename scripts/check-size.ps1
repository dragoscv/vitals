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

    - INITIAL: what the webview parses before the first screen appears. This
      is the one the user feels as startup latency. It grows when something
      is imported eagerly that should have been lazy.
    - SHIPPED: every JS and CSS asset. Grows when a dependency is added.
    - INSTALLER: what the user downloads. Grows when a non-code asset creeps
      into `dist` — sourcemaps did exactly this, at 3 MB.

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

# The entry chunk, the vendor chunk it statically imports, the module runtime
# and the stylesheet: everything the window must have before it can paint.
# Route chunks are deliberately excluded — they are the point of the split.
$eagerPattern = '^(index|react|src|rolldown-runtime)-|\.css$'
$eager = $assets | Where-Object { $_.Name -match $eagerPattern }

$measured = [ordered]@{
    initialGzipBytes = ($eager | Measure-Object Gzip -Sum).Sum
    shippedGzipBytes = ($assets | Measure-Object Gzip -Sum).Sum
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
