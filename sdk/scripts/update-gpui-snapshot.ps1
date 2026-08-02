[CmdletBinding()]param()
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$gpui=Join-Path $repo 'vendor\gpui-ce';$snap=Join-Path $repo 'sdk\snapshot\approved-gpui.json'
Import-Module (Join-Path $repo 'sdk\scripts\update-gpui-snapshot-support.psm1') -Force
function Invoke-Step([string]$Path,[string[]]$Arguments){& powershell.exe -NoProfile -File $Path @Arguments; if($LASTEXITCODE -ne 0){throw "gate failed: $Path ($LASTEXITCODE)"}}
function Invoke-CommandStep([string]$File,[string[]]$Arguments){& $File @Arguments; if($LASTEXITCODE -ne 0){throw "command failed: $File ($LASTEXITCODE)"}}
Push-Location $repo
try {
 if((git status --porcelain)){throw 'dirty worktree; update requires a clean repository'}
 $origin=(git -C $gpui remote get-url origin).Trim();if($origin -ne 'https://github.com/damody/gpui-ce-explorer.git'){throw 'unauthorized GPUI origin'}
 $oldMeta=Get-Content $snap -Raw|ConvertFrom-Json;$old=$oldMeta.source.revision;$oldBundle=(Get-Content (Join-Path $repo 'sdk\sdk-lock.json') -Raw|ConvertFrom-Json).bundle_id;$oldHead=(git -C $gpui rev-parse HEAD).Trim();git -C $gpui fetch origin main --quiet
 $new=(git -C $gpui rev-parse origin/main).Trim();$tree=(git -C $gpui rev-parse "$new`^{tree}").Trim();$parent=(git -C $gpui rev-list -1 "$new^" ).Trim();$time=(git -C $gpui show -s --format=%cI $new).Trim();$ff=((git -C $gpui merge-base $old $new).Trim() -eq $old)
 $runId=$env:SUPEREXPLORER_GPUI_UPDATE_RUN_ID;$nonce=$env:SUPEREXPLORER_GPUI_UPDATE_NONCE;if([string]::IsNullOrWhiteSpace($runId)-or[string]::IsNullOrWhiteSpace($nonce)){throw 'workflow run identity/nonce required'}
 $plan="$old`n$new`n$tree`n$runId`n$nonce";$sha=[Security.Cryptography.SHA256]::Create();try{$digest=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($plan)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
 $approval=$null;if($env:SUPEREXPLORER_GPUI_UPDATE_APPROVAL){$approval=$env:SUPEREXPLORER_GPUI_UPDATE_APPROVAL|ConvertFrom-Json};Assert-GpuiUpdateApproval $approval $old $new $tree $digest $runId $nonce $ff ([DateTime]::UtcNow)|Out-Null
 if($new -eq $old){throw 'candidate revision must change bundle ID'}
 $approvalFields=if($null -eq $approval){$null}else{$approval}
 $meta=[pscustomobject]@{schema_version=1;source=[pscustomobject]@{repository=$origin;update_branch='main';resolved_ref='refs/remotes/origin/main';revision=$new;tree=$tree;parent=$parent;commit_time=$time;package='gpui';package_version='0.2.2'};approval=$approvalFields;candidate_plan_digest=$digest;workflow_run_id=$runId;nonce=$nonce;production=[pscustomobject]@{default_features=$false;features=@()};release_frozen=$false}
 $json=$meta|ConvertTo-Json -Depth 8;$runTemp=if($env:RUNNER_TEMP){$env:RUNNER_TEMP}else{[IO.Path]::GetTempPath()};$runDir=Join-Path $runTemp 'superexplorer-gpui-update';New-Item -ItemType Directory -Force -Path $runDir|Out-Null;$candidate=Join-Path $runDir 'candidate-attestation.json'
 $paths=@($snap,$candidate,(Join-Path $repo 'sdk\sdk-lock.json'),(Join-Path $repo 'sdk\bundle-manifest.json'),(Join-Path $repo 'sdk\ui-abi-fingerprint.json'))
 Invoke-WithFileTransaction $paths {
   Invoke-CommandStep 'git' @('-C',$gpui,'checkout','--detach',$new)
   [IO.File]::WriteAllText($candidate,$json,[Text.UTF8Encoding]::new($false));[IO.File]::WriteAllText($snap,$json,[Text.UTF8Encoding]::new($false))
   Invoke-Step (Join-Path $repo 'sdk\tests\toolchain-contract.ps1') @();Invoke-Step (Join-Path $repo 'sdk\tests\abi-layout-contract.ps1') @();Invoke-Step (Join-Path $repo 'sdk\tests\gpui-baseline-contract.ps1') @();Invoke-Step (Join-Path $repo 'sdk\tests\protected-dependency-contract.ps1') @();Invoke-Step (Join-Path $repo 'sdk\tests\bundle-generator-contract.ps1') @();Invoke-Step (Join-Path $repo 'sdk\tests\ui-abi-fingerprint-contract.ps1') @();Invoke-Step (Join-Path $repo 'sdk\tests\offline-host-plugin-contract.ps1') @()
   $bundle=(Get-Content (Join-Path $repo 'sdk\sdk-lock.json') -Raw|ConvertFrom-Json).bundle_id;if($bundle -eq $oldBundle){throw 'bundle ID did not change'}
   git -C $gpui fetch origin main --quiet;$check=(git -C $gpui rev-parse origin/main).Trim();if($check -ne $new){throw 'remote main advanced during update; refusing publish'}
 }
 Write-Output 'GPUI snapshot transaction succeeded; changes left for CI artifact collection'
} catch { try { & git -C $gpui checkout --detach $oldHead | Out-Null } catch {}; try { $gen=Join-Path $repo 'sdk\tools\bundle-generator'; Push-Location $gen; & cargo.exe run --release --locked -- verify | Out-Null; Pop-Location } catch { try { Pop-Location } catch {} }; throw } finally { Pop-Location }
