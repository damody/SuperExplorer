[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$generator = Join-Path $workspace 'sdk\tools\bundle-generator'
$sdkLockPath = Join-Path $workspace 'sdk\sdk-lock.json'
$manifestPath = Join-Path $workspace 'sdk\bundle-manifest.json'
$fingerprintPath = Join-Path $workspace 'sdk\ui-abi-fingerprint.json'
$mutationPath = Join-Path $workspace 'sdk\src\lib.rs'

function Invoke-Generator {
    param([Parameter(Mandatory = $true)][ValidateSet('generate', 'verify')][string]$Command)

    Push-Location $generator
    try {
        & cargo.exe run --release --locked -- $Command
        if ($LASTEXITCODE -ne 0) {
            throw "bundle generator $Command failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

function Get-Sha256Hex {
    param([Parameter(Mandatory = $true)][AllowEmptyCollection()][byte[]]$Bytes)

    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return (($sha256.ComputeHash($Bytes) | ForEach-Object { $_.ToString('x2') }) -join '')
    } finally {
        $sha256.Dispose()
    }
}

function Get-FileSha256Hex {
    param([Parameter(Mandatory = $true)][string]$Path)

    return Get-Sha256Hex ([IO.File]::ReadAllBytes($Path))
}

function Test-ByteEqual {
    param(
        [Parameter(Mandatory = $true)][byte[]]$Left,
        [Parameter(Mandatory = $true)][byte[]]$Right
    )

    return [Convert]::ToBase64String($Left) -ceq [Convert]::ToBase64String($Right)
}

function Get-InventoryRootHash {
    param([Parameter(Mandatory = $true)]$Files)

    $builder = [Text.StringBuilder]::new()
    foreach ($file in $Files) {
        [void]$builder.Append($file.path)
        [void]$builder.Append([char]0)
        [void]$builder.Append($file.sha256)
        [void]$builder.Append("`n")
    }
    return Get-Sha256Hex ([Text.Encoding]::UTF8.GetBytes($builder.ToString()))
}

Invoke-Generator generate
$firstLock = [IO.File]::ReadAllBytes($sdkLockPath)
$firstManifest = [IO.File]::ReadAllBytes($manifestPath)
$firstFingerprint = [IO.File]::ReadAllBytes($fingerprintPath)
Invoke-Generator generate
if (-not (Test-ByteEqual $firstLock ([IO.File]::ReadAllBytes($sdkLockPath)))) {
    throw 'sdk-lock.json was not byte-identical across two generations'
}
if (-not (Test-ByteEqual $firstManifest ([IO.File]::ReadAllBytes($manifestPath)))) {
    throw 'bundle-manifest.json was not byte-identical across two generations'
}
if (-not (Test-ByteEqual $firstFingerprint ([IO.File]::ReadAllBytes($fingerprintPath)))) {
    throw 'ui-abi-fingerprint.json was not byte-identical across two generations'
}
$sdkLockText = [Text.Encoding]::UTF8.GetString($firstLock)
$manifestText = [Text.Encoding]::UTF8.GetString($firstManifest)
$fingerprintText = [Text.Encoding]::UTF8.GetString($firstFingerprint)
if ($sdkLockText -match '(?i)[a-z]:\\' -or $manifestText -match '(?i)[a-z]:\\' -or $fingerprintText -match '(?i)[a-z]:\\') {
    throw 'generated bundle records contain an absolute local path'
}

$lock = $sdkLockText | ConvertFrom-Json
$manifest = $manifestText | ConvertFrom-Json
$fingerprint = $fingerprintText | ConvertFrom-Json
foreach ($field in @('cargo_sha256','rustc_sha256')) {
    if ([string]$lock.toolchain.$field -notmatch '^[0-9a-f]{64}$') {
        throw "sdk-lock toolchain.$field must record the actual pinned executable SHA-256"
    }
}
$generatorBinary = Join-Path $generator 'target\release\superexplorer-bundle-generator.exe'
if (-not (Test-Path -LiteralPath $generatorBinary)) { throw 'release bundle generator binary was not produced' }
$fakeToolchainPath = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-generator-fake-rustup-' + [guid]::NewGuid().ToString('N'))
$savedPath = [Environment]::GetEnvironmentVariable('PATH', 'Process')
$savedUserProfile = [Environment]::GetEnvironmentVariable('USERPROFILE', 'Process')
$savedRustupHome = [Environment]::GetEnvironmentVariable('RUSTUP_HOME', 'Process')
try {
    New-Item -ItemType Directory -Path $fakeToolchainPath -Force | Out-Null
    foreach ($name in @('cargo.exe','rustc.exe','rustup.exe')) { Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\cmd.exe') -Destination (Join-Path $fakeToolchainPath $name) }
    [Environment]::SetEnvironmentVariable('PATH', "$fakeToolchainPath;$savedPath", 'Process')
    [Environment]::SetEnvironmentVariable('USERPROFILE', $fakeToolchainPath, 'Process')
    [Environment]::SetEnvironmentVariable('RUSTUP_HOME', $fakeToolchainPath, 'Process')
    & $generatorBinary verify
    if ($LASTEXITCODE -ne 0) { throw 'bundle generator accepted PATH-prepended fake cargo/rustc/rustup authority' }
} finally {
    [Environment]::SetEnvironmentVariable('PATH', $savedPath, 'Process')
    [Environment]::SetEnvironmentVariable('USERPROFILE', $savedUserProfile, 'Process')
    [Environment]::SetEnvironmentVariable('RUSTUP_HOME', $savedRustupHome, 'Process')
    if (Test-Path -LiteralPath $fakeToolchainPath) { Remove-Item -LiteralPath $fakeToolchainPath -Recurse -Force }
}
if ($manifest.files.path -notcontains 'sdk/vendor/cargo-sources/cc/src/target/apple.rs') {
    throw 'inventory omitted vendored cc/src/target/apple.rs source'
}
if ($manifest.files.path | Where-Object { $_ -match '(^|/)\.git($|/)' }) {
    throw 'inventory contains Git metadata'
}
if ($lock.bundle_id -ne $manifest.bundle_id) {
    throw 'bundle ID differs between sdk lock and bundle manifest'
}
if ($lock.inventory_root_sha256 -ne $manifest.inventory_root_sha256) {
    throw 'inventory root differs between sdk lock and bundle manifest'
}
if ((Get-FileSha256Hex $sdkLockPath) -ne $manifest.sdk_lock_sha256) {
    throw 'bundle manifest sdk_lock_sha256 does not match sdk-lock.json'
}
$fingerprintEntries = @($manifest.generated_artifacts | Where-Object path -eq 'sdk/ui-abi-fingerprint.json')
if ($fingerprintEntries.Count -ne 1 -or $fingerprintEntries[0].sha256 -ne (Get-FileSha256Hex $fingerprintPath)) {
    throw 'bundle manifest does not record the exact UI ABI fingerprint artifact hash'
}
if ($fingerprint.bundle_id -ne $lock.bundle_id -or [string]::IsNullOrWhiteSpace($fingerprint.fingerprint)) {
    throw 'UI ABI fingerprint artifact does not identify the canonical bundle'
}
$trustArtifactPath = Join-Path $workspace 'crates\explorer-extension-host\trusted-publisher-keys-v1.json'
$hostValidationSourcePath = Join-Path $workspace 'crates\explorer-extension-host\src\package_validation.rs'
$trustArtifact = Get-Content -LiteralPath $trustArtifactPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($trustArtifact.sdk_bundle_id -ne $lock.bundle_id) {
    throw 'release trust-root artifact bundle ID differs from the canonical SDK bundle'
}
$hostValidationSource = Get-Content -LiteralPath $hostValidationSourcePath -Raw -Encoding UTF8
$expectedTrustRootConstant = 'pub(crate) const RELEASE_TRUST_ROOTS_BUNDLE_ID_V1: &str = "' + [string]$lock.bundle_id + '";'
if (-not $hostValidationSource.Contains($expectedTrustRootConstant)) {
    throw 'host release trust-root bundle constant differs from the canonical SDK bundle'
}
foreach ($file in $manifest.files) {
    if ([IO.Path]::IsPathRooted($file.path) -or $file.path.Contains('\')) {
        throw "inventory path is not stable and relative: $($file.path)"
    }
    $fullPath = Join-Path $workspace $file.path
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
        throw "inventory file is missing: $($file.path)"
    }
    if ((Get-FileSha256Hex $fullPath) -ne $file.sha256) {
        throw "inventory hash mismatch: $($file.path)"
    }
}
if ((Get-InventoryRootHash $manifest.files) -ne $manifest.inventory_root_sha256) {
    throw 'inventory root hash cannot be recomputed from the manifest files'
}

$originalSource = [IO.File]::ReadAllBytes($mutationPath)
try {
    $mutatedSource = [byte[]]($originalSource + [Text.Encoding]::UTF8.GetBytes("`n// bundle generator contract mutation`n"))
    [IO.File]::WriteAllBytes($mutationPath, $mutatedSource)
    Invoke-Generator generate
    $mutatedLock = ([Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($sdkLockPath)) | ConvertFrom-Json)
    $mutatedFingerprint = ([Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($fingerprintPath)) | ConvertFrom-Json)
    if ($mutatedLock.bundle_id -eq $lock.bundle_id) {
        throw 'changing one SDK source file did not change the bundle ID'
    }
    if ($mutatedFingerprint.fingerprint -eq $fingerprint.fingerprint -or $mutatedFingerprint.bundle_id -ne $mutatedLock.bundle_id) {
        throw 'changing one SDK source file did not update the UI ABI fingerprint artifact'
    }
} finally {
    [IO.File]::WriteAllBytes($mutationPath, $originalSource)
}
Invoke-Generator generate
if (-not (Test-ByteEqual $firstLock ([IO.File]::ReadAllBytes($sdkLockPath)))) {
    throw 'restored source did not restore sdk-lock.json byte-for-byte'
}
if (-not (Test-ByteEqual $firstManifest ([IO.File]::ReadAllBytes($manifestPath)))) {
    throw 'restored source did not restore bundle-manifest.json byte-for-byte'
}
if (-not (Test-ByteEqual $firstFingerprint ([IO.File]::ReadAllBytes($fingerprintPath)))) {
    throw 'restored source did not restore ui-abi-fingerprint.json byte-for-byte'
}
Invoke-Generator verify
Write-Host 'Bundle generator contract passed.'
