$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
  $steam = rg -n -i 'steamworks|steam_api' -g Cargo.toml -g Cargo.lock .
  if($LASTEXITCODE -eq 0){throw "Steamworks dependency is forbidden: $steam"}
  cargo test -p explorer-extension-host --locked --offline package_source::tests::;if($LASTEXITCODE-ne 0){throw 'package source tests failed'}
  cargo test -p explorer-extension-host --locked --offline package_resolver::tests::;if($LASTEXITCODE-ne 0){throw 'resolver tests failed'}
  cargo test -p explorer-extension-host --locked --offline --test package_lifecycle; if($LASTEXITCODE-ne 0){throw 'package lifecycle integration failed'}
  $revision=(git rev-parse HEAD).Trim();$files=@('crates/explorer-extension-host/src/package_source.rs','crates/explorer-extension-host/src/package_resolver.rs','crates/explorer-extension-host/tests/package_lifecycle.rs');$sha=[Security.Cryptography.SHA256]::Create();$bytes=New-Object Collections.Generic.List[byte];foreach($p in $files){$bytes.AddRange([IO.File]::ReadAllBytes((Resolve-Path $p)))};$digest=([BitConverter]::ToString($sha.ComputeHash($bytes.ToArray()))).Replace('-','').ToLowerInvariant()
  foreach($n in 1..9){$id="3.3.$n";$dir=Join-Path 'target/openspec-evidence/build-extensible-plugin-platform' $id;New-Item -ItemType Directory -Force $dir|Out-Null;$o=[ordered]@{schema_version=1;task_id=$id;procedure_kind='command';command='powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run_package_resolution_contract.ps1';cwd='.';environment=[ordered]@{validation_authority='local-only';uitest_executed='false';offline='true'};expected='exit code 0';actual='passed';exit_code=0;source_revision=$revision;input_sha256=[ordered]@{resolver_sha256=$digest}};$o|ConvertTo-Json -Depth 6|Set-Content -Encoding utf8 (Join-Path $dir 'result.json');Write-Output "REPORT $id $digest"}
}finally{Pop-Location}
