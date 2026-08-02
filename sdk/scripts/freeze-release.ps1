[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^gpui-sdk-v')]
    [string]$ReleaseTag,
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]*$')]
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
$validator = Join-Path $repo 'sdk\tools\release-freeze-validator'
$policyPath = Join-Path $repo 'sdk\ci\release-policy.json'
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

$policy = Get-Content -LiteralPath $policyPath -Raw | ConvertFrom-Json
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
    Invoke-Git @('verify-tag', $ReleaseTag) | Out-Null
    $primary = (& gpg.exe --homedir $env:GNUPGHOME --with-colons --fingerprint $GpgPrimaryFingerprint 2>$null | Where-Object { $_ -like 'fpr:*' } | ForEach-Object { ($_ -split ':')[9] } | Select-Object -First 1)
    if ($primary -ne $GpgPrimaryFingerprint.ToUpperInvariant()) { throw 'Trusted GPG home does not contain the required primary signer fingerprint.' }
} finally {
    [Environment]::SetEnvironmentVariable('GNUPGHOME', $savedGpgHome, 'Process')
}
$revision = Invoke-Git @('rev-parse', '--verify', "$ReleaseTag^{}")
$tree = Invoke-Git @('show', '-s', '--format=%T', $revision)

$lockPath = Join-Path $repo 'sdk\sdk-lock.json'
$manifestPath = Join-Path $repo 'sdk\bundle-manifest.json'
$fingerprintPath = Join-Path $repo 'sdk\ui-abi-fingerprint.json'
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
if ($lock.gpui.revision -ne $revision -or $lock.gpui.tree -ne $tree) { throw 'The generated SDK lock is not the frozen annotated-tag revision/tree.' }
$previousLedger = Get-Content -LiteralPath $ledger -Raw | ConvertFrom-Json
if ($previousLedger.schema_version -ne 1 -or $null -eq $previousLedger.releases) { throw 'The canonical release ledger has an unsupported schema.' }
if (@($previousLedger.releases | Where-Object { $_.rc_id -eq $RcId }).Count -ne 0) { throw 'RC ID is already immutable in the release ledger; a changed revision requires a new RC and bundle.' }

# Fail closed: detached bundle signature must verify against the supplied release keyring.
& gpgv --keyring $GpgKeyring $DetachedSignature $manifestPath
if ($LASTEXITCODE -ne 0) { throw 'Detached release-bundle signature verification failed.' }

$metadata = [ordered]@{
    schema_version = 2
    release_frozen = $true
    evidence_mode = 'production'
    protected_tag = [ordered]@{ name = $ReleaseTag; tag_object = $tagObject; object_revision = $revision; tree = $tree }
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
    $metadata.signature = [ordered]@{ verification = 'detached_gpg'; signer = $Signer; artifact = (ArtifactAtPath (Join-Path $releaseDirectory 'bundle.sig') $stagedSignature) }
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
    Publish-ReleaseFreezeTransaction -LedgerPath $ledger -StagedLedgerPath $stagedLedger -SnapshotPath $snapshot -StagedSnapshotPath $staged -EvidenceDirectory $releaseDirectory -StagedEvidenceDirectory $stagedReleaseDirectory -VerifyPublished {
        Push-Location $validator
        try { & cargo.exe run --release --locked --offline -- verify } finally { Pop-Location }
        if ($LASTEXITCODE -ne 0) { throw 'Release-freeze validator rejected the published production record.' }
    }
} finally {
    foreach ($path in @($stagedLedger, $staged)) { if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force } }
    if (Test-Path -LiteralPath $stagedReleaseDirectory) { Remove-Item -LiteralPath $stagedReleaseDirectory -Recurse -Force }
}

Write-Output "Release freeze recorded for $ReleaseTag ($revision)."
