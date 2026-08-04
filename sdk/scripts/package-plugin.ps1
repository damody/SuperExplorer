[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = (Resolve-Path -LiteralPath $PluginRoot).Path
$manifestPath = Join-Path $root 'plugin-project.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'plugin-project.json required' }
$sdkLock = Get-Content -LiteralPath (Join-Path $sdk 'sdk-lock.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$localCargoConfig = (& (Join-Path $PSHOME 'powershell.exe') -NoProfile -File (Join-Path $PSScriptRoot 'prepare-local-cargo-source.ps1') -PluginRoot $root | Select-Object -Last 1)
if (-not $localCargoConfig -or -not (Test-Path -LiteralPath $localCargoConfig -PathType Leaf)) { throw 'local exact-version Cargo source bootstrap failed' }
$localCargoHome = Split-Path -Parent $localCargoConfig
Import-Module (Join-Path $PSScriptRoot 'sealed-cargo-authority.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'canonical-store-zip.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'consumer-snapshot.psm1') -Force
$cargoAuthority = $null
$cargoPath = $null
$cargoHash = $null
$cargoDirectory = $null
$privateSnapshotRoot = $null
$stageRoot = $null
$templateTarget = $null
$savedTemplateEnvironment = @{}
$templateMaterialization = $null
try {
    # Start the outer cleanup boundary before sealed authority or private
    # inputs exist. No pre-publication error may leave either behind.
    $cargoAuthority = New-SealedCargoAuthority $sdkLock.toolchain
    $cargoPath = $cargoAuthority.Path
    $cargoHash = $cargoAuthority.Sha256
    $cargoDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($cargoPath))
    $rustcDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($cargoAuthority.RustcPath))
$forbiddenOverrides = @([Environment]::GetEnvironmentVariables('Process').GetEnumerator() | Where-Object { $_.Value -and ([string]$_.Key -match '^(RUSTC|RUSTC_BOOTSTRAP|RUSTFLAGS|RUSTDOCFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_ENCODED_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_HOME|CARGO_BUILD_RUST(FLAGS|C|C_WRAPPER|C_WORKSPACE_WRAPPER)?|CARGO_PROFILE_|CARGO_TARGET_.*_(RUSTFLAGS|LINKER|RUNNER)|RUSTUP_(TOOLCHAIN|HOME|DIST_SERVER|UPDATE_ROOT)|SUPEREXPLORER_TRUSTED_(CARGO|RUSTC)(_SHA256)?|CC|CXX|AR|LINKER|[A-Z0-9_]+_(CC|CXX|AR|LINKER))$') })
if ($forbiddenOverrides.Count -gt 0) { throw "fingerprint-affecting package environment override is forbidden: $($forbiddenOverrides[0].Key)" }
$localCargoRegistry = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) '.cargo\registry'
if (-not (Test-Path -LiteralPath (Join-Path $localCargoRegistry 'cache') -PathType Container) -or -not (Test-Path -LiteralPath (Join-Path $localCargoRegistry 'index') -PathType Container)) {
    throw 'the local Cargo registry cache is unavailable; install the locked crates before running this offline package build'
}
$templateTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-package-template-target-' + [guid]::NewGuid().ToString('N'))
foreach ($name in @('CARGO_HOME','CARGO_TARGET_DIR','RUSTC','PATH','SUPEREXPLORER_TRUSTED_CARGO','SUPEREXPLORER_TRUSTED_CARGO_SHA256','SUPEREXPLORER_TRUSTED_RUSTC','SUPEREXPLORER_TRUSTED_RUSTC_SHA256')) { $savedTemplateEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
New-Item -ItemType Directory -Path $templateTarget -Force | Out-Null
$env:CARGO_HOME = $localCargoHome
$env:CARGO_TARGET_DIR = $templateTarget
$env:RUSTC = $cargoAuthority.RustcPath
$env:PATH = "$cargoDirectory;$rustcDirectory;$($savedTemplateEnvironment['PATH'])"
$env:SUPEREXPLORER_TRUSTED_CARGO = $cargoPath
$env:SUPEREXPLORER_TRUSTED_CARGO_SHA256 = $cargoHash
$env:SUPEREXPLORER_TRUSTED_RUSTC = $cargoAuthority.RustcPath
$env:SUPEREXPLORER_TRUSTED_RUSTC_SHA256 = $cargoAuthority.RustcSha256
$buildRoot = Join-Path $root ("target\superexplorer\$($sdkLock.bundle_id)")
$buildReportPath = Join-Path $buildRoot 'reports\build.json'
$buildCompletePath = Join-Path $buildRoot 'reports\build.complete.json'
$validationReportPath = Join-Path $buildRoot 'reports\validation.json'
$dllPath = Join-Path $buildRoot 'build\plugin.dll'
if (-not (Test-Path -LiteralPath $buildReportPath) -or -not (Test-Path -LiteralPath $buildCompletePath) -or -not (Test-Path -LiteralPath $validationReportPath) -or -not (Test-Path -LiteralPath $dllPath)) {
    throw 'a complete marked build, sealed validation report, and plugin DLL are required; packaging never rebuilds automatically'
}
foreach ($path in @($manifestPath, (Join-Path $root 'Cargo.lock'), $buildReportPath, $buildCompletePath, $validationReportPath, $dllPath)) {
    if ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'package input is a symlink or reparse point' }
}
$privateSnapshotRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-package-inputs-' + [guid]::NewGuid().ToString('N'))
$liveInputs = [ordered]@{ 'plugin-project.json' = $manifestPath; 'Cargo.lock' = (Join-Path $root 'Cargo.lock'); 'reports/build.json' = $buildReportPath; 'reports/build.complete.json' = $buildCompletePath; 'reports/validation.json' = $validationReportPath; 'plugin/plugin.dll' = $dllPath }
$liveInputHashes = @{}
$consumerTreeDigest = Get-BoundedConsumerTreeDigest $root
Copy-BoundedConsumerSnapshot $root $privateSnapshotRoot | Out-Null
if ((Get-BoundedConsumerTreeDigest $privateSnapshotRoot) -ne $consumerTreeDigest) { throw 'private package source snapshot is not one complete consumer generation' }
foreach ($name in $liveInputs.Keys) {
    $live = $liveInputs[$name]
    $metadata = Get-Item -LiteralPath $live -Force
    if ($metadata.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'package input is a symlink or reparse point' }
    $snapshot = Join-Path $privateSnapshotRoot $name.Replace('/','\')
    New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($snapshot))) -Force | Out-Null
    Copy-Item -LiteralPath $live -Destination $snapshot -Force
    $liveInputHashes[$name] = (Get-FileHash -LiteralPath $live -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($liveInputHashes[$name] -ne (Get-FileHash -LiteralPath $snapshot -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'package input changed while creating the single private snapshot' }
}
$privateManifestPath = Join-Path $privateSnapshotRoot 'plugin-project.json'
$privateLockPath = Join-Path $privateSnapshotRoot 'Cargo.lock'
$privateBuildReportPath = Join-Path $privateSnapshotRoot 'reports\build.json'
$privateBuildCompletePath = Join-Path $privateSnapshotRoot 'reports\build.complete.json'
$privateValidationReportPath = Join-Path $privateSnapshotRoot 'reports\validation.json'
$privateDllPath = Join-Path $privateSnapshotRoot 'plugin\plugin.dll'
$templateJson = & $cargoPath run --release --locked --offline --config $localCargoConfig --manifest-path (Join-Path $sdk 'tools\plugin-tooling\Cargo.toml') -- materialize-folder-size-template $privateSnapshotRoot ([string]$sdkLock.bundle_id) ([string]$sdkLock.build_policy.abi_schema_version)
if ($LASTEXITCODE -ne 0) { throw 'private plugin template materialization failed before packaging' }
$templateMaterialization = ($templateJson -join "`n") | ConvertFrom-Json
if ($templateMaterialization.template_manifest_sha256 -notmatch '^[0-9a-f]{64}$' -or $templateMaterialization.resolved_manifest_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'template materialization emitted invalid digests' }
$manifest = Get-Content -LiteralPath $privateManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
$buildReport = Get-Content -LiteralPath $privateBuildReportPath -Raw -Encoding UTF8 | ConvertFrom-Json
$buildComplete = Get-Content -LiteralPath $privateBuildCompletePath -Raw -Encoding UTF8 | ConvertFrom-Json
$manifestHash = (Get-FileHash -LiteralPath $privateManifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
$lockHash = (Get-FileHash -LiteralPath $privateLockPath -Algorithm SHA256).Hash.ToLowerInvariant()
$dllHash = (Get-FileHash -LiteralPath $privateDllPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($buildReport.bundle_id -ne $manifest.sdk.bundle_id -or $buildReport.inputs.manifest_sha256 -ne $manifestHash -or $buildReport.inputs.resolved_manifest_sha256 -ne $manifestHash -or $buildReport.inputs.template_manifest_sha256 -ne [string]$templateMaterialization.template_manifest_sha256 -or $buildReport.inputs.cargo_lock_sha256 -ne $lockHash -or $buildReport.plugin_dll.sha256 -ne $dllHash) { throw 'private package snapshot does not match the validated build' }
if ($buildComplete.schema_version -ne 1 -or $buildComplete.bundle_id -ne $manifest.sdk.bundle_id -or $buildComplete.build_report_sha256 -ne (Get-FileHash -LiteralPath $privateBuildReportPath -Algorithm SHA256).Hash.ToLowerInvariant() -or $buildComplete.validation_report_sha256 -ne (Get-FileHash -LiteralPath $privateValidationReportPath -Algorithm SHA256).Hash.ToLowerInvariant() -or $buildComplete.consumer_tree_sha256 -ne $buildReport.inputs.consumer_tree_sha256) { throw 'build generation completion marker does not authenticate the immutable build and validation reports' }
if ([string]$buildReport.inputs.consumer_tree_sha256 -notmatch '^[0-9a-f]{64}$' -or $buildReport.inputs.consumer_tree_sha256 -ne $consumerTreeDigest) { throw 'live bounded consumer tree does not match the build snapshot; rebuild before packaging' }

Add-Type -AssemblyName System.IO.Compression
$dist = Join-Path $root 'dist'
function Assert-NoReparseAncestors([string]$Path, [string]$Purpose) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($true) {
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "$Purpose contains a symlink, junction, or reparse point" }
        }
        $parent = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($cursor))
        if (-not $parent -or $parent -eq $cursor) { break }
        $cursor = $parent
    }
}
function Remove-BoundedPackageAttempt([string]$Path) {
    $attemptInfo = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($attemptInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'stale package attempt is a symlink, junction, or reparse point' }
    $files = @(Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction Stop)
    if ($files.Count -gt 10000 -or (($files | Measure-Object -Property Length -Sum).Sum -gt 536870912)) { throw 'stale package attempt exceeds the bounded recovery limit' }
    foreach ($item in @($files) + @(Get-ChildItem -LiteralPath $Path -Directory -Recurse -Force -ErrorAction Stop)) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'stale package attempt contains a symlink, junction, or reparse point' }
    }
    Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop
}
function Repair-StalePackageAttempts([string]$PublicationDirectory, [string]$Package, [string]$Checksum, [string]$Report) {
    foreach ($attempt in @(Get-ChildItem -LiteralPath $PublicationDirectory -Directory -Force -ErrorAction Stop | Where-Object { $_.Name -match '^\.stage-[0-9a-f]{32}$' })) {
        Remove-BoundedPackageAttempt $attempt.FullName
    }
    # The .sepack is the complete-publication marker. A process killed after a
    # sidecar move leaves no package marker; remove those stale sidecars before
    # the next attempt instead of treating them as an immutable publication.
    if (-not (Test-Path -LiteralPath $Package)) {
        foreach ($sidecar in @($Checksum,$Report)) {
            if (Test-Path -LiteralPath $sidecar) {
                if ((Get-Item -LiteralPath $sidecar -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'stale package sidecar is a symlink or reparse point' }
                Remove-Item -LiteralPath $sidecar -Force -ErrorAction Stop
            }
        }
    }
}
Assert-NoReparseAncestors $dist 'package publication directory'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
Assert-NoReparseAncestors $dist 'package publication directory'
$baseName = "$($manifest.package.id)-$($manifest.package.version)-$($manifest.sdk.bundle_id)"
if ($baseName -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') { throw 'package publication name is not a safe file name' }
$finalPackage = Join-Path $dist "$baseName.sepack"
$finalHash = "$finalPackage.sha256"
$finalReport = Join-Path $dist "$baseName.package-report.json"
Repair-StalePackageAttempts $dist $finalPackage $finalHash $finalReport
$stageRoot = Join-Path $dist ('.stage-' + [guid]::NewGuid().ToString('N'))
$stage = Join-Path $stageRoot "$baseName.sepack"
$stageHash = Join-Path $stageRoot "$baseName.sepack.sha256"
$stageReport = Join-Path $stageRoot "$baseName.package-report.json"
try {
    Assert-NoReparseAncestors $stageRoot 'package staging directory'
    New-Item -ItemType Directory -Path $stageRoot -ErrorAction Stop | Out-Null
    Assert-NoReparseAncestors $stageRoot 'package staging directory'
    # Core owns the entire runtime inventory (manifest, DLL, private-dependency
    # licenses, and provenance notice). The wrapper supplies a newly absent
    # private output directory and never synthesizes runtime entries itself.
    $coreStageDirectory = Join-Path $stageRoot 'core-stage'
    $archiveInputRoot = Join-Path $stageRoot 'archive-inputs'
    if (Test-Path -LiteralPath $coreStageDirectory) { throw 'private core package stage already exists' }
    $temporaryTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-package-synthesis-target-' + [guid]::NewGuid().ToString('N'))
    $savedEnvironment = @{}
    foreach ($name in @('CARGO_HOME','CARGO_TARGET_DIR','RUSTC','RUSTUP_TOOLCHAIN','PATH','SUPEREXPLORER_TRUSTED_CARGO','SUPEREXPLORER_TRUSTED_CARGO_SHA256','SUPEREXPLORER_TRUSTED_RUSTC','SUPEREXPLORER_TRUSTED_RUSTC_SHA256')) { $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    try {
        New-Item -ItemType Directory -Path $temporaryTarget -Force | Out-Null
        $env:CARGO_TARGET_DIR = $temporaryTarget
        $env:RUSTC = $cargoAuthority.RustcPath
        $env:PATH = "$cargoDirectory;$rustcDirectory;$($savedEnvironment['PATH'])"
        $env:SUPEREXPLORER_TRUSTED_CARGO = $cargoPath
        $env:SUPEREXPLORER_TRUSTED_CARGO_SHA256 = $cargoHash
        $env:SUPEREXPLORER_TRUSTED_RUSTC = $cargoAuthority.RustcPath
        $env:SUPEREXPLORER_TRUSTED_RUSTC_SHA256 = $cargoAuthority.RustcSha256
        Push-Location $sdk
        try {
            & $cargoPath run --release --locked --offline --config $localCargoConfig --manifest-path (Join-Path $sdk 'tools\plugin-tooling\Cargo.toml') -- stage-package $privateSnapshotRoot $privateDllPath ([IO.Path]::GetFullPath($coreStageDirectory))
        } finally { Pop-Location }
        if ($LASTEXITCODE -ne 0) { throw 'production PackageManifestV1 staging failed' }
    } finally {
        foreach ($name in $savedEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
        foreach ($temporary in @($temporaryTarget)) { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force } }
    }
    if (-not (Test-Path -LiteralPath $coreStageDirectory -PathType Container)) { throw 'core package staging did not create its requested private output directory' }
    # Bounded no-reparse traversal converts the core output to an immutable
    # wrapper-owned archive input tree before any ZIP hashing or publication.
    Copy-BoundedConsumerSnapshot $coreStageDirectory $archiveInputRoot -IncludeBuildOutputs | Out-Null
    $runtimeManifestPath = Join-Path $archiveInputRoot 'manifest.json'
    if (-not (Test-Path -LiteralPath $runtimeManifestPath -PathType Leaf)) { throw 'core package staging omitted manifest.json' }
    $runtimeManifest = Get-Content -LiteralPath $runtimeManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($runtimeManifest.manifest_version -ne 1 -or $runtimeManifest.signature.kind -ne 'unsigned' -or @($runtimeManifest.payloads).Count -eq 0) { throw 'core PackageManifestV1 staging emitted an invalid runtime manifest' }
    $entries = [ordered]@{ 'manifest.json' = $runtimeManifestPath }
    $seenPayloadPaths = @{}
    foreach ($payload in @($runtimeManifest.payloads)) {
        $name = [string]$payload.path
        if ($name -notmatch '^[A-Za-z0-9][A-Za-z0-9._/-]*$' -or $name.Contains('..') -or $name.Contains('//') -or $seenPayloadPaths.ContainsKey($name.ToLowerInvariant())) { throw 'core PackageManifestV1 staging emitted an unsafe or colliding payload path' }
        $seenPayloadPaths[$name.ToLowerInvariant()] = $true
        $payloadPath = Join-Path $archiveInputRoot $name.Replace('/', '\')
        if (-not (Test-Path -LiteralPath $payloadPath -PathType Leaf) -or (Get-Item -LiteralPath $payloadPath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'core PackageManifestV1 inventory payload is absent or unsafe' }
        if ([Int64]$payload.size -ne (Get-Item -LiteralPath $payloadPath).Length -or [string]$payload.sha256 -ne (Get-FileHash -LiteralPath $payloadPath -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'core PackageManifestV1 inventory hash or size differs from its staged file' }
        $entries[$name] = $payloadPath
    }
    $stagedFiles = @(Get-ChildItem -LiteralPath $archiveInputRoot -File -Recurse -Force | ForEach-Object { $_.FullName.Substring($archiveInputRoot.Length).TrimStart([char]92, [char]47).Replace('\', '/') } | Sort-Object -CaseSensitive)
    $expectedFiles = @($entries.Keys | Sort-Object -CaseSensitive)
    if (($stagedFiles -join "`n") -ne ($expectedFiles -join "`n")) { throw 'core package staging directory differs from the exact runtime manifest inventory' }
    $orderedNames = $expectedFiles
    $buildInputSnapshot = $privateManifestPath
    if ($env:SUPEREXPLORER_PACKAGE_TEST_MUTATE_AFTER_SNAPSHOT -eq '1') {
        [IO.File]::AppendAllText($manifestPath, "`n", [Text.UTF8Encoding]::new($false))
    }
    $stagedDllHash = (Get-FileHash -LiteralPath $entries['plugin/plugin.dll'] -Algorithm SHA256).Hash.ToLowerInvariant()
    $stagedBuildManifestHash = (Get-FileHash -LiteralPath $buildInputSnapshot -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($buildReport.inputs.manifest_sha256 -ne $stagedBuildManifestHash -or $buildReport.plugin_dll.sha256 -ne $stagedDllHash -or $stagedDllHash -ne $dllHash) {
        throw 'private package snapshot no longer matches the validated build report'
    }
    foreach ($name in $liveInputs.Keys) { if ((Get-FileHash -LiteralPath $liveInputs[$name] -Algorithm SHA256).Hash.ToLowerInvariant() -ne $liveInputHashes[$name]) { throw 'package input changed after the single private snapshot was validated' } }
    if ((Get-BoundedConsumerTreeDigest $root) -ne $consumerTreeDigest) { throw 'consumer source changed after the bounded private package snapshot was validated' }
    # .NET ZipArchive's `NoCompression` still emits deflate (method 8) on
    # supported runtimes. The production importer intentionally accepts only
    # method 0, so use the canonical writer rather than weakening the importer.
    Write-CanonicalStoreOnlyZip $stage $entries
    Assert-CanonicalStoreOnlyZip $stage ([string[]]$orderedNames)

    $seen = @{}
    $readStream = [IO.File]::OpenRead($stage)
    try {
        $archive = [IO.Compression.ZipArchive]::new($readStream, [IO.Compression.ZipArchiveMode]::Read, $false, [Text.Encoding]::UTF8)
        try {
            foreach ($entry in $archive.Entries) {
                $folded = $entry.FullName.ToLowerInvariant()
                if ($seen.ContainsKey($folded) -or $entry.FullName -notin $orderedNames -or $entry.FullName.Contains('..') -or $entry.Length -gt (512MB)) {
                    throw 'package archive verification rejected an entry'
                }
                $seen[$folded] = $true
                $sourceHash = (Get-FileHash -LiteralPath $entries[$entry.FullName] -Algorithm SHA256).Hash.ToLowerInvariant()
                $entryStream = $entry.Open()
                try {
                    $sha = [Security.Cryptography.SHA256]::Create()
                    try { $entryHash = ([BitConverter]::ToString($sha.ComputeHash($entryStream))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
                } finally { $entryStream.Dispose() }
                if ($entryHash -ne $sourceHash) { throw 'package archive payload hash mismatch' }
            }
            if ($seen.Count -ne $orderedNames.Count) { throw 'package archive entry count mismatch' }
        } finally { $archive.Dispose() }
    } finally { $readStream.Dispose() }

    $packageHash = (Get-FileHash -LiteralPath $stage -Algorithm SHA256).Hash.ToLowerInvariant()
    $report = [ordered]@{
        schema_version = 1; package_id = [string]$manifest.package.id; version = [string]$manifest.package.version
        bundle_id = [string]$manifest.sdk.bundle_id; package = "$baseName.sepack"; sha256 = $packageHash
        template_manifest_sha256 = [string]$templateMaterialization.template_manifest_sha256
        resolved_manifest_sha256 = [string]$templateMaterialization.resolved_manifest_sha256
        entries = @($orderedNames | ForEach-Object { [ordered]@{ path = $_; size = (Get-Item -LiteralPath $entries[$_]).Length; sha256 = (Get-FileHash -LiteralPath $entries[$_] -Algorithm SHA256).Hash.ToLowerInvariant() } })
    }
    [IO.File]::WriteAllText($stageHash, "$packageHash  $baseName.sepack`n", [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($stageReport, (($report | ConvertTo-Json -Depth 8) + "`n"), [Text.UTF8Encoding]::new($false))
    $finalPaths = @($finalPackage, $finalHash, $finalReport)
    Assert-NoReparseAncestors $dist 'package publication directory'
    $existing = @($finalPaths | Where-Object { Test-Path -LiteralPath $_ })
    if ($existing.Count -ne 0 -and $existing.Count -ne $finalPaths.Count) {
        throw 'an incomplete package publication already exists; refusing to repair or overwrite it'
    }
    if ($existing.Count -eq $finalPaths.Count) {
        $existingHash = (Get-FileHash -LiteralPath $finalPackage -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($existingHash -ne $packageHash) { throw 'a different package already exists; refusing to overwrite it' }
        foreach ($pair in @(@($finalHash, $stageHash), @($finalReport, $stageReport))) {
            if ((Get-FileHash -LiteralPath $pair[0] -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $pair[1] -Algorithm SHA256).Hash) {
                throw 'published package sidecar differs from the staged immutable publication'
            }
        }
    } else {
        $published = @()
        try {
            # The package is the final complete-publication marker; roll back sidecars on failure.
            Move-Item -LiteralPath $stageHash -Destination $finalHash -ErrorAction Stop
            $published += $finalHash
            if ($env:SUPEREXPLORER_PACKAGE_TEST_FAIL_AFTER_SIDECAR -eq '1') { throw 'injected package publication failure' }
            if ($env:SUPEREXPLORER_PACKAGE_TEST_WAIT_AFTER_SIDECAR) {
                [IO.File]::WriteAllText($env:SUPEREXPLORER_PACKAGE_TEST_WAIT_AFTER_SIDECAR, 'ready', [Text.UTF8Encoding]::new($false))
                while ($true) { Start-Sleep -Seconds 1 }
            }
            Move-Item -LiteralPath $stageReport -Destination $finalReport -ErrorAction Stop
            $published += $finalReport
            Move-Item -LiteralPath $stage -Destination $finalPackage -ErrorAction Stop
            $published += $finalPackage
        } catch {
            foreach ($path in $published) {
                if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
            }
            throw
        }
    }
    Write-Output $finalPackage
} finally {
    if ($stageRoot -and (Test-Path -LiteralPath $stageRoot)) { Remove-Item -LiteralPath $stageRoot -Recurse -Force }
}
} finally {
    if ($privateSnapshotRoot -and (Test-Path -LiteralPath $privateSnapshotRoot)) { Remove-Item -LiteralPath $privateSnapshotRoot -Recurse -Force }
    foreach ($name in $savedTemplateEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $savedTemplateEnvironment[$name], 'Process') }
    foreach ($temporary in @($templateTarget)) { if ($temporary -and (Test-Path -LiteralPath $temporary)) { Remove-Item -LiteralPath $temporary -Recurse -Force } }
    Remove-SealedCargoAuthority $cargoAuthority
}
