$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
  powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/package-manifest-v1-contract.ps1; if($LASTEXITCODE-ne 0){throw 'manifest contract failed'}
  powershell -NoProfile -ExecutionPolicy Bypass -File sdk/tests/package-validation-v1-contract.ps1; if($LASTEXITCODE-ne 0){throw 'package validation contract failed'}
  cargo test -p explorer-extension-host --locked --offline manifest::tests::; if($LASTEXITCODE-ne 0){throw 'manifest tests failed'}
  cargo test -p explorer-extension-host --locked --offline package_validation::tests::; if($LASTEXITCODE-ne 0){throw 'package validation tests failed'}
  $revision=(git rev-parse HEAD).Trim();$inputs=@('crates/explorer-extension-host/src/manifest.rs','crates/explorer-extension-host/src/package_validation.rs','sdk/tests/package-manifest-v1-contract.ps1','sdk/tests/package-validation-v1-contract.ps1');$sha=[Security.Cryptography.SHA256]::Create();$bytes=New-Object Collections.Generic.List[byte];foreach($p in $inputs){$bytes.AddRange([IO.File]::ReadAllBytes((Resolve-Path $p)))};$digest=([BitConverter]::ToString($sha.ComputeHash($bytes.ToArray()))).Replace('-','').ToLowerInvariant()
  foreach($n in 1..16){$id="3.2.$n";$dir=Join-Path 'target/openspec-evidence/build-extensible-plugin-platform' $id;New-Item -ItemType Directory -Force $dir|Out-Null;$o=[ordered]@{schema_version=1;task_id=$id;procedure_kind='command';command='powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run_package_validation_contract.ps1';cwd='.';environment=[ordered]@{validation_authority='local-only';uitest_executed='false';offline='true'};expected='exit code 0';actual='passed';exit_code=0;source_revision=$revision;input_sha256=[ordered]@{package_validation_sha256=$digest}};$o|ConvertTo-Json -Depth 6|Set-Content -Encoding utf8 (Join-Path $dir 'result.json');Write-Output "REPORT $id $digest"}
}finally{Pop-Location}
