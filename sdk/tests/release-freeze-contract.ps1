$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$tool = Join-Path $repo 'sdk\tools\release-freeze-validator'
$canonicalFreeze = Join-Path $repo 'sdk\snapshot\release-freeze.json'
if (Test-Path -LiteralPath $canonicalFreeze) {
    throw 'The development checkout must not contain a canonical release-freeze record.'
}
Import-Module (Join-Path $sdkRoot 'scripts\release-freeze-transaction.psm1') -Force
Import-Module (Join-Path $sdkRoot 'scripts\release-freeze-support.psm1') -Force
$fixturePrimary = (('A' * 40) -join ''); $fixtureSubkey = (('B' * 40) -join ''); $otherPrimary = (('C' * 40) -join '')
if ((Get-GpgValidSigPrimaryFingerprintV1 -StatusLines @("[GNUPG:] VALIDSIG $fixturePrimary 0 0 0 4 0 1 10 00") -ExpectedPrimaryFingerprint $fixturePrimary -EvidenceName 'primary-direct fixture') -ne $fixturePrimary) { throw 'primary-direct VALIDSIG fixture did not resolve the signing primary fingerprint' }
if ((Get-GpgValidSigPrimaryFingerprintV1 -StatusLines @("[GNUPG:] VALIDSIG $fixtureSubkey 0 0 0 4 0 1 10 00 $fixturePrimary") -ExpectedPrimaryFingerprint $fixturePrimary -EvidenceName 'subkey fixture') -ne $fixturePrimary) { throw 'subkey VALIDSIG fixture did not resolve the primary fingerprint' }
$twoKeyRejected = $false
try { Get-GpgValidSigPrimaryFingerprintV1 -StatusLines @("[GNUPG:] VALIDSIG $otherPrimary 0 0 0 4 0 1 10 00") -ExpectedPrimaryFingerprint $fixturePrimary -EvidenceName 'two-key negative fixture' | Out-Null } catch { $twoKeyRejected = $true }
if (-not $twoKeyRejected) { throw 'two-key negative VALIDSIG fixture accepted an untrusted primary key' }
$remoteTagFixture = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-release-remote-tag-' + [guid]::NewGuid().ToString('N'))
try {
    $remoteBare = Join-Path $remoteTagFixture 'remote.git'; $clone = Join-Path $remoteTagFixture 'clone'
    New-Item -ItemType Directory -Path $remoteTagFixture -Force | Out-Null
    & git init --bare --quiet $remoteBare; if ($LASTEXITCODE -ne 0) { throw 'remote tag fixture could not initialize bare origin' }
    & git clone --quiet $remoteBare $clone; if ($LASTEXITCODE -ne 0) { throw 'remote tag fixture could not clone origin' }
    & git -C $clone config user.email 'fixture@example.invalid'; & git -C $clone config user.name 'Remote Tag Fixture'
    [IO.File]::WriteAllText((Join-Path $clone 'source.txt'), "one`n", [Text.UTF8Encoding]::new($false)); & git -C $clone add source.txt; & git -C $clone commit --quiet -m one; & git -C $clone tag -a gpui-sdk-v0.0.1 -m one; & git -C $clone push --quiet origin HEAD refs/tags/gpui-sdk-v0.0.1
    $oldTagObject = (& git -C $clone rev-parse refs/tags/gpui-sdk-v0.0.1).Trim(); $oldRevision = (& git -C $clone rev-parse 'gpui-sdk-v0.0.1^{}').Trim(); $oldTree = (& git -C $clone show -s --format=%T $oldRevision).Trim()
    Assert-ExactRemoteProtectedTagV1 -Repository $clone -Tag 'gpui-sdk-v0.0.1' -ExpectedTagObject $oldTagObject -ExpectedRevision $oldRevision -ExpectedTree $oldTree
    [IO.File]::WriteAllText((Join-Path $clone 'source.txt'), "two`n", [Text.UTF8Encoding]::new($false)); & git -C $clone add source.txt; & git -C $clone commit --quiet -m two; & git -C $clone tag -fa gpui-sdk-v0.0.1 -m two; & git -C $clone push --quiet --force origin refs/tags/gpui-sdk-v0.0.1
    $remoteMoveRejected = $false
    try { Assert-ExactRemoteProtectedTagV1 -Repository $clone -Tag 'gpui-sdk-v0.0.1' -ExpectedTagObject $oldTagObject -ExpectedRevision $oldRevision -ExpectedTree $oldTree } catch { $remoteMoveRejected = $true }
    if (-not $remoteMoveRejected) { throw 'remote protected tag fixture accepted a moved tag/revision after publication preflight' }
} finally {
    if (Test-Path -LiteralPath $remoteTagFixture) { Remove-Item -LiteralPath $remoteTagFixture -Recurse -Force }
}
$policy = Get-Content -LiteralPath (Join-Path $sdkRoot 'ci\release-policy.json') -Raw | ConvertFrom-Json
if ($policy.schema_version -ne 1 -or $policy.policy_id -ne 'sdk-release-freeze-v1' -or $policy.protection.provider -ne 'github-environment') {
    throw 'versioned release trust policy is incomplete'
}
$freezeSchema = Get-Content -LiteralPath (Join-Path $sdkRoot 'schemas\release-freeze.schema.json') -Raw | ConvertFrom-Json
$ledgerSchema = Get-Content -LiteralPath (Join-Path $sdkRoot 'schemas\release-ledger.schema.json') -Raw | ConvertFrom-Json
$protectionSchema = Get-Content -LiteralPath (Join-Path $sdkRoot 'schemas\release-protection-record.schema.json') -Raw | ConvertFrom-Json
if ($freezeSchema.properties.release_frozen.const -ne $true -or
    $freezeSchema.properties.rc_id.pattern -notmatch 'A-Za-z0-9' -or
    $freezeSchema.'$defs'.protected_tag.properties.name.pattern -ne '^gpui-sdk-v[0-9A-Za-z._-]+$' -or
    $ledgerSchema.properties.releases.items.properties.rc_id.pattern -notmatch 'A-Za-z0-9') {
    throw 'release schemas do not constrain immutable frozen release identifiers'
}
if ($freezeSchema.'$defs'.protected_tag.required -notcontains 'repository' -or $freezeSchema.'$defs'.protected_tag.required -notcontains 'signer_primary_fingerprint' -or $freezeSchema.'$defs'.signature.required -notcontains 'primary_fingerprint' -or $protectionSchema.required -notcontains 'tag_object' -or $protectionSchema.required -notcontains 'object_revision') {
    throw 'release schemas do not bind signatures and protection evidence to the exact protected tag identity'
}
$releaseWorkflow = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\freeze-gpui-release.yml') -Raw
foreach ($requiredWorkflowControl in @('environment: sdk-release-freeze', 'fetch-depth: 0', 'fetch-tags: true', '--batch --import', 'Invoke-OfflineSdkGuest.template.ps1', "load.mode -ne 'compatible'", 'RELEASE_TAG:', 'RELEASE_BASE_SHA', 'refs/heads/main', '--force-with-lease')) {
    if (-not $releaseWorkflow.Contains($requiredWorkflowControl)) { throw "protected release workflow is missing control: $requiredWorkflowControl" }
}
foreach ($requiredFrozenGuestControl in @('release-frozen-offline-attestation.json', 'frozen release offline guest gate failed', 'proof.bundle_sha256 -ne $manifestHash', 'frozenSnapshot.release_frozen', 'lock.gpui.approved_snapshot.release_frozen')) {
    if (-not $releaseWorkflow.Contains($requiredFrozenGuestControl)) { throw "protected release workflow is missing frozen guest binding: $requiredFrozenGuestControl" }
}
$freezeStep = $releaseWorkflow.IndexOf('name: freeze-release-under-protected-policy')
$frozenGuestStep = $releaseWorkflow.IndexOf('name: Rebuild frozen SDK fixtures in an isolated guest')
$commitStep = $releaseWorkflow.IndexOf('name: Commit immutable freeze record')
if ($freezeStep -lt 0 -or $frozenGuestStep -le $freezeStep -or $commitStep -le $frozenGuestStep) {
    throw 'release workflow must rebuild the frozen bundle after freeze and before commit'
}
if ($releaseWorkflow.Contains('GPG_HOME_B64')) { throw 'protected release workflow uses a misleading GPG home secret' }
$freezeSource = Get-Content -LiteralPath (Join-Path $sdkRoot 'scripts\freeze-release.ps1') -Raw
$freezeSupportSource = Get-Content -LiteralPath (Join-Path $sdkRoot 'scripts\release-freeze-support.psm1') -Raw
if (-not $freezeSource.Contains('& git -C $gpui @Arguments') -or -not $freezeSource.Contains("git -C `$repo status --porcelain") -or -not $freezeSource.Contains("remote', 'get-url', 'origin'")) {
    throw 'freeze script must resolve protected tags in the authorized GPUI repository while checking superproject cleanliness separately'
}
foreach ($requiredFreezeControl in @('approvedSnapshot.release_frozen = $true', "approval.channel -ne 'development'", "approval.state -ne 'approved'", 'Get-CanonicalGateAttestationDigestV1', 'Assert-RemoteProtectedTag', 'verify-tag --raw', 'Invoke-GpgvVerifiedPrimaryV1', 'Assert-ProtectionRecord', 'cargo.exe run --release --locked --offline -- generate', 'approved_snapshot.release_frozen -ne $true', 'Bundle ID is already immutable for a different frozen revision/tree', '[IO.File]::WriteAllBytes($path, [byte[]]$releaseArtifactBaselines[$path])')) {
    if (-not $freezeSource.Contains($requiredFreezeControl)) { throw "freeze script is missing frozen-bundle transaction control: $requiredFreezeControl" }
}
foreach ($requiredSupportControl in @('VALIDSIG', 'fields[2]', '--status-fd 1', 'ls-remote --tags origin', 'Assert-ExactRemoteProtectedTagV1')) {
    if (-not $freezeSupportSource.Contains($requiredSupportControl)) { throw "release-freeze support module is missing signature/remote authority control: $requiredSupportControl" }
}
if ([regex]::Matches($freezeSource, 'Assert-RemoteProtectedTag \$ReleaseTag \$tagObject \$revision \$tree').Count -lt 2) {
    throw 'freeze production callgraph must revalidate the exact remote protected tag before both freeze and publication'
}

function Write-Json([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path, (($Value | ConvertTo-Json -Depth 30) + "`n"), [Text.UTF8Encoding]::new($false))
}
function Read-Json([string]$Path) { return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json }
function Get-Sha256([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Get-Relative([string]$Root, [string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Not contained in fixture root: $Path" }
    return $full.Substring($prefix.Length).Replace('\', '/')
}
function Artifact([string]$Root, [string]$Path) {
    return [ordered]@{ path = (Get-Relative $Root $Path); sha256 = (Get-Sha256 $Path) }
}
function Invoke-Tool([string[]]$Arguments, [bool]$ShouldPass, [string]$Case) {
    Push-Location $tool
    try {
        $saved = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $output = (& cargo.exe run --release --locked --offline -- @Arguments 2>$null | Out-String).Trim()
        $exitCode = $LASTEXITCODE
        $ErrorActionPreference = $saved
    } finally { Pop-Location }
    if ($ShouldPass -and $exitCode -ne 0) { throw "$Case unexpectedly failed: $output" }
    if (-not $ShouldPass -and $exitCode -eq 0) { throw "$Case unexpectedly passed" }
    return $output
}
function Set-InputDigest([string]$MetadataPath) {
    $output = Invoke-Tool @('digest', '--metadata', $MetadataPath) $true 'digest calculation'
    $digest = ($output -split "`r?`n" | Where-Object { $_ -match '^[0-9a-f]{64}$' } | Select-Object -Last 1)
    if ($null -eq $digest) { throw "Release digest command did not emit a SHA-256: $output" }
    $metadata = Read-Json $MetadataPath
    $metadata.release_input_digest = $digest
    Write-Json $MetadataPath $metadata
}
function Invoke-FixtureValidator([string]$Fixture, [bool]$ShouldPass, [string]$Case) {
    Invoke-Tool @('verify-fixture', '--root', $Fixture) $ShouldPass $Case | Out-Null
}
function Invoke-OfflineFixtureBuild([string]$ManifestPath, [string]$CargoHome, [string]$TargetDir) {
    $oldHome = $env:CARGO_HOME; $oldTarget = $env:CARGO_TARGET_DIR
    try {
        $env:CARGO_HOME = $CargoHome
        $env:CARGO_TARGET_DIR = $TargetDir
        & cargo.exe build --manifest-path $ManifestPath --locked --offline
        if ($LASTEXITCODE -ne 0) { throw "offline fixture rebuild failed: $ManifestPath" }
    } finally {
        if ($null -eq $oldHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $oldHome }
        if ($null -eq $oldTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $oldTarget }
    }
}

$transactionFixture = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-release-transaction-' + [guid]::NewGuid().ToString('N'))
try {
    New-Item -ItemType Directory -Path $transactionFixture -Force | Out-Null
    $transactionLedger = Join-Path $transactionFixture 'release-ledger.json'
    $transactionLedgerStage = Join-Path $transactionFixture 'release-ledger.stage'
    $transactionSnapshot = Join-Path $transactionFixture 'release-freeze.json'
    $transactionSnapshotStage = Join-Path $transactionFixture 'release-freeze.stage'
    $transactionEvidence = Join-Path $transactionFixture 'evidence'
    $transactionEvidenceStage = Join-Path $transactionFixture 'evidence.stage'
    [IO.File]::WriteAllText($transactionLedger, "old-ledger`n", [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($transactionLedgerStage, "new-ledger`n", [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($transactionSnapshotStage, "new-snapshot`n", [Text.UTF8Encoding]::new($false))
    New-Item -ItemType Directory -Path $transactionEvidenceStage -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $transactionEvidenceStage 'protection.json'), "new-evidence`n", [Text.UTF8Encoding]::new($false))
    $rolledBack = $false
    try { Publish-ReleaseFreezeTransaction -LedgerPath $transactionLedger -StagedLedgerPath $transactionLedgerStage -SnapshotPath $transactionSnapshot -StagedSnapshotPath $transactionSnapshotStage -EvidenceDirectory $transactionEvidence -StagedEvidenceDirectory $transactionEvidenceStage -VerifyPublished { throw 'injected post-publish validation failure' } } catch { $rolledBack = $true }
    if (-not $rolledBack -or [IO.File]::ReadAllText($transactionLedger) -ne "old-ledger`n" -or (Test-Path -LiteralPath $transactionSnapshot) -or (Test-Path -LiteralPath $transactionEvidence)) {
        throw 'release-freeze transaction did not roll back a partial publication'
    }
} finally {
    if (Test-Path -LiteralPath $transactionFixture) { Remove-Item -LiteralPath $transactionFixture -Recurse -Force }
}

$fixture = Join-Path ([IO.Path]::GetTempPath()) ("superexplorer-release-freeze-contract-" + [Guid]::NewGuid().ToString('N'))
try {
    New-Item -ItemType Directory -Path $fixture -Force | Out-Null
    foreach ($relative in @('sdk', 'sdk\snapshot', 'sdk\ci', 'evidence')) {
        New-Item -ItemType Directory -Path (Join-Path $fixture $relative) -Force | Out-Null
    }
    & git -C $fixture init --quiet
    & git -C $fixture config user.email 'fixture@example.invalid'
    & git -C $fixture config user.name 'Release Fixture'
    [IO.File]::WriteAllText((Join-Path $fixture 'tracked.txt'), "fixture`n", [Text.UTF8Encoding]::new($false))
    & git -C $fixture add tracked.txt
    & git -C $fixture commit --quiet -m fixture
    $revision = (& git -C $fixture rev-parse HEAD).Trim()
    $tree = (& git -C $fixture show -s --format=%T HEAD).Trim()
    & git -C $fixture tag -a gpui-sdk-v0.1.0-rc.1 -m fixture-tag
    $tagObject = (& git -C $fixture rev-parse refs/tags/gpui-sdk-v0.1.0-rc.1).Trim()

    $lockPath = Join-Path $fixture 'sdk\sdk-lock.json'
    $manifestPath = Join-Path $fixture 'sdk\bundle-manifest.json'
    $fingerprintPath = Join-Path $fixture 'sdk\ui-abi-fingerprint.json'
    $ledgerPath = Join-Path $fixture 'sdk\snapshot\release-ledger.json'
    $gateManifestPath = Join-Path $fixture 'sdk\ci\gpui-update-gates.json'
    $protectionPath = Join-Path $fixture 'evidence\protection.json'
    $signaturePath = Join-Path $fixture 'evidence\signature.json'
    $provenancePath = Join-Path $fixture 'evidence\provenance.json'
    $metadataPath = Join-Path $fixture 'sdk\snapshot\release-freeze.json'
    Copy-Item -LiteralPath (Join-Path $sdkRoot 'ci\gpui-update-gates.json') -Destination $gateManifestPath -Force
    $fixtureGateManifest = Get-Content -LiteralPath $gateManifestPath -Raw | ConvertFrom-Json
    $fixturePlanDigest = (('D' * 64) -join '').ToLowerInvariant(); $fixtureNonce = (('1' * 32) -join '')
    $fixtureProof = [ordered]@{ baseline_revision = $revision; old_revision = $revision; new_revision = $revision; new_tree = $tree; candidate_plan_digest = $fixturePlanDigest; workflow_run_id = 'fixture-run'; nonce = $fixtureNonce }
    $fixtureGates = [ordered]@{ schema_version = 1; gate_manifest_sha256 = Get-Sha256 $gateManifestPath; candidate_plan_digest = $fixturePlanDigest; workflow_run_id = 'fixture-run'; nonce = $fixtureNonce; results = @($fixtureGateManifest.gates | Where-Object { $_.required -eq $true } | ForEach-Object { [ordered]@{ id = [string]$_.id; exit_code = 0 } }); attestation_sha256 = ('0' * 64) }
    $fixtureGates.attestation_sha256 = Get-CanonicalGateAttestationDigestV1 $fixtureGates
    $lock = [ordered]@{
        bundle_id = 'fixture-bundle'
        toolchain = [ordered]@{ rustc_release = '1.97.1'; rustc_commit_hash = 'a'; cargo_release = '1.97.1'; cargo_commit_hash = 'b'; target = 'x86_64-pc-windows-msvc' }
        gpui = [ordered]@{ revision = $revision; tree = $tree; approved_snapshot = [ordered]@{ source = [ordered]@{ revision = $revision; tree = $tree }; approval = [ordered]@{ channel = 'development'; state = 'approved'; proof = $fixtureProof; gates = $fixtureGates }; candidate_plan_digest = $fixturePlanDigest; production = [ordered]@{ features = @() }; release_frozen = $true } }
        protected_dependency_graph = @()
        protected_dependency_contract = [ordered]@{ schema_version = 2; edge_digest = 'x' }
        sdk_public_source_hashes = @()
        release_profiles = [ordered]@{}
        build_policy = [ordered]@{ profile = [ordered]@{ panic = 'unwind'; lto = 'thin'; codegen_units = 1 }; allocator = [ordered]@{}; crt = [ordered]@{}; rustflags = @(); abi_schema_version = 1 }
    }
    Write-Json $lockPath $lock
    $fingerprintOutput = Invoke-Tool @('ui-fingerprint', '--lock', $lockPath) $true 'offline UI ABI fingerprint'
    $fingerprint = ($fingerprintOutput -split "`r?`n" | Where-Object { $_ -match '^[0-9a-f]{64}$' } | Select-Object -Last 1)
    if ($null -eq $fingerprint) { throw 'UI ABI fingerprint command did not return a SHA-256.' }
    Write-Json $manifestPath ([ordered]@{ bundle_id = 'fixture-bundle'; files = @() })
    Write-Json $fingerprintPath ([ordered]@{ bundle_id = 'fixture-bundle'; fingerprint = $fingerprint })
    Write-Json $ledgerPath ([ordered]@{ schema_version = 1; releases = @() })
    Write-Json $protectionPath ([ordered]@{ schema_version = 1; provider = 'fixture'; policy_id = 'fixture-policy'; repository = 'https://fixture.invalid/gpui.git'; tag_name = 'gpui-sdk-v0.1.0-rc.1'; tag_object = $tagObject; object_revision = $revision; tree = $tree })
    Write-Json $signaturePath ([ordered]@{ fixture_unsigned = $true })
    Write-Json $provenancePath ([ordered]@{ builder = 'fixture' })
    $metadata = [ordered]@{
        schema_version = 2
        release_frozen = $true
        evidence_mode = 'fixture'
        protected_tag = [ordered]@{ name = 'gpui-sdk-v0.1.0-rc.1'; repository = 'https://github.com/damody/gpui-ce-explorer.git'; tag_object = $tagObject; object_revision = $revision; tree = $tree; signer_primary_fingerprint = $fixturePrimary }
        source = [ordered]@{ revision = $revision; tree = $tree }
        rc_id = '0.1.0-rc.1'
        bundle_id = 'fixture-bundle'
        release_input_digest = ('0' * 64)
        artifacts = [ordered]@{ sdk_lock = (Artifact $fixture $lockPath); bundle_manifest = (Artifact $fixture $manifestPath); ui_abi_fingerprint = (Artifact $fixture $fingerprintPath) }
        protection = [ordered]@{ provider = 'fixture'; policy_id = 'fixture-policy'; record = (Artifact $fixture $protectionPath) }
        signature = [ordered]@{ verification = 'fixture_unsigned'; signer = 'fixture'; primary_fingerprint = $fixturePrimary; artifact = (Artifact $fixture $signaturePath) }
        provenance = [ordered]@{ builder = 'fixture'; predicate_type = 'fixture'; artifact = (Artifact $fixture $provenancePath) }
        prior_release_ledger = (Artifact $fixture $ledgerPath)
    }
    Write-Json $metadataPath $metadata
    Set-InputDigest $metadataPath
    $metadataBackup = [IO.File]::ReadAllBytes($metadataPath)
    $lockBackup = [IO.File]::ReadAllBytes($lockPath)
    $ledgerBackup = [IO.File]::ReadAllBytes($ledgerPath)
    $protectionBackup = [IO.File]::ReadAllBytes($protectionPath)

    Invoke-FixtureValidator $fixture $true 'valid annotated fixture release'
    Invoke-Tool @('verify', '--root', $fixture) $false 'production CLI must not accept a fixture root'

    $lock = Read-Json $lockPath
    $lock.gpui.approved_snapshot.approval.state = 'candidate'
    Write-Json $lockPath $lock
    $metadata = Read-Json $metadataPath; $metadata.artifacts.sdk_lock.sha256 = Get-Sha256 $lockPath; Write-Json $metadataPath $metadata; Set-InputDigest $metadataPath
    Invoke-FixtureValidator $fixture $false 'candidate approval state cannot become a release freeze'
    [IO.File]::WriteAllBytes($lockPath, $lockBackup); [IO.File]::WriteAllBytes($metadataPath, $metadataBackup)

    $lock = Read-Json $lockPath
    $lock.gpui.approved_snapshot.approval.gates.attestation_sha256 = (('0' * 64) -join '')
    Write-Json $lockPath $lock
    $metadata = Read-Json $metadataPath; $metadata.artifacts.sdk_lock.sha256 = Get-Sha256 $lockPath; Write-Json $metadataPath $metadata; Set-InputDigest $metadataPath
    Invoke-FixtureValidator $fixture $false 'tampered full-gate attestation digest'
    [IO.File]::WriteAllBytes($lockPath, $lockBackup); [IO.File]::WriteAllBytes($metadataPath, $metadataBackup)

    $metadata = Read-Json $metadataPath
    $metadata.signature.verification = 'detached_gpg'
    Write-Json $metadataPath $metadata
    Set-InputDigest $metadataPath
    Invoke-FixtureValidator $fixture $false 'fixture mode cannot claim production signature evidence'
    [IO.File]::WriteAllBytes($metadataPath, $metadataBackup)

    $metadata = Read-Json $metadataPath
    $metadata | Add-Member -NotePropertyName untrusted -NotePropertyValue $true
    Write-Json $metadataPath $metadata
    Invoke-Tool @('digest', '--metadata', $metadataPath) $false 'unknown metadata property'
    [IO.File]::WriteAllBytes($metadataPath, $metadataBackup)

    $metadata = Read-Json $metadataPath
    $metadata.protected_tag.tag_object = ('0' * 40)
    Write-Json $metadataPath $metadata
    Set-InputDigest $metadataPath
    Invoke-FixtureValidator $fixture $false 'annotated tag object mismatch'
    [IO.File]::WriteAllBytes($metadataPath, $metadataBackup)

    [IO.File]::WriteAllText($protectionPath, '{"policy":"tampered"}' + "`n", [Text.UTF8Encoding]::new($false))
    Invoke-FixtureValidator $fixture $false 'referenced evidence hash mismatch'
    [IO.File]::WriteAllBytes($protectionPath, $protectionBackup)

    $ledger = Read-Json $ledgerPath
    $ledger.releases = @([ordered]@{
        rc_id = '0.1.0-rc.1'; bundle_id = 'regenerated-bundle'
        source = [ordered]@{ revision = ('4' * 40); tree = $tree }
    })
    Write-Json $ledgerPath $ledger
    $metadata = Read-Json $metadataPath
    $metadata.prior_release_ledger.sha256 = Get-Sha256 $ledgerPath
    Write-Json $metadataPath $metadata
    Set-InputDigest $metadataPath
    Invoke-FixtureValidator $fixture $false 'immutable ledger rejects RC reuse after regenerated inputs'
    [IO.File]::WriteAllBytes($metadataPath, $metadataBackup)
    [IO.File]::WriteAllBytes($ledgerPath, $ledgerBackup)

    $freezeScript = Join-Path $repo 'sdk\scripts\freeze-release.ps1'
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $gpgHome = Join-Path $fixture 'trusted-gpg-home'; New-Item -ItemType Directory -Path $gpgHome -Force | Out-Null
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $freezeScript -ReleaseTag 'gpui-sdk-v0.1.0-rc.1' -RcId '0.1.0-rc.1' -ProtectionProvider fixture -ProtectionPolicyId fixture -ProtectionRecord $protectionPath -DetachedSignature $signaturePath -Signer fixture -GpgKeyring $signaturePath -GpgHome $gpgHome -GpgPrimaryFingerprint $fixturePrimary -Provenance $provenancePath -Builder fixture -PredicateType fixture 2>$null
    $freezeExit = $LASTEXITCODE
    $ErrorActionPreference = $saved
    if ($freezeExit -eq 0) { throw 'production freeze script accepted unsigned fixture evidence' }
    if (Test-Path -LiteralPath $canonicalFreeze) { throw 'failed production freeze attempt created a canonical record' }

    # A frozen tag remains rebuildable after the synthetic remote main advances.
    $remote = Join-Path $fixture 'synthetic-remote'
    New-Item -ItemType Directory -Path $remote -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $remote 'sdk\fixtures') -Force | Out-Null
    foreach ($fixtureName in @('abi-contract', 'abi-root-host', 'abi-root-plugin')) {
        $sourceFixture = Join-Path $sdkRoot "fixtures\$fixtureName"
        $destinationFixture = Join-Path $remote "sdk\fixtures\$fixtureName"
        New-Item -ItemType Directory -Path $destinationFixture -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $sourceFixture 'Cargo.toml') -Destination $destinationFixture -Force
        Copy-Item -LiteralPath (Join-Path $sourceFixture 'Cargo.lock') -Destination $destinationFixture -Force
        Copy-Item -LiteralPath (Join-Path $sourceFixture 'src') -Destination $destinationFixture -Recurse -Force
    }
    $frozenVendor = Join-Path $remote 'sdk\vendor\cargo-sources'
    $remoteCargoConfig = Join-Path $remote 'sdk\.cargo\config.toml'
    New-Item -ItemType Directory -Path (Split-Path -Parent $remoteCargoConfig) -Force | Out-Null
    $savedVendorHome = $env:CARGO_HOME
    try {
        # Resolve once from the approved SDK closure, then commit the resulting closure into the frozen tree.
        $vendorHome = Join-Path $fixture 'vendor-cargo-home'; New-Item -ItemType Directory -Path $vendorHome -Force | Out-Null
        $sourceVendor = (Join-Path $sdkRoot 'vendor\cargo-sources').Replace('\','/')
        [IO.File]::WriteAllText($remoteCargoConfig, "[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$sourceVendor'`n", [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText((Join-Path $vendorHome 'config.toml'), "[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$sourceVendor'`n", [Text.UTF8Encoding]::new($false))
        Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue
        & cargo.exe vendor --manifest-path (Join-Path $sdkRoot 'fixtures\abi-root-host\Cargo.toml') --locked --offline $frozenVendor | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'unable to materialize a frozen offline vendor closure' }
        [IO.File]::WriteAllText($remoteCargoConfig, "[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = 'vendor/cargo-sources'`n", [Text.UTF8Encoding]::new($false))
    } finally {
        if ($null -eq $savedVendorHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedVendorHome }
    }
    & git -C $remote init --quiet
    & git -C $remote config user.email 'fixture@example.invalid'
    & git -C $remote config user.name 'Release Fixture'
    & git -C $remote config core.autocrlf false
    [IO.File]::WriteAllText((Join-Path $remote 'snapshot.txt'), "frozen`n", [Text.UTF8Encoding]::new($false))
    & git -C $remote add .; & git -C $remote commit --quiet -m frozen
    & git -C $remote branch -M main
    & git -C $remote tag -a gpui-sdk-v0.2.0-rc.1 -m frozen
    $frozenRevision = (& git -C $remote rev-parse 'gpui-sdk-v0.2.0-rc.1^{}').Trim()
    [IO.File]::WriteAllText((Join-Path $remote 'snapshot.txt'), "advanced`n", [Text.UTF8Encoding]::new($false))
    & git -C $remote add snapshot.txt; & git -C $remote commit --quiet -m advanced
    $mainRevision = (& git -C $remote rev-parse main).Trim()
    if ($mainRevision -eq $frozenRevision) { throw 'synthetic remote main did not advance' }
    if ((& git -C $remote rev-parse 'gpui-sdk-v0.2.0-rc.1^{}').Trim() -ne $frozenRevision) { throw 'frozen tag moved after remote main advanced' }
    $offlineRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-release-offline-' + [guid]::NewGuid().ToString('N'))
    $buildCheckout = Join-Path $offlineRoot 'frozen-checkout'
    $offlineCargo = Join-Path $offlineRoot 'cargo-home'; $offlineTarget = Join-Path $offlineRoot 'target'
    New-Item -ItemType Directory -Path $offlineCargo,$offlineTarget -Force | Out-Null
    & git -C $remote worktree add --detach $buildCheckout $frozenRevision | Out-Null
    $checkoutRevision = (& git -C $buildCheckout rev-parse HEAD).Trim()
    if ($checkoutRevision -ne $frozenRevision -or $checkoutRevision -eq $mainRevision) { throw 'offline rebuild checkout is not pinned to the frozen revision' }
    $vendor = (Join-Path $buildCheckout 'sdk\vendor\cargo-sources').Replace('\','/')
    if ($vendor -like "$(Join-Path $sdkRoot 'vendor\cargo-sources').Replace('\','/')") { throw 'frozen rebuild may not reference the live SDK vendor tree' }
    [IO.File]::WriteAllText((Join-Path $offlineCargo 'config.toml'), "[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$vendor'`n", [Text.UTF8Encoding]::new($false))
    try {
        Invoke-OfflineFixtureBuild (Join-Path $buildCheckout 'sdk\fixtures\abi-root-host\Cargo.toml') $offlineCargo $offlineTarget
        Invoke-OfflineFixtureBuild (Join-Path $buildCheckout 'sdk\fixtures\abi-root-plugin\Cargo.toml') $offlineCargo $offlineTarget
    } finally {
        & git -C $remote worktree remove --force $buildCheckout 2>$null
        if (Test-Path -LiteralPath $offlineRoot) { Remove-Item -LiteralPath $offlineRoot -Recurse -Force }
    }
} finally {
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}

Write-Output 'release freeze contract passed'
