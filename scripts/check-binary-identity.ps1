<#
.SYNOPSIS
    Reads the icon and the version block back off the binaries a release ships.

.DESCRIPTION
    A Windows executable carries two things a user reads before the program ever
    runs: the icon Explorer, Task Manager and the Windows Defender Firewall
    prompt draw, and the version block behind the file's Properties dialog.
    Neither comes from the source — both come from a resource section that has
    to be compiled into the binary, and `rustc` writes none of its own.

    1.5.0 shipped three console binaries with neither: `vitals.exe`,
    `vitals-watchdog.exe` and `vitals-sensors.exe` had no icon and no version
    block at all, so every one of them was drawn with Windows' generic
    application icon and reported an empty product name. Nothing in the pipeline
    looked, which is why it reached a person instead of a build failure. A test
    over the source cannot catch it either: the source was the same as the
    desktop's, which was fine because Tauri runs a resource compiler for it.

    So this reads the artefacts themselves, the way Windows does, and fails when
    one of them is not the product. What it asserts, and why each assertion is
    the one worth making:

      * The version block names the product, the publisher and this version. An
        iconless binary has no version block, so this is the defect's own
        signature and needs no pinned hash to detect it.
      * Every application binary carries the same icon as the desktop binary. The
        desktop's is embedded by tauri-build from `bundle.icon`, so it is the
        reference the others have to agree with; comparing binaries to each other
        rather than to a pinned hash means regenerating the brand icon does not
        invalidate this check.
      * The MSIX manifest declares this version and the product, and the
        executables inside the package pass the same checks, because the store
        builds its listing from that manifest.

    Containers NSIS generates — the `-setup.exe` installer and its
    `uninstall.exe` — are checked for the version block only. Their icons are
    chosen by NSIS from Tauri's configuration, they are not a Vitals binary
    compiled from this tree, and a failed assertion there would be a claim about
    Tauri's packaging rather than about our build.

    A missing binary is a failure, not a skip: `-Only` names the artefact set
    the caller promises to publish, and every one of them has to be present and
    pass, because the quieter failure mode is publishing nothing rather than
    publishing it wrong. Without `-Only` every executable and package under the
    path is read, which is right on a directory that holds nothing else.

.EXAMPLE
    pwsh -NoProfile -File scripts/check-binary-identity.ps1 -Path target/x86_64-pc-windows-msvc/release -Version 1.5.1 -Only 'vitals.exe,vitals-desktop.exe,vitals-watchdog.exe,vitals-sensors.exe'
#>
[CmdletBinding()]
param(
    # A directory (searched recursively) or a single file. A comma-separated list
    # binds as one string under `pwsh -File`, so callers pass paths one call at a
    # time rather than discovering that silently.
    [Parameter(Mandatory = $true)]
    [string]$Path,

    # The version every artefact has to declare, without a leading `v`.
    [Parameter(Mandatory = $true)]
    [string]$Version,

    [string]$Product = 'Vitals',
    [string]$Publisher = 'Dragos Catalin Vladulescu',

    # The binary whose icon the others must match. Defaults to the desktop binary
    # found under -Path, which is the artefact tauri-build embeds the brand icon
    # into.
    [string]$Reference,

    # Comma-separated file names (with their extension) that make up the artefact
    # set being published. Every one must be found and must pass.
    [string]$Only = '',

    # Skips the icon comparison and keeps the version block. For an artefact
    # built for another architecture: the shell on this host cannot always draw
    # an ARM64 binary's icon, and the version block — the part that reports
    # nothing at all when a binary has no resource section — reads either way.
    [switch]$SkipIcon
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.IO.Compression.FileSystem

$failures = New-Object System.Collections.Generic.List[string]
$checked = New-Object System.Collections.Generic.List[string]

function Get-IconHash {
    param([string]$File)
    # A host that cannot draw icons at all is a limitation to report, not a
    # defect in the binary: return the fact instead of throwing, so the caller
    # can tell "this file has no icon" from "this machine cannot read one".
    $icon = $null
    try { $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($File) } catch { return 'none' }
    if ($null -eq $icon) { return 'none' }
    try {
        $bitmap = $icon.ToBitmap()
        $stream = New-Object System.IO.MemoryStream
        $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
        $bytes = $stream.ToArray()
        $stream.Dispose()
        $bitmap.Dispose()
        $sha = [System.Security.Cryptography.SHA256]::Create()
        $hash = $sha.ComputeHash($bytes)
        $sha.Dispose()
        return (($hash | ForEach-Object { $_.ToString('x2') }) -join '').Substring(0, 16).ToUpperInvariant()
    } finally {
        $icon.Dispose()
    }
}

# An NSIS-generated container, whose icon belongs to the installer rather than to
# a binary compiled from this tree.
function Test-Container {
    param([string]$Name)
    return ($Name -like '*-setup.exe' -or $Name -ieq 'uninstall.exe')
}

function Assert-Binary {
    param([string]$File, [string]$Label, [string]$ExpectedIcon)

    $info = (Get-Item -LiteralPath $File).VersionInfo
    $problems = New-Object System.Collections.Generic.List[string]
    $isContainer = Test-Container -Name (Split-Path -Leaf $File)

    if ($info.ProductName -ne $Product) {
        $problems.Add("the version block names the product '$($info.ProductName)' instead of '$Product' — a binary with no resource section reports nothing here at all")
    }
    if ($info.FileVersion -ne $Version) {
        $problems.Add("the version block says '$($info.FileVersion)' instead of '$Version'")
    }
    # The installer and its uninstaller are compiled by NSIS from Tauri's
    # configuration, which fills the product name and the version but leaves the
    # company field empty. Asserting it there would be a claim about the
    # packager, and what a user sees in the elevation dialog is the Authenticode
    # publisher anyway — a separate thing this gate does not own.
    if (-not $isContainer -and $info.CompanyName -ne $Publisher) {
        $problems.Add("the publisher is '$($info.CompanyName)' instead of '$Publisher' — the firewall prompt has nothing to show without it")
    }

    $iconHash = 'container'
    if ($isContainer) {
        $iconHash = Get-IconHash $File
    } elseif ($SkipIcon) {
        $iconHash = 'skipped'
    } elseif ($ExpectedIcon) {
        $iconHash = Get-IconHash $File
        if ($iconHash -ne $ExpectedIcon) {
            $problems.Add("the icon Windows draws is $iconHash, not the brand icon $ExpectedIcon the desktop binary carries")
        }
    } else {
        $iconHash = Get-IconHash $File
    }

    foreach ($problem in $problems) { $failures.Add("${Label}: $problem") }
    $checked.Add($Label)
    $state = if ($problems.Count -eq 0) { 'ok' } else { 'FAILED' }
    Write-Output ('  {0,-52} {1,-7} icon={2} prod={3} ver={4}' -f $Label, $state, $iconHash, $info.ProductName, $info.FileVersion)
}

$root = (Resolve-Path -LiteralPath $Path).Path
$exeFiles = New-Object System.Collections.Generic.List[object]
$archives = New-Object System.Collections.Generic.List[object]

if (Test-Path -LiteralPath $root -PathType Container) {
    $all = @(Get-ChildItem -LiteralPath $root -Recurse -File)
} else {
    $all = @(Get-Item -LiteralPath $root)
}

# `-Only` is how a caller names the set it publishes. Absence is a failure, and
# so is getting here with nothing to read: an empty directory must not look like
# a pass.
if ($Only) {
    foreach ($name in ($Only -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
        $match = $all | Where-Object { $_.Name -ieq $name } | Select-Object -First 1
        if (-not $match) {
            $failures.Add("$name is not under $Path — what is published has to contain what was promised")
            continue
        }
        switch -Regex ($match.Extension.ToLowerInvariant()) {
            '^\.exe$' { $exeFiles.Add($match) }
            '^\.(zip|msix|msixbundle|appx|appxbundle)$' { $archives.Add($match) }
        }
    }
}

foreach ($file in $all) {
    if ($Only) { continue }
    switch -Regex ($file.Extension.ToLowerInvariant()) {
        '^\.exe$' { $exeFiles.Add($file) }
        '^\.(zip|msix|msixbundle|appx|appxbundle)$' { $archives.Add($file) }
    }
}

if ($exeFiles.Count -eq 0 -and $archives.Count -eq 0) {
    $failures.Add("${Path}: nothing to read — no .exe and no package or zip under it")
}

# The reference icon: the desktop binary, which tauri-build embeds the brand mark
# into. Without it there is nothing to compare against, and a gate that cannot
# compare must say so rather than pass.
$referenceIcon = ''
if ($Reference) {
    if (-not (Test-Path -LiteralPath $Reference)) { $failures.Add("the reference binary $Reference does not exist") }
    else { $referenceIcon = Get-IconHash (Resolve-Path -LiteralPath $Reference).Path }
} else {
    $desktop = $exeFiles | Where-Object { $_.Name -ieq 'vitals-desktop.exe' } | Select-Object -First 1
    if ($desktop) { $referenceIcon = Get-IconHash $desktop.FullName }
    else { $failures.Add("no vitals-desktop.exe under $Path to take the brand icon from — pass -Reference") }
}

if (-not $SkipIcon -and $referenceIcon -in @('', 'none')) {
    # Say so loudly rather than pass quietly: the icon half of this gate is not
    # running, and a reader of the log has to be able to see that.
    $SkipIcon = $true
    Write-Output 'warning: the brand icon could not be read as a reference on this host, so the icon comparison is skipped; the version block is still checked, and a binary with no resource section reports nothing there'
}

$iconNote = if ($SkipIcon) { 'not compared' } else { $referenceIcon }
Write-Output "reading $Path (version $Version, product $Product, brand icon $iconNote)"

foreach ($file in $exeFiles) {
    Assert-Binary -File $file.FullName -Label (Split-Path -Leaf $file.FullName) -ExpectedIcon $referenceIcon
}

# A zip or a package is opened in memory-or-temp and the executables inside are
# held to the same rules as the loose ones: the CLI ships as a zip, and an
# unchecked archive is exactly how a broken binary would get out.
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('vitals-binary-identity-' + [Guid]::NewGuid().ToString('n').Substring(0, 8))
$null = New-Item -ItemType Directory -Path $tempRoot -Force
try {
    foreach ($archive in $archives) {
        $label = Split-Path -Leaf $archive.FullName
        if ($archive.Extension.ToLowerInvariant() -eq '.zip') {
            $target = Join-Path $tempRoot $label
            $null = New-Item -ItemType Directory -Path $target -Force
            [System.IO.Compression.ZipFile]::ExtractToDirectory($archive.FullName, $target)
            $inner = @(Get-ChildItem -LiteralPath $target -Recurse -File -Filter *.exe)
            if ($inner.Count -eq 0) { $failures.Add("${label}: no executable inside, so what it publishes is not checkable") }
            foreach ($file in $inner) {
                Assert-Binary -File $file.FullName -Label "$label!$($file.Name)" -ExpectedIcon $referenceIcon
            }
            continue
        }

        # A package: the manifest is what the store reads, so read it too.
        $target = Join-Path $tempRoot $label
        $null = New-Item -ItemType Directory -Path $target -Force
        [System.IO.Compression.ZipFile]::ExtractToDirectory($archive.FullName, $target)
        $manifest = Get-ChildItem -LiteralPath $target -Recurse -File | Where-Object { $_.Name -ieq 'AppxManifest.xml' } | Select-Object -First 1
        if (-not $manifest) {
            $failures.Add("${label}: no AppxManifest.xml, so it is not a package the store could take")
        } else {
            [xml]$xml = [IO.File]::ReadAllText($manifest.FullName)
            $identityVersion = $xml.Package.Identity.Version
            $displayName = $xml.Package.Properties.DisplayName
            # A package identity carries four fields; the release version is the
            # first three of them.
            if (-not ("$identityVersion".StartsWith($Version + '.', [StringComparison]::Ordinal))) {
                $failures.Add("${label}: the manifest declares version $identityVersion, which is not $Version")
            }
            if ("$displayName" -notlike "*$Product*") {
                $failures.Add("${label}: the manifest display name '$displayName' does not name $Product")
            }
            Write-Output ('  {0,-52} {1,-7} manifest version={2} display={3}' -f $label, 'ok', $identityVersion, $displayName)
            $checked.Add($label)
        }
        foreach ($file in @(Get-ChildItem -LiteralPath $target -Recurse -File -Filter *.exe)) {
            Assert-Binary -File $file.FullName -Label "$label!$($file.Name)" -ExpectedIcon $referenceIcon
        }
    }
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) {
    Write-Output ''
    foreach ($failure in $failures) { Write-Output "FAIL $failure" }
    Write-Output ''
    Write-Output "binary identity: $($failures.Count) failure(s), $($checked.Count) artefact(s) read"
    exit 1
}

# Say exactly what was proved: with -SkipIcon the mark was not compared, and a
# verdict that claimed it would be the kind of green check this gate exists to
# stop.
$claim = if ($SkipIcon) { "the product, the publisher and version $Version" }
else { "the mark, the product, the publisher and version $Version" }
Write-Output "binary identity: every artefact under $Path carries $claim"
exit 0
