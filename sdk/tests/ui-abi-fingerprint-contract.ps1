[CmdletBinding()] param()
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$tool = Join-Path $root 'sdk\tools\ui-abi-fingerprint'
Push-Location $tool
try {
    & cargo.exe test --locked
    if ($LASTEXITCODE -ne 0) { throw 'fingerprint synthetic tests failed' }
    & cargo.exe run --release --locked -- verify
    if ($LASTEXITCODE -ne 0) { throw 'production UI ABI fingerprint verification failed' }
} finally { Pop-Location }
$fixture = Join-Path $root 'sdk\fixtures\ui-fingerprint-loader'
Push-Location $fixture
try {
    & cargo.exe test --locked --offline
    if ($LASTEXITCODE -ne 0) { throw 'pre-callback loader fixture failed' }
} finally { Pop-Location }
$artifact = Get-Content (Join-Path $root 'sdk\ui-abi-fingerprint.json') -Raw | ConvertFrom-Json
$lock = Get-Content (Join-Path $root 'sdk\sdk-lock.json') -Raw | ConvertFrom-Json
if ($artifact.bundle_id -ne $lock.bundle_id -or $artifact.fingerprint -notmatch '^[0-9a-f]{64}$') {
    throw 'production UI ABI fingerprint artifact is malformed or bound to another bundle'
}
Write-Host 'UI ABI fingerprint synthetic and production contracts passed.'
