[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('1.2.1', '1.2.2', '1.2.3', '1.2.4', '1.2.5')]
    [string]$TaskId
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$resultDir = Join-Path $repoRoot "target/openspec-evidence/build-extensible-plugin-platform/$TaskId"
$resultPath = Join-Path $resultDir 'result.json'
$matrix = 'openspec/changes/build-extensible-plugin-platform/traceability/traceability-matrix.json'
$commands = @{
    '1.2.1' = @('python','-m','unittest','scripts.tests.test_traceability_matrix.TraceabilityMatrixTests.test_real_matrix_has_eleven_capabilities_and_all_tasks')
    '1.2.2' = @('python','scripts/traceability_matrix.py','--validate',$matrix,'--spec-root','openspec/changes/build-extensible-plugin-platform/specs','--tasks','openspec/changes/build-extensible-plugin-platform/tasks.md')
    '1.2.3' = @('python','-m','unittest','scripts.tests.test_traceability_matrix.TraceabilityMatrixTests.test_real_matrix_has_eleven_capabilities_and_all_tasks')
    '1.2.4' = @('python','-m','unittest','scripts.tests.test_traceability_matrix.TraceabilityMatrixTests.test_missing_unknown_orphan_and_mock_only_fail_independently')
    '1.2.5' = @('python','scripts/traceability_matrix.py','--validate',$matrix,'--spec-root','openspec/changes/build-extensible-plugin-platform/specs','--tasks','openspec/changes/build-extensible-plugin-platform/tasks.md')
}
$arguments = @($commands[$TaskId]); $executable = $arguments[0]; $argumentList = @($arguments | Select-Object -Skip 1)
Push-Location $repoRoot
try {
    & $executable @argumentList
    $exitCode = $LASTEXITCODE; if ($null -eq $exitCode) { $exitCode = 0 }
    $revision = (& git rev-parse HEAD).Trim()
} finally { Pop-Location }
$report = [ordered]@{
    schema_version=1; task_id=$TaskId; procedure_kind='command'; command=($arguments -join ' '); cwd='.'
    environment=[ordered]@{validation_authority='local-only';uitest_executed='false'}
    expected='exit code 0'; actual=$(if ($exitCode -eq 0) {'passed'} else {'failed'}); exit_code=$exitCode
    source_revision=$revision
    input_sha256=[ordered]@{}
}
foreach ($relative in @('scripts/traceability_matrix.py','scripts/tests/test_traceability_matrix.py',$matrix,'openspec/changes/build-extensible-plugin-platform/tasks.md')) {
    $report.input_sha256[$relative] = (Get-FileHash -LiteralPath (Join-Path $repoRoot $relative) -Algorithm SHA256).Hash.ToLowerInvariant()
}
New-Item -ItemType Directory -Force $resultDir | Out-Null
[IO.File]::WriteAllText($resultPath, (($report | ConvertTo-Json -Depth 5) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
$digest=(Get-FileHash -LiteralPath $resultPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "REPORT $TaskId $digest $resultPath"
if ($exitCode -ne 0) { exit $exitCode }
