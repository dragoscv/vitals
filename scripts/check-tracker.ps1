<#
.SYNOPSIS
    Fails when docs/tracker.csv and docs/TRACKER.md disagree with each other or
    with their own shape.

.DESCRIPTION
    The tracker is hand-edited a row at a time, which is exactly the record that
    rots silently: a field pasted twice shifts every column of a row, an id
    typed once too often breaks "one row per item", and a reference to an id
    that no longer exists reads as a working cross-link until someone follows
    it. check-drift.ps1 already owns the CSV's own shape (a row of the wrong
    arity, a duplicated id, a status outside the vocabulary). This gate is the
    other half: the agreement between the two files, which nothing else checks.

    Checks, each a defect that has actually been seen:

      1. Every record has the header's column count. A quoted or pasted field
         is how a row silently shifts by one column.
      2. Ids are unique.
      3. The id names the slice it sits in: `S26-19` in slice `S26`. A mistyped
         id is otherwise invisible until a reference stops resolving.
      4. Every id referenced in docs/TRACKER.md has a row in the CSV — a
         dangling reference points at nothing.
      5. Every slice in the CSV has a section in docs/TRACKER.md. The two files
         are deliberately not one-to-one (TRACKER.md records the reasoning, the
         CSV the per-item state — it says so at the top of its own text), so
         the reverse direction is checked at the slice grain: a slice with rows
         and no section is the defect, not a done row without prose.
      6. The status is a lowercase hyphenated token drawn from the vocabulary
         actually in use, which is read from the file rather than declared
         here, and printed in the message so a typo is answered with the set
         it should have come from.
        7. No record ends inside a quoted field. A missing closing quote swallows
           the rest of the file into one field, which leaves the arity of every
           record intact and simply *drops* the rows it swallowed: checks 1, 2 and
           6 then report nothing at all about a file whose tail is invisible.

    Read-only, a few tens of milliseconds, one line per problem, and a final
    `tracker: N problem(s)` line. A clean tree prints that line and nothing
    else, so it is safe inside scripts/verify.ps1.

.PARAMETER Csv
    The tracker CSV. Default docs/tracker.csv. Point it at a copy to prove the
    gate fails without touching the real file.

.PARAMETER Md
    The narrative tracker. Default docs/TRACKER.md.

.EXAMPLE
    pwsh -NoProfile -File scripts/check-tracker.ps1

.EXAMPLE
    pwsh -NoProfile -File scripts/check-tracker.ps1 -Csv .copilot-tmp/tracker-short.csv

.NOTES
    Exit code is the number of problems (capped at 255), so a wrapper can read
    how many there were; 0 means the conventions hold.
#>
[CmdletBinding()]
param(
    [string]$Csv = 'docs/tracker.csv',
    [string]$Md = 'docs/TRACKER.md'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Resolve-UnderRoot {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) { return $Path }
    return (Join-Path $root $Path)
}

$csvPath = Resolve-UnderRoot $Csv
$mdPath = Resolve-UnderRoot $Md

foreach ($f in @($csvPath, $mdPath)) {
    if (-not (Test-Path -LiteralPath $f)) {
        Write-Host "$f not found"
        exit 1
    }
}

$problems = [System.Collections.Generic.List[string]]::new()

# A real CSV reader, not `-split ','` and not Import-Csv. A field here holds
# commas and `""` escapes (the `detail` column holds both, and newlines), and
# Import-Csv pads a row with too FEW fields to the header's width, so the one
# mutation this gate exists for — a field pasted twice or dropped — reads as a
# correct row. This honours quotes, keeps newlines inside a quoted field, takes
# LF and CRLF endings alike, and remembers the physical line each record starts
# on so a problem can be found in an editor.
function Read-CsvRecords {
    param(
        [string]$Text,
        # Handed the defects this reader cannot see for itself — see the
        # `$inQuotes` check at the end of the loop, which is the one shape that
        # makes a record vanish rather than look wrong.
        [System.Collections.Generic.List[string]]$Problems
    )
    $records = [System.Collections.Generic.List[object]]::new()
    $fields = [System.Collections.Generic.List[string]]::new()
    $sb = [System.Text.StringBuilder]::new()
    $inQuotes = $false
    $quoteOpenLine = 0
    $line = 1
    $recordLine = 1
    $i = 0
    $n = $Text.Length
    while ($i -lt $n) {
        $c = $Text[$i]
        if ($inQuotes) {
            if ($c -eq '"') {
                if (($i + 1) -lt $n -and $Text[$i + 1] -eq '"') {
                    [void]$sb.Append('"'); $i += 2
                } else {
                    $inQuotes = $false; $i += 1
                }
            } else {
                if ($c -eq "`n") { $line += 1 }
                [void]$sb.Append($c); $i += 1
            }
            continue
        }
        if ($c -eq '"') { $inQuotes = $true; $quoteOpenLine = $line; $i += 1; continue }
        if ($c -eq ',') { $fields.Add($sb.ToString()); [void]$sb.Clear(); $i += 1; continue }
        if ($c -eq "`r") { $i += 1; continue }
        if ($c -eq "`n") {
            $fields.Add($sb.ToString()); [void]$sb.Clear()
            $records.Add([pscustomobject]@{ Line = $recordLine; Fields = $fields.ToArray() })
            $fields = [System.Collections.Generic.List[string]]::new()
            $line += 1; $recordLine = $line
            $i += 1; continue
        }
        [void]$sb.Append($c); $i += 1
    }
    # The file carries no trailing newline, so the last record is still in
    # flight when the loop ends.
    if ($sb.Length -gt 0 -or $fields.Count -gt 0) {
        $fields.Add($sb.ToString())
        $records.Add([pscustomobject]@{ Line = $recordLine; Fields = $fields.ToArray() })
    }
    # The defect this reader must report itself: an unterminated quote does not
    # make a record look wrong, it makes every later record disappear into this
    # one — and a vanished row is exactly what nothing else here can detect.
    # `-ne $null`, not a bare truthiness test: an empty List[string] converts to
    # `$false` (PowerShell reads a collection's emptiness, not its handle), so
    # the obvious `-and $Problems` would skip this report until some earlier
    # check had already added a problem.
    if ($inQuotes -and $null -ne $Problems) {
        $Problems.Add("${Csv}: the file ends inside a quoted field opened on line $quoteOpenLine - a closing quote is missing, so the rest of the file was swallowed into that one field and no record after it exists as far as this gate is concerned")
    }
    return $records
}

$csvText = [System.IO.File]::ReadAllText($csvPath)
$records = @(Read-CsvRecords -Text $csvText -Problems $problems)
if ($records.Count -eq 0) {
    Write-Host "${Csv} is empty"
    exit 1
}

$expected = 8   # id,slice,category,title,detail,decision,status,verification
$header = $records[0].Fields
$col = @{}
for ($i = 0; $i -lt $header.Count; $i++) {
    $name = $header[$i].Trim()
    if (-not $col.ContainsKey($name)) { $col[$name] = $i }
}
if ($header.Count -ne $expected) {
    $problems.Add("${Csv}: header has $($header.Count) columns, expected $expected (id,slice,category,title,detail,decision,status,verification)")
}
$missing = @('id', 'slice', 'status' | Where-Object { -not $col.ContainsKey($_) })
if ($missing.Count) {
    $problems.Add("${Csv}: header is missing column(s) $($missing -join ', ') — nothing can be checked without them")
    $problems | ForEach-Object { Write-Host $_ }
    Write-Host "tracker: $($problems.Count) problem(s)"
    exit [Math]::Min($problems.Count, 255)
}

# ── shape (1), and the values every other check reads ────────────────────
$rows = [System.Collections.Generic.List[object]]::new()
for ($r = 1; $r -lt $records.Count; $r++) {
    $rec = $records[$r]
    $f = $rec.Fields
    $id = if ($col.id -lt $f.Count) { $f[$col.id] } else { '' }
    $slice = if ($col.slice -lt $f.Count) { $f[$col.slice] } else { '' }
    $status = if ($col.status -lt $f.Count) { $f[$col.status] } else { '' }
    $rows.Add([pscustomobject]@{
            Row = $r; Line = $rec.Line; Id = $id; Slice = $slice; Status = $status; Count = $f.Count
        })
    if ($f.Count -ne $expected) {
        $label = if ($id) { "id '$id'" } else { 'no id field' }
        $problems.Add("${Csv}: row $r (line $($rec.Line), $label) has $($f.Count) columns, expected $expected — a field is missing, quoted or pasted twice")
    }
}

# ── (2) unique ids, and (3) id names its slice ───────────────────────────
# Ids are matched ordinally. PowerShell's own `-eq`, `-match` and hashtable
# keys all fold case, which would let `s26-19` stand in for `S26-19` and turn a
# typo into a silent duplicate-or-not decision.
$firstRow = [System.Collections.Generic.Dictionary[string, int]]::new([System.StringComparer]::Ordinal)
foreach ($row in $rows) {
    if (-not $row.Id) {
        $problems.Add("${Csv}: row $($row.Row) (line $($row.Line)) has an empty id")
        continue
    }
    if ($firstRow.ContainsKey($row.Id)) {
        $problems.Add("${Csv}: row $($row.Row) id '$($row.Id)' is a duplicate (first seen at row $($firstRow[$row.Id]))")
    } else {
        $firstRow[$row.Id] = $row.Row
    }
    if ($row.Id -cnotmatch '^S(\d{1,2})-\d{2}$') {
        $problems.Add("${Csv}: row $($row.Row) id '$($row.Id)' is not shaped S<slice>-<nn>")
    } elseif ($row.Slice -and ("S$($Matches[1])") -cne $row.Slice) {
        $problems.Add("${Csv}: row $($row.Row) id '$($row.Id)' names slice 'S$($Matches[1])' but the slice field says '$($row.Slice)'")
    }
}

$mdText = [System.IO.File]::ReadAllText($mdPath)

# ── (4) every id referenced in the markdown exists in the CSV ────────────
$referenced = [regex]::Matches($mdText, '\bS\d{1,2}-\d{2}\b') |
    ForEach-Object { $_.Value } | Sort-Object -Unique
foreach ($ref in $referenced) {
    if (-not $firstRow.ContainsKey($ref)) {
        $problems.Add("${Md}: id '$ref' is referenced but has no row in ${Csv}")
    }
}

# ── (5) every slice in the CSV has a section in the markdown ─────────────
# A slice is covered when its token appears at all: the Slices table row
# (`| S26 `), a slice-level heading, or any of its ids. Word boundaries keep
# `S2` from matching `S26`.
foreach ($g in ($rows | Where-Object { $_.Slice } | Group-Object Slice | Sort-Object Name)) {
    if ([regex]::IsMatch($mdText, "\b$([regex]::Escape($g.Name))\b")) { continue }
    $first = $g.Group | Sort-Object Row | Select-Object -First 1
    $problems.Add("${Md}: slice '$($g.Name)' (first at row $($first.Row), id '$($first.Id)') has no section — $($g.Count) row(s) with no reasoning recorded")
}

# ── (6) the status is drawn from the vocabulary in use ───────────────────
# The vocabulary is read from the file, never declared here. (The legend in
# TRACKER.md says `doing`; the CSV has used `in-progress` for every row that
# ever needed it, and the CSV is the file of record.) The test is case
# SENSITIVE on purpose — `-match` and `-eq` fold case and would let `Done`
# through — and the message prints the vocabulary so a typo is answered with
# the set it should have come from.
$statusShape = '^[a-z][a-z0-9-]*$'
$vocabulary = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($row in $rows) {
    if ($row.Status -cmatch $statusShape) { [void]$vocabulary.Add($row.Status) }
}
$vocabText = ($vocabulary | Sort-Object) -join ', '
foreach ($row in $rows) {
    if (-not $row.Status) {
        $problems.Add("${Csv}: row $($row.Row) id '$($row.Id)' has an empty status")
    } elseif ($row.Status -cnotmatch $statusShape) {
        $problems.Add("${Csv}: row $($row.Row) id '$($row.Id)' has status '$($row.Status)'; expected a lowercase hyphenated token from the vocabulary in use ($vocabText)")
    }
}

foreach ($p in $problems) { Write-Host $p }
Write-Host "tracker: $($problems.Count) problem(s)"
exit [Math]::Min($problems.Count, 255)
