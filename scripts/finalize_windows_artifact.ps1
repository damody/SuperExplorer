param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifestPath = Join-Path $workspaceRoot 'crates\explorer-app\app.manifest'

if (-not $SkipBuild) {
    $cargoArguments = @('build', '-p', 'explorer-app', '-p', 'explorer-extension-broker', '--locked')
    if ($Profile -eq 'release') {
        $cargoArguments += '--release'
    }
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
}

$targetRoot = if ($env:CARGO_TARGET_DIR) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $env:CARGO_TARGET_DIR))
    }
} else {
    Join-Path $workspaceRoot 'target'
}
$executablePath = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "explorer-app executable not found: $executablePath"
}
$brokerPath = Join-Path $targetRoot "$Profile\explorer-extension-broker.exe"
$workerPath = Join-Path $targetRoot "$Profile\explorer-extension-worker.exe"
foreach ($requiredBinary in @($brokerPath, $workerPath)) {
    if (-not (Test-Path -LiteralPath $requiredBinary -PathType Leaf)) {
        throw "required extension isolation binary not found: $requiredBinary"
    }
}

$manifestTool = (Get-Command mt.exe -ErrorAction SilentlyContinue).Source
if (-not $manifestTool) {
    $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $manifestTool = Get-ChildItem -LiteralPath $sdkRoot -Directory |
        Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64\mt.exe' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
}
if (-not $manifestTool) {
    throw 'Windows Manifest Tool (mt.exe) was not found.'
}

$evidenceDirectory = Join-Path $targetRoot 'manifest-evidence'
New-Item -ItemType Directory -Force -Path $evidenceDirectory | Out-Null
$manifestStagingPath = Join-Path $evidenceDirectory ("SuperExplorer-$Profile-staging.exe")
Copy-Item -LiteralPath $executablePath -Destination $manifestStagingPath -Force

$manifestExitCode = 1
for ($attempt = 1; $attempt -le 5; $attempt++) {
    & $manifestTool '-nologo' "-manifest" $manifestPath "-outputresource:$manifestStagingPath;#1"
    $manifestExitCode = $LASTEXITCODE
    if ($manifestExitCode -eq 0) { break }
    if ($attempt -lt 5) { Start-Sleep -Milliseconds (250 * $attempt) }
}
if ($manifestExitCode -ne 0) {
    throw "manifest update failed after 5 attempts with exit code $manifestExitCode"
}
Copy-Item -LiteralPath $manifestStagingPath -Destination $executablePath -Force
Remove-Item -LiteralPath $manifestStagingPath -Force

$extractedManifest = Join-Path $evidenceDirectory "SuperExplorer-$Profile.manifest"
& $manifestTool '-nologo' "-inputresource:$executablePath;#1" "-out:$extractedManifest"
if ($LASTEXITCODE -ne 0) {
    throw "manifest extraction failed with exit code $LASTEXITCODE"
}
& $manifestTool '-nologo' '-validate_manifest' '-manifest' $extractedManifest
if ($LASTEXITCODE -ne 0) {
    throw "manifest validation failed with exit code $LASTEXITCODE"
}

$manifestText = Get-Content -Raw -Encoding utf8 -LiteralPath $extractedManifest
$requiredManifestValues = @(
    'name="Damody.SuperExplorer"',
    'processorArchitecture="amd64"',
    '>PerMonitorV2<',
    '>SegmentHeap<',
    'name="Microsoft.Windows.Common-Controls"',
    'version="6.0.0.0"'
)
foreach ($requiredValue in $requiredManifestValues) {
    if (-not $manifestText.Contains($requiredValue)) {
        throw "final manifest is missing required value: $requiredValue"
    }
}

$executableBytes = [System.IO.File]::ReadAllBytes($executablePath)
if ($executableBytes.Length -lt 64) {
    throw 'executable is too small to contain a valid PE header.'
}
$peOffset = [BitConverter]::ToInt32($executableBytes, 0x3c)
if ($peOffset -lt 0 -or $peOffset + 6 -gt $executableBytes.Length) {
    throw 'executable contains an invalid PE header offset.'
}
$peSignature = [BitConverter]::ToUInt32($executableBytes, $peOffset)
$machine = [BitConverter]::ToUInt16($executableBytes, $peOffset + 4)
if ($peSignature -ne 0x00004550 -or $machine -ne 0x8664) {
    throw ('expected an x64 PE executable (machine 0x8664), got signature 0x{0:X8}, machine 0x{1:X4}' -f $peSignature, $machine)
}

$versionInfo = (Get-Item -LiteralPath $executablePath).VersionInfo
if ($versionInfo.FileDescription -ne 'SuperExplorer' -or
    $versionInfo.ProductName -ne 'SuperExplorer' -or
    $versionInfo.InternalName -ne 'SuperExplorer' -or
    $versionInfo.OriginalFilename -ne 'SuperExplorer.exe') {
    throw 'VERSIONINFO metadata did not match the expected Explorer application values.'
}

foreach ($extensionBinary in @($brokerPath, $workerPath)) {
    $bytes = [System.IO.File]::ReadAllBytes($extensionBinary)
    if ($bytes.Length -lt 1024) { throw "extension binary is unexpectedly small: $extensionBinary" }
    $offset = [BitConverter]::ToInt32($bytes, 0x3c)
    $signature = [BitConverter]::ToUInt32($bytes, $offset)
    $binaryMachine = [BitConverter]::ToUInt16($bytes, $offset + 4)
    if ($signature -ne 0x00004550 -or $binaryMachine -ne 0x8664) {
        throw "extension binary is not a valid x64 PE: $extensionBinary"
    }
    $marker = (& $extensionBinary --version-json | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $marker.Contains('"protocol":1') -or -not $marker.Contains('"arch":"x64"')) {
        throw "extension binary protocol/build marker is invalid: $extensionBinary"
    }
}

Write-Output "Finalized and validated: $executablePath"
Write-Output "Extracted manifest: $extractedManifest"
Write-Output "VERSIONINFO: $($versionInfo.FileDescription) $($versionInfo.ProductVersion)"
Write-Output ('PE machine: 0x{0:X4} (x64)' -f $machine)
Write-Output "Validated broker: $brokerPath"
Write-Output "Validated worker: $workerPath"
