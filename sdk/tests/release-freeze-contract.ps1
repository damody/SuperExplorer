$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$tool = Join-Path $repo 'sdk\tools\release-freeze-validator'
$canonicalFreeze = Join-Path $repo 'sdk\snapshot\release-freeze.json'
if (Test-Path -LiteralPath $canonicalFreeze) {
    throw 'The development checkout must not contain a canonical release-freeze record.'
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
        rc_id = '0.1.0-rc.1'; bundle_id = 'regenerated-bundle'; release_input_digest = ('a' * 64)
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
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $freezeScript -ReleaseTag 'gpui-sdk-v0.1.0-rc.1' -RcId '0.1.0-rc.1' -ProtectionProvider fixture -ProtectionPolicyId fixture -ProtectionRecord $protectionPath -DetachedSignature $signaturePath -Signer fixture -GpgKeyring $signaturePath -Provenance $provenancePath -Builder fixture -PredicateType fixture 2>$null
    $freezeExit = $LASTEXITCODE
    $ErrorActionPreference = $saved
    if ($freezeExit -eq 0) { throw 'production freeze script accepted unsigned fixture evidence' }
    if (Test-Path -LiteralPath $canonicalFreeze) { throw 'failed production freeze attempt created a canonical record' }
} finally {
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}

Write-Output 'release freeze contract passed'
