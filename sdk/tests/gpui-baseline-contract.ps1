[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$SnapshotMetadataPath = Join-Path $RepositoryRoot 'sdk\snapshot\approved-gpui.json'

$expectedSource = 'https://github.com/damody/gpui-ce-explorer.git'
$submodulePath = Join-Path $RepositoryRoot 'vendor\gpui-ce'
$gitmodulesPath = Join-Path $RepositoryRoot '.gitmodules'
$snapshot = Get-Content -Raw -Encoding utf8 -LiteralPath $SnapshotMetadataPath | ConvertFrom-Json
Import-Module (Join-Path $PSScriptRoot 'gpui-contract-test-support.psm1') -Force

if ($snapshot.schema_version -ne 1 -or $snapshot.approval.channel -ne 'development' -or $snapshot.approval.state -ne 'approved') {
    throw 'GPUI snapshot metadata is not an approved schema-v1 development snapshot.'
}
if ($snapshot.source.repository -ne $expectedSource -or $snapshot.source.update_branch -ne 'main' -or
    $snapshot.source.resolved_ref -ne 'refs/remotes/origin/main') {
    throw 'GPUI snapshot metadata does not use the authorized repository/main update channel.'
}
if ($snapshot.production.default_features -ne $false -or @($snapshot.production.features).Count -ne 0) {
    throw 'GPUI snapshot metadata must declare an empty production feature set.'
}

$gitmoduleUrl = (& git config -f $gitmodulesPath --get submodule.gpui-ce.url 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $gitmoduleUrl -ne $expectedSource) {
    throw "GPUI source authority mismatch in .gitmodules; expected '$expectedSource', got '$gitmoduleUrl'."
}

$origin = (& git -C $submodulePath remote get-url origin 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "Unable to read the GPUI origin: $origin" }
if ($origin -ne $expectedSource) {
    throw "GPUI origin mismatch: expected '$expectedSource', got '$origin'."
}

$workingRev = (& git -C $submodulePath rev-parse HEAD 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "Unable to resolve the GPUI checkout: $workingRev" }
$gitlinkRev = (& git -C $RepositoryRoot rev-parse 'HEAD:vendor/gpui-ce' 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "Unable to resolve the parent GPUI gitlink: $gitlinkRev" }
if ($workingRev -ne $gitlinkRev) {
    throw "GPUI checkout does not match the parent gitlink: expected '$gitlinkRev', got '$workingRev'."
}
if ($workingRev -ne $snapshot.source.revision) {
    throw "GPUI checkout does not match approved snapshot revision '$($snapshot.source.revision)'."
}
if ($workingRev -notmatch '^[0-9a-f]{40}$') {
    throw "GPUI revision is not a full 40-character commit: '$workingRev'."
}

$trackedMain = (& git -C $submodulePath rev-parse $snapshot.source.resolved_ref 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $trackedMain -notmatch '^[0-9a-f]{40}$') {
    throw "Unable to resolve tracked GPUI main ref '$($snapshot.source.resolved_ref)'."
}
& git -C $submodulePath merge-base --is-ancestor $workingRev $trackedMain
if ($LASTEXITCODE -ne 0) {
    throw "Approved GPUI revision '$workingRev' is not an ancestor of tracked origin/main '$trackedMain'."
}

$treeRevision = "$workingRev`^{tree}"
$tree = (& git -C $submodulePath rev-parse $treeRevision 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "Unable to resolve the GPUI tree: $tree" }
if ($tree -notmatch '^[0-9a-f]{40}$') {
    throw "GPUI tree is not a full 40-character object ID: '$tree'."
}
if ($tree -ne $snapshot.source.tree) {
    throw "GPUI tree does not match approved snapshot tree '$($snapshot.source.tree)'."
}

$parents = (& git -C $submodulePath show -s --format='%P' $workingRev 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $parents -ne $snapshot.source.parent) {
    throw "GPUI parent metadata mismatch: expected '$($snapshot.source.parent)', got '$parents'."
}
$commitTime = (& git -C $submodulePath show -s --format='%cI' $workingRev 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $commitTime -ne $snapshot.source.commit_time) {
    throw "GPUI commit-time metadata mismatch: expected '$($snapshot.source.commit_time)', got '$commitTime'."
}
$gpuiManifest = Get-Content -Raw -Encoding utf8 -LiteralPath (Join-Path $submodulePath 'crates\gpui\Cargo.toml')
$packageVersion = [regex]::Match($gpuiManifest, '(?m)^version\s*=\s*"([^"]+)"\s*$').Groups[1].Value
if ($snapshot.source.package -ne 'gpui' -or $packageVersion -ne $snapshot.source.package_version) {
    throw "GPUI package metadata mismatch: expected gpui@$($snapshot.source.package_version), got gpui@$packageVersion."
}

Push-Location $RepositoryRoot
try {
    $metadataOutput = (& cargo metadata --manifest-path (Join-Path $RepositoryRoot 'Cargo.toml') --locked --filter-platform x86_64-pc-windows-msvc --format-version 1 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed while validating the production GPUI graph: $metadataOutput"
    }
    $featureTreeOutput = (& cargo tree --manifest-path (Join-Path $RepositoryRoot 'Cargo.toml') --locked --target x86_64-pc-windows-msvc -p explorer-app -e normal,build --prefix none --no-dedupe --format '{p}|{f}' 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo tree failed while validating production-only GPUI features: $featureTreeOutput"
    }
} finally {
    Pop-Location
}
$metadata = $metadataOutput | ConvertFrom-Json
$approvedManifest = Join-Path $submodulePath 'crates\gpui\Cargo.toml'
$null = Assert-ApprovedGpuiMetadata -Metadata $metadata -ExpectedVersion $snapshot.source.package_version -ExpectedManifestPath $approvedManifest
$resolvedFeatures = Get-GpuiProductionFeatures -CargoTreeOutput $featureTreeOutput -ExpectedVersion $snapshot.source.package_version
Assert-ExactGpuiFeatureSet -Actual @($resolvedFeatures) -Expected @($snapshot.production.features)

[pscustomobject]@{
    Source = $origin
    Revision = $workingRev
    Tree = $tree
    ProductionFeatures = @()
    Status = 'ok'
}
