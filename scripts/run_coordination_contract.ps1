[CmdletBinding()]
param([Parameter(Mandatory=$true)][ValidateSet('1.3.1','1.3.2','1.3.3','1.3.4','1.3.5','1.3.6')][string]$TaskId)
$ErrorActionPreference='Stop'; $root=Split-Path -Parent $PSScriptRoot
$tests=@{'1.3.1'='test_policy_and_complete_handoff_pass';'1.3.2'='test_policy_and_complete_handoff_pass';'1.3.3'='test_two_owners_for_one_mutable_path_fail';'1.3.4'='test_a_refinement_preserves_ids_and_lineage';'1.3.5'='test_b_correction_stales_pauses_and_revalidates';'1.3.6'='test_c_change_requires_user_approval'}
$command="python -m unittest scripts.tests.test_coordination_policy_validator.CoordinationPolicyTests.$($tests[$TaskId])"
Push-Location $root; try { Invoke-Expression $command; $code=$LASTEXITCODE; if($null -eq $code){$code=0}; $rev=(& git rev-parse HEAD).Trim() } finally { Pop-Location }
$dir=Join-Path $root "target/openspec-evidence/build-extensible-plugin-platform/$TaskId"; New-Item -ItemType Directory -Force $dir|Out-Null
$report=[ordered]@{schema_version=1;task_id=$TaskId;procedure_kind='command';command=$command;cwd='.';environment=[ordered]@{validation_authority='local-only';uitest_executed='false'};expected='exit code 0';actual=$(if($code-eq 0){'passed'}else{'failed'});exit_code=$code;source_revision=$rev;input_sha256=[ordered]@{}}
foreach($path in @('scripts/coordination_policy_validator.py','scripts/tests/test_coordination_policy_validator.py','openspec/changes/build-extensible-plugin-platform/coordination/coordination-policy.json')){$report.input_sha256[$path]=(Get-FileHash (Join-Path $root $path)-Algorithm SHA256).Hash.ToLowerInvariant()}
$out=Join-Path $dir 'result.json'; [IO.File]::WriteAllText($out,(($report|ConvertTo-Json -Depth 5)+[Environment]::NewLine),[Text.UTF8Encoding]::new($false)); Write-Output "REPORT $TaskId $((Get-FileHash $out -Algorithm SHA256).Hash.ToLowerInvariant()) $out"; if($code-ne 0){exit $code}
