$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
  python scripts/abi_v1_contract_validator.py; if($LASTEXITCODE-ne 0){throw 'ABI V1 policy failed'}
  python scripts/tests/test_abi_v1_contract_validator.py; if($LASTEXITCODE-ne 0){throw 'ABI V1 validator tests failed'}
  python scripts/author_abi_surface_audit.py; if($LASTEXITCODE-ne 0){throw 'author ABI surface failed'}
  cargo test -p explorer-extension-api --locked --offline; if($LASTEXITCODE-ne 0){throw 'API tests failed'}
  powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/extension-api-abi-contract.ps1; if($LASTEXITCODE-ne 0){throw 'extension ABI fixture failed'}
  powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/job-context-v1-abi-contract.ps1; if($LASTEXITCODE-ne 0){throw 'job ABI fixture failed'}
  cargo test -p explorer-extension-host --locked --offline resident_load_state_returns_cached_roots_and_preserves_rejections; if($LASTEXITCODE-ne 0){throw 'resident loader test failed'}
  cargo test -p explorer-extension-host --locked --offline successful_drain_is_resident_idempotent_and_reenable_advances_epoch; if($LASTEXITCODE-ne 0){throw 'resident lifetime test failed'}
  $revision=(git rev-parse HEAD).Trim();$reviewHash=(Get-FileHash 'openspec/changes/build-extensible-plugin-platform/abi/v1-baseline-review.json' -Algorithm SHA256).Hash.ToLowerInvariant()
  $ids=1..17|ForEach-Object{"3.1.$_"};foreach($id in $ids){$dir=Join-Path 'target/openspec-evidence/build-extensible-plugin-platform' $id;New-Item -ItemType Directory -Force $dir|Out-Null;$o=[ordered]@{schema_version=1;task_id=$id;procedure_kind='command';command='powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run_abi_v1_contract.ps1';cwd='.';environment=[ordered]@{validation_authority='local-only';uitest_executed='false';offline='true'};expected='exit code 0';actual='passed';exit_code=0;source_revision=$revision;input_sha256=[ordered]@{review_sha256=$reviewHash}};$o|ConvertTo-Json -Depth 6|Set-Content -Encoding utf8 (Join-Path $dir 'result.json');Write-Output "REPORT $id $reviewHash"}
} finally {Pop-Location}
