<#
.SYNOPSIS
    Fast checks on staged files, before the commit is written.

.DESCRIPTION
    Deliberately NOT the full gate set. A pre-commit hook that takes four
    minutes gets bypassed with --no-verify within a day, and a bypassed hook
    is worse than none because everyone believes it ran. Everything here is
    seconds, and everything here corresponds to a mistake actually made in
    this repo:

      * A commit landed in the same shell command as its gates, so the gates'
        exit codes were ignored and two type errors shipped. Formatting and
        contract drift are now checked here, where they cannot be skipped by
        accident.
      * A secret nearly reached a commit. Pairing tokens and .env files must
        never be staged.
      * `dbg!`, `console.log` and `.only(` have all been committed before.
        A focused test is the worst of them: the suite goes green while
        running one case.

    Full verification stays in scripts/verify.ps1 and CI.

.NOTES
    Installed by scripts/hooks/install.ps1 into .git/hooks/pre-commit.
    Bypass with `git commit --no-verify` when you genuinely need to.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (git rev-parse --show-toplevel)
Set-Location $root

$staged = @(git diff --cached --name-only --diff-filter=ACMR)
if ($staged.Count -eq 0) { exit 0 }

$problems = [System.Collections.Generic.List[string]]::new()

# ── Secrets and local state must never be staged ───────────────────────

$forbidden = @(
    '(^|/)\.env($|\.)',
    'lan-tokens\.json$',
    '(^|/)history\.sqlite',
    '\.pfx$', '\.p12$', '\.pem$',
    '(^|/)\.copilot-tmp/'
)
foreach ($file in $staged) {
    foreach ($pattern in $forbidden) {
        if ($file -match $pattern) {
            $problems.Add("$file must never be committed (matches /$pattern/)")
        }
    }
}

# A pairing secret is 43 base64url characters. Cheap to look for, and the
# consequence of missing one is that anyone with the repo can pair.
$textFiles = $staged | Where-Object { $_ -match '\.(rs|ts|tsx|js|mjs|json|md|ps1|yml|yaml|toml)$' -and (Test-Path $_) }
foreach ($file in $textFiles) {
    $content = Get-Content $file -Raw -ErrorAction SilentlyContinue
    if ($null -eq $content) { continue }
    if ($content -match '"secret"\s*:\s*"[A-Za-z0-9_-]{40,}"') {
        $problems.Add("$file appears to contain a pairing secret")
    }
}

# ── Debug leftovers ────────────────────────────────────────────────────

foreach ($file in $textFiles) {
    $lines = Get-Content $file -ErrorAction SilentlyContinue
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        $n = $i + 1

        # A focused test turns the whole suite green while running one case.
        # The most dangerous leftover there is, so it is checked everywhere.
        if ($line -match '\b(it|test|describe)\.only\(') {
            $problems.Add("${file}:${n} focused test (.only) — the suite would pass while running one case")
        }
        if ($file -match '\.rs$' -and $line -match '\bdbg!\(') {
            $problems.Add("${file}:${n} dbg! left in")
        }
        # console.* is legitimate in scripts and in the CLI, not in app code.
        if ($file -match '^(apps/desktop/src|packages)/.*\.tsx?$' -and
            $file -notmatch '\.test\.' -and
            $line -match '\bconsole\.(log|debug|dir)\(') {
            $problems.Add("${file}:${n} console.$($Matches[1]) in app code")
        }
    }
}

# ── Formatting, on staged files only ───────────────────────────────────

$rustStaged = @($staged | Where-Object { $_ -match '\.rs$' })
if ($rustStaged.Count -gt 0) {
    & cargo fmt --all -- --check 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        $problems.Add('cargo fmt --all -- --check failed (run: cargo fmt --all)')
    }
}

$webStaged = @($staged | Where-Object { $_ -match '\.(ts|tsx|json|css|md|mjs)$' -and (Test-Path $_) })
if ($webStaged.Count -gt 0) {
    & npx prettier --check @webStaged 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        $problems.Add("prettier found unformatted files (run: npx prettier --write $($webStaged -join ' '))")
    }
}

# ── Contract drift ─────────────────────────────────────────────────────

# Only when something that can cause drift is staged; two seconds is cheap
# but not free, and a docs-only commit cannot break a wire contract.
$driftRelevant = $staged | Where-Object {
    $_ -match '\.rs$' -or $_ -match '^apps/desktop/src/' -or $_ -match '^packages/i18n/'
}
if ($driftRelevant) {
    & pwsh -NoProfile -File scripts/check-drift.ps1 2>&1 | Out-String -OutVariable driftOut | Out-Null
    if ($LASTEXITCODE -ne 0) {
        $problems.Add("contract drift (run: pwsh -NoProfile -File scripts/check-drift.ps1)")
    }
}

# ── Report ─────────────────────────────────────────────────────────────

if ($problems.Count -eq 0) { exit 0 }

Write-Host ''
Write-Host 'pre-commit refused this commit:' -ForegroundColor Red
foreach ($p in $problems) { Write-Host "  - $p" -ForegroundColor Red }
Write-Host ''
Write-Host 'Fix these, or bypass deliberately with: git commit --no-verify' -ForegroundColor DarkGray
exit 1
