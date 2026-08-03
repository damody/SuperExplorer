[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^gpui-sdk-v[0-9A-Za-z._-]+$')]
    [string]$ReleaseTag,
    [Parameter(Mandatory)]
    [ValidatePattern('^(?!\.\.?$)[A-Za-z0-9][A-Za-z0-9._-]*$')]
    [string]$RcId,
    [Parameter(Mandatory)]
    [string]$ProtectionProvider,
    [Parameter(Mandatory)]
    [string]$ProtectionPolicyId,
    [Parameter(Mandatory)]
    [string]$ProtectionRecord,
    [Parameter(Mandatory)]
    [string]$DetachedSignature,
    [Parameter(Mandatory)]
    [string]$Signer,
    [Parameter(Mandatory)]
    [string]$GpgKeyring,
    [Parameter(Mandatory)]
    [string]$GpgHome,
    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9A-Fa-f]{40}$')]
    [string]$GpgPrimaryFingerprint,
    [Parameter(Mandatory)]
    [string]$Provenance,
    [Parameter(Mandatory)]
    [string]$Builder,
    [Parameter(Mandatory)]
    [string]$PredicateType
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$gpui = Join-Path $repo 'vendor\gpui-ce'
$snapshot = Join-Path $repo 'sdk\snapshot\release-freeze.json'
$ledger = Join-Path $repo 'sdk\snapshot\release-ledger.json'
$approvedSnapshotPath = Join-Path $repo 'sdk\snapshot\approved-gpui.json'
$validator = Join-Path $repo 'sdk\tools\release-freeze-validator'
$bundleGenerator = Join-Path $repo 'sdk\tools\bundle-generator'
$policyPath = Join-Path $repo 'sdk\ci\release-policy.json'
$gateManifestPath = Join-Path $repo 'sdk\ci\gpui-update-gates.json'
if (Test-Path -LiteralPath $snapshot) {
    throw 'A release-freeze record already exists. Releases are immutable; cut a new checkout/RC instead of rewriting it.'
}
foreach ($path in @($ledger, $ProtectionRecord, $DetachedSignature, $GpgKeyring, $Provenance)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required release evidence is absent: $path" }
}
if (-not (Test-Path -LiteralPath $GpgHome -PathType Container)) { throw 'Trusted GPG home is absent.' }

function Invoke-Git([string[]]$Arguments) {
    $output = & git -C $gpui @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git $($Arguments -join ' ') failed: $output" }
    return ($output | Out-String).Trim()
}
function Get-Sha256([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function To-RepoRelative([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    $prefix = $repo.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Release evidence must remain under repository root: $Path" }
    return $full.Substring($prefix.Length).Replace('\', '/')
}
function Artifact([string]$Path) { return [ordered]@{ path = (To-RepoRelative $Path); sha256 = (Get-Sha256 $Path) } }
function ArtifactAtPath([string]$PublishedPath, [string]$ContentPath) { return [ordered]@{ path = (To-RepoRelative $PublishedPath); sha256 = (Get-Sha256 $ContentPath) } }
function Require-PolicySecret([string]$Name) {
    $value = [Environment]::GetEnvironmentVariable($Name, 'Process')
    if ([string]::IsNullOrWhiteSpace($value)) { throw "Protected release environment did not provide $Name" }
    return $value
}
function Invoke-VerifiedTagPrimary([string]$Tag, [string]$ExpectedPrimaryFingerprint) {
    # `git verify-tag --raw` forwards GnuPG's machine-readable --status-fd
    # records.  Parsing VALIDSIG binds a signing subkey to its policy-approved
    # primary fingerprint instead of merely trusting a keyring membership.
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $status = & git -C $gpui verify-tag --raw $Tag 2>&1; $exitCode = $LASTEXITCODE }
    finally { $ErrorActionPreference = $saved }
    if ($exitCode -ne 0) { throw "Protected tag signature verification failed: $($status -join "`n")" }
    return Get-GpgValidSigPrimaryFingerprintV1 -StatusLines @($status | ForEach-Object { [string]$_ }) -ExpectedPrimaryFingerprint $ExpectedPrimaryFingerprint -EvidenceName 'Protected GPUI tag'
}
function Assert-RemoteProtectedTag([string]$Tag, [string]$ExpectedTagObject, [string]$ExpectedRevision, [string]$ExpectedTree) {
    Assert-ExactRemoteProtectedTagV1 -Repository $gpui -Tag $Tag -ExpectedTagObject $ExpectedTagObject -ExpectedRevision $ExpectedRevision -ExpectedTree $ExpectedTree
}
function Assert-ProtectionRecord([string]$Path, [string]$Provider, [string]$PolicyId, [string]$Repository, [string]$Tag, [string]$TagObject, [string]$Revision, [string]$Tree) {
    try { $record = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json } catch { throw 'Protected tag record is not valid JSON.' }
    $required = @('schema_version', 'provider', 'policy_id', 'repository', 'tag_name', 'tag_object', 'object_revision', 'tree')
    if (@($record.PSObject.Properties.Name | Where-Object { $_ -notin $required }).Count -ne 0 -or @($required | Where-Object { $null -eq $record.$_ }).Count -ne 0) { throw 'Protected tag record has an unsupported schema.' }
    if ($record.schema_version -ne 1 -or $record.provider -ne $Provider -or $record.policy_id -ne $PolicyId -or $record.repository -ne $Repository -or $record.tag_name -ne $Tag -or $record.tag_object -ne $TagObject -or $record.object_revision -ne $Revision -or $record.tree -ne $Tree) {
        throw 'Protected tag record does not bind the authorized tag name, tag object, revision, and tree.'
    }
}

$policy = Get-Content -LiteralPath $policyPath -Raw | ConvertFrom-Json
Import-Module (Join-Path $PSScriptRoot 'release-freeze-support.psm1') -Force
if ($policy.schema_version -ne 1 -or $ProtectionPolicyId -ne $policy.policy_id -or $ProtectionProvider -ne $policy.protection.provider) { throw 'Release protection provider/policy does not match the versioned trust policy.' }
if ($Signer -ne (Require-PolicySecret $policy.signature.signer_env) -or $GpgPrimaryFingerprint.ToUpperInvariant() -ne (Require-PolicySecret $policy.signature.gpg_primary_fingerprint_env).ToUpperInvariant()) { throw 'Release signer does not match the protected trust policy.' }
if ($Builder -ne (Require-PolicySecret $policy.provenance.builder_env) -or $PredicateType -ne (Require-PolicySecret $policy.provenance.predicate_type_env)) { throw 'Release provenance identity does not match the protected trust policy.' }
if ((Get-Sha256 $GpgKeyring) -ne (Require-PolicySecret $policy.signature.gpg_keyring_sha256_env).ToLowerInvariant() -or (Get-Sha256 $ProtectionRecord) -ne (Require-PolicySecret $policy.protection.record_sha256_env).ToLowerInvariant()) { throw 'Release trust-anchor evidence hash does not match the protected trust policy.' }

if ((& git -C $repo status --porcelain) -or $LASTEXITCODE -ne 0) { throw 'Release freeze requires a clean superproject checkout.' }
$gpuiOrigin = Invoke-Git @('remote', 'get-url', 'origin')
if ($gpuiOrigin -ne 'https://github.com/damody/gpui-ce-explorer.git') { throw 'Release freeze requires the authorized GPUI origin.' }
$tagObject = Invoke-Git @('rev-parse', '--verify', "refs/tags/$ReleaseTag")
if ((Invoke-Git @('cat-file', '-t', $tagObject)) -ne 'tag') { throw 'Release tag must be an annotated tag.' }
# Fail closed: verify the tag using only the explicitly supplied trusted GPG home.
$savedGpgHome = [Environment]::GetEnvironmentVariable('GNUPGHOME', 'Process')
try {
    $env:GNUPGHOME = (Resolve-Path -LiteralPath $GpgHome).Path
    $tagPrimaryFingerprint = Invoke-VerifiedTagPrimary $ReleaseTag $GpgPrimaryFingerprint
} finally {
    [Environment]::SetEnvironmentVariable('GNUPGHOME', $savedGpgHome, 'Process')
}
$revision = Invoke-Git @('rev-parse', '--verify', "$ReleaseTag^{}")
$tree = Invoke-Git @('show', '-s', '--format=%T', $revision)
Assert-RemoteProtectedTag $ReleaseTag $tagObject $revision $tree
Assert-ProtectionRecord $ProtectionRecord $ProtectionProvider $ProtectionPolicyId $gpuiOrigin $ReleaseTag $tagObject $revision $tree

$lockPath = Join-Path $repo 'sdk\sdk-lock.json'
$manifestPath = Join-Path $repo 'sdk\bundle-manifest.json'
$fingerprintPath = Join-Path $repo 'sdk\ui-abi-fingerprint.json'
$releaseArtifactPaths = @($approvedSnapshotPath, $lockPath, $manifestPath, $fingerprintPath)
$releaseArtifactBaselines = @{}
foreach ($path in $releaseArtifactPaths) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required generated release input is absent: $path" }
    $releaseArtifactBaselines[$path] = [IO.File]::ReadAllBytes($path)
}
$freezePublished = $false
try {
    # The SDK lock embeds this snapshot and derives its bundle ID from it.  Freeze the
    # embedded source first, then regenerate all derived bundle artifacts before their
    # detached signature and release evidence are accepted.
    $approvedSnapshot = Get-Content -LiteralPath $approvedSnapshotPath -Raw | ConvertFrom-Json
    if ($approvedSnapshot.source.revision -ne $revision -or $approvedSnapshot.source.tree -ne $tree) {
        throw 'The approved GPUI snapshot is not the frozen annotated-tag revision/tree.'
    }
    if ($approvedSnapshot.approval.channel -ne 'development' -or $approvedSnapshot.approval.state -ne 'approved' -or $null -eq $approvedSnapshot.approval.proof -or $null -eq $approvedSnapshot.approval.gates) {
        throw 'Release freeze requires an approved development snapshot with a retained full-gate attestation.'
    }
    $gateManifest = Get-Content -LiteralPath $gateManifestPath -Raw | ConvertFrom-Json
    $gates = $approvedSnapshot.approval.gates
    $expectedGateIds = @($gateManifest.gates | Where-Object { $_.required -eq $true } | ForEach-Object { [string]$_.id })
    if ($gates.schema_version -ne 1 -or $gates.gate_manifest_sha256 -ne (Get-Sha256 $gateManifestPath) -or $gates.candidate_plan_digest -ne $approvedSnapshot.approval.proof.candidate_plan_digest -or $gates.workflow_run_id -ne $approvedSnapshot.approval.proof.workflow_run_id -or $gates.nonce -ne $approvedSnapshot.approval.proof.nonce -or @($gates.results).Count -ne $expectedGateIds.Count -or @($gates.results | Where-Object { $_.id -notin $expectedGateIds -or $_.exit_code -ne 0 }).Count -ne 0 -or @($gates.results.id | Select-Object -Unique).Count -ne $expectedGateIds.Count -or $gates.attestation_sha256 -ne (Get-CanonicalGateAttestationDigestV1 $gates)) {
        throw 'Approved snapshot full-gate attestation is not digest-bound to the required gate manifest and candidate identity.'
    }
    $approvedSnapshot.release_frozen = $true
    [IO.File]::WriteAllText($approvedSnapshotPath, (($approvedSnapshot | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
    Push-Location $bundleGenerator
    try { & cargo.exe run --release --locked --offline -- generate } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw 'Unable to regenerate the SDK bundle from the frozen approved snapshot.' }

    $lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
    if ($lock.gpui.revision -ne $revision -or $lock.gpui.tree -ne $tree -or
        $lock.gpui.approved_snapshot.release_frozen -ne $true -or
        $lock.gpui.approved_snapshot.source.revision -ne $revision -or
        $lock.gpui.approved_snapshot.source.tree -ne $tree) {
        throw 'The regenerated SDK lock does not embed the frozen annotated-tag source.'
    }
    $previousLedger = Get-Content -LiteralPath $ledger -Raw | ConvertFrom-Json
    if ($previousLedger.schema_version -ne 1 -or $null -eq $previousLedger.releases) { throw 'The canonical release ledger has an unsupported schema.' }
    if (@($previousLedger.releases | Where-Object { $_.rc_id -eq $RcId }).Count -ne 0) { throw 'RC ID is already immutable in the release ledger; a changed revision requires a new RC and bundle.' }
    if (@($previousLedger.releases | Where-Object { $_.bundle_id -eq $lock.bundle_id -and ($_.source.revision -ne $revision -or $_.source.tree -ne $tree) }).Count -ne 0) {
        throw 'Bundle ID is already immutable for a different frozen revision/tree; cut a new bundle.'
    }

    # Fail closed: detached bundle signature must verify the regenerated frozen bundle
    # against the supplied release keyring.
    $bundlePrimaryFingerprint = Invoke-GpgvVerifiedPrimaryV1 -Keyring $GpgKeyring -Signature $DetachedSignature -Data $manifestPath -ExpectedPrimaryFingerprint $GpgPrimaryFingerprint

    $metadata = [ordered]@{
    schema_version = 2
    release_frozen = $true
    evidence_mode = 'production'
    protected_tag = [ordered]@{ name = $ReleaseTag; repository = $gpuiOrigin; tag_object = $tagObject; object_revision = $revision; tree = $tree; signer_primary_fingerprint = $tagPrimaryFingerprint }
    source = [ordered]@{ revision = $revision; tree = $tree }
    rc_id = $RcId
    bundle_id = $lock.bundle_id
    release_input_digest = ('0' * 64)
    artifacts = [ordered]@{ sdk_lock = (Artifact $lockPath); bundle_manifest = (Artifact $manifestPath); ui_abi_fingerprint = (Artifact $fingerprintPath) }
    protection = $null
    signature = $null
    provenance = $null
    prior_release_ledger = $null
    }
    $stagedLedger = "$ledger.$([Guid]::NewGuid().ToString('N')).stage"
    $staged = "$snapshot.$([Guid]::NewGuid().ToString('N')).stage"
    $releaseDirectory = Join-Path $repo "sdk\releases\$RcId"
    $stagedReleaseDirectory = "$releaseDirectory.$([Guid]::NewGuid().ToString('N')).stage"
    try {
    if (Test-Path -LiteralPath $releaseDirectory) { throw 'immutable release evidence directory already exists for this RC' }
    New-Item -ItemType Directory -Path $stagedReleaseDirectory -ErrorAction Stop | Out-Null
    $stagedProtection = Join-Path $stagedReleaseDirectory 'protection.json'
    $stagedSignature = Join-Path $stagedReleaseDirectory 'bundle.sig'
    $stagedProvenance = Join-Path $stagedReleaseDirectory 'provenance.json'
    Copy-Item -LiteralPath $ProtectionRecord -Destination $stagedProtection -ErrorAction Stop
    Copy-Item -LiteralPath $DetachedSignature -Destination $stagedSignature -ErrorAction Stop
    Copy-Item -LiteralPath $Provenance -Destination $stagedProvenance -ErrorAction Stop
    $metadata.protection = [ordered]@{ provider = $ProtectionProvider; policy_id = $ProtectionPolicyId; record = (ArtifactAtPath (Join-Path $releaseDirectory 'protection.json') $stagedProtection) }
    $metadata.signature = [ordered]@{ verification = 'detached_gpg'; signer = $Signer; primary_fingerprint = $bundlePrimaryFingerprint; artifact = (ArtifactAtPath (Join-Path $releaseDirectory 'bundle.sig') $stagedSignature) }
    $metadata.provenance = [ordered]@{ builder = $Builder; predicate_type = $PredicateType; artifact = (ArtifactAtPath (Join-Path $releaseDirectory 'provenance.json') $stagedProvenance) }
    $finalLedger = [ordered]@{ schema_version = 1; releases = @($previousLedger.releases) + @([ordered]@{ rc_id = $RcId; bundle_id = $lock.bundle_id; source = [ordered]@{ revision = $revision; tree = $tree } }) }
    [IO.File]::WriteAllText($stagedLedger, (($finalLedger | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
    $metadata.prior_release_ledger = (ArtifactAtPath $ledger $stagedLedger)
    [IO.File]::WriteAllText($staged, (($metadata | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
    Push-Location $validator
    try {
        $digest = (& cargo.exe run --release --locked --offline -- digest --metadata $staged 2>&1 | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or $digest -notmatch '^[0-9a-f]{64}$') { throw "Unable to calculate release-input digest: $digest" }
    } finally { Pop-Location }
    $metadata.release_input_digest = $digest
    [IO.File]::WriteAllText($staged, (($metadata | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
    Push-Location $validator
    try { & cargo.exe run --release --locked --offline -- verify-staged --metadata $staged --ledger $stagedLedger --evidence-dir $stagedReleaseDirectory } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw 'Release-freeze validator rejected the staged production record.' }
    Import-Module (Join-Path $PSScriptRoot 'release-freeze-transaction.psm1') -Force
        Assert-RemoteProtectedTag $ReleaseTag $tagObject $revision $tree
        Publish-ReleaseFreezeTransaction -LedgerPath $ledger -StagedLedgerPath $stagedLedger -SnapshotPath $snapshot -StagedSnapshotPath $staged -EvidenceDirectory $releaseDirectory -StagedEvidenceDirectory $stagedReleaseDirectory -VerifyPublished {
        Push-Location $validator
        try { & cargo.exe run --release --locked --offline -- verify } finally { Pop-Location }
        if ($LASTEXITCODE -ne 0) { throw 'Release-freeze validator rejected the published production record.' }
        }
    } finally {
        foreach ($path in @($stagedLedger, $staged)) { if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force } }
        if (Test-Path -LiteralPath $stagedReleaseDirectory) { Remove-Item -LiteralPath $stagedReleaseDirectory -Recurse -Force }
    }
    $freezePublished = $true
} finally {
    if (-not $freezePublished) {
        foreach ($path in $releaseArtifactPaths) { [IO.File]::WriteAllBytes($path, [byte[]]$releaseArtifactBaselines[$path]) }
    }
}

Write-Output "Release freeze recorded for $ReleaseTag ($revision)."
