[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Import-Module (Join-Path $repo 'sdk\scripts\gpui-snapshot-transaction.psm1') -Force

function Invoke-Git([string] $Directory, [string[]] $Arguments) {
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $output = & git -C $Directory @Arguments 2>&1; $exit = $LASTEXITCODE }
    finally { $ErrorActionPreference = $saved }
    if ($exit -ne 0) { throw "git failed: $($Arguments -join ' '): $($output -join "`n")" }
}

$temp = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-gpui-e2e-' + [guid]::NewGuid().ToString('N'))
$root = Join-Path $temp 'root'
$gpui = Join-Path $root 'vendor\gpui-ce'
try {
    New-Item -ItemType Directory -Force -Path $root, $gpui, (Join-Path $root 'sdk\snapshot') | Out-Null
    Invoke-Git $root @('init')
    Invoke-Git $root @('config', 'user.email', 'e2e@example.invalid')
    Invoke-Git $root @('config', 'user.name', 'e2e')
    Invoke-Git $gpui @('init')
    Invoke-Git $gpui @('config', 'user.email', 'e2e@example.invalid')
    Invoke-Git $gpui @('config', 'user.name', 'e2e')
    Set-Content (Join-Path $gpui 'value') 'one'
    Invoke-Git $gpui @('add', '.')
    Invoke-Git $gpui @('commit', '-m', 'one')
    $old = (git -C $gpui rev-parse HEAD).Trim()
    Set-Content (Join-Path $gpui 'value') 'two'
    Invoke-Git $gpui @('commit', '-am', 'two')
    $new = (git -C $gpui rev-parse HEAD).Trim()
    Invoke-Git $gpui @('remote', 'add', 'origin', $gpui)
    Invoke-Git $gpui @('update-ref', 'refs/remotes/origin/main', $new)
    Invoke-Git $gpui @('checkout', '--detach', $old)
    Invoke-Git $root @('update-index', '--add', '--cacheinfo', "160000,$old,vendor/gpui-ce")
    foreach ($path in @('sdk/snapshot/approved-gpui.json','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json')) {
        $full = Join-Path $root $path
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $full) | Out-Null
        Set-Content $full 'old'
    }
    Invoke-Git $root @('add', '.')
    Invoke-Git $root @('commit', '-m', 'baseline')
    Invoke-Git $gpui @('checkout', '--detach', $new)
    Invoke-Git $root @('add', 'vendor/gpui-ce')
    Set-Content (Join-Path $root 'sdk/snapshot/approved-gpui.json') 'candidate'
    $patch = Join-Path $temp 'candidate.patch'
    & git -C $root diff --cached --binary HEAD "--output=$patch"
    if ($LASTEXITCODE -ne 0) { throw 'could not create cached candidate patch' }
    Invoke-Git $root @('reset', '--hard', 'HEAD')
    Invoke-Git $gpui @('checkout', '--detach', $old)
    $authority = New-GpuiSnapshotAuthorityV1 -RepositoryRoot $root -ExpectedOrigin $gpui -GpuiRepository $gpui -GateManifestPath (Join-Path $temp 'synthetic-gates.json') -CommandRunner { param($kind, $args) throw "unexpected command $kind" }
    Set-Content (Join-Path $temp 'synthetic-gates.json') '{}'
    $required = @('sdk/snapshot/approved-gpui.json','sdk/sdk-lock.json','sdk/bundle-manifest.json','sdk/ui-abi-fingerprint.json','vendor/gpui-ce')
    Invoke-GpuiSnapshotPromotionCore -Authority $authority -CandidateRevision $new -CandidateTree ((git -C $gpui rev-parse "$new`^{tree}").Trim()) -ExpectedOrigin $gpui -CandidatePatch $patch -RequiredChanged $required -Transition { Set-Content (Join-Path $root 'sdk/snapshot/approved-gpui.json') 'approved' } -DependencyRunner { foreach ($path in $required[1..3]) { Set-Content (Join-Path $root $path) 'generated'; Invoke-Git $root @('add', $path) } }
    if ((git -C $root rev-parse ':vendor/gpui-ce').Trim() -ne $new) { throw 'authority core failed to stage candidate gitlink' }
    $postCoreBaseline = (git -C $root rev-parse HEAD).Trim()
    $postCoreGpui = (git -C $gpui rev-parse HEAD).Trim()
    $remoteAdvanced = $false
    try {
        Invoke-GpuiPromotionFinalizeV1 -Authority $authority -Baseline $postCoreBaseline -GpuiHead $postCoreGpui -ExpectedRevision $new -VerifyRemote { param($expected) throw 'synthetic gate advanced origin/main' } -Commit {} -Push {}
    } catch { $remoteAdvanced = $_.Exception.Message -like '*advanced origin/main*' }
    if (-not $remoteAdvanced -or (git -C $root rev-parse HEAD).Trim() -ne $postCoreBaseline -or (git -C $gpui rev-parse HEAD).Trim() -ne $postCoreGpui -or @(git -C $gpui status --porcelain).Count) { throw 'post-gate remote advance did not reject and restore clean baselines' }
    $commitFailed = $false
    try {
        Invoke-GpuiPromotionFinalizeV1 -Authority $authority -Baseline $postCoreBaseline -GpuiHead $postCoreGpui -ExpectedRevision $new -VerifyRemote { param($expected) if($expected -ne $new){throw 'wrong expected revision'} } -Commit { throw 'synthetic post-core commit failure' } -Push {}
    } catch { $commitFailed = $_.Exception.Message -like '*post-core commit failure*' }
    if (-not $commitFailed -or (git -C $root rev-parse HEAD).Trim() -ne $postCoreBaseline -or (git -C $gpui rev-parse HEAD).Trim() -ne $postCoreGpui -or @(git -C $gpui status --porcelain).Count) { throw 'post-core failure did not restore root and GPUI clean baselines' }
    Write-Output 'gpui snapshot authority E2E passed'
} finally {
    if (Test-Path $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}
