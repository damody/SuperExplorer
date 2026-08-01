[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'protected-dependency-test-support.psm1') -Force
$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repoRoot = (Resolve-Path (Join-Path $sdkRoot '..')).Path
$oldCargoHome = $env:CARGO_HOME; $oldTarget = $env:CARGO_TARGET_DIR
$cargoHome = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-sdk-cargo-' + [guid]::NewGuid().ToString('N'))
$targetDir = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-sdk-target-' + [guid]::NewGuid().ToString('N'))
$locationPushed = $false; $createdTemp = @()
function Remove-VerifiedTempDirectory([string]$Path, [string]$Label) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw "Cannot clean ${Label}: target does not exist." }
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or $resolved -eq $tempRoot.TrimEnd('\')) { throw "Refusing to clean ${Label} outside temp root: $resolved" }
    try { Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction Stop } catch { throw "Failed to clean ${Label} '$resolved': $($_.Exception.Message)" }
}
try {
    Push-Location $sdkRoot; $locationPushed = $true
    New-Item -ItemType Directory -Path $cargoHome | Out-Null; $createdTemp += $cargoHome
    New-Item -ItemType Directory -Path $targetDir | Out-Null; $createdTemp += $targetDir
    $env:CARGO_HOME = $cargoHome; $env:CARGO_TARGET_DIR = $targetDir
    $metadataText = (& cargo metadata --locked --offline --format-version 1 | Out-String)
    if ($LASTEXITCODE) { throw "cargo metadata --locked --offline failed with exit code $LASTEXITCODE" }
    $metadata = $metadataText | ConvertFrom-Json
    $closure = Get-Content (Join-Path $sdkRoot 'snapshot\protected-dependency-closure.json') -Raw | ConvertFrom-Json
    $gpuiSnapshot = Get-Content (Join-Path $sdkRoot 'snapshot\approved-gpui.json') -Raw | ConvertFrom-Json
    $lockText = Get-Content (Join-Path $sdkRoot 'Cargo.lock') -Raw
    $result = Assert-ProtectedDependencyMetadata $metadata $closure $repoRoot $gpuiSnapshot $lockText
    $checkEap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'; & cargo check --locked --offline 2>&1 | Out-Host; $ErrorActionPreference = $checkEap
    if ($LASTEXITCODE) { throw "cargo check --locked --offline failed with exit code $LASTEXITCODE" }
    $abiBlock = [regex]::Matches((Get-Content (Join-Path $sdkRoot 'Cargo.lock') -Raw), '(?ms)^\[\[package\]\]\s*name = "abi_stable".*?(?=^\[\[package\]\]|\z)')
    if ($abiBlock.Count -ne 1 -or $abiBlock[0].Value -notmatch ('checksum = "' + [regex]::Escape($closure.abi_stable.checksum) + '"')) { throw 'abi_stable lock checksum drifted.' }
    $gpuiRoot = Join-Path $repoRoot 'vendor\gpui-ce'; $gpui = @($metadata.packages | Where-Object name -eq 'gpui')[0]
    if ((Resolve-Path (Split-Path $gpui.manifest_path -Parent)).Path -ne (Resolve-Path (Join-Path $gpuiRoot 'crates\gpui')).Path) { throw 'gpui path drifted.' }
    if ($gpuiSnapshot.source.repository -ne 'https://github.com/damody/gpui-ce-explorer.git') { throw 'GPUI source authority drifted.' }
    $gpuiHead = (& git -C $gpuiRoot rev-parse HEAD 2>$null).Trim(); if ($LASTEXITCODE -or $gpuiHead -ne $gpuiSnapshot.source.revision) { throw 'GPUI revision drifted.' }
    $gpuiTree = (& git -C $gpuiRoot rev-parse "$gpuiHead`^{tree}" 2>$null).Trim(); if ($LASTEXITCODE -or $gpuiTree -ne $gpuiSnapshot.source.tree) { throw 'GPUI tree drifted.' }
    [pscustomobject]@{ Status='ok'; EdgeDigest=$result.EdgeDigest; PackageCount=$result.PackageCount; Offline='verified'; IsolatedCargoHome=$cargoHome; IsolatedTarget=$targetDir }
} finally {
    if ($null -eq $oldCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME=$oldCargoHome }
    if ($null -eq $oldTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR=$oldTarget }
    if ($locationPushed) { Pop-Location }
    $cleanupErrors = @()
    foreach ($entry in @(@($cargoHome,'CARGO_HOME'),@($targetDir,'target'))) {
        if ($entry[0] -notin $createdTemp) { continue }
        try { Remove-VerifiedTempDirectory $entry[0] $entry[1] } catch { $cleanupErrors += $_.Exception.Message }
    }
    if ($cleanupErrors.Count) { throw ($cleanupErrors -join '; ') }
}
