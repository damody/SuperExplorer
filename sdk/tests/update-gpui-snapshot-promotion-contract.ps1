[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$promotion = Get-Content -LiteralPath (Join-Path $repo 'sdk\scripts\promote-gpui-snapshot.ps1') -Raw
$candidate = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\update-gpui-snapshot.yml') -Raw
$workflow = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\promote-gpui-snapshot.yml') -Raw
foreach ($required in @('SUPEREXPLORER_GPUI_APPROVAL_HMAC_KEY', 'candidate approval/digest identity mismatch', 'candidate metadata does not retain the development candidate channel/state/proof schema', 'promoted GPUI metadata did not preserve approved channel/state/proof', 'staged GPUI gitlink does not match the promoted candidate revision', 'Invoke-GpuiSnapshotPromotionCore', 'Invoke-GpuiPromotionFinalizeV1', 'Invoke-CanonicalBundleGeneration', 'invoke-gpui-update-gates.ps1', "'add','--','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json'", 'promotion did not stage the regenerated approved SDK outputs', 'promotion-proof.json', 'repository baseline changed; compare-and-swap promotion refused', 'GPUI remote head changed; compare-and-swap promotion refused', 'GPUI remote head advanced during promotion; compare-and-swap refused', 'Restore-GpuiPromotionBaselineV1', "'commit'", "'push'", 'promotion failed:')) {
    if ($promotion -notlike "*$required*") { throw "promotion script lost required safety boundary: $required" }
}
$core = Get-Content -LiteralPath (Join-Path $repo 'sdk\scripts\gpui-snapshot-transaction.psm1') -Raw
foreach ($required in @('Invoke-GpuiSnapshotPromotionCore', 'apply --index', 'promotion core staged gitlink mismatch', 'reset --hard', 'checkout --detach $CandidateRevision', 'requires complete GPUI history', 'candidate tree mismatch', 'GPUI rollback failed')) { if ($core -notlike "*$required*") { throw "promotion transaction core lost required behavior: $required" } }
if($core -notlike '*Export-ModuleMember*Restore-GpuiPromotionBaselineV1*'){throw 'promotion transaction module does not export the shared GPUI rollback helper'}
foreach ($required in @('promotion manifest payload set mismatch', '$manifestNames.Count -ne $payloadFiles.Count', 'Select-Object -Unique', "'candidate.patch'", "'sdk/Cargo.lock'", "'sdk/vendor/cargo-sources'", "'sdk/fixtures/rust-folder-size-visual-column/Cargo.lock'", 'promotion artifact schema mismatch')) {
    if ($promotion -notlike "*$required*") { throw "promotion script does not reject omitted/duplicate payloads: $required" }
}
foreach ($required in @('GPUI_SNAPSHOT_APPROVAL_HMAC_KEY', 'promotion-manifest.json', 'promotion-proof.json', 'HMACSHA256', 'Candidate outputs only; publication requires a separate protected compare-and-swap promotion.')) {
    if ($candidate -notlike "*$required*") { throw "candidate workflow lost protected promotion evidence: $required" }
}
foreach ($required in @('contents: write', 'gpui-snapshot-promotion', 'run-id:', 'github-token:', 'GPUI_SNAPSHOT_APPROVAL_HMAC_KEY', 'promote-gpui-snapshot.ps1', '-Push')) {
    if ($workflow -notlike "*$required*") { throw "promotion workflow lost required gate: $required" }
}
function Assert-RejectsPayloadSet([string[]]$Names, [string]$Label) {
    $temp=Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-promotion-' + [guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $temp|Out-Null
    try {
        foreach($name in @('approval.json','candidate-attestation.json','approved-gpui.json','sdk-lock.json','bundle-manifest.json','ui-abi-fingerprint.json','gpui-revision.txt','candidate.patch')){[IO.File]::WriteAllText((Join-Path $temp $name),'{}',[Text.UTF8Encoding]::new($false))}
        [IO.File]::WriteAllText((Join-Path $temp 'promotion-proof.json'),([ordered]@{schema_version=1;hmac_sha256=('a'*64)}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
        $entries=@($Names|ForEach-Object{[ordered]@{name=$_;sha256=('a'*64)}})
        [IO.File]::WriteAllText((Join-Path $temp 'promotion-manifest.json'),([ordered]@{schema_version=1;repository_baseline_commit=('a'*40);files=$entries}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
        $old=$env:SUPEREXPLORER_GPUI_APPROVAL_HMAC_KEY;$env:SUPEREXPLORER_GPUI_APPROVAL_HMAC_KEY='contract'
        try{$saved=$ErrorActionPreference;$ErrorActionPreference='Continue';try{& powershell.exe -NoProfile -File (Join-Path $repo 'sdk\scripts\promote-gpui-snapshot.ps1') -CandidateDirectory $temp -Branch main 2>$null;$exit=$LASTEXITCODE}finally{$ErrorActionPreference=$saved}}finally{if($null -eq $old){Remove-Item Env:SUPEREXPLORER_GPUI_APPROVAL_HMAC_KEY -ErrorAction SilentlyContinue}else{$env:SUPEREXPLORER_GPUI_APPROVAL_HMAC_KEY=$old}}
        if($exit -eq 0){throw "$Label payload set was accepted"}
    } finally {Remove-Item -LiteralPath $temp -Recurse -Force}
}
$source=Get-Content -LiteralPath (Join-Path $repo 'sdk\scripts\promote-gpui-snapshot.ps1') -Raw
foreach($requiredControl in @('Invoke-AttestedFullGate', 'full-gate-attestation.json', 'approval.gates', 'Invoke-AttestedFullGate $CandidateDirectory $approval $promotedPath', 'Invoke-CanonicalBundleGeneration; Invoke-Contract')) { if(-not $source.Contains($requiredControl)){throw "promotion does not construct and persist full gate attestation: $requiredControl"} }
$exact=@('approval.json','candidate-attestation.json','approved-gpui.json','sdk-lock.json','bundle-manifest.json','ui-abi-fingerprint.json','gpui-revision.txt','candidate.patch')
Assert-RejectsPayloadSet @($exact|Where-Object{$_ -ne 'candidate.patch'}) 'omitted candidate.patch'
Assert-RejectsPayloadSet @($exact + 'candidate.patch') 'duplicate candidate.patch'
Write-Output 'gpui snapshot promotion contract passed'
