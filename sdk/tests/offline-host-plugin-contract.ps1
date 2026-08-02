[CmdletBinding()]
param(
    [string]$EvidencePath,
    [string]$ArtifactOutputRoot
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$hostManifest = Join-Path $sdkRoot 'fixtures\abi-root-host\Cargo.toml'
$pluginManifest = Join-Path $sdkRoot 'fixtures\abi-root-plugin\Cargo.toml'
$targetTriple = 'x86_64-pc-windows-msvc'
$cargoHome = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fixture-cargo-' + [guid]::NewGuid().ToString('N'))
$hostTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fixture-host-' + [guid]::NewGuid().ToString('N'))
$pluginTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fixture-plugin-' + [guid]::NewGuid().ToString('N'))
$oldCargoHome = $env:CARGO_HOME
$oldTarget = $env:CARGO_TARGET_DIR
$oldCargoOffline = $env:CARGO_NET_OFFLINE

function Remove-VerifiedTempDirectory([string]$Path, [string]$Label) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw "Cannot clean ${Label}: target does not exist." }
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or $resolved -eq $tempRoot.TrimEnd('\')) { throw "Refusing to clean $Label outside temp root: $resolved" }
    Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction Stop
}

function Get-DirectoryEntryCount([string]$Path) {
    return @((Get-ChildItem -LiteralPath $Path -Force -ErrorAction Stop)).Count
}

function Invoke-OfflineBuild([string]$ManifestPath, [string]$TargetPath, [string]$Label) {
    $env:CARGO_TARGET_DIR = $TargetPath
    & cargo.exe build --manifest-path $ManifestPath --locked --offline --target $targetTriple | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "${Label} fixture build failed with exit code $LASTEXITCODE" }
}

$locationPushed = $false
$createdTemp = @()
$evidence = $null
try {
    Push-Location $sdkRoot
    $locationPushed = $true
    New-Item -ItemType Directory -Path $cargoHome, $hostTarget, $pluginTarget | Out-Null
    $createdTemp += $cargoHome, $hostTarget, $pluginTarget
    if ((Get-DirectoryEntryCount $cargoHome) -ne 0) { throw 'isolated CARGO_HOME was not empty at test start' }

    $env:CARGO_HOME = $cargoHome
    $env:CARGO_NET_OFFLINE = 'true'
    Invoke-OfflineBuild $hostManifest $hostTarget 'host'
    Invoke-OfflineBuild $pluginManifest $pluginTarget 'plugin'

    if ($hostTarget -eq $pluginTarget) { throw 'host and plugin fixture target directories must be distinct' }
    $hostExe = Join-Path $hostTarget "$targetTriple\debug\abi-root-fixture-host.exe"
    $pluginDll = Join-Path $pluginTarget "$targetTriple\debug\abi_root_fixture_plugin.dll"
    if (-not (Test-Path -LiteralPath $hostExe -PathType Leaf) -or -not (Test-Path -LiteralPath $pluginDll -PathType Leaf)) { throw 'isolated fixture artifacts were not produced' }

    & $hostExe compatible $pluginDll
    $loadExitCode = $LASTEXITCODE
    if ($loadExitCode -ne 0) { throw "isolated host failed to load plugin (exit code $loadExitCode)" }
    $cargoHomeEntriesAfterBuild = Get-DirectoryEntryCount $cargoHome

    $evidence = [ordered]@{
        schema_version = 1
        cargo = [ordered]@{
            home_initially_empty = $true
            home_entry_count_after_build = $cargoHomeEntriesAfterBuild
            target_directories_distinct = $true
            host_command = 'cargo build --manifest-path fixtures/abi-root-host/Cargo.toml --locked --offline --target x86_64-pc-windows-msvc'
            plugin_command = 'cargo build --manifest-path fixtures/abi-root-plugin/Cargo.toml --locked --offline --target x86_64-pc-windows-msvc'
        }
        artifacts = [ordered]@{
            host = [ordered]@{ name = 'abi-root-fixture-host.exe'; sha256 = (Get-FileHash -LiteralPath $hostExe -Algorithm SHA256).Hash.ToLowerInvariant() }
            plugin = [ordered]@{ name = 'abi_root_fixture_plugin.dll'; sha256 = (Get-FileHash -LiteralPath $pluginDll -Algorithm SHA256).Hash.ToLowerInvariant() }
        }
        load = [ordered]@{ mode = 'compatible'; exit_code = $loadExitCode }
    }
    if ($ArtifactOutputRoot) {
        if (-not (Test-Path -LiteralPath $ArtifactOutputRoot)) { New-Item -ItemType Directory -Force -Path $ArtifactOutputRoot | Out-Null }
        Copy-Item -LiteralPath $hostExe -Destination (Join-Path $ArtifactOutputRoot 'abi-root-fixture-host.exe') -Force
        Copy-Item -LiteralPath $pluginDll -Destination (Join-Path $ArtifactOutputRoot 'abi_root_fixture_plugin.dll') -Force
    }
    if ($EvidencePath) {
        $parent = Split-Path -Parent $EvidencePath
        if ($parent -and -not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
        [IO.File]::WriteAllText($EvidencePath, ($evidence | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
    }
    [pscustomobject]$evidence
} finally {
    if ($null -eq $oldCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $oldCargoHome }
    if ($null -eq $oldTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $oldTarget }
    if ($null -eq $oldCargoOffline) { Remove-Item Env:CARGO_NET_OFFLINE -ErrorAction SilentlyContinue } else { $env:CARGO_NET_OFFLINE = $oldCargoOffline }
    if ($locationPushed) { Pop-Location }
    $cleanupErrors = @()
    foreach ($entry in @(@($cargoHome, 'CARGO_HOME'), @($hostTarget, 'host target'), @($pluginTarget, 'plugin target'))) {
        if ($entry[0] -notin $createdTemp) { continue }
        try { Remove-VerifiedTempDirectory $entry[0] $entry[1] } catch { $cleanupErrors += $_.Exception.Message }
    }
    if ($cleanupErrors.Count) { throw ($cleanupErrors -join '; ') }
}
