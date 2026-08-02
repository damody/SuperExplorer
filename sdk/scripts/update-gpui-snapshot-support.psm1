Set-StrictMode -Version Latest
function Assert-GpuiUpdateApproval {
 param($Approval,[Parameter(Mandatory)][string]$OldRevision,[Parameter(Mandatory)][string]$NewRevision,[Parameter(Mandatory)][string]$NewTree,[Parameter(Mandatory)][string]$CandidatePlanDigest,[Parameter(Mandatory)][string]$WorkflowRunId,[Parameter(Mandatory)][string]$Nonce,[Parameter(Mandatory)][bool]$FastForward,[datetime]$Now=[DateTime]::UtcNow)
 if($FastForward -and $null -eq $Approval){return};if($null -eq $Approval){throw 'approval required'}
 $required=@('schema_version','baseline_revision','old_revision','new_revision','new_tree','candidate_plan_digest','workflow_run_id','nonce','reason','approver','issued_utc','expires_utc');$actual=@($Approval.PSObject.Properties.Name);if(@($required|?{$_ -notin $actual}).Count -or @($actual|?{$_ -notin $required}).Count){throw 'approval schema mismatch'}
 if($Approval.schema_version -ne 1 -or $Approval.baseline_revision -notmatch '^[0-9a-f]{40}$' -or $Approval.old_revision -notmatch '^[0-9a-f]{40}$' -or $Approval.new_revision -notmatch '^[0-9a-f]{40}$' -or $Approval.new_tree -notmatch '^[0-9a-f]{40}$' -or $Approval.candidate_plan_digest -notmatch '^[0-9a-f]{64}$'){throw 'approval hex/schema invalid'}
 foreach($p in 'workflow_run_id','nonce','reason','approver'){if([string]::IsNullOrWhiteSpace([string]$Approval.$p)){throw 'approval string invalid'}}
 if($Approval.baseline_revision -ne $OldRevision -or $Approval.old_revision -ne $OldRevision -or $Approval.new_revision -ne $NewRevision -or $Approval.new_tree -ne $NewTree -or $Approval.candidate_plan_digest -ne $CandidatePlanDigest -or $Approval.workflow_run_id -ne $WorkflowRunId -or $Approval.nonce -ne $Nonce){throw 'approval identity mismatch'}
 try{$issued=[datetime]::Parse($Approval.issued_utc).ToUniversalTime();$expires=[datetime]::Parse($Approval.expires_utc).ToUniversalTime()}catch{throw 'approval timestamp invalid'};if($issued -gt $Now.ToUniversalTime() -or $Now.ToUniversalTime() -ge $expires -or $expires -gt $issued.AddHours(24)){throw 'approval window invalid'};return $Approval
}

function Invoke-WithFileTransaction {
 param(
  [Parameter(Mandatory)][string[]]$Path,
  [Parameter(Mandatory)][scriptblock]$Action
 )
 $backup=@{}
 foreach($item in @($Path|Select-Object -Unique)){
  $backup[$item]=if(Test-Path -LiteralPath $item){[IO.File]::ReadAllBytes($item)}else{$null}
 }
 try { & $Action }
 catch {
  $failure=$_;$rollbackErrors=@()
  foreach($item in $backup.Keys){
   try {
    if($null -eq $backup[$item]){
     if(Test-Path -LiteralPath $item){Remove-Item -LiteralPath $item -Force -ErrorAction Stop}
    } else {
     $parent=Split-Path -Parent $item
     if(-not (Test-Path -LiteralPath $parent)){New-Item -ItemType Directory -Path $parent -Force -ErrorAction Stop|Out-Null}
     [IO.File]::WriteAllBytes($item,$backup[$item])
    }
   } catch {$rollbackErrors+="${item}: $($_.Exception.Message)"}
  }
  if($rollbackErrors.Count){throw "transaction failed ($($failure.Exception.Message)); rollback failed: $($rollbackErrors -join '; ')"}
  throw $failure
 }
}

function Invoke-GpuiGit {
 param([Parameter(Mandatory)][string]$Repository,[Parameter(Mandatory)][string[]]$Arguments)
 $savedErrorActionPreference=$ErrorActionPreference;$ErrorActionPreference='Continue'
 try{$output=& git -C $Repository @Arguments 2>&1;$exit=$LASTEXITCODE}finally{$ErrorActionPreference=$savedErrorActionPreference}
 if($exit -ne 0){throw "git -C $Repository $($Arguments -join ' ') failed ($exit): $($output -join "`n")"}
 return @($output)
}

function Get-GpuiCheckoutState {
 param([Parameter(Mandatory)][string]$Repository)
 $head=((Invoke-GpuiGit $Repository @('rev-parse','HEAD'))-join "`n").Trim()
 $branchOutput=& git -C $Repository symbolic-ref --quiet --short HEAD 2>$null;$branchExit=$LASTEXITCODE
 if($branchExit -ne 0 -and $branchExit -ne 1){throw "could not determine GPUI checkout branch (exit $branchExit)"}
 [pscustomobject]@{head=$head;branch=if($branchExit -eq 0){($branchOutput-join "`n").Trim()}else{$null}}
}

function Assert-GpuiRepositoryComplete {
 param([Parameter(Mandatory)][string]$Repository,[Parameter(Mandatory)][string[]]$Revisions)
 $shallow=((Invoke-GpuiGit $Repository @('rev-parse','--is-shallow-repository'))-join "`n").Trim()
 if($shallow -ne 'false'){throw 'GPUI repository is shallow; complete history is required'}
 foreach($revision in $Revisions){
  if($revision -notmatch '^[0-9a-f]{40}$'){throw 'GPUI revision must be a complete commit hash'}
  Invoke-GpuiGit $Repository @('cat-file','-e',"$revision`^{commit}")|Out-Null
  Invoke-GpuiGit $Repository @('cat-file','-e',"$revision`^{tree}")|Out-Null
 }
}

function Restore-GpuiCheckoutState {
 param([Parameter(Mandatory)][string]$Repository,[Parameter(Mandatory)]$State)
 if([string]::IsNullOrWhiteSpace([string]$State.branch)){Invoke-GpuiGit $Repository @('checkout','--detach',$State.head)|Out-Null}
 else {Invoke-GpuiGit $Repository @('checkout',[string]$State.branch)|Out-Null}
 Invoke-GpuiGit $Repository @('reset','--hard',$State.head)|Out-Null
 $restored=Get-GpuiCheckoutState $Repository
 if($restored.head -ne $State.head -or $restored.branch -ne $State.branch){throw 'GPUI checkout rollback did not restore the original HEAD state'}
 if(@(Invoke-GpuiGit $Repository @('status','--porcelain')).Count){throw 'GPUI checkout rollback left tracked changes'}
}

Export-ModuleMember Assert-GpuiUpdateApproval,Invoke-WithFileTransaction,Invoke-GpuiGit,Get-GpuiCheckoutState,Assert-GpuiRepositoryComplete,Restore-GpuiCheckoutState
