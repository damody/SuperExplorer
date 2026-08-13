$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$change = Join-Path $workspace 'openspec\changes\mft-sqlite-foreground-persistence'
$tasksPath = Join-Path $change 'tasks.md'
$tasks = Get-Content -Raw -Encoding UTF8 -LiteralPath $tasksPath
$taskRows = [regex]::Matches($tasks, '(?m)^- \[(?<done>[ x])\] (?<id>\d+\.\d+\.\d+) (?<text>.+)$')
if ($taskRows.Count -ne 66) { throw "Expected 66 detailed tasks; found $($taskRows.Count)." }
$ids = @($taskRows | ForEach-Object { $_.Groups['id'].Value })
if (($ids | Sort-Object -Unique).Count -ne $ids.Count) { throw 'Detailed task IDs are not unique.' }
foreach ($required in @('Gate','Evidence')) {
    if ($tasks -notmatch [regex]::Escape($required)) { throw "Detailed task metadata is missing $required." }
}
$artifactPaths = @(
    (Join-Path $change 'proposal.md')
    (Join-Path $change 'design.md')
    (Join-Path $change 'specs\mft-sqlite-foreground-persistence\spec.md')
    $tasksPath
    (Join-Path $workspace 'openspec\changes\event-driven-mft-index-updates\proposal.md')
    (Join-Path $workspace 'openspec\changes\event-driven-mft-index-updates\design.md')
    (Join-Path $workspace 'openspec\changes\event-driven-mft-index-updates\specs\event-driven-mft-index\spec.md')
)
foreach ($path in $artifactPaths) {
    $content = Get-Content -Raw -Encoding UTF8 -LiteralPath $path
    if ($content -match '(?im)^\s*(TODO|TBD|FIXME|PLACEHOLDER)\b') {
        throw "Placeholder marker remains in $path"
    }
}
$oldSpec = Get-Content -Raw -Encoding UTF8 -LiteralPath $artifactPaths[-1]
if ($oldSpec -notmatch 'governed by `mft-sqlite-foreground-persistence`' -or
    $oldSpec -notmatch 'New durable state SHALL use the atomic SQLite contract') {
    throw 'Related event-driven spec does not explicitly record the superseding durability contract.'
}
& openspec validate mft-sqlite-foreground-persistence --strict
if ($LASTEXITCODE -ne 0) { throw 'Strict validation failed for the SQLite change.' }
& openspec validate event-driven-mft-index-updates --strict
if ($LASTEXITCODE -ne 0) { throw 'Strict validation failed for the related event-driven change.' }
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'validate_mft_evidence.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Evidence-index validation failed.' }
Write-Output "MFT SQLite detailed-task/placeholder/contradiction/strict validation PASS ($($taskRows.Count) tasks)."
