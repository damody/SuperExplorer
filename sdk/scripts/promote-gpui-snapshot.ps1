[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$CandidateDirectory,
    [Parameter(Mandatory)][string]$Branch,
    [switch]$Push
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$CandidateDirectory = (Resolve-Path -LiteralPath $CandidateDirectory).Path
$key = [string]$env:SUPEREXPLORER_GPUI_APPROVAL_HMAC_KEY
if ([string]::IsNullOrWhiteSpace($key)) { throw 'protected GPUI approval HMAC key is required' }
function Get-Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Invoke-Git([string[]]$Arguments) { $output = & git -C $repo @Arguments 2>&1; if ($LASTEXITCODE -ne 0) { throw "git $($Arguments -join ' ') failed: $($output -join "`n")" }; @($output) }
function Get-Hmac([string]$Value) { $hmac=[Security.Cryptography.HMACSHA256]::new([Text.Encoding]::UTF8.GetBytes($key));try{([BitConverter]::ToString($hmac.ComputeHash([Text.Encoding]::UTF8.GetBytes($Value)))).Replace('-','').ToLowerInvariant()}finally{$hmac.Dispose()} }
function Invoke-Contract([string]$Path) { & powershell.exe -NoProfile -File $Path; if($LASTEXITCODE -ne 0){throw "promotion contract failed: $Path ($LASTEXITCODE)"} }
function Invoke-AttestedFullGate([string]$CandidatePath, $Approval, [string]$SnapshotPath) {
    $attestationPath = Join-Path $CandidatePath 'full-gate-attestation.json'
    & powershell.exe -NoProfile -File (Join-Path $repo 'sdk\scripts\invoke-gpui-update-gates.ps1') -RepositoryRoot $repo -AttestationPath $attestationPath -CandidatePlanDigest ([string]$Approval.candidate_plan_digest) -WorkflowRunId ([string]$Approval.workflow_run_id) -Nonce ([string]$Approval.nonce)
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $attestationPath -PathType Leaf)) { throw 'full GPUI gate attestation generation failed' }
    $attestation = Get-Content -LiteralPath $attestationPath -Raw | ConvertFrom-Json
    $snapshot = Get-Content -LiteralPath $SnapshotPath -Raw | ConvertFrom-Json
    $snapshot.approval | Add-Member -NotePropertyName gates -NotePropertyValue $attestation -Force
    [IO.File]::WriteAllText($SnapshotPath, (($snapshot | ConvertTo-Json -Depth 12) + "`n"), [Text.UTF8Encoding]::new($false))
    Invoke-Git @('add', '--', 'sdk/snapshot/approved-gpui.json') | Out-Null
}
function Invoke-CanonicalBundleGeneration {
    Push-Location (Join-Path $repo 'sdk\tools\bundle-generator')
    try { & cargo.exe run --release --locked -- generate; if($LASTEXITCODE -ne 0){throw "canonical bundle generation failed ($LASTEXITCODE)"} }
    finally { Pop-Location }
    Invoke-Git @('add','--','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json') | Out-Null
}
function Assert-ApprovedRemote([string]$ExpectedRevision) {
    & git -C $gpui fetch --no-tags origin main --quiet
    if($LASTEXITCODE){throw 'could not revalidate GPUI remote head'}
    $actual=(git -C $gpui rev-parse origin/main).Trim()
    if($actual -ne $ExpectedRevision){throw 'GPUI remote head advanced during promotion; compare-and-swap refused'}
}

$payloadFiles = @('approval.json','candidate-attestation.json','approved-gpui.json','sdk-lock.json','bundle-manifest.json','ui-abi-fingerprint.json','gpui-revision.txt','candidate.patch')
$required = @($payloadFiles + @('promotion-manifest.json','promotion-proof.json'))
foreach ($name in $required) { if (-not (Test-Path -LiteralPath (Join-Path $CandidateDirectory $name) -PathType Leaf)) { throw "candidate artifact missing $name" } }
$manifestPath=Join-Path $CandidateDirectory 'promotion-manifest.json';$proofPath=Join-Path $CandidateDirectory 'promotion-proof.json'
$manifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json;$promotionProof=Get-Content -LiteralPath $proofPath -Raw|ConvertFrom-Json
if($manifest.schema_version -ne 1 -or $promotionProof.schema_version -ne 1 -or @($manifest.PSObject.Properties.Name|Where-Object{$_ -notin @('schema_version','repository_baseline_commit','files')}).Count -or @($promotionProof.PSObject.Properties.Name|Where-Object{$_ -notin @('schema_version','hmac_sha256')}).Count){throw 'promotion artifact schema mismatch'}
$manifestNames=@($manifest.files|ForEach-Object{[string]$_.name});if($manifestNames.Count -ne $payloadFiles.Count -or @($manifestNames|Select-Object -Unique).Count -ne $payloadFiles.Count -or @($payloadFiles|Where-Object{$_ -notin $manifestNames}).Count -or @($manifestNames|Where-Object{$_ -notin $payloadFiles}).Count){throw 'promotion manifest payload set mismatch'}
foreach($entry in @($manifest.files)){if(@($entry.PSObject.Properties.Name|Where-Object{$_ -notin @('name','sha256')}).Count -or [string]$entry.name -notin $payloadFiles -or [string]$entry.sha256 -notmatch '^[0-9a-f]{64}$' -or (Get-Hash (Join-Path $CandidateDirectory $entry.name)) -ne $entry.sha256){throw 'candidate artifact hash mismatch'}}
$approval=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'approval.json') -Raw|ConvertFrom-Json;$candidate=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'candidate-attestation.json') -Raw|ConvertFrom-Json;$candidateSnapshot=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'approved-gpui.json') -Raw|ConvertFrom-Json
$planInput="$($approval.old_revision)`n$($approval.new_revision)`n$($approval.new_tree)`n$($approval.workflow_run_id)`n$($approval.nonce)";$sha=[Security.Cryptography.SHA256]::Create();try{$recomputedDigest=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($planInput)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
if($approval.baseline_revision -ne $approval.old_revision -or $approval.candidate_plan_digest -ne $recomputedDigest -or $candidate.candidate_plan_digest -ne $recomputedDigest -or $candidate.source.revision -ne $approval.new_revision -or $candidate.source.tree -ne $approval.new_tree){throw 'candidate approval/digest identity mismatch'}
foreach($metadata in @($candidate,$candidateSnapshot)){
 if($metadata.schema_version -ne 1 -or $metadata.approval.channel -ne 'development' -or $metadata.approval.state -ne 'candidate' -or $null -eq $metadata.approval.proof){throw 'candidate metadata does not retain the development candidate channel/state/proof schema'}
 $candidateProof=$metadata.approval.proof
 foreach($field in @('baseline_revision','old_revision','new_revision','new_tree','candidate_plan_digest','workflow_run_id','nonce')){if([string]$candidateProof.$field -ne [string]$approval.$field){throw "candidate nested proof mismatch: $field"}}
 if($metadata.source.revision -ne $approval.new_revision -or $metadata.source.tree -ne $approval.new_tree){throw 'candidate metadata source identity mismatch'}
}
$payload="$($approval.baseline_revision)`n$($approval.new_revision)`n$($approval.new_tree)`n$($approval.candidate_plan_digest)`n$($approval.workflow_run_id)`n$($approval.nonce)`n$($manifest.repository_baseline_commit)`n$(Get-Hash $manifestPath)"
if($promotionProof.hmac_sha256 -notmatch '^[0-9a-f]{64}$' -or $promotionProof.hmac_sha256 -cne (Get-Hmac $payload)){throw 'promotion proof HMAC is invalid'}
$baseline=(Invoke-Git @('rev-parse','HEAD')|Select-Object -Last 1).Trim();if($baseline -ne $manifest.repository_baseline_commit){throw 'repository baseline changed; compare-and-swap promotion refused'}
if(@(Invoke-Git @('status','--porcelain')).Count){throw 'promotion requires a clean repository'}
$gpui=Join-Path $repo 'vendor\gpui-ce';$gpuiBaseline=(git -C $gpui rev-parse HEAD).Trim();$gpuiBranch=& git -C $gpui symbolic-ref --quiet --short HEAD 2>$null;if($LASTEXITCODE -ne 0){$gpuiBranch=$null};$origin=(git -C $gpui remote get-url origin).Trim();if($origin -ne 'https://github.com/damody/gpui-ce-explorer.git'){throw 'unauthorized GPUI origin'}
git -C $gpui fetch --no-tags origin main --quiet;if($LASTEXITCODE){throw 'could not revalidate GPUI remote head'};$remote=(git -C $gpui rev-parse origin/main).Trim();if($remote -ne $approval.new_revision -or $candidate.source.revision -ne $remote){throw 'GPUI remote head changed; compare-and-swap promotion refused'}
$requiredChanged=@('sdk/snapshot/approved-gpui.json','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json','vendor/gpui-ce')
$allowed=@($requiredChanged+@('Cargo.lock','sdk/Cargo.lock','sdk/snapshot/protected-dependency-closure.json','sdk/fixtures/rust-folder-size-visual-column/Cargo.lock'))
Import-Module (Join-Path $repo 'sdk\scripts\gpui-snapshot-transaction.psm1') -Force
$authority=New-GpuiSnapshotAuthorityV1 -RepositoryRoot $repo -ExpectedOrigin 'https://github.com/damody/gpui-ce-explorer.git' -GpuiRepository (Join-Path $repo 'vendor\gpui-ce') -GateManifestPath (Join-Path $repo 'sdk\ci\gpui-update-gates.json') -CommandRunner { param($kind,$arguments) throw "unexpected authority command: $kind" }
try {
 $promotedPath=Join-Path $repo 'sdk\snapshot\approved-gpui.json'
 $baseline=Invoke-GpuiSnapshotPromotionCore -Authority $authority -CandidateRevision $remote -CandidateTree $approval.new_tree -CandidatePatch (Join-Path $CandidateDirectory 'candidate.patch') -RequiredChanged $requiredChanged -Transition { $promoted=$candidateSnapshot;$promoted.approval.state='approved';[IO.File]::WriteAllText($promotedPath,($promoted|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false)) } -DependencyRunner { Invoke-Contract (Join-Path $repo 'sdk\scripts\invoke-gpui-update-gates.ps1'); Invoke-AttestedFullGate $CandidateDirectory $approval $promotedPath; Invoke-CanonicalBundleGeneration; Invoke-Contract (Join-Path $repo 'sdk\scripts\invoke-gpui-update-gates.ps1') }
 $changed=@(Invoke-Git @('diff','--cached','--name-only'));$unexpected=@($changed|Where-Object{$_ -notin $allowed});if($unexpected.Count -or @($requiredChanged|Where-Object{$_ -notin $changed}).Count){throw 'candidate patch changes an unexpected promotion surface'}
 $stagedGitlink=(Invoke-Git @('rev-parse',':vendor/gpui-ce')|Select-Object -Last 1).Trim();if($stagedGitlink -ne $remote){throw 'staged GPUI gitlink does not match the promoted candidate revision'}
 $promotedCheck=Get-Content -LiteralPath $promotedPath -Raw|ConvertFrom-Json;if($promotedCheck.approval.channel -ne 'development' -or $promotedCheck.approval.state -ne 'approved' -or $null -eq $promotedCheck.approval.proof -or $null -eq $promotedCheck.approval.gates){throw 'promoted GPUI metadata did not preserve approved channel/state/proof/full-gate attestation'}
 foreach($field in @('baseline_revision','old_revision','new_revision','new_tree','candidate_plan_digest','workflow_run_id','nonce')){if([string]$promotedCheck.approval.proof.$field -ne [string]$approval.$field){throw "promoted nested proof mismatch: $field"}}
 $stagedOutputs=@(Invoke-Git @('diff','--cached','--name-only'));$requiredOutputs=@('sdk/snapshot/approved-gpui.json','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json');if(@($requiredOutputs|Where-Object{$_ -notin $stagedOutputs}).Count){throw 'promotion did not stage the regenerated approved SDK outputs'}
 Invoke-GpuiPromotionFinalizeV1 -Authority $authority -Baseline $baseline -GpuiHead $gpuiBaseline -GpuiBranch $gpuiBranch -ExpectedRevision $approval.new_revision -VerifyRemote { param($expected) Assert-ApprovedRemote $expected } -Commit { Invoke-Git @('commit','-m',"chore(sdk): promote GPUI snapshot $remote")|Out-Null } -Push { if($Push){Invoke-Git @('push','origin',"HEAD:refs/heads/$Branch")|Out-Null} }
 Write-Output 'GPUI snapshot promotion committed at one atomic Git commit boundary'
} catch { $failure=$_;try{Invoke-Git @('reset','--hard',$baseline)|Out-Null;Restore-GpuiPromotionBaselineV1 -Repository $gpui -Head $gpuiBaseline -Branch $gpuiBranch}catch{throw "promotion failed: $($failure.Exception.Message); rollback failed: $($_.Exception.Message)"};throw $failure }
