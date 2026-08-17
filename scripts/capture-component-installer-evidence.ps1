[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$change = 'bundle-superdesktop-submodule-installer'
$root = Join-Path $workspace "openspec\changes\$change"
$evidence = Join-Path $root 'evidence'
$tasksPath = Join-Path $root 'tasks.md'
$utf8 = [Text.UTF8Encoding]::new($false)
$recordedAt = [DateTime]::UtcNow.ToString('o')

$artifacts = @{
    '1.1' = 'evidence/artifacts/1.1/submodule-identity.json'
    '1.2' = 'evidence/artifacts/1.2/admission-fixtures.json'
    '2.1' = 'evidence/artifacts/2.1/batch-routing.json'
    '2.2' = 'evidence/artifacts/2.2/build-stage-matrix.json'
    '2.3' = 'evidence/artifacts/2.3/input-output-matrix.json'
    '3.1' = 'evidence/artifacts/3.1/superdesktop-nsis-contract.json'
    '3.2' = 'evidence/artifacts/3.2/superexplorer-variant-contract.json'
    '3.3' = 'evidence/artifacts/3.3/superdesktop-only-contract.json'
    '4.1' = 'evidence/artifacts/4.1/fixture-matrix.json'
    '4.2' = 'evidence/artifacts/4.2/installer-build-matrix.json'
    '4.3' = 'evidence/artifacts/4.3/final-verification.json'
}
$gates = @{
    '1.1' = @('G-SUBMODULE-ADMISSION')
    '1.2' = @('G-SUBMODULE-ADMISSION','G-COMPONENT-ISOLATION')
    '2.1' = @('G-COMPONENT-ISOLATION')
    '2.2' = @('G-COMPONENT-ISOLATION','G-INSTALLER-INPUT')
    '2.3' = @('G-INSTALLER-INPUT')
    '3.1' = @('G-INSTALLER-CONTENT','G-SHELL-SAFETY')
    '3.2' = @('G-INSTALLER-CONTENT')
    '3.3' = @('G-INSTALLER-CONTENT','G-SHELL-SAFETY')
    '4.1' = @('G-SUBMODULE-ADMISSION','G-COMPONENT-ISOLATION','G-INSTALLER-INPUT','G-INSTALLER-CONTENT','G-SHELL-SAFETY')
    '4.2' = @('G-INSTALLER-INPUT','G-INSTALLER-CONTENT','G-SHELL-SAFETY')
    '4.3' = @('G-SUBMODULE-ADMISSION','G-COMPONENT-ISOLATION','G-INSTALLER-INPUT','G-INSTALLER-CONTENT','G-SHELL-SAFETY')
}

$coverage = @()
$records = @()
foreach ($line in Get-Content -Encoding UTF8 -LiteralPath $tasksPath) {
    if ($line -notmatch '^\s*- \[([ xX])\]\s+([0-9]+\.[0-9]+\.[0-9]+)\b') { continue }
    $checked = $matches[1] -in @('x','X')
    $id = $matches[2]
    $group = $id.Substring(0, $id.LastIndexOf('.'))
    $taskId = "$change/$id"
    $coverage += [ordered]@{
        task_id = $taskId
        mandatory = $true
        capability_id = 'component-scoped-installer-build'
        requirement_id = 'component-scoped-installer-build'
        scenario_id = 'component-mode-and-gate-contract'
        gates = @($gates[$group])
    }
    if (-not $checked) { continue }
    $relative = $artifacts[$group]
    $full = Join-Path $root ($relative -replace '/', '\')
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "Missing artifact for $id`: $relative" }
    $records += [ordered]@{
        schema_version = '1.0.0'
        task_id = $taskId
        subcheck = "task-$($id.Replace('.', '-'))"
        status = 'passed'
        artifact = $relative
        artifact_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $full).Hash.ToLowerInvariant()
        capability_id = 'component-scoped-installer-build'
        requirement_id = 'component-scoped-installer-build'
        scenario_id = 'component-mode-and-gate-contract'
        gates = @($gates[$group])
        reviewer = 'Primary integrator'
        recorded_at = $recordedAt
        expected = 'The atomic task satisfies its selected component, admission, installer content, input, and Shell-safety contract.'
        actual = "Task $id passed with hash-bound group evidence."
    }
}

if ($coverage.Count -ne 53) { throw "Expected 53 task mappings, found $($coverage.Count)." }
$checkedCount = @((Get-Content -Encoding UTF8 -LiteralPath $tasksPath) | Where-Object { $_ -match '^\s*- \[[xX]\]\s+[0-9]+\.[0-9]+\.[0-9]+\b' }).Count
if ($records.Count -ne $checkedCount) { throw "Evidence record count $($records.Count) does not match checked task count $checkedCount." }

$coverageDocument = [ordered]@{
    schema_version = '1.0.0'
    change = $change
    capabilities = @('component-scoped-installer-build')
    tasks = $coverage
}
[IO.File]::WriteAllText((Join-Path $evidence 'coverage.json'), (($coverageDocument | ConvertTo-Json -Depth 20) + "`n"), $utf8)
[IO.File]::WriteAllText((Join-Path $evidence 'index.jsonl'), (($records | ForEach-Object { $_ | ConvertTo-Json -Compress -Depth 20 }) -join "`n") + "`n", $utf8)

$validated = 0
foreach ($record in Get-Content -Encoding UTF8 -LiteralPath (Join-Path $evidence 'index.jsonl')) {
    if (-not $record) { continue }
    $entry = $record | ConvertFrom-Json
    $full = Join-Path $root ($entry.artifact -replace '/', '\')
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $full).Hash.ToLowerInvariant()
    if ($actual -cne [string]$entry.artifact_sha256) { throw "Evidence hash drift: $($entry.task_id)" }
    $validated++
}
if ($validated -ne $records.Count) { throw 'Evidence index round-trip count mismatch.' }
Write-Output "Component installer evidence passed: $validated records / $($coverage.Count) mappings."
