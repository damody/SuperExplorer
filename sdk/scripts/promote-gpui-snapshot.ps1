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

$payloadFiles = @('approval.json','candidate-attestation.json','approved-gpui.json','sdk-lock.json','bundle-manifest.json','ui-abi-fingerprint.json','gpui-revision.txt','candidate.patch')
$required = @($payloadFiles + @('promotion-manifest.json','promotion-proof.json'))
foreach ($name in $required) { if (-not (Test-Path -LiteralPath (Join-Path $CandidateDirectory $name) -PathType Leaf)) { throw "candidate artifact missing $name" } }
$manifestPath=Join-Path $CandidateDirectory 'promotion-manifest.json';$proofPath=Join-Path $CandidateDirectory 'promotion-proof.json'
$manifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json;$proof=Get-Content -LiteralPath $proofPath -Raw|ConvertFrom-Json
if($manifest.schema_version -ne 1 -or $proof.schema_version -ne 1 -or @($manifest.PSObject.Properties.Name|Where-Object{$_ -notin @('schema_version','repository_baseline_commit','files')}).Count -or @($proof.PSObject.Properties.Name|Where-Object{$_ -notin @('schema_version','hmac_sha256')}).Count){throw 'promotion artifact schema mismatch'}
$manifestNames=@($manifest.files|ForEach-Object{[string]$_.name});if($manifestNames.Count -ne $payloadFiles.Count -or @($manifestNames|Select-Object -Unique).Count -ne $payloadFiles.Count -or @($payloadFiles|Where-Object{$_ -notin $manifestNames}).Count -or @($manifestNames|Where-Object{$_ -notin $payloadFiles}).Count){throw 'promotion manifest payload set mismatch'}
foreach($entry in @($manifest.files)){if(@($entry.PSObject.Properties.Name|Where-Object{$_ -notin @('name','sha256')}).Count -or [string]$entry.name -notin $payloadFiles -or [string]$entry.sha256 -notmatch '^[0-9a-f]{64}$' -or (Get-Hash (Join-Path $CandidateDirectory $entry.name)) -ne $entry.sha256){throw 'candidate artifact hash mismatch'}}
$approval=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'approval.json') -Raw|ConvertFrom-Json;$candidate=Get-Content -LiteralPath (Join-Path $CandidateDirectory 'candidate-attestation.json') -Raw|ConvertFrom-Json
$planInput="$($approval.old_revision)`n$($approval.new_revision)`n$($approval.new_tree)`n$($approval.workflow_run_id)`n$($approval.nonce)";$sha=[Security.Cryptography.SHA256]::Create();try{$recomputedDigest=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($planInput)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
if($approval.baseline_revision -ne $approval.old_revision -or $approval.candidate_plan_digest -ne $recomputedDigest -or $candidate.candidate_plan_digest -ne $recomputedDigest -or $candidate.source.revision -ne $approval.new_revision -or $candidate.source.tree -ne $approval.new_tree){throw 'candidate approval/digest identity mismatch'}
$payload="$($approval.baseline_revision)`n$($approval.new_revision)`n$($approval.new_tree)`n$($approval.candidate_plan_digest)`n$($approval.workflow_run_id)`n$($approval.nonce)`n$($manifest.repository_baseline_commit)`n$(Get-Hash $manifestPath)"
if($proof.hmac_sha256 -notmatch '^[0-9a-f]{64}$' -or $proof.hmac_sha256 -cne (Get-Hmac $payload)){throw 'promotion proof HMAC is invalid'}
$baseline=(Invoke-Git @('rev-parse','HEAD')|Select-Object -Last 1).Trim();if($baseline -ne $manifest.repository_baseline_commit){throw 'repository baseline changed; compare-and-swap promotion refused'}
if(@(Invoke-Git @('status','--porcelain')).Count){throw 'promotion requires a clean repository'}
$gpui=Join-Path $repo 'vendor\gpui-ce';$origin=(git -C $gpui remote get-url origin).Trim();if($origin -ne 'https://github.com/damody/gpui-ce-explorer.git'){throw 'unauthorized GPUI origin'}
git -C $gpui fetch --no-tags origin main --quiet;if($LASTEXITCODE){throw 'could not revalidate GPUI remote head'};$remote=(git -C $gpui rev-parse origin/main).Trim();if($remote -ne $approval.new_revision -or $candidate.source.revision -ne $remote){throw 'GPUI remote head changed; compare-and-swap promotion refused'}
$allowed=@('sdk/snapshot/approved-gpui.json','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json','vendor/gpui-ce')
try {
 Invoke-Git @('apply','--index',(Join-Path $CandidateDirectory 'candidate.patch'))|Out-Null
 $changed=@(Invoke-Git @('diff','--cached','--name-only'));if(@($changed|Where-Object{$_ -notin $allowed}).Count -or @($changed|Where-Object{$_ -in $allowed}).Count -ne $allowed.Count){throw 'candidate patch changes an unexpected promotion surface'}
 Invoke-Git @('commit','-m',"chore(sdk): promote GPUI snapshot $remote")|Out-Null
 if($Push){Invoke-Git @('push','origin',"HEAD:refs/heads/$Branch")|Out-Null}
 Write-Output 'GPUI snapshot promotion committed at one atomic Git commit boundary'
} catch { $failure=$_;try{Invoke-Git @('reset','--hard',$baseline)|Out-Null}catch{throw "promotion failed: $($failure.Exception.Message); rollback failed: $($_.Exception.Message)"};throw $failure }
