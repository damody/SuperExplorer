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

if ((& git -C $repo status --porcelain) -or $LASTEXITCODE -ne 0) { throw 'Release freeze requires a clean superproject checkout.' }
$tagObject = Invoke-Git @('rev-parse', '--verify', "refs/tags/$ReleaseTag")
if ((Invoke-Git @('cat-file', '-t', $tagObject)) -ne 'tag') { throw 'Release tag must be an annotated tag.' }
# Fail closed: verify the tag using only the explicitly supplied trusted GPG home.
$savedGpgHome = [Environment]::GetEnvironmentVariable('GNUPGHOME', 'Process')
try {
    $env:GNUPGHOME = (Resolve-Path -LiteralPath $GpgHome).Path
    Invoke-Git @('verify-tag', $ReleaseTag) | Out-Null
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
    protection = [ordered]@{ provider = $ProtectionProvider; policy_id = $ProtectionPolicyId; record = (Artifact $ProtectionRecord) }
    signature = [ordered]@{ verification = 'detached_gpg'; signer = $Signer; artifact = (Artifact $DetachedSignature) }
    provenance = [ordered]@{ builder = $Builder; predicate_type = $PredicateType; artifact = (Artifact $Provenance) }
    prior_release_ledger = (Artifact $ledger)
}
$temporary = Join-Path ([IO.Path]::GetTempPath()) ("superexplorer-release-freeze-" + [Guid]::NewGuid().ToString('N') + '.json')
try {
    [IO.File]::WriteAllText($temporary, (($metadata | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
    Push-Location $validator
    try {
        $digest = (& cargo.exe run --release --locked --offline -- digest --metadata $temporary 2>&1 | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or $digest -notmatch '^[0-9a-f]{64}$') { throw "Unable to calculate release-input digest: $digest" }
    } finally { Pop-Location }
    $metadata.release_input_digest = $digest
    $staged = "$snapshot.$([Guid]::NewGuid().ToString('N')).tmp"
    [IO.File]::WriteAllText($staged, (($metadata | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $staged -Destination $snapshot -ErrorAction Stop
    Push-Location $validator
    try { & cargo.exe run --release --locked --offline -- verify } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw 'Release-freeze validator rejected the generated production record.' }
} catch {
    if (Test-Path -LiteralPath $snapshot) { Remove-Item -LiteralPath $snapshot -Force }
    throw
} finally {
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
}

Write-Output "Release freeze recorded for $ReleaseTag ($revision)."
