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
$policy = Get-Content -LiteralPath (Join-Path $sdkRoot 'ci\release-policy.json') -Raw | ConvertFrom-Json
if ($policy.schema_version -ne 1 -or $policy.policy_id -ne 'sdk-release-freeze-v1' -or $policy.protection.provider -ne 'github-environment') {
    throw 'versioned release trust policy is incomplete'
}
$releaseWorkflow = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\freeze-gpui-release.yml') -Raw
foreach ($requiredWorkflowControl in @('environment: sdk-release-freeze', 'fetch-depth: 0', 'fetch-tags: true', '--batch --import', 'Invoke-OfflineSdkGuest.template.ps1', "load.mode -ne 'compatible'", 'RELEASE_TAG:', 'RELEASE_BASE_SHA', '--force-with-lease')) {
    if (-not $releaseWorkflow.Contains($requiredWorkflowControl)) { throw "protected release workflow is missing control: $requiredWorkflowControl" }
}
if ($releaseWorkflow.Contains('GPG_HOME_B64')) { throw 'protected release workflow uses a misleading GPG home secret' }
$freezeSource = Get-Content -LiteralPath (Join-Path $sdkRoot 'scripts\freeze-release.ps1') -Raw
if (-not $freezeSource.Contains('& git -C $gpui @Arguments') -or -not $freezeSource.Contains("git -C `$repo status --porcelain") -or -not $freezeSource.Contains("remote', 'get-url', 'origin'")) {
    throw 'freeze script must resolve protected tags in the authorized GPUI repository while checking superproject cleanliness separately'
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
    foreach ($relative in @('sdk', 'sdk\snapshot', 'evidence')) {
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
    $protectionPath = Join-Path $fixture 'evidence\protection.json'
    $signaturePath = Join-Path $fixture 'evidence\signature.json'
    $provenancePath = Join-Path $fixture 'evidence\provenance.json'
    $metadataPath = Join-Path $fixture 'sdk\snapshot\release-freeze.json'
    $lock = [ordered]@{
        bundle_id = 'fixture-bundle'
        toolchain = [ordered]@{ rustc_release = '1.97.1'; rustc_commit_hash = 'a'; cargo_release = '1.97.1'; cargo_commit_hash = 'b'; target = 'x86_64-pc-windows-msvc' }
        gpui = [ordered]@{ revision = $revision; tree = $tree; approved_snapshot = [ordered]@{ production = [ordered]@{ features = @() } } }
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
    Write-Json $protectionPath ([ordered]@{ policy = 'fixture-protected-tag' })
    Write-Json $signaturePath ([ordered]@{ fixture_unsigned = $true })
    Write-Json $provenancePath ([ordered]@{ builder = 'fixture' })
    $metadata = [ordered]@{
        schema_version = 2
        release_frozen = $true
        evidence_mode = 'fixture'
        protected_tag = [ordered]@{ name = 'gpui-sdk-v0.1.0-rc.1'; tag_object = $tagObject; object_revision = $revision; tree = $tree }
        source = [ordered]@{ revision = $revision; tree = $tree }
        rc_id = '0.1.0-rc.1'
        bundle_id = 'fixture-bundle'
        release_input_digest = ('0' * 64)
        artifacts = [ordered]@{ sdk_lock = (Artifact $fixture $lockPath); bundle_manifest = (Artifact $fixture $manifestPath); ui_abi_fingerprint = (Artifact $fixture $fingerprintPath) }
        protection = [ordered]@{ provider = 'fixture'; policy_id = 'fixture-policy'; record = (Artifact $fixture $protectionPath) }
        signature = [ordered]@{ verification = 'fixture_unsigned'; signer = 'fixture'; artifact = (Artifact $fixture $signaturePath) }
        provenance = [ordered]@{ builder = 'fixture'; predicate_type = 'fixture'; artifact = (Artifact $fixture $provenancePath) }
        prior_release_ledger = (Artifact $fixture $ledgerPath)
    }
    Write-Json $metadataPath $metadata
    Set-InputDigest $metadataPath
    $metadataBackup = [IO.File]::ReadAllBytes($metadataPath)
    $ledgerBackup = [IO.File]::ReadAllBytes($ledgerPath)
    $protectionBackup = [IO.File]::ReadAllBytes($protectionPath)

    Invoke-FixtureValidator $fixture $true 'valid annotated fixture release'
    Invoke-Tool @('verify', '--root', $fixture) $false 'production CLI must not accept a fixture root'

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
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $freezeScript -ReleaseTag 'gpui-sdk-v0.1.0-rc.1' -RcId '0.1.0-rc.1' -ProtectionProvider fixture -ProtectionPolicyId fixture -ProtectionRecord $protectionPath -DetachedSignature $signaturePath -Signer fixture -GpgKeyring $signaturePath -GpgHome $gpgHome -GpgPrimaryFingerprint ('A' * 40) -Provenance $provenancePath -Builder fixture -PredicateType fixture 2>$null
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
