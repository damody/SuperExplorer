[CmdletBinding()]
param([string]$Root = '')
$ErrorActionPreference = 'Stop'
$evidenceRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($Root)) { $Root = Split-Path -Parent $evidenceRoot }
$tasks = Join-Path $Root 'tasks.md'
$mapPath = Join-Path $evidenceRoot 'legacy-lineage-map.json'
$backfillPath = Join-Path $evidenceRoot 'legacy-5.1-final-go-backfill.json'
$valid = Select-String -LiteralPath $tasks -Pattern '- \[(?: |x|X|deferred)\]\s+(\d+\.\d+\.\d+)' |
    ForEach-Object { $_.Matches.Groups[1].Value }
$map = Get-Content -LiteralPath $mapPath -Raw | ConvertFrom-Json
$canonical = $map.expected_semantic_mappings
if (-not $canonical) { throw 'Missing expected_semantic_mappings baseline' }
if ($map.entries.Count -ne 33) { throw "Expected 33 legacy entries, got $($map.entries.Count)" }
$counts = @{1=10;2=8;3=7;4=8}; $expected = 1..4 | ForEach-Object { $section=$_; 1..$counts[$section] | ForEach-Object { "$section.$_" } }
if ((@($map.entries.legacy_id) -join ',') -ne ($expected -join ',')) { throw 'Legacy IDs are not the deterministic 1.1-4.8 sequence' }
$seen = @{}
foreach ($entry in $map.entries) {
    $expectedIds = @($canonical.($entry.legacy_id)); if ((@($entry.new_l3_ids) -join ',') -ne ($expectedIds -join ',')) { throw "Canonical semantic mapping mismatch on $($entry.legacy_id)" }
    foreach ($field in @('legacy_task_text','historical_checked','first_completion_commit','original_test_provenance')) { if (-not ($entry.PSObject.Properties.Name -contains $field)) { throw "Missing required field $field on $($entry.legacy_id)" } }
    if ([string]::IsNullOrWhiteSpace($entry.legacy_task_text)) { throw "Empty historical task text on $($entry.legacy_id)" }
    $mustCheck = [int]($entry.legacy_id.Split('.')[0]) -lt 4 -or ($entry.legacy_id -in @('4.1','4.2'))
    if ([bool]$entry.historical_checked -ne $mustCheck) { throw "Checked-state mismatch on $($entry.legacy_id)" }
    if ($mustCheck -and ($entry.first_completion_commit -notmatch '^[0-9a-f]{40}$')) { throw "Invalid first commit on $($entry.legacy_id)" }
    if (-not $mustCheck -and $null -ne $entry.first_completion_commit) { throw "Unchecked entry has a commit: $($entry.legacy_id)" }
    if (@($entry.original_test_provenance).Count -lt 1) { throw "Missing test provenance on $($entry.legacy_id)" }
    foreach ($prov in @($entry.original_test_provenance)) { if ([string]::IsNullOrWhiteSpace($prov.name)) { throw "Empty provenance name on $($entry.legacy_id)" }; if ([string]::IsNullOrWhiteSpace($prov.kind) -or [string]::IsNullOrWhiteSpace($prov.status)) { throw "Provenance lacks kind/status on $($entry.legacy_id)" }; if ([string]::IsNullOrWhiteSpace($prov.command) -and [string]::IsNullOrWhiteSpace($prov.reason)) { throw "Provenance lacks command/reason on $($entry.legacy_id)" }; if (-not $entry.historical_checked -and $null -ne $prov.command) { throw "Unchecked entry has command on $($entry.legacy_id)" }; if ($prov.kind -eq 'manifest' -and $null -ne $prov.command) { throw "Manifest provenance must have null command on $($entry.legacy_id)" }; if ($prov.kind -eq 'rust-test' -and $prov.command -notmatch '^cargo test ') { throw "Rust provenance command invalid on $($entry.legacy_id)" }; if ($prov.kind -eq 'powershell-test' -and $prov.command -notmatch 'sdk/tests/.+\.ps1') { throw "PowerShell provenance path invalid on $($entry.legacy_id)" } }
    foreach ($id in $entry.new_l3_ids) {
        if ($id -notin $valid) { throw "Mapped L3 ID '$id' is absent from tasks.md" }
    }
    if ($seen.ContainsKey($entry.legacy_id)) { throw "Duplicate legacy ID $($entry.legacy_id)" }
    $seen[$entry.legacy_id] = $true
    if (-not $entry.commits -and $entry.confidence -ne 'unverified') { throw "Missing commit must be unverified: $($entry.legacy_id)" }
}
$backfill = Get-Content -LiteralPath $backfillPath -Raw | ConvertFrom-Json
if ($backfill.status -ne 'candidate-not-verified') { throw '5.1 backfill must remain candidate-not-verified' }
if ($backfill.historical_claims_to_reconcile.Count -ne 4) { throw 'Historical 5.1 claims are incomplete' }
Write-Output "PASS: $($map.entries.Count) entries; $($valid.Count) authoritative L3 IDs; all mapped IDs exist."



