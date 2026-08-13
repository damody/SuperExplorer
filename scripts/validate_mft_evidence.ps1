param(
    [string]$EvidenceRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target\openspec-evidence\mft-sqlite-foreground-persistence'),
    [string]$OutputPath = (Join-Path $EvidenceRoot 'evidence-index.json'),
    [switch]$PrintSourceFingerprintOnly
)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$revision = (& git -C $workspace rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or -not $revision) { throw 'Unable to resolve source revision.' }

$sourcePaths = @(
    'Cargo.toml',
    'Cargo.lock',
    'crates/explorer-app/Cargo.toml',
    'crates/explorer-app/src/application.rs',
    'crates/explorer-app/src/bin/mft_service.rs',
    'crates/explorer-app/src/folder_size_service.rs',
    'crates/explorer-app/src/mft_focus.rs',
    'crates/explorer-app/src/mft_journal.rs',
    'crates/explorer-app/src/mft_migration.rs',
    'crates/explorer-app/src/mft_persistence.rs',
    'crates/explorer-app/src/mft_query.rs',
    'crates/explorer-app/src/mft_runtime.rs',
    'crates/explorer-app/src/mft_size_map.rs',
    'crates/explorer-app/src/mft_sqlite.rs',
    'installer/SuperExplorer.nsi',
    'build_test_install.bat',
    'scripts/capture_mft_installed_evidence.ps1',
    'scripts/hold_installed_superexplorer_focus.ps1',
    'scripts/smoke_mft_event_service.ps1',
    'scripts/smoke_size_map_plugin.ps1',
    'scripts/test_installer_mft_service_lifecycle.ps1',
    'scripts/validate_mft_evidence.ps1',
    'openspec/changes/mft-sqlite-foreground-persistence/proposal.md',
    'openspec/changes/mft-sqlite-foreground-persistence/design.md',
    'openspec/changes/mft-sqlite-foreground-persistence/specs/mft-sqlite-foreground-persistence/spec.md',
    'openspec/changes/mft-sqlite-foreground-persistence/tasks.md'
)
$sourceHashes = [ordered]@{}
foreach ($relative in $sourcePaths) {
    $path = Join-Path $workspace $relative
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Fingerprint source missing: $relative" }
    $sourceHashes[$relative] = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
}
$sourceMaterial = ($sourceHashes.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join "`n"
$hasher = [Security.Cryptography.SHA256]::Create()
try {
    $sourceFingerprint = -join ($hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($sourceMaterial)) | ForEach-Object { $_.ToString('x2') })
} finally {
    $hasher.Dispose()
}
if ($PrintSourceFingerprintOnly) {
    Write-Output $sourceFingerprint
    return
}

$tasksPath = Join-Path $workspace 'openspec\changes\mft-sqlite-foreground-persistence\tasks.md'
$checkedEvidenceTasks = [regex]::Matches(
    (Get-Content -Raw -Encoding UTF8 -LiteralPath $tasksPath),
    '(?m)^- \[x\] (4\.2\.4|5\.[12]\.[1-9][0-9]*|6\.1\.[1-9][0-9]*)\b'
) | ForEach-Object { $_.Groups[1].Value }

$seen = @{}
$taskRecords = @{}
$entries = [Collections.Generic.List[object]]::new()
Get-ChildItem -LiteralPath $EvidenceRoot -Recurse -Filter result.json -File | Sort-Object FullName | ForEach-Object {
    $record = Get-Content -Raw -Encoding UTF8 -LiteralPath $_.FullName | ConvertFrom-Json
    if ([string]$record.status -ne 'passed' -or [int]$record.exit_code -ne 0) {
        throw "Non-countable evidence result: $($_.FullName)"
    }
    $task = if ($record.task) { [string]$record.task } else { [string]$record.task_id }
    if (-not $task) { throw "Missing task ID: $($_.FullName)" }
    if ($task -ne $_.Directory.Name) { throw "Task/directory mismatch: $task != $($_.Directory.Name)" }
    $subcheck = if ($record.subcheck) { [string]$record.subcheck } else { 'default' }
    $key = "$task`:$subcheck"
    if ($seen.ContainsKey($key)) { throw "Duplicate evidence task/subcheck: $key" }
    $seen[$key] = $true
    $taskRecords[$task] = $true
    if ([string]$record.source_revision -ne $revision) { throw "Source revision mismatch: $($_.FullName)" }
    if ([string]$record.source_fingerprint -ne $sourceFingerprint) { throw "Dirty-source fingerprint mismatch: $($_.FullName)" }
    $timestamp = [DateTimeOffset]::MinValue
    if (-not [DateTimeOffset]::TryParse([string]$record.timestamp_utc, [ref]$timestamp)) {
        throw "Invalid evidence timestamp: $($_.FullName)"
    }
    if (-not $record.gates -or @($record.gates).Count -eq 0) { throw "Missing gate IDs: $($_.FullName)" }
    if (-not $record.command) { throw "Missing command/procedure: $($_.FullName)" }
    $rawOutputs = @($record.raw_outputs)
    if ($rawOutputs.Count -eq 0) { throw "Missing raw outputs: $($_.FullName)" }
    $rawHashes = [ordered]@{}
    foreach ($rawRelative in $rawOutputs) {
        $rawPath = [IO.Path]::GetFullPath((Join-Path $workspace ([string]$rawRelative)))
        if (-not $rawPath.StartsWith($workspace + '\', [StringComparison]::OrdinalIgnoreCase)) {
            throw "Raw evidence path escapes workspace: $rawRelative"
        }
        if (-not (Test-Path -LiteralPath $rawPath -PathType Leaf)) { throw "Raw evidence missing: $rawRelative" }
        $rawHashes[[string]$rawRelative] = (Get-FileHash -Algorithm SHA256 -LiteralPath $rawPath).Hash.ToLowerInvariant()
    }
    $relative = $_.FullName.Substring($workspace.Length + 1).Replace('\','/')
    $entries.Add([ordered]@{
        task = $task
        subcheck = $subcheck
        path = $relative
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
        bytes = $_.Length
        raw_output_sha256 = $rawHashes
        gates = @($record.gates)
    })
}
foreach ($task in $checkedEvidenceTasks) {
    if (-not $taskRecords.ContainsKey($task)) { throw "Checked task lacks evidence: $task" }
}
if ($entries.Count -eq 0) { throw 'No evidence records found.' }
$index = [ordered]@{
    schema = 'superexplorer-mft-evidence-index-v2'
    source_revision = $revision
    source_fingerprint = $sourceFingerprint
    source_sha256 = $sourceHashes
    generated_utc = [DateTimeOffset]::UtcNow.ToString('O')
    adjustment_lineage = @('Only passing, current-fingerprint records are counted.', 'Raw outputs and result records are independently hashed.')
    unique_task_subchecks = $true
    entry_count = $entries.Count
    entries = $entries
}
[IO.File]::WriteAllText($OutputPath, ($index | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
$roundTrip = Get-Content -Raw -Encoding UTF8 -LiteralPath $OutputPath | ConvertFrom-Json
if ($roundTrip.schema -ne 'superexplorer-mft-evidence-index-v2' -or
    $roundTrip.source_fingerprint -ne $sourceFingerprint -or
    $roundTrip.entry_count -ne $entries.Count) {
    throw 'Evidence index round-trip validation failed.'
}
Write-Output "Evidence index PASS: $($entries.Count) records; source=$sourceFingerprint"
