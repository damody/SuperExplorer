$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$tool = Join-Path $repo 'sdk\tools\release-freeze-validator'
$paths = @{
    Metadata = Join-Path $repo 'sdk\snapshot\release-freeze.json'
    Lock = Join-Path $repo 'sdk\sdk-lock.json'
    Manifest = Join-Path $repo 'sdk\bundle-manifest.json'
    Fingerprint = Join-Path $repo 'sdk\ui-abi-fingerprint.json'
}
$backup = @{}
foreach ($entry in $paths.GetEnumerator()) {
    $backup[$entry.Key] = if (Test-Path -LiteralPath $entry.Value) {
        [IO.File]::ReadAllBytes($entry.Value)
    } else {
        $null
    }
}

function Write-Json([string]$Path, $Value) {
    $json = $Value | ConvertTo-Json -Depth 100
    [IO.File]::WriteAllText($Path, "$json`n", [Text.UTF8Encoding]::new($false))
}

function Restore-CanonicalFiles {
    foreach ($entry in $paths.GetEnumerator()) {
        $bytes = $backup[$entry.Key]
        if ($null -eq $bytes) {
            if (Test-Path -LiteralPath $entry.Value) {
                Remove-Item -LiteralPath $entry.Value -Force
            }
        } else {
            [IO.File]::WriteAllBytes($entry.Value, $bytes)
        }
    }
}

function Invoke-Validator([bool]$ShouldPass, [string]$Case) {
    Push-Location $tool
    try {
        $savedErrorAction = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & cargo.exe run --release --locked --offline -- verify 2>&1 | Out-Null
        $exitCode = $LASTEXITCODE
        $ErrorActionPreference = $savedErrorAction
    } finally {
        $ErrorActionPreference = 'Stop'
        Pop-Location
    }
    if ($ShouldPass -and $exitCode -ne 0) { throw "$Case unexpectedly failed" }
    if (-not $ShouldPass -and $exitCode -eq 0) { throw "$Case unexpectedly passed" }
}

function Reset-PositiveFixture {
    Restore-CanonicalFiles
    $lock = Get-Content -LiteralPath $paths.Lock -Raw | ConvertFrom-Json
    $fingerprint = Get-Content -LiteralPath $paths.Fingerprint -Raw | ConvertFrom-Json
    $metadata = [ordered]@{
        schema_version = 1
        release_frozen = $true
        protected_tag = [ordered]@{
            name = 'gpui-sdk-v0.1.0-rc.1'
            object_revision = $lock.gpui.revision
            tree = $lock.gpui.tree
            protection_record = 'fixture://protected-tag-policy/1'
        }
        source = [ordered]@{ revision = $lock.gpui.revision; tree = $lock.gpui.tree }
        rc_id = '0.1.0-rc.1'
        bundle_id = $lock.bundle_id
        release_input_fingerprint = $fingerprint.fingerprint
        signature_reference = 'fixture://signature/1'
        provenance_reference = 'fixture://provenance/1'
    }
    Write-Json $paths.Metadata $metadata
}

try {
    Reset-PositiveFixture
    Invoke-Validator $true 'frozen release'

    Push-Location $tool
    try {
        $savedErrorAction = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & cargo.exe run --release --locked --offline -- verify unexpected-argument 2>&1 | Out-Null
        $extraArgumentExit = $LASTEXITCODE
        $ErrorActionPreference = $savedErrorAction
    } finally {
        $ErrorActionPreference = 'Stop'
        Pop-Location
    }
    if ($extraArgumentExit -eq 0) { throw 'release validator accepted an unexpected argument' }

    $metadata = Get-Content $paths.Metadata -Raw | ConvertFrom-Json
    $metadata.protected_tag = $null
    Write-Json $paths.Metadata $metadata
    Invoke-Validator $false 'missing protected tag'

    Reset-PositiveFixture
    $metadata = Get-Content $paths.Metadata -Raw | ConvertFrom-Json
    $metadata.release_frozen = $false
    Write-Json $paths.Metadata $metadata
    Invoke-Validator $false 'unfrozen release'

    Reset-PositiveFixture
    $metadata = Get-Content $paths.Metadata -Raw | ConvertFrom-Json
    $metadata.protected_tag.tree = '3' * 40
    Write-Json $paths.Metadata $metadata
    Invoke-Validator $false 'tag/source mismatch'

    foreach ($artifact in @('Lock', 'Manifest', 'Fingerprint')) {
        Reset-PositiveFixture
        $value = Get-Content $paths[$artifact] -Raw | ConvertFrom-Json
        $value.bundle_id = 'drifted-bundle'
        Write-Json $paths[$artifact] $value
        Invoke-Validator $false "$artifact bundle drift"
    }

    Reset-PositiveFixture
    $metadata = Get-Content $paths.Metadata -Raw | ConvertFrom-Json
    $lock = Get-Content $paths.Lock -Raw | ConvertFrom-Json
    $metadata.protected_tag.object_revision = '4' * 40
    $metadata.source.revision = '4' * 40
    $lock.gpui.revision = '4' * 40
    Write-Json $paths.Metadata $metadata
    Write-Json $paths.Lock $lock
    Invoke-Validator $false 'revision change without a new fingerprint and bundle'

    Reset-PositiveFixture
    $metadata = Get-Content $paths.Metadata -Raw | ConvertFrom-Json
    $metadata | Add-Member -NotePropertyName remote_main -NotePropertyValue ('5' * 40)
    Write-Json $paths.Metadata $metadata
    Invoke-Validator $true 'remote main movement'
} finally {
    Restore-CanonicalFiles
}

foreach ($entry in $paths.GetEnumerator()) {
    $expected = $backup[$entry.Key]
    if ($null -eq $expected) {
        if (Test-Path -LiteralPath $entry.Value) { throw "$($entry.Key) was not removed during rollback" }
    } elseif ([Convert]::ToBase64String($expected) -ne [Convert]::ToBase64String([IO.File]::ReadAllBytes($entry.Value))) {
        throw "$($entry.Key) was not restored byte-identically"
    }
}

Write-Output 'release freeze contract passed'
