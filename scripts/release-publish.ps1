<#
.SYNOPSIS
  Publishes a tagged build as a GitHub release: payload, assets, then public.

.DESCRIPTION
  Every published file already sits in one directory (publish/), assembled by
  the release workflow from the build legs. This creates the release, uploads
  those files, and only then makes the release public.

  Created as a DRAFT and flipped afterwards, deliberately. The endpoint every
  installed copy polls is releases/latest/download/latest.json, and
  /releases/latest/ resolves a draft only once it is published — so a release
  that goes live before its assets are attached answers 404 for as long as the
  upload takes, which is exactly the window an update check can land in. Taking
  it public last means the manifest and the files it names appear together.

  Two API details this works around:

    * no `target_commitish` is ever sent. With GITHUB_TOKEN, POST /releases is
      refused with 403 "Resource not accessible by integration" when the body
      carries a `target_commitish` whose commit's `.github/workflows/` differs
      from the default branch (softprops/action-gh-release#836). By the time
      this runs the tag exists and is authoritative, so no commit target is
      needed and none is sent.
    * the release id is read from the create call's own JSON response rather
      than scraped from its text, so the PATCH cannot address the wrong thing.

  -DryRun is the rehearsal path: it assembles the same payload, computes the
  same asset manifest and runs the same assertions, then returns before the
  first `gh` call — so no release exists and no asset is uploaded. The release
  workflow's `rehearse` job is the only caller. It is a separate job rather than
  a flag on the real one so that the create/upload/flip code is not merely
  skipped in a rehearsal: it is absent from the run.

.PARAMETER Tag
  The tag to publish, or — with -DryRun — the tag the rehearsal pretends to
  publish. The tag already exists; it is never created here.

.PARAMETER Title
  The release title (the `plan` job's `title` output).

.PARAMETER Notes
  Path to the release body, written by scripts/release-notes.ps1.

.PARAMETER Directory
  The directory holding every file to attach. Defaults to publish/.

.PARAMETER Repository
  owner/name. Defaults to $env:REPOSITORY, then dragoscv/vitals.

.PARAMETER Out
  Where release-payload.json and release-manifest.txt are written. Defaults to
  the working directory; the rehearsal job points it at the directory it
  uploads as an artefact.

.PARAMETER DryRun
  Assemble, assert and report, then stop. Makes no API call.

.EXAMPLE
  pwsh -NoProfile -File scripts/release-publish.ps1 -Tag v1.3.2 -Title 'Vitals 1.3.2' -Notes release-notes.md

.EXAMPLE
  pwsh -NoProfile -File scripts/release-publish.ps1 -Tag v1.3.2 -Title 'Vitals 1.3.2 (rehearsal)' -Notes release-notes.md -Directory publish -Out rehearsal -DryRun
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Tag,
  [Parameter(Mandatory)][string]$Title,
  [Parameter(Mandatory)][string]$Notes,
  [string]$Directory = 'publish',
  [string]$Repository,
  [string]$Out = '.',
  [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

if (-not $Repository) { $Repository = if ($env:REPOSITORY) { $env:REPOSITORY } else { 'dragoscv/vitals' } }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
if (-not (Test-Path -LiteralPath $Notes)) {
  throw "the release notes file '$Notes' is missing; the release body would be empty"
}

# The upload glob is "every file in the directory" — gh release upload is given
# all of them — so this manifest is the directory's own contents and never a
# list kept here. A new artefact joins the release and its check for free.
$assets = @(Get-ChildItem -LiteralPath $Directory -File | Sort-Object Name)
if ($assets.Count -eq 0) {
  throw "'$Directory' is empty — refusing to create a release with no assets"
}
$names = @($assets | ForEach-Object { $_.Name })

function Get-SignatureFormat([string]$name) {
  # `.tar.gz` is one format, not `gz`; every other signature-carrying artefact
  # this repository produces ends in a single extension.
  if ($name -like '*.tar.gz') { return 'tar.gz' }
  return [IO.Path]::GetExtension($name).TrimStart('.')
}

function Test-IsCopyOfASignedFile([System.IO.FileInfo]$file) {
  # The fixed-name copies scripts/stable-assets.ps1 writes are byte-identical to
  # a versioned file and deliberately carry no .sig: the updater never fetches
  # them, and a signature beside a duplicate would be one more asset to explain.
  # Such a copy is identified by content, not by name — and only when its twin
  # IS signed, so that a versioned artefact whose signature is missing cannot be
  # excused by the very copy of itself that shipped without one.
  $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
  foreach ($other in $assets) {
    if ($other.FullName -eq $file.FullName) { continue }
    if ($other.Length -ne $file.Length) { continue }
    if (-not (Test-Path -LiteralPath "$($other.FullName).sig")) { continue }
    if ((Get-FileHash -LiteralPath $other.FullName -Algorithm SHA256).Hash -eq $hash) { return $true }
  }
  return $false
}

function Write-StepSummary([string]$text) {
  # GITHUB_STEP_SUMMARY is set by the runner and empty when this is run by hand.
  if ($env:GITHUB_STEP_SUMMARY) {
    Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value $text -Encoding utf8NoBOM
  }
}

# latest.json is what every installed copy polls; a release without it is a
# release the updater cannot see at all.
if ('latest.json' -notin $names) {
  throw "latest.json is not in '$Directory'; without it the updater endpoint answers 404"
}

# Every signature-carrying artefact must have been signed, or the updater
# refuses the file it points at. "Signature-carrying" is decided from the data,
# not from a platform list: the formats this run actually signed are the ones
# that have a .sig, so a format nothing was signed for (a Debian package, whose
# integrity is its own GPG signature rather than an updater one) is not expected
# to have one, and a new platform needs no edit here.
$signaturePatterns = @('*.exe', '*.msi', '*.dmg', '*.AppImage', '*.deb', '*.tar.gz')
$carriers = @($assets | Where-Object { $n = $_.Name; $signaturePatterns | Where-Object { $n -like $_ } })
$signedFormats = @($names | Where-Object { $_ -like '*.sig' } | ForEach-Object { Get-SignatureFormat ($_ -replace '\.sig$', '') })

$unsigned = @()
foreach ($carrier in $carriers) {
  if (Test-Path -LiteralPath "$($carrier.FullName).sig") { continue }
  if ((Get-SignatureFormat $carrier.Name) -notin $signedFormats) { continue }
  if (Test-IsCopyOfASignedFile $carrier) { continue }
  $unsigned += $carrier.Name
}
if ($unsigned.Count -gt 0) {
  throw "no .sig beside $(($unsigned -join ', ')) (expected $(@($unsigned | ForEach-Object { "$_.sig" }) -join ', ')); an update from these files is refused as unsigned"
}

# A manifest naming a file that is not here is worse than a missing manifest:
# the updater downloads a 404 and reports a failed update on every machine.
$manifest = Get-Content -LiteralPath (Join-Path $Directory 'latest.json') -Raw | ConvertFrom-Json
$referenced = @($manifest.platforms.PSObject.Properties | ForEach-Object { [uri]::UnescapeDataString(($_.Value.url -split '/')[-1]) })
$dangling = @($referenced | Where-Object { $_ -notin $names })
if ($dangling.Count -gt 0) {
  throw "latest.json names $(($dangling -join ', ')), which $(if ($dangling.Count -eq 1) { 'is' } else { 'are' }) not in '$Directory'; every update would download a 404"
}
$unsignedInManifest = @($referenced | Where-Object { "$($_).sig" -notin $names })
if ($unsignedInManifest.Count -gt 0) {
  throw "latest.json names $(($unsignedInManifest -join ', ')) with no .sig beside $(if ($unsignedInManifest.Count -eq 1) { 'it' } else { 'them' }); every installed copy would refuse the update"
}

$payloadPath = Join-Path $Out 'release-payload.json'
$payload = [ordered]@{
  tag_name               = $Tag
  name                   = $Title
  body                   = (Get-Content -LiteralPath $Notes -Raw)
  draft                  = $true
  prerelease             = $false
  make_latest            = 'true'
  generate_release_notes = $true
} | ConvertTo-Json -Depth 3
# utf8NoBOM: this file is handed straight to the API, and a BOM would make the
# body start with a stray character.
Set-Content -LiteralPath $payloadPath -Value $payload -Encoding utf8NoBOM
Set-Content -LiteralPath (Join-Path $Out 'release-manifest.txt') -Value ($names -join "`n") -Encoding utf8NoBOM

Write-Host "release $Tag — $($names.Count) asset(s):"
$names | ForEach-Object { Write-Host "  $_" }
Write-Host "payload written to $payloadPath"

$mode = if ($DryRun) { 'rehearsal — asserted, nothing published' } else { 'published' }
$summary = @()
$summary += "### Release $Tag — $mode"
$summary += ''
$summary += "$($names.Count) asset(s):"
$summary += '```'
$summary += $names
$summary += '```'
$summary += ''
$summary += 'Release payload:'
$summary += '```json'
$summary += $payload
$summary += '```'
Write-StepSummary ($summary -join "`n")

if ($DryRun) {
  # Everything above is network-free: the payload and the manifest exist only as
  # files, and the assertions read the directory. This return is what makes a
  # rehearsal unable to publish.
  Write-Host 'rehearsal complete: no release was created, no asset was uploaded, no API call was made'
  return
}

$created = gh api --method POST "repos/$Repository/releases" --input $payloadPath
if ($LASTEXITCODE -ne 0) { throw "creating the release failed with exit $LASTEXITCODE; no release exists and nothing was uploaded" }
if ([string]::IsNullOrWhiteSpace($created)) { throw 'the create call returned no body, so the release id cannot be read; nothing was uploaded' }
$releaseId = ($created | ConvertFrom-Json).id
if (-not $releaseId) { throw 'the create response carried no release id; refusing to continue rather than address an unknown release' }

# --clobber keeps a re-run of the same tag idempotent.
gh release upload $Tag $assets.FullName --clobber
if ($LASTEXITCODE -ne 0) { throw "uploading the release assets failed with exit $LASTEXITCODE; release $releaseId exists as a DRAFT with a partial asset set — complete or delete it before retrying" }

# The flip. Until this succeeds the release is invisible to
# /releases/latest/, which is what both the updater and the stable download
# links resolve through.
gh api --method PATCH "repos/$Repository/releases/$releaseId" -f draft=false | Out-Null
if ($LASTEXITCODE -ne 0) { throw "publishing the draft release failed with exit $LASTEXITCODE; release $releaseId holds its assets but is still a DRAFT, so releases/latest/download/latest.json answers 404" }

# Read back what the API stored: the run's own proof that the release is public
# with every file, not just that the calls returned 0.
$state = gh release view $Tag --json tagName,isDraft,isPrerelease,assets
if ($LASTEXITCODE -ne 0) { throw "reading the release back failed with exit $LASTEXITCODE; release $releaseId was published but this run cannot confirm its asset set" }
$view = $state | ConvertFrom-Json
if ($view.isDraft) { throw "release $Tag is still a draft after the flip; the updater endpoint would not resolve it" }

$uploaded = @($view.assets | ForEach-Object { $_.name })
$notUploaded = @($names | Where-Object { $_ -notin $uploaded })
$unexpected = @($uploaded | Where-Object { $_ -notin $names })
if ($notUploaded.Count -gt 0 -or $unexpected.Count -gt 0) {
  throw "the release's asset set does not match '$Directory' — expected but missing: [$($notUploaded -join ', ')]; uploaded but unexpected: [$($unexpected -join ', ')]"
}

Write-Host "published $($view.tagName) draft=$($view.isDraft) prerelease=$($view.isPrerelease) assets=$($uploaded.Count)"
