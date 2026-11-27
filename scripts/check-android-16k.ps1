<#
.SYNOPSIS
    Fails when a built Android bundle ships a 64-bit native library that is not
    aligned to 16 KB.

.DESCRIPTION
    Android devices with 16 KB memory pages cannot load a shared object whose
    PT_LOAD segments are aligned to 4 KB, and Google Play requires 16 KB support
    for new apps and updates. The alignment lives in the ELF program headers of
    every .so inside the bundle, so it is not visible in the source, in the
    Gradle files or in the merged manifest: the only honest evidence is the
    built archive. This reads each `lib/<abi>/*.so` entry and asserts, for every
    64-bit ABI, that the smallest p_align across its PT_LOAD segments is at
    least 16384.

    32-bit ABIs (armeabi-v7a, x86) are exempt and are counted rather than
    judged: a 16 KB-page device runs 64-bit code, and the NDK has no 16 KB
    alignment mode for the 32-bit ABIs. They are still reported, because a
    bundle that suddenly loses its 64-bit libraries would otherwise pass this
    check by finding nothing to judge — a bundle with native libraries and zero
    64-bit ones is a failure, not a pass.

    An ABI this script does not recognise is a failure too, and for the same
    reason as the zero-64-bit rule: a new ABI has to be classified deliberately
    by whoever adds it, not exempted by being unknown.

    The archives are read through the zip API, one entry at a time, never
    extracted to disk: these bundles contain entries whose names collide on a
    case-insensitive filesystem (`res/-m.xml` and `res/-M.xml` in the same
    archive), so `ExtractToDirectory` fails on Windows with "The file ... already
    exists" — measured 2026-11-27, which is how this script came to read the
    entries in memory.

    The modest cost of that choice is that an ELF cannot be handed to an
    external tool by path; the program headers are parsed here instead, which is
    forty lines and works on any OS with PowerShell.

    The only skip is having no archive at all, which is the normal state on a
    checkout that has never built the app. It is printed, never silent, and
    -RequireBuilt (or -Strict) turns it into a failure where a build has just
    run.

    -SelfTest proves the check can fail, which is the only thing that makes a
    green run mean anything: it writes a deliberately 4 KB-aligned and a
    16 KB-aligned 64-bit library into two synthetic bundles and asserts that the
    first is rejected and the second accepted.

.EXAMPLE
    pwsh -NoProfile -File scripts/check-android-16k.ps1 -SelfTest
    pwsh -NoProfile -File scripts/check-android-16k.ps1 -RequireBuilt -Strict
    pwsh -NoProfile -File scripts/check-android-16k.ps1 -Path .copilot-tmp/android-unsigned
#>
[CmdletBinding()]
param(
    # Files or directories to read. Defaults to the three Gradle output trees
    # under apps/android, which is where a local build leaves its artefacts.
    [string[]]$Path,

    # No archive is a failure instead of a skip, so a run that checked nothing
    # cannot look green in a job that has just built one.
    [switch]$RequireBuilt,

    # The same refusal under the name CI reads as "prove it".
    [switch]$Strict,

    # Assert the checker itself rejects a misaligned library.
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# The requirement, in bytes. Spelled once: a device page size and the alignment
# Android asks for are the same number, and two copies of it would drift.
$Required = 16384

# Which ABIs carry 64-bit code that must be aligned, and which are 32-bit and
# therefore out of scope. Anything else is unclassified and fails.
$Abi64 = @('arm64-v8a', 'x86_64', 'riscv64')
$Abi32 = @('armeabi-v7a', 'armeabi', 'x86')

function Get-MinLoadAlign {
    <#
        The smallest p_align among the PT_LOAD segments of a 64-bit
        little-endian ELF, or the reason it could not be determined. A reason is
        never a silent zero: an ELF this cannot read is a library whose
        alignment is unknown, and "unknown" must not read as "fine".
    #>
    param([byte[]]$Bytes)

    if ($Bytes.Length -lt 64) { return [pscustomobject]@{ Align = $null; Why = 'shorter than an ELF header' } }
    if (-not ($Bytes[0] -eq 0x7F -and $Bytes[1] -eq 0x45 -and $Bytes[2] -eq 0x4C -and $Bytes[3] -eq 0x46)) {
        return [pscustomobject]@{ Align = $null; Why = 'not an ELF file' }
    }
    # e_ident[EI_CLASS] 2 = ELF64, e_ident[EI_DATA] 1 = two's complement LSB.
    if ($Bytes[4] -ne 2) { return [pscustomobject]@{ Align = $null; Why = ('ELF class {0}, not 64-bit' -f $Bytes[4]) } }
    if ($Bytes[5] -ne 1) { return [pscustomobject]@{ Align = $null; Why = 'big-endian, which no Android ABI is' } }

    $phoff = [BitConverter]::ToUInt64($Bytes, 0x20)
    $phentsize = [BitConverter]::ToUInt16($Bytes, 0x36)
    $phnum = [BitConverter]::ToUInt16($Bytes, 0x38)
    if ($phentsize -lt 56) { return [pscustomobject]@{ Align = $null; Why = ('program header entry size {0}' -f $phentsize) } }
    if ($phoff + ([uint64]$phnum * $phentsize) -gt [uint64]$Bytes.Length) {
        return [pscustomobject]@{ Align = $null; Why = 'program headers run past the end of the file' }
    }

    $min = $null
    $loads = 0
    for ($i = 0; $i -lt $phnum; $i++) {
        $o = [int]($phoff + $i * $phentsize)
        if ([BitConverter]::ToUInt32($Bytes, $o) -ne 1) { continue }  # PT_LOAD
        $loads++
        $align = [BitConverter]::ToUInt64($Bytes, $o + 0x30)
        if ($null -eq $min -or $align -lt $min) { $min = $align }
    }
    if ($loads -eq 0) { return [pscustomobject]@{ Align = $null; Why = 'no PT_LOAD segment' } }
    return [pscustomobject]@{ Align = $min; Why = $null }
}

function Get-ArchiveVerdict {
    <#
        Every native library in one archive, classified. Returns a summary the
        caller prints and judges; nothing is written to the console here, so the
        self-test can call it on a fixture without the output looking like a
        real run.
    #>
    param([string]$File)

    $libs = [System.Collections.Generic.List[object]]::new()
    $zip = [System.IO.Compression.ZipFile]::OpenRead($File)
    try {
        foreach ($entry in $zip.Entries) {
            # An APK stores `lib/<abi>/<name>.so`; an app bundle stores
            # `base/lib/<abi>/<name>.so`, and a feature module prefix of its own.
            # Matching on the `lib` segment wherever it sits covers both, and the
            # fixture below is what proved the difference matters — a rule that
            # only accepted the APK layout found no library at all in an .aab and
            # then passed, which is the vacuity this script exists to prevent.
            $parts = $entry.FullName.Split('/')
            $libAt = [array]::IndexOf($parts, 'lib')
            if ($libAt -lt 0 -or $parts.Count -lt $libAt + 3) { continue }
            if (-not $parts[$parts.Count - 1].EndsWith('.so')) { continue }
            $abi = $parts[$libAt + 1]
            $leaf = $parts[$parts.Count - 1]

            $stream = $entry.Open()
            try {
                $memory = [System.IO.MemoryStream]::new()
                $stream.CopyTo($memory)
                $bytes = $memory.ToArray()
            } finally { $stream.Dispose() }

            $size = if ($abi -in $Abi32) { '32' } elseif ($abi -in $Abi64) { '64' } else { '?' }
            $parsed = Get-MinLoadAlign -Bytes $bytes
            $libs.Add([pscustomobject]@{
                    Abi     = $abi
                    Name    = $leaf
                    Width   = $size
                    Align   = $parsed.Align
                    Why     = $parsed.Why
                    Aligned = ($null -ne $parsed.Align -and $parsed.Align -ge $Required)
                })
        }
    } finally { $zip.Dispose() }

    return [pscustomobject]@{ File = $File; Libs = $libs }
}

function Test-VerdictOk {
    <#
        The rule, in one place: every 64-bit library aligned, no unclassified
        ABI, and at least one 64-bit library whenever the archive carries native
        code at all.
    #>
    param($Verdict)

    if ($Verdict.Libs.Count -eq 0) { return $true }
    $sixtyFour = @($Verdict.Libs | Where-Object { $_.Width -eq '64' })
    foreach ($lib in $Verdict.Libs) {
        if ($lib.Width -eq '64' -and -not $lib.Aligned) { return $false }
        if ($lib.Width -eq '?') { return $false }
    }
    if ($sixtyFour.Count -eq 0) { return $false }
    return $true
}

# ── The self-test, before anything else reads real artefacts ───────────
#
# A fixture that cannot fail is decoration. These two libraries differ in one
# field — p_align — and the assertion is that the verdicts differ with it.

function New-FixtureElf {
    param([uint64]$Align)
    $bytes = [byte[]]::new(128)
    $bytes[0] = 0x7F; $bytes[1] = 0x45; $bytes[2] = 0x4C; $bytes[3] = 0x46
    $bytes[4] = 2; $bytes[5] = 1; $bytes[6] = 1                      # ELF64, LE, version 1
    [BitConverter]::GetBytes([uint16]2).CopyTo($bytes, 0x10)          # e_type ET_EXEC
    [BitConverter]::GetBytes([uint16]0xB7).CopyTo($bytes, 0x12)       # e_machine aarch64
    [BitConverter]::GetBytes([uint32]1).CopyTo($bytes, 0x14)          # e_version
    [BitConverter]::GetBytes([uint64]0x40).CopyTo($bytes, 0x20)       # e_phoff
    [BitConverter]::GetBytes([uint16]64).CopyTo($bytes, 0x34)         # e_ehsize
    [BitConverter]::GetBytes([uint16]56).CopyTo($bytes, 0x36)         # e_phentsize
    [BitConverter]::GetBytes([uint16]1).CopyTo($bytes, 0x38)          # e_phnum
    [BitConverter]::GetBytes([uint32]1).CopyTo($bytes, 0x40)          # p_type PT_LOAD
    [BitConverter]::GetBytes([uint32]5).CopyTo($bytes, 0x44)          # p_flags R+X
    [BitConverter]::GetBytes($Align).CopyTo($bytes, 0x70)             # p_align
    return $bytes
}

if ($SelfTest) {
    $probe = Join-Path $root '.copilot-tmp/android-16k-selftest'
    if (Test-Path $probe) { Remove-Item -Recurse -Force $probe }
    $null = New-Item -ItemType Directory -Force -Path $probe
    $cases = @(
        @{ Name = 'aligned.aab'; Align = [uint64]16384; Expect = $true }
        @{ Name = 'misaligned.aab'; Align = [uint64]4096; Expect = $false }
    )
    $bad = 0
    foreach ($case in $cases) {
        $file = Join-Path $probe $case.Name
        $zip = [System.IO.Compression.ZipFile]::Open($file, 'Create')
        try {
            $entry = $zip.CreateEntry('base/lib/arm64-v8a/libfixture.so')
            $stream = $entry.Open()
            try { $stream.Write((New-FixtureElf -Align $case.Align), 0, 128) } finally { $stream.Dispose() }
        } finally { $zip.Dispose() }
        $verdict = Get-ArchiveVerdict -File $file
        $ok = Test-VerdictOk -Verdict $verdict
        $expected = 'refuse'
        if ($case.Expect) { $expected = 'accept' }
        $got = 'refuse'
        if ($ok) { $got = 'accept' }
        if ($got -ne $expected) {
            $bad++
            Write-Host ("check-android-16k: self-test FAIL {0} (p_align {1}): expected to {2}, the check says {3}" -f $case.Name, $case.Align, $expected, $got) -ForegroundColor Red
        } else {
            Write-Host ("check-android-16k: self-test ok {0} (p_align {1}) -> {2}" -f $case.Name, $case.Align, $got) -ForegroundColor DarkGray
        }
    }
    Remove-Item -Recurse -Force $probe -ErrorAction SilentlyContinue
    if ($bad -gt 0) { exit 1 }
    Write-Host 'check-android-16k: self-test PASS — a 4 KB-aligned 64-bit library is refused and a 16 KB-aligned one is accepted' -ForegroundColor Green
    exit 0
}

# ── Read the artefacts ─────────────────────────────────────────────────

$roots = $Path
if (-not $roots -or $roots.Count -eq 0) {
    $roots = @('apps/android/app/build/outputs', 'apps/android/tv/build/outputs', 'apps/android/wear/build/outputs') | ForEach-Object { Join-Path $root $_ }
}

$archives = [System.Collections.Generic.List[string]]::new()
foreach ($item in $roots) {
    $resolved = $item
    if (-not [System.IO.Path]::IsPathRooted($resolved)) { $resolved = Join-Path $root $item }
    if (Test-Path -LiteralPath $resolved -PathType Leaf) { $archives.Add($resolved); continue }
    if (-not (Test-Path -LiteralPath $resolved)) { continue }
    Get-ChildItem -LiteralPath $resolved -Recurse -File -Include *.aab, *.apk |
        # intermediates holds unpacked copies of everything the packager reads;
        # asserting on those would double-count and judge pre-package bytes.
        Where-Object { $_.FullName -notmatch '[\\/]intermediates[\\/]' } |
        ForEach-Object { $archives.Add($_.FullName) }
}

if ($archives.Count -eq 0) {
    if ($RequireBuilt -or $Strict) {
        Write-Host 'check-android-16k: -RequireBuilt was set, but there is no built .aab or .apk here to verify 16 KB alignment on' -ForegroundColor Red
        exit 1
    }
    Write-Host 'check-android-16k: no built .aab or .apk here — nothing to check' -ForegroundColor Yellow
    exit 0
}

$failures = [System.Collections.Generic.List[string]]::new()
$totalLibs = 0
$total64 = 0

foreach ($archive in $archives) {
    $verdict = Get-ArchiveVerdict -File $archive
    $name = Split-Path -Leaf $archive
    if ($verdict.Libs.Count -eq 0) {
        Write-Host "check-android-16k: $name carries no native library, nothing to align"
        continue
    }
    $sixtyFour = @($verdict.Libs | Where-Object { $_.Width -eq '64' })
    $thirtyTwo = @($verdict.Libs | Where-Object { $_.Width -eq '32' })
    $unknown = @($verdict.Libs | Where-Object { $_.Width -eq '?' })
    $totalLibs += $verdict.Libs.Count
    $total64 += $sixtyFour.Count

    foreach ($lib in $verdict.Libs) {
        $where = "$name lib/$($lib.Abi)/$($lib.Name)"
        if ($lib.Width -eq '?') {
            $failures.Add("$where uses an ABI this check does not classify ($($lib.Abi)); classify it deliberately rather than exempting it")
        } elseif ($lib.Width -eq '64') {
            if ($null -ne $lib.Why) {
                $failures.Add("${where}: 64-bit library whose alignment could not be read — $($lib.Why); a library of unknown alignment cannot be called aligned")
            } elseif (-not $lib.Aligned) {
                $failures.Add("${where}: smallest PT_LOAD p_align is $($lib.Align) bytes, below $Required; a 16 KB-page device cannot load it")
            }
        }
    }
    if ($null -eq $unknown -or $unknown.Count -eq 0) {
        if ($sixtyFour.Count -eq 0) {
            $failures.Add("$name carries native libraries but none for a 64-bit ABI; the alignment rule would pass by finding nothing to judge")
        }
    }
    Write-Host ("check-android-16k: {0}: {1} native libraries — {2} 64-bit, {3} 32-bit exempt, {4} unclassified" -f $name, $verdict.Libs.Count, $sixtyFour.Count, $thirtyTwo.Count, $unknown.Count)
}

foreach ($failure in $failures) { Write-Host "check-android-16k: FAIL $failure" -ForegroundColor Red }

if ($failures.Count -gt 0) {
    Write-Host "check-android-16k: $($failures.Count) problem(s) across $($archives.Count) archive(s)" -ForegroundColor Red
    exit 1
}

Write-Host "check-android-16k: $total64 64-bit library/libraries across $($archives.Count) archive(s) are all aligned to at least $Required bytes, out of $totalLibs native libraries read" -ForegroundColor Green
exit 0
