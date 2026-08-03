Set-StrictMode -Version Latest

# The sole test seam for the snapshot pipeline. Production wrappers construct
# this from their fixed policy; tests may construct a local authority with an
# explicit command runner. No wrapper accepts a caller-provided authority.
function New-GpuiSnapshotAuthorityV1 {
 param(
  [Parameter(Mandatory)][string]$RepositoryRoot,
  [Parameter(Mandatory)][string]$ExpectedOrigin,
  [Parameter(Mandatory)][string]$GpuiRepository,
  [Parameter(Mandatory)][string]$GateManifestPath,
  [Parameter(Mandatory)][scriptblock]$CommandRunner
 )
 $root=(Resolve-Path -LiteralPath $RepositoryRoot).Path;$gpui=(Resolve-Path -LiteralPath $GpuiRepository).Path;$manifest=if(Test-Path -LiteralPath $GateManifestPath){(Resolve-Path -LiteralPath $GateManifestPath).Path}else{$GateManifestPath}
 if([string]::IsNullOrWhiteSpace($ExpectedOrigin)){throw 'GPUI snapshot authority requires an exact origin'}
 [pscustomobject]@{schema_version=1;repository_root=$root;expected_origin=$ExpectedOrigin;gpui_repository=$gpui;gate_manifest=$manifest;command_runner=$CommandRunner}
}

function Invoke-GpuiSnapshotAuthorityCommand {
 param([Parameter(Mandatory)]$Authority,[Parameter(Mandatory)][string]$Kind,[Parameter(Mandatory)][string[]]$Arguments)
 if($Authority.schema_version -ne 1 -or $null -eq $Authority.command_runner){throw 'invalid GPUI snapshot authority'}
 & $Authority.command_runner $Kind $Arguments
}

function Invoke-GpuiSnapshotCandidatePipelineCore {
 param([Parameter(Mandatory)]$Authority,[Parameter(Mandatory)][scriptblock]$Update,[Parameter(Mandatory)][scriptblock]$Refresh,[Parameter(Mandatory)][scriptblock]$Aggregate,[Parameter(Mandatory)][scriptblock]$Artifact)
 if($Authority.schema_version -ne 1){throw 'invalid GPUI snapshot authority'}
 & $Update
 & $Refresh
 & $Aggregate
 & $Artifact
}

function New-GpuiSnapshotArtifactProofV1 {
 param([Parameter(Mandatory)]$Authority,[Parameter(Mandatory)][string]$CandidateDirectory,[Parameter(Mandatory)]$Approval,[Parameter(Mandatory)][string]$HmacKey,[Parameter(Mandatory)][string]$BaselineCommit)
 if($Authority.schema_version -ne 1 -or [string]::IsNullOrWhiteSpace($HmacKey)){throw 'invalid artifact authority or protected key'}
 $files=@('approval.json','candidate-attestation.json','approved-gpui.json','sdk-lock.json','bundle-manifest.json','ui-abi-fingerprint.json','gpui-revision.txt','candidate.patch')
 $entries=@($files|ForEach-Object{$p=Join-Path $CandidateDirectory $_;if(-not(Test-Path -LiteralPath $p)){throw "candidate artifact missing $_"};[ordered]@{name=$_;sha256=(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}})
 $manifest=[ordered]@{schema_version=1;repository_baseline_commit=$BaselineCommit;files=$entries};$manifestPath=Join-Path $CandidateDirectory 'promotion-manifest.json';[IO.File]::WriteAllText($manifestPath,($manifest|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false));$payload="$($Approval.baseline_revision)`n$($Approval.new_revision)`n$($Approval.new_tree)`n$($Approval.candidate_plan_digest)`n$($Approval.workflow_run_id)`n$($Approval.nonce)`n$BaselineCommit`n$((Get-FileHash $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant())";$h=[Security.Cryptography.HMACSHA256]::new([Text.Encoding]::UTF8.GetBytes($HmacKey));try{$mac=([BitConverter]::ToString($h.ComputeHash([Text.Encoding]::UTF8.GetBytes($payload)))).Replace('-','').ToLowerInvariant()}finally{$h.Dispose()};[IO.File]::WriteAllText((Join-Path $CandidateDirectory 'promotion-proof.json'),([ordered]@{schema_version=1;hmac_sha256=$mac}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
}
function Test-GpuiSnapshotArtifactProofV1 { param([Parameter(Mandatory)]$Authority,[Parameter(Mandatory)][string]$CandidateDirectory,[Parameter(Mandatory)]$Approval,[Parameter(Mandatory)][string]$HmacKey,[Parameter(Mandatory)][string]$BaselineCommit) $copy=Join-Path ([IO.Path]::GetTempPath()) ('gpui-proof-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $copy|Out-Null;try{Copy-Item (Join-Path $CandidateDirectory '*') $copy -Recurse -Force;New-GpuiSnapshotArtifactProofV1 $Authority $copy $Approval $HmacKey $BaselineCommit;((Get-Content (Join-Path $copy 'promotion-proof.json') -Raw|ConvertFrom-Json).hmac_sha256)}finally{Remove-Item $copy -Recurse -Force}}

# Shared transaction boundary for the fixed-authority production wrappers and
# local synthetic contracts.  Callers supply their already-authorized root and
# the expensive dependency runner; this module never resolves an authority.
function Invoke-GpuiSnapshotPromotionCore {
 param(
  [Parameter(Mandatory)]$Authority,
  [Parameter(Mandatory)][string]$CandidateRevision,
  [Parameter(Mandatory)][string]$CandidatePatch,
  [Parameter(Mandatory)][string[]]$RequiredChanged,
  [Parameter(Mandatory)][scriptblock]$Transition,
  [Parameter(Mandatory)][scriptblock]$DependencyRunner,
  [string]$CandidateTree,
 [string]$ExpectedOrigin='https://github.com/damody/gpui-ce-explorer.git'
 )
 $RepositoryRoot=[string]$Authority.repository_root
 if($Authority.schema_version -ne 1 -or $Authority.expected_origin -ne $ExpectedOrigin -or $Authority.gpui_repository -ne (Join-Path $RepositoryRoot 'vendor\gpui-ce')){throw 'promotion core received an invalid fixed-authority context'}
 $baseline=(& git -C $RepositoryRoot rev-parse HEAD).Trim();if($LASTEXITCODE){throw 'promotion core cannot resolve baseline'}
 $gpui=[string]$Authority.gpui_repository;$gpuiHead=(& git -C $gpui rev-parse HEAD).Trim();if($LASTEXITCODE){throw 'promotion core cannot resolve baseline GPUI checkout'};$gpuiBranch=& git -C $gpui symbolic-ref --quiet --short HEAD 2>$null;if($LASTEXITCODE -ne 0){$gpuiBranch=$null}
 try{
  $origin=(& git -C $gpui remote get-url origin).Trim();if($LASTEXITCODE -or $origin -ne $ExpectedOrigin){throw 'promotion core unauthorized GPUI origin'}
  $shallow=(& git -C $gpui rev-parse --is-shallow-repository).Trim();if($LASTEXITCODE -or $shallow -ne 'false'){throw 'promotion core requires complete GPUI history'}
  & git -C $gpui cat-file -e "$CandidateRevision`^{commit}";if($LASTEXITCODE){throw 'promotion core candidate revision is not a complete commit'}
  if($CandidateTree){$tree=(& git -C $gpui rev-parse "$CandidateRevision`^{tree}").Trim();if($LASTEXITCODE -or $tree -ne $CandidateTree){throw 'promotion core candidate tree mismatch'}}
  & git -C $gpui checkout --detach $CandidateRevision;if($LASTEXITCODE){throw 'promotion core could not materialize candidate GPUI checkout'}
  & git -C $RepositoryRoot apply --index $CandidatePatch;if($LASTEXITCODE){throw 'promotion core could not apply candidate patch'}
  & $Transition
  & git -C $RepositoryRoot add -- 'sdk/snapshot/approved-gpui.json' 'sdk/sdk-lock.json' 'sdk/bundle-manifest.json' 'sdk/ui-abi-fingerprint.json';if($LASTEXITCODE){throw 'promotion core could not stage transition outputs'}
  $gitlink=(& git -C $RepositoryRoot rev-parse ':vendor/gpui-ce').Trim();if($LASTEXITCODE -or $gitlink -ne $CandidateRevision){throw 'promotion core staged gitlink mismatch'}
  & $DependencyRunner
  $changed=@(& git -C $RepositoryRoot diff --cached --name-only);if($LASTEXITCODE){throw 'promotion core could not inspect index'};if(@($RequiredChanged|Where-Object{$_ -notin $changed}).Count){throw 'promotion core omitted required staged output'}
  return $baseline
 }catch{
  $failure=$_;& git -C $RepositoryRoot reset --hard $baseline|Out-Null;& git -C $RepositoryRoot clean -fd -- sdk/vendor/cargo-sources|Out-Null;if($LASTEXITCODE){throw "promotion core failed: $($failure.Exception.Message); rollback failed"};if($null -eq $gpuiBranch){& git -C $gpui checkout --detach $gpuiHead|Out-Null}else{& git -C $gpui checkout $gpuiBranch|Out-Null;& git -C $gpui reset --hard $gpuiHead|Out-Null};if($LASTEXITCODE){throw "promotion core failed: $($failure.Exception.Message); GPUI rollback failed"};throw $failure
 }
}
function Invoke-GpuiPromotionFinalizeV1 {
 param([Parameter(Mandatory)]$Authority,[Parameter(Mandatory)][string]$Baseline,[Parameter(Mandatory)][string]$GpuiHead,[string]$GpuiBranch,[Parameter(Mandatory)][string]$ExpectedRevision,[Parameter(Mandatory)][scriptblock]$VerifyRemote,[Parameter(Mandatory)][scriptblock]$Commit,[Parameter(Mandatory)][scriptblock]$Push)
 $root=$Authority.repository_root;$gpui=$Authority.gpui_repository
 try { & $VerifyRemote $ExpectedRevision; & $Commit; & $VerifyRemote $ExpectedRevision; & $Push }
 catch { $failure=$_;& git -C $root reset --hard $Baseline|Out-Null;if($LASTEXITCODE){throw "promotion finalize root rollback failed: $($failure.Exception.Message)"};if([string]::IsNullOrWhiteSpace($GpuiBranch)){& git -C $gpui checkout --detach $GpuiHead|Out-Null;if($LASTEXITCODE){throw "promotion finalize GPUI checkout rollback failed: $($failure.Exception.Message)"};$branch=& git -C $gpui symbolic-ref --quiet --short HEAD 2>$null;if($LASTEXITCODE -ne 1){throw "promotion finalize detached GPUI rollback failed: $($failure.Exception.Message)"}}else{& git -C $gpui checkout $GpuiBranch|Out-Null;if($LASTEXITCODE){throw "promotion finalize GPUI branch checkout failed: $($failure.Exception.Message)"};& git -C $gpui reset --hard $GpuiHead|Out-Null;if($LASTEXITCODE){throw "promotion finalize GPUI reset rollback failed: $($failure.Exception.Message)"};if((git -C $gpui symbolic-ref --short HEAD).Trim() -ne $GpuiBranch){throw "promotion finalize GPUI branch state rollback failed: $($failure.Exception.Message)"}};if((git -C $gpui rev-parse HEAD).Trim() -ne $GpuiHead -or @(git -C $gpui status --porcelain).Count -or @(git -C $root status --porcelain).Count){throw "promotion finalize rollback failed: $($failure.Exception.Message)"};throw $failure }
}
function Restore-GpuiPromotionBaselineV1 { param([Parameter(Mandatory)][string]$Repository,[Parameter(Mandatory)][string]$Head,[string]$Branch) if([string]::IsNullOrWhiteSpace($Branch)){& git -C $Repository checkout --detach $Head|Out-Null;if($LASTEXITCODE){throw 'GPUI detached checkout rollback failed'};$ignored=& git -C $Repository symbolic-ref --quiet --short HEAD 2>$null;if($LASTEXITCODE -ne 1){throw 'GPUI detached rollback state mismatch'}}else{& git -C $Repository checkout $Branch|Out-Null;if($LASTEXITCODE){throw 'GPUI branch checkout rollback failed'};& git -C $Repository reset --hard $Head|Out-Null;if($LASTEXITCODE){throw 'GPUI branch reset rollback failed'};if((git -C $Repository symbolic-ref --short HEAD).Trim() -ne $Branch){throw 'GPUI branch rollback state mismatch'}};if((git -C $Repository rev-parse HEAD).Trim() -ne $Head -or @(git -C $Repository status --porcelain).Count){throw 'GPUI rollback did not restore clean HEAD'} }
Export-ModuleMember -Function New-GpuiSnapshotAuthorityV1,Invoke-GpuiSnapshotAuthorityCommand,Invoke-GpuiSnapshotCandidatePipelineCore,New-GpuiSnapshotArtifactProofV1,Invoke-GpuiSnapshotPromotionCore,Invoke-GpuiPromotionFinalizeV1,Restore-GpuiPromotionBaselineV1
