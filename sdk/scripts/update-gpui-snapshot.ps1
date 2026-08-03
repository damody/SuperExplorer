[CmdletBinding()]param()
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$gpui=Join-Path $repo 'vendor\gpui-ce';$snap=Join-Path $repo 'sdk\snapshot\approved-gpui.json'
Import-Module (Join-Path $repo 'sdk\scripts\update-gpui-snapshot-support.psm1') -Force
Import-Module (Join-Path $repo 'sdk\scripts\gpui-snapshot-transaction.psm1') -Force
$authority=New-GpuiSnapshotAuthorityV1 -RepositoryRoot $repo -ExpectedOrigin 'https://github.com/damody/gpui-ce-explorer.git' -GpuiRepository $gpui -GateManifestPath (Join-Path $repo 'sdk\ci\gpui-update-gates.json') -CommandRunner { param($kind,$arguments) throw "unexpected production authority command: $kind" }
function Invoke-Step([string]$Path,[string[]]$Arguments){& powershell.exe -NoProfile -File $Path @Arguments; if($LASTEXITCODE -ne 0){throw "gate failed: $Path ($LASTEXITCODE)"}}
function Invoke-CommandStep([string]$File,[string[]]$Arguments){& $File @Arguments; if($LASTEXITCODE -ne 0){throw "command failed: $File ($LASTEXITCODE)"}}
Push-Location $repo
$oldState=$null
try {
 if((git status --porcelain)){throw 'dirty worktree; update requires a clean repository'}
 $origin=(git -C $gpui remote get-url origin).Trim();if($origin -ne 'https://github.com/damody/gpui-ce-explorer.git'){throw 'unauthorized GPUI origin'}
 $oldMeta=Get-Content $snap -Raw|ConvertFrom-Json;$old=$oldMeta.source.revision;$oldBundle=(Get-Content (Join-Path $repo 'sdk\sdk-lock.json') -Raw|ConvertFrom-Json).bundle_id;$oldState=Get-GpuiCheckoutState $gpui
 if($old -notmatch '^[0-9a-f]{40}$' -or $old -ne $oldState.head){throw 'approved GPUI revision must be the current complete checkout HEAD'}
 Invoke-CommandStep 'git' @('-C',$gpui,'fetch','--no-tags','origin','main','--quiet')
 $new=((Invoke-GpuiGit $gpui @('rev-parse','origin/main'))-join "`n").Trim();Assert-GpuiRepositoryComplete $gpui @($old,$new)
 $tree=((Invoke-GpuiGit $gpui @('rev-parse',"$new`^{tree}"))-join "`n").Trim();$parent=((Invoke-GpuiGit $gpui @('rev-list','-1',"$new^"))-join "`n").Trim();$time=((Invoke-GpuiGit $gpui @('show','-s','--format=%cI',$new))-join "`n").Trim();$ff=(((Invoke-GpuiGit $gpui @('merge-base',$old,$new))-join "`n").Trim() -eq $old)
 $runId=$env:SUPEREXPLORER_GPUI_UPDATE_RUN_ID;$nonce=$env:SUPEREXPLORER_GPUI_UPDATE_NONCE;if([string]::IsNullOrWhiteSpace($runId)-or[string]::IsNullOrWhiteSpace($nonce)){throw 'workflow run identity/nonce required'}
 $plan="$old`n$new`n$tree`n$runId`n$nonce";$sha=[Security.Cryptography.SHA256]::Create();try{$digest=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($plan)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
 $approval=$null;if($env:SUPEREXPLORER_GPUI_UPDATE_APPROVAL){$approval=$env:SUPEREXPLORER_GPUI_UPDATE_APPROVAL|ConvertFrom-Json};Assert-GpuiUpdateApproval $approval $old $new $tree $digest $runId $nonce $ff ([DateTime]::UtcNow)|Out-Null
 if($new -eq $old){throw 'candidate revision must change bundle ID'}
 $approvalProof=if($null -eq $approval){[pscustomobject]@{schema_version=1;kind='fast-forward';baseline_revision=$old;old_revision=$old;new_revision=$new;new_tree=$tree;candidate_plan_digest=$digest;workflow_run_id=$runId;nonce=$nonce;reason='automatic-fast-forward';approver='automation';issued_utc=[DateTime]::UtcNow.ToString('o');expires_utc=[DateTime]::UtcNow.AddHours(1).ToString('o')}}else{$approval}
 $candidateApproval=[pscustomobject]@{channel='development';state='candidate';proof=$approvalProof}
 $meta=[pscustomobject]@{schema_version=1;source=[pscustomobject]@{repository=$origin;update_branch='main';resolved_ref='refs/remotes/origin/main';revision=$new;tree=$tree;parent=$parent;commit_time=$time;package='gpui';package_version='0.2.2'};approval=$candidateApproval;candidate_plan_digest=$digest;workflow_run_id=$runId;nonce=$nonce;production=[pscustomobject]@{default_features=$false;features=@()};release_frozen=$false}
 $json=$meta|ConvertTo-Json -Depth 8;$runTemp=if($env:RUNNER_TEMP){$env:RUNNER_TEMP}else{[IO.Path]::GetTempPath()};$runDir=Join-Path $runTemp 'superexplorer-gpui-update';New-Item -ItemType Directory -Force -Path $runDir|Out-Null;$candidate=Join-Path $runDir 'candidate-attestation.json'
 $paths=@($snap,$candidate,(Join-Path $repo 'Cargo.lock'),(Join-Path $repo 'sdk\Cargo.lock'),(Join-Path $repo 'sdk\sdk-lock.json'),(Join-Path $repo 'sdk\bundle-manifest.json'),(Join-Path $repo 'sdk\ui-abi-fingerprint.json'),(Join-Path $repo 'sdk\snapshot\protected-dependency-closure.json'),(Join-Path $repo 'sdk\fixtures\p0-consumer\Cargo.lock'))
 $rollbackGitPaths=@('sdk/snapshot/approved-gpui.json','Cargo.lock','sdk/Cargo.lock','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json','sdk/snapshot/protected-dependency-closure.json','sdk/vendor/cargo-sources','sdk/fixtures/p0-consumer/Cargo.lock','vendor/gpui-ce')
 Invoke-GpuiCandidateTransaction $paths $rollbackGitPaths $repo $gpui $origin $new $tree '0.2.2' 'vendor/gpui-ce' {
   [IO.File]::WriteAllText($candidate,$json,[Text.UTF8Encoding]::new($false));[IO.File]::WriteAllText($snap,$json,[Text.UTF8Encoding]::new($false))
 } {
   Invoke-GpuiSnapshotCandidatePipelineCore -Authority $authority -Update {} -Refresh { Invoke-Step (Join-Path $repo 'sdk\scripts\refresh-gpui-dependency-snapshot.ps1') @('-RepositoryRoot',$repo,'-GpuiRevision',$new) } -Aggregate { Invoke-Step (Join-Path $repo 'sdk\scripts\invoke-gpui-update-gates.ps1') @('-RepositoryRoot',$repo) } -Artifact {}
   $bundle=(Get-Content (Join-Path $repo 'sdk\sdk-lock.json') -Raw|ConvertFrom-Json).bundle_id;if($bundle -eq $oldBundle){throw 'bundle ID did not change'}
 } {
   Invoke-CommandStep 'git' @('-C',$gpui,'fetch','--no-tags','origin','main','--quiet');$check=((Invoke-GpuiGit $gpui @('rev-parse','origin/main'))-join "`n").Trim();if($check -ne $new){throw 'remote main advanced during update; refusing publish'}
 }
 Write-Output 'GPUI snapshot candidate transaction succeeded; outputs remain candidate-only pending separate protected promotion'
} catch { $failure=$_; if($null -ne $oldState){try{Restore-GpuiCheckoutState $gpui $oldState}catch{throw "snapshot update failed: $($failure.Exception.Message); GPUI checkout rollback failed: $($_.Exception.Message)"}}; throw $failure } finally { Pop-Location }
