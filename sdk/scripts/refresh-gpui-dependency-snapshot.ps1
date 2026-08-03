[CmdletBinding()]
param([Parameter(Mandatory)][string]$RepositoryRoot,[Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{40}$')][string]$GpuiRevision)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$RepositoryRoot=(Resolve-Path -LiteralPath $RepositoryRoot).Path
$temp=Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-gpui-refresh-'+[guid]::NewGuid().ToString('N'));$stage=Join-Path $temp 'worktree';$sourceGpui=Join-Path $RepositoryRoot 'vendor\gpui-ce';$stageGpui=Join-Path $stage 'vendor\gpui-ce';$onlineCargo=Join-Path $temp 'online-cargo-home';$offlineCargo=Join-Path $temp 'offline-cargo-home';$offlineTarget=Join-Path $temp 'offline-target';$savedCargoHome=$env:CARGO_HOME;$savedCargoTarget=$env:CARGO_TARGET_DIR;New-Item -ItemType Directory -Path $temp -Force|Out-Null
try{
 & git -C $RepositoryRoot worktree add --detach $stage HEAD;if($LASTEXITCODE){throw 'could not create isolated dependency refresh worktree'}
 if(Test-Path -LiteralPath $stageGpui){Remove-Item -LiteralPath $stageGpui -Recurse -Force -ErrorAction Stop};New-Item -ItemType Directory -Path (Split-Path -Parent $stageGpui) -Force|Out-Null
 & git -C $sourceGpui worktree add --detach $stageGpui $GpuiRevision;if($LASTEXITCODE){throw 'could not materialize the resolved GPUI revision in the isolated refresh worktree'}
 Copy-Item -LiteralPath (Join-Path $RepositoryRoot 'sdk\snapshot\approved-gpui.json') -Destination (Join-Path $stage 'sdk\snapshot\approved-gpui.json') -Force
 # The parent worktree may contain the precise candidate-only dirty surface.
 # Resolution always happens in this detached clean baseline worktree instead.
 Push-Location $stage
 try{
  New-Item -ItemType Directory -Path $onlineCargo -Force|Out-Null;$env:CARGO_HOME=$onlineCargo;Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
  & cargo.exe metadata --manifest-path Cargo.toml --format-version 1|Out-Null;if($LASTEXITCODE){throw 'root dependency metadata refresh failed'}
  & cargo.exe generate-lockfile --manifest-path sdk\Cargo.toml;if($LASTEXITCODE){throw 'SDK lock refresh failed'}
  $vendorStage=Join-Path $stage 'sdk\vendor\cargo-sources.refresh-stage';& cargo.exe vendor --manifest-path sdk\Cargo.toml --locked --versioned-dirs $vendorStage|Out-Null;if($LASTEXITCODE){throw 'versioned vendor refresh failed'}
  Remove-Item -LiteralPath (Join-Path $stage 'sdk\vendor\cargo-sources') -Recurse -Force -ErrorAction Stop;Move-Item -LiteralPath $vendorStage -Destination (Join-Path $stage 'sdk\vendor\cargo-sources') -ErrorAction Stop
  # Prove the generated vendor tree is self-sufficient with a second, empty
  # Cargo home and target. No resolver cache survives into this consumer phase.
  New-Item -ItemType Directory -Path $offlineCargo,$offlineTarget -Force|Out-Null;$vendor=(Join-Path $stage 'sdk\vendor\cargo-sources') -replace '\\','/';$config=@('[source.crates-io]','replace-with = "cargo-sources"','[source.cargo-sources]',("directory = `"$vendor`"")) -join [Environment]::NewLine;[IO.File]::WriteAllText((Join-Path $offlineCargo 'config.toml'),$config,[Text.UTF8Encoding]::new($false));$env:CARGO_HOME=$offlineCargo;$env:CARGO_TARGET_DIR=$offlineTarget
  foreach($manifest in @('sdk\Cargo.toml','sdk\fixtures\p0-consumer\Cargo.toml','sdk\fixtures\abi-root-host\Cargo.toml','sdk\fixtures\abi-root-plugin\Cargo.toml')){& cargo.exe metadata --manifest-path $manifest --locked --offline --format-version 1|Out-Null;if($LASTEXITCODE){throw "locked offline consumer metadata failed: $manifest"}}
  $metadataText=& cargo.exe metadata --manifest-path sdk\Cargo.toml --locked --offline --format-version 1;if($LASTEXITCODE){throw 'SDK protected dependency metadata refresh failed'};$metadata=$metadataText|ConvertFrom-Json;$snapshot=Get-Content -LiteralPath 'sdk\snapshot\approved-gpui.json' -Raw|ConvertFrom-Json;$lockText=Get-Content -LiteralPath 'sdk\Cargo.lock' -Raw;Import-Module (Join-Path $stage 'sdk\tests\protected-dependency-test-support.psm1') -Force
  $abi=@($metadata.packages|Where-Object name -eq 'abi_stable');$gpui=@($metadata.packages|Where-Object name -eq 'gpui');if($abi.Count -ne 1 -or $gpui.Count -ne 1){throw 'protected dependency metadata does not resolve exactly one abi_stable and gpui package'}
  $checksum=[regex]::Match($lockText,'(?ms)^\[\[package\]\]\s*name = "abi_stable".*?^checksum = "([0-9a-f]{64})"').Groups[1].Value;if([string]::IsNullOrWhiteSpace($checksum)){throw 'canonical abi_stable checksum is missing from refreshed SDK lock'}
  $closure=[ordered]@{schema_version=2;algorithm='normalized-package-edges-v2';edge_digest=(Get-DependencyEdgeDigest $metadata $stage $snapshot $lockText);required_roots=@('abi_stable@0.11.3','gpui@0.2.2');abi_stable=[ordered]@{version=$abi[0].version;source=$abi[0].source;checksum=$checksum;default_features=$false};gpui=[ordered]@{version=$gpui[0].version;default_features=$false;features=@()}}
  [IO.File]::WriteAllText((Join-Path $stage 'sdk\snapshot\protected-dependency-closure.json'),(($closure|ConvertTo-Json -Depth 8)+"`n"),[Text.UTF8Encoding]::new($false));Assert-ProtectedDependencyMetadata $metadata $closure $stage $snapshot $lockText|Out-Null
  Push-Location 'sdk\tools\bundle-generator';try{& cargo.exe run --release --locked -- generate;if($LASTEXITCODE){throw 'protected closure/bundle generation failed'}}finally{Pop-Location}
 }finally{Pop-Location}
 foreach($relative in @('Cargo.lock','sdk\Cargo.lock','sdk\sdk-lock.json','sdk\bundle-manifest.json','sdk\ui-abi-fingerprint.json','sdk\snapshot\protected-dependency-closure.json')){Copy-Item -LiteralPath (Join-Path $stage $relative) -Destination (Join-Path $RepositoryRoot $relative) -Force}
 $targetVendor=Join-Path $RepositoryRoot 'sdk\vendor\cargo-sources';Remove-Item -LiteralPath $targetVendor -Recurse -Force -ErrorAction Stop;Copy-Item -LiteralPath (Join-Path $stage 'sdk\vendor\cargo-sources') -Destination $targetVendor -Recurse -Force
 & git -C $RepositoryRoot add -A -- 'Cargo.lock' 'sdk/Cargo.lock' 'sdk/sdk-lock.json' 'sdk/bundle-manifest.json' 'sdk/ui-abi-fingerprint.json' 'sdk/snapshot/protected-dependency-closure.json' 'sdk/vendor/cargo-sources';if($LASTEXITCODE){throw 'could not stage refreshed canonical dependency outputs'}
}finally{if($null -eq $savedCargoHome){Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue}else{$env:CARGO_HOME=$savedCargoHome};if($null -eq $savedCargoTarget){Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue}else{$env:CARGO_TARGET_DIR=$savedCargoTarget};if(Test-Path -LiteralPath $stageGpui){& git -C $sourceGpui worktree remove --force $stageGpui 2>$null};if(Test-Path -LiteralPath $stage){& git -C $RepositoryRoot worktree remove --force $stage 2>$null};if(Test-Path -LiteralPath $temp){Remove-Item -LiteralPath $temp -Recurse -Force}}
