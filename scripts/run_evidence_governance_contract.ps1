[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('1.1.1', '1.1.2', '1.1.3', '1.1.4', '1.1.5', '1.1.6', '1.1.7', '1.1.8')]
    [string]$TaskId
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$outputRoot = Join-Path $repoRoot "target/openspec-evidence/build-extensible-plugin-platform/$TaskId"
$outputPath = Join-Path $outputRoot 'result.json'

$commands = @{
    '1.1.1' = @(
        'python', '-m', 'unittest',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_accepts_one_terminal_l3_record',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_leaf_result_requires_no_release_bundle_locator'
    )
    '1.1.2' = @(
        'python', '-m', 'unittest',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_rejects_missing_required_field_and_duplicate_event_identity',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_repeated_task_id_requires_append_only_lineage_links',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_leaf_completion_rechecks_report_hash_and_actual'
    )
    '1.1.3' = @(
        'powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        'openspec/changes/build-extensible-plugin-platform/evidence/legacy-lineage-contract.ps1'
    )
    '1.1.4' = @(
        'powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        'openspec/changes/build-extensible-plugin-platform/evidence/legacy-lineage-contract.ps1'
    )
    '1.1.5' = @(
        'python', '-m', 'unittest',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_nonclosing_latest_states_are_valid_history_but_fail_completion'
    )
    '1.1.6' = @(
        'python', '-m', 'unittest',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_rejects_duplicate_subcheck_and_preserves_one_l3_to_one_subcheck'
    )
    '1.1.7' = @(
        'python', '-m', 'unittest',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_not_applicable_requires_authoritative_policy_even_for_conditional_leaf',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_mandatory_p1_cannot_be_not_applicable',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_authoritative_supersession_transitively_stales_and_allows_bound_revalidation',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_authoritative_supersession_rejects_cycles_nonterminal_replacements_and_unbound_revalidation'
    )
    '1.1.8' = @(
        'python', '-m', 'unittest',
        'scripts.tests.test_signed_release_bundle',
        'scripts.tests.test_evidence_index_validator.EvidenceIndexValidatorTests.test_release_closure_requires_signed_bundle_trust_root'
    )
}

$arguments = @($commands[$TaskId])
$executable = $arguments[0]
$argumentList = @($arguments | Select-Object -Skip 1)
$displayCommand = ($arguments | ForEach-Object {
    if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
}) -join ' '

Push-Location $repoRoot
try {
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $nativeOutput = @(& $executable @argumentList 2>&1)
    $exitCode = $LASTEXITCODE
    if ($null -eq $exitCode) { $exitCode = 0 }
    if ($exitCode -ne 0) { $nativeOutput | ForEach-Object { Write-Error $_.ToString() } }
    $ErrorActionPreference = $previousErrorActionPreference
    $sourceRevision = (& git rev-parse HEAD).Trim()
} finally {
    $ErrorActionPreference = 'Stop'
    Pop-Location
}

$report = [ordered]@{
    schema_version = 1
    task_id = $TaskId
    procedure_kind = 'command'
    command = $displayCommand
    cwd = '.'
    environment = [ordered]@{
        validation_authority = 'local-only'
        uitest_executed = 'false'
    }
    expected = 'exit code 0'
    actual = if ($exitCode -eq 0) { 'passed' } else { 'failed' }
    exit_code = $exitCode
    source_revision = $sourceRevision
    input_sha256 = [ordered]@{}
}

$inputPaths = @(
    'openspec/changes/build-extensible-plugin-platform/evidence/evidence-index.schema.json',
    'openspec/changes/build-extensible-plugin-platform/evidence/evidence-policy.schema.json',
    'openspec/changes/build-extensible-plugin-platform/evidence/legacy-lineage-map.json',
    'openspec/changes/build-extensible-plugin-platform/evidence/legacy-5.1-final-go-backfill.json',
    'openspec/changes/build-extensible-plugin-platform/evidence/legacy-lineage-contract.ps1',
    'scripts/evidence_index_validator.py',
    'scripts/signed_release_bundle.py',
    'scripts/tests/test_evidence_index_validator.py',
    'scripts/tests/test_signed_release_bundle.py',
    'scripts/run_evidence_governance_contract.ps1'
)
foreach ($relativePath in $inputPaths) {
    $fullPath = Join-Path $repoRoot $relativePath
    $report.input_sha256[$relativePath] = (Get-FileHash -LiteralPath $fullPath -Algorithm SHA256).Hash.ToLowerInvariant()
}

New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$json = $report | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($outputPath, $json + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
$digest = (Get-FileHash -LiteralPath $outputPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "REPORT $TaskId $digest $outputPath"
if ($exitCode -ne 0) { exit $exitCode }
