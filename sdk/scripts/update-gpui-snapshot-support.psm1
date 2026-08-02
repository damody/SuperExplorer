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
 foreach($item in $Path){
  $backup[$item]=if(Test-Path -LiteralPath $item){[IO.File]::ReadAllBytes($item)}else{$null}
 }
 try { & $Action }
 catch {
  foreach($item in $Path){
   if($null -eq $backup[$item]){
    if(Test-Path -LiteralPath $item){Remove-Item -LiteralPath $item -Force}
   } else {
    $parent=Split-Path -Parent $item
    if(-not (Test-Path -LiteralPath $parent)){New-Item -ItemType Directory -Path $parent -Force|Out-Null}
    [IO.File]::WriteAllBytes($item,$backup[$item])
   }
  }
  throw
 }
}

Export-ModuleMember Assert-GpuiUpdateApproval,Invoke-WithFileTransaction
