[CmdletBinding()]
param(
    [string]$RepositoryRoot,
    [string]$AttestationPath,
    [string]$CandidatePlanDigest,
    [string]$WorkflowRunId,
    [string]$Nonce
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
if([string]::IsNullOrWhiteSpace($RepositoryRoot)){$RepositoryRoot=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path}else{$RepositoryRoot=(Resolve-Path -LiteralPath $RepositoryRoot).Path}
$manifestPath=Join-Path $RepositoryRoot 'sdk\ci\gpui-update-gates.json';if(-not(Test-Path -LiteralPath $manifestPath -PathType Leaf)){throw 'GPUI update aggregate gate manifest is missing'}
$manifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json
if($manifest.schema_version -ne 1 -or $manifest.required_gate_count -ne 8){throw 'GPUI update aggregate gate manifest schema/count is invalid'}
$gates=@($manifest.gates|Where-Object{$_.required -eq $true});if($gates.Count -ne $manifest.required_gate_count -or @($gates.id|Select-Object -Unique).Count -ne $gates.Count){throw 'GPUI update aggregate gate manifest must have exactly eight unique required gates'}
if (($AttestationPath -or $CandidatePlanDigest -or $WorkflowRunId -or $Nonce) -and ([string]::IsNullOrWhiteSpace($AttestationPath) -or $CandidatePlanDigest -notmatch '^[0-9a-f]{64}$' -or [string]::IsNullOrWhiteSpace($WorkflowRunId) -or $Nonce -notmatch '^[0-9a-f]{32}$')) { throw 'full-gate attestation parameters must be supplied together with canonical candidate identity' }
$results = @()
foreach($gate in $gates){if($gate.kind -ne 'powershell' -or [string]::IsNullOrWhiteSpace([string]$gate.id) -or [string]$gate.path -notmatch '^sdk/tests/[A-Za-z0-9._-]+\.ps1$'){throw 'GPUI update aggregate gate entry is invalid'};$path=Join-Path $RepositoryRoot $gate.path;if(-not(Test-Path -LiteralPath $path -PathType Leaf)){throw "required GPUI update gate path is missing: $($gate.id)"};& powershell.exe -NoProfile -File $path;$exitCode=$LASTEXITCODE;if($exitCode -ne 0){throw "GPUI update aggregate gate failed: $($gate.id) ($exitCode)"};$results += [ordered]@{id=[string]$gate.id;exit_code=0}}
if ($AttestationPath) {
    Import-Module (Join-Path $PSScriptRoot 'release-freeze-support.psm1') -Force
    $attestation = [ordered]@{ schema_version = 1; gate_manifest_sha256 = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant(); candidate_plan_digest = $CandidatePlanDigest; workflow_run_id = $WorkflowRunId; nonce = $Nonce; results = $results; attestation_sha256 = ('0' * 64) }
    $attestation.attestation_sha256 = Get-CanonicalGateAttestationDigestV1 $attestation
    [IO.File]::WriteAllText($AttestationPath, (($attestation | ConvertTo-Json -Depth 10) + "`n"), [Text.UTF8Encoding]::new($false))
}
Write-Output 'GPUI update aggregate gates passed'
