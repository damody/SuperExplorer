[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'

# Directory sources must contain every locked crate plus Cargo's checksum
# inventory. Keep the derived tree ignored; this bootstrap never downloads.
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$plugin = (Resolve-Path -LiteralPath $PluginRoot).Path
$toolingLockPath = Join-Path $repo 'sdk\tools\plugin-tooling\Cargo.lock'
if (-not (Test-Path -LiteralPath $toolingLockPath -PathType Leaf)) {
    throw 'the plugin tooling Cargo.lock is required for local Cargo source bootstrap'
}

function Get-Sha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-TreeDigest([string]$Root) {
    $base = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\')
    $lines = foreach ($item in @(Get-ChildItem -LiteralPath $base -File -Recurse -Force | Sort-Object FullName)) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "first-party SDK source contains a reparse point: $($item.FullName)" }
        $relative = $item.FullName.Substring($base.Length).TrimStart('\').Replace('\','/')
        "$relative`t$(Get-Sha256 $item.FullName)"
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($lines -join "`n") + "`n")
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
}

function Get-LockedRegistryPackages([string]$CargoLock) {
    $records = @()
    foreach ($block in ($CargoLock -split '(?m)^\[\[package\]\]\s*$')) {
        $nameMatch = [regex]::Match($block, '(?m)^name\s*=\s*"([^"]+)"\s*$')
        $versionMatch = [regex]::Match($block, '(?m)^version\s*=\s*"([^"]+)"\s*$')
        $sourceMatch = [regex]::Match($block, '(?m)^source\s*=\s*"(registry\+[^"]+)"\s*$')
        if (-not $nameMatch.Success -or -not $versionMatch.Success -or -not $sourceMatch.Success) { continue }
        $checksumMatch = [regex]::Match($block, '(?m)^checksum\s*=\s*"([0-9a-f]{64})"\s*$')
        $records += [pscustomobject]@{
            Name = $nameMatch.Groups[1].Value
            Version = $versionMatch.Groups[1].Value
            Checksum = if ($checksumMatch.Success) { $checksumMatch.Groups[1].Value } else { $null }
        }
    }
    return @($records | Sort-Object Name,Version -Unique)
}

function Expand-VerifiedCrate([string]$ArchivePath, [string]$ExpectedChecksum, [string]$ExpectedRoot, [string]$PackagesRoot) {
    $tar = Join-Path $env:SystemRoot 'System32\tar.exe'
    if (-not (Test-Path -LiteralPath $tar -PathType Leaf)) { throw 'the Windows system tar extractor is unavailable' }
    $privateArchive = Join-Path (Split-Path -Parent $PackagesRoot) ('.crate-' + [guid]::NewGuid().ToString('N'))
    try {
        Copy-Item -LiteralPath $ArchivePath -Destination $privateArchive -ErrorAction Stop
        if ((Get-Sha256 $privateArchive) -cne $ExpectedChecksum) { throw "registry archive changed while being copied: $ArchivePath" }
        $entries = @(& $tar -tzf $privateArchive)
        if ($LASTEXITCODE -ne 0 -or $entries.Count -eq 0 -or $entries.Count -gt 10000) { throw "registry archive has an invalid entry count: $ArchivePath" }
        foreach ($entry in $entries) {
            $normalized = ([string]$entry).Replace('\','/')
            $components = @($normalized.Split('/') | Where-Object { $_ -ne '' })
            if ($normalized.StartsWith('/') -or -not $normalized.StartsWith("$ExpectedRoot/", [StringComparison]::Ordinal) -or '..' -in $components -or $normalized.Contains(':')) {
                throw "registry archive contains an unsafe or unexpected path: $entry"
            }
        }
        $verboseEntries = @(& $tar -tvzf $privateArchive)
        if ($LASTEXITCODE -ne 0 -or @($verboseEntries | Where-Object { [string]$_ -match '^[lh]' }).Count -ne 0) { throw "registry archive contains a link or unreadable entry: $ArchivePath" }
        & $tar -xzf $privateArchive -C $PackagesRoot
        $destination = Join-Path $PackagesRoot $ExpectedRoot
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $destination -PathType Container)) { throw "could not extract registry archive: $ArchivePath" }
        $files = @(Get-ChildItem -LiteralPath $destination -File -Recurse -Force)
        if ($files.Count -gt 10000 -or (($files | Measure-Object -Property Length -Sum).Sum -gt 536870912)) { throw "registry archive exceeds extraction limits: $ArchivePath" }
        foreach ($item in @($files) + @(Get-ChildItem -LiteralPath $destination -Directory -Recurse -Force)) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "registry archive extracted a reparse point: $($item.FullName)" }
        }
    } finally {
        if (Test-Path -LiteralPath $privateArchive) { Remove-Item -LiteralPath $privateArchive -Force }
    }
}

function Write-DirectoryChecksum([string]$PackageRoot, [string]$PackageChecksum) {
    $base = (Resolve-Path -LiteralPath $PackageRoot).Path.TrimEnd('\')
    $files = [ordered]@{}
    foreach ($item in @(Get-ChildItem -LiteralPath $base -File -Recurse -Force | Sort-Object FullName)) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "directory source contains a reparse point: $($item.FullName)" }
        $relative = $item.FullName.Substring($base.Length).TrimStart('\').Replace('\','/')
        if ($relative -eq '.cargo-checksum.json') { continue }
        $files[$relative] = Get-Sha256 $item.FullName
    }
    $checksum = [ordered]@{ files = $files; package = $PackageChecksum }
    [IO.File]::WriteAllText((Join-Path $base '.cargo-checksum.json'), ($checksum | ConvertTo-Json -Compress -Depth 4), [Text.UTF8Encoding]::new($false))
}

function Copy-FirstPartyApi([string]$Source, [string]$Destination, [string]$Manifest) {
    if (-not (Test-Path -LiteralPath (Join-Path $Source 'src') -PathType Container)) { throw "first-party source is incomplete: $Source" }
    [IO.Directory]::CreateDirectory($Destination) | Out-Null
    Copy-Item -LiteralPath (Join-Path $Source 'src') -Destination (Join-Path $Destination 'src') -Recurse -Force
    [IO.File]::WriteAllText((Join-Path $Destination 'Cargo.toml'), $Manifest.TrimStart() + "`n", [Text.UTF8Encoding]::new($false))
    Write-DirectoryChecksum $Destination $null
}

$apiSource = (Resolve-Path (Join-Path $repo 'crates\explorer-extension-api')).Path
$uiApiSource = (Resolve-Path (Join-Path $repo 'crates\explorer-extension-ui-api')).Path
$apiManifest = Get-Content -LiteralPath (Join-Path $apiSource 'Cargo.toml') -Raw -Encoding UTF8
$uiApiManifest = Get-Content -LiteralPath (Join-Path $uiApiSource 'Cargo.toml') -Raw -Encoding UTF8
if ($apiManifest -notmatch '(?m)^name\s*=\s*"explorer-extension-api"\s*$' -or $apiManifest -notmatch '(?m)^version\s*=\s*"1\.2\.0"\s*$' -or $uiApiManifest -notmatch '(?m)^name\s*=\s*"explorer-extension-ui-api"\s*$' -or $uiApiManifest -notmatch '(?m)^version\s*=\s*"1\.2\.0"\s*$') {
    throw 'the first-party public API sources must remain exactly version 1.2.0'
}

$directLock = Join-Path $plugin 'Cargo.lock'
$lockPaths = if (Test-Path -LiteralPath $directLock -PathType Leaf) {
    @((Get-Item -LiteralPath $directLock))
} else {
    @(Get-ChildItem -LiteralPath $plugin -Filter Cargo.lock -File -Recurse -Force | Where-Object { $_.FullName -notmatch '[\\/]target[\\/]' } | Sort-Object FullName)
}
if ($lockPaths.Count -eq 0) { throw 'the plugin Cargo.lock is required for local Cargo source bootstrap' }
$lockText = (($lockPaths | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 }) -join "`n") + "`n" + (Get-Content -LiteralPath $toolingLockPath -Raw -Encoding UTF8)
$lockIdentity = ($lockPaths | ForEach-Object { "$($_.FullName.Substring($plugin.Length).TrimStart('\').Replace('\','/'))`t$(Get-Sha256 $_.FullName)" }) -join "`n"
$lockHashAlgorithm = [Security.Cryptography.SHA256]::Create()
try { $lockHash = ([BitConverter]::ToString($lockHashAlgorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($lockIdentity + "`n")))).Replace('-','').ToLowerInvariant() } finally { $lockHashAlgorithm.Dispose() }
$toolingLockHash = Get-Sha256 $toolingLockPath
$materialHashInput = "bootstrap-v2`n$lockHash`n$toolingLockHash`n$(Get-TreeDigest $apiSource)`n$(Get-TreeDigest $uiApiSource)`n"
$materialHashBytes = [Text.Encoding]::UTF8.GetBytes($materialHashInput)
$materialHashAlgorithm = [Security.Cryptography.SHA256]::Create()
try { $materialHash = ([BitConverter]::ToString($materialHashAlgorithm.ComputeHash($materialHashBytes))).Replace('-','').ToLowerInvariant() } finally { $materialHashAlgorithm.Dispose() }

$cacheRoot = Join-Path $repo '.cache\cargo-directory-sources'
$sourceRoot = Join-Path $cacheRoot $materialHash
$packageRoot = Join-Path $sourceRoot 'packages'
$configPath = Join-Path $sourceRoot 'config.toml'
$markerPath = Join-Path $sourceRoot 'complete.json'
if ((Test-Path -LiteralPath $markerPath -PathType Leaf) -and (Test-Path -LiteralPath $configPath -PathType Leaf) -and (Test-Path -LiteralPath (Join-Path $packageRoot 'explorer-extension-api-1.2.0\.cargo-checksum.json') -PathType Leaf) -and (Test-Path -LiteralPath (Join-Path $packageRoot 'explorer-extension-ui-api-1.2.0\.cargo-checksum.json') -PathType Leaf)) {
    Write-Output $configPath
    return
}

$registryCache = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) '.cargo\registry\cache'
if (-not (Test-Path -LiteralPath $registryCache -PathType Container)) { throw 'the local Cargo registry cache is unavailable; install the locked crates before running this offline bootstrap' }

$stage = Join-Path $cacheRoot ('.stage-' + [guid]::NewGuid().ToString('N'))
try {
    [IO.Directory]::CreateDirectory((Join-Path $stage 'packages')) | Out-Null
    $firstPartyNames = @('explorer-extension-api','explorer-extension-ui-api')
    foreach ($package in Get-LockedRegistryPackages $lockText) {
        $packageDirectoryName = "$($package.Name)-$($package.Version)"
        $destination = Join-Path (Join-Path $stage 'packages') $packageDirectoryName
        if ($package.Name -in $firstPartyNames) {
            if ($package.Checksum) { throw "first-party package unexpectedly has a registry checksum: $($package.Name)" }
            continue
        }
        if (-not $package.Checksum) { throw "locked registry package has no checksum: $($package.Name) $($package.Version)" }
        $candidateName = "$($package.Name)-$($package.Version).crate"
        $matches = @(Get-ChildItem -LiteralPath $registryCache -File -Recurse -Filter $candidateName -ErrorAction SilentlyContinue | Where-Object { (Get-Sha256 $_.FullName) -eq $package.Checksum })
        if ($matches.Count -eq 0) { throw "the locked registry crate is missing or fails checksum verification: $candidateName" }
        Expand-VerifiedCrate $matches[0].FullName $package.Checksum $packageDirectoryName (Join-Path $stage 'packages')
        Write-DirectoryChecksum $destination $package.Checksum
    }

    Copy-FirstPartyApi $apiSource (Join-Path (Join-Path $stage 'packages') 'explorer-extension-api-1.2.0') @'
[package]
name = "explorer-extension-api"
version = "1.2.0"
description = "Public non-UI extension API boundary for SuperExplorer."
edition = "2024"
rust-version = "1.97.1"
license = "LicenseRef-SuperExplorer-Proprietary"
repository = "https://github.com/damody/file_explorer"
publish = false

[dependencies]
abi_stable = { version = "=0.11.3", default-features = false }
'@
    Copy-FirstPartyApi $uiApiSource (Join-Path (Join-Path $stage 'packages') 'explorer-extension-ui-api-1.2.0') @'
[package]
name = "explorer-extension-ui-api"
version = "1.2.0"
description = "Public GPUI-facing extension API boundary for SuperExplorer."
edition = "2024"
rust-version = "1.97.1"
license = "LicenseRef-SuperExplorer-Proprietary"
repository = "https://github.com/damody/file_explorer"
publish = false

[dependencies]
explorer-extension-api = "=1.2.0"
'@

    $configDirectory = ([IO.Path]::GetFullPath($packageRoot)).Replace('\','/')
    $config = "[source.crates-io]`nreplace-with = `"superexplorer-local-directory`"`n`n[source.superexplorer-local-directory]`ndirectory = `"$configDirectory`"`n"
    [IO.File]::WriteAllText((Join-Path $stage 'config.toml'), $config, [Text.UTF8Encoding]::new($false))
    $marker = [ordered]@{ schema_version = 1; plugin_lock_sha256 = $lockHash; tooling_lock_sha256 = $toolingLockHash; source_sha256 = $materialHash }
    [IO.File]::WriteAllText((Join-Path $stage 'complete.json'), (($marker | ConvertTo-Json -Compress) + "`n"), [Text.UTF8Encoding]::new($false))

    [IO.Directory]::CreateDirectory($cacheRoot) | Out-Null
    if (-not (Test-Path -LiteralPath $sourceRoot)) {
        Move-Item -LiteralPath $stage -Destination $sourceRoot -ErrorAction Stop
        $stage = $null
    }
} finally {
    if ($stage -and (Test-Path -LiteralPath $stage)) { Remove-Item -LiteralPath $stage -Recurse -Force }
}

if (-not (Test-Path -LiteralPath $configPath -PathType Leaf)) { throw 'local Cargo directory source bootstrap did not publish its isolated config' }
Write-Output $configPath
