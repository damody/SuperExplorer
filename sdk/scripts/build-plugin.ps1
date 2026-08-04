[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = (Resolve-Path -LiteralPath $PluginRoot).Path
$manifestPath = Join-Path $root 'plugin-project.json'
if (-not (Test-Path -LiteralPath (Join-Path $root 'Cargo.toml')) -or -not (Test-Path -LiteralPath $manifestPath)) {
    throw 'PluginRoot must contain Cargo.toml and plugin-project.json'
}

$sdkLock = Get-Content -LiteralPath (Join-Path $sdk 'sdk-lock.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$localCargoConfig = (& (Join-Path $PSHOME 'powershell.exe') -NoProfile -File (Join-Path $PSScriptRoot 'prepare-local-cargo-source.ps1') -PluginRoot $root | Select-Object -Last 1)
if (-not $localCargoConfig -or -not (Test-Path -LiteralPath $localCargoConfig -PathType Leaf)) { throw 'local exact-version Cargo source bootstrap failed' }
$localCargoHome = Split-Path -Parent $localCargoConfig
Import-Module (Join-Path $PSScriptRoot 'sealed-cargo-authority.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'consumer-snapshot.psm1') -Force
$targetTriple = 'x86_64-pc-windows-msvc'
$manifest = $null
$crateFile = $null
$temporaryTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-target-' + [guid]::NewGuid().ToString('N'))
$outputRoot = Join-Path $root ('target\superexplorer\' + [string]$sdkLock.bundle_id)
$stage = $null
$dangerous = @('RUSTC','RUSTC_BOOTSTRAP','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTFLAGS','RUSTDOCFLAGS','CARGO_BUILD_RUSTFLAGS','CARGO_BUILD_RUSTC','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER','CARGO_ENCODED_RUSTFLAGS','CARGO_INCREMENTAL','CARGO_TARGET_DIR','CARGO_HOME','RUSTUP_TOOLCHAIN','RUSTUP_HOME','CC','CXX','AR','LINKER','SUPEREXPLORER_TRUSTED_CARGO','SUPEREXPLORER_TRUSTED_CARGO_SHA256','SUPEREXPLORER_TRUSTED_RUSTC','SUPEREXPLORER_TRUSTED_RUSTC_SHA256')
$forbiddenOverrides = @([Environment]::GetEnvironmentVariables('Process').GetEnumerator() | Where-Object {
    $_.Value -and ([string]$_.Key -match '^(RUSTC|RUSTC_BOOTSTRAP|RUSTFLAGS|RUSTDOCFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_ENCODED_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_BUILD_RUST(FLAGS|C|C_WRAPPER|C_WORKSPACE_WRAPPER)?|CARGO_PROFILE_|CARGO_TARGET_.*_(RUSTFLAGS|LINKER|RUNNER)|RUSTUP_(TOOLCHAIN|HOME|DIST_SERVER|UPDATE_ROOT)|SUPEREXPLORER_TRUSTED_(CARGO|RUSTC)(_SHA256)?|CC|CXX|AR|LINKER|[A-Z0-9_]+_(CC|CXX|AR|LINKER))$')
})
if ($forbiddenOverrides.Count -gt 0) { throw "fingerprint-affecting build environment override is forbidden: $($forbiddenOverrides[0].Key)" }
$localCargoRegistry = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) '.cargo\registry'
if (-not (Test-Path -LiteralPath (Join-Path $localCargoRegistry 'cache') -PathType Container) -or -not (Test-Path -LiteralPath (Join-Path $localCargoRegistry 'index') -PathType Container)) {
    throw 'the local Cargo registry cache is unavailable; install the locked crates before running this offline build'
}
foreach ($configName in @('.cargo\config.toml','.cargo\config','rust-toolchain.toml','rust-toolchain')) {
    if (Test-Path -LiteralPath (Join-Path $root $configName)) { throw 'consumer Cargo config overrides are forbidden' }
}
# Cargo walks ancestor directories for .cargo configuration. Build a private,
# no-follow snapshot below the temporary hierarchy so neither a consumer nor an
# ancestor can supply compiler/linker authority after preflight.
$stagedRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-source-' + [guid]::NewGuid().ToString('N'))
function Get-ConsumerTreeDigest([string]$Base) {
    return Get-BoundedConsumerTreeDigest $Base
}
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
function Remove-BoundedBuildAttempt([string]$Path) {
    # A crashed producer can leave only SDK-owned attempt names behind. Bound
    # the traversal before removal so recovery cannot turn an output cleanup
    # into an unbounded delete.
    $attemptInfo = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($attemptInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'stale build attempt is a symlink, junction, or reparse point' }
    $files = @(Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction Stop)
    if ($files.Count -gt 10000 -or (($files | Measure-Object -Property Length -Sum).Sum -gt 536870912)) {
        throw 'stale build attempt exceeds the bounded recovery limit'
    }
    foreach ($item in @($files) + @(Get-ChildItem -LiteralPath $Path -Directory -Recurse -Force -ErrorAction Stop)) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'stale build attempt contains a symlink, junction, or reparse point' }
    }
    Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop
}
function Repair-IncompleteBuildPublication {
    if (-not (Test-Path -LiteralPath $outputRoot -PathType Container)) { return }
    Assert-NoReparseAncestors $outputRoot 'plugin build output root'
    foreach ($attempt in @(Get-ChildItem -LiteralPath $outputRoot -Directory -Force -ErrorAction Stop | Where-Object { $_.Name -match '^\.build-stage-[0-9a-f]{32}$' })) {
        Remove-BoundedBuildAttempt $attempt.FullName
    }
    $reports = Join-Path $outputRoot 'reports'
    $complete = Join-Path $reports 'build.complete.json'
    if (-not (Test-Path -LiteralPath $complete)) {
        # Build and report are intentionally invisible to downstream consumers
        # without this final marker. Remove a previous interrupted attempt while
        # retaining an independently published validation report.
        $partialBuild = Join-Path $outputRoot 'build'
        $partialReport = Join-Path $reports 'build.json'
        if (Test-Path -LiteralPath $partialBuild) { Remove-BoundedBuildAttempt $partialBuild }
        if (Test-Path -LiteralPath $partialReport) { Remove-Item -LiteralPath $partialReport -Force -ErrorAction Stop }
    }
}
$manifestInfo = Get-Item -LiteralPath $manifestPath -Force
if ($manifestInfo.PSIsContainer -or ($manifestInfo.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'plugin-project.json is a symlink, junction, or reparse point' }
if ($manifestInfo.Length -gt 1MB) { throw 'plugin-project.json exceeds the 1 MiB build manifest limit' }
$consumerTreeDigest = Get-ConsumerTreeDigest $root
$saved = @{}
foreach ($name in $dangerous) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$savedPath = [Environment]::GetEnvironmentVariable('PATH', 'Process')
$cargoAuthority = $null
$cargoPath = $null
$cargoHash = $null
$cargoDirectory = $null
$pushed = $false
$templateMaterialization = $null
try {
    # Once authority exists, every later operation is covered by this outer
    # cleanup boundary, including snapshot and publication failures.
    $cargoAuthority = New-SealedCargoAuthority $sdkLock.toolchain
    $cargoPath = $cargoAuthority.Path
    $cargoHash = $cargoAuthority.Sha256
    $cargoDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($cargoPath))
    $rustcDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($cargoAuthority.RustcPath))
    foreach ($name in $dangerous) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
    $env:CARGO_HOME = $localCargoHome
    $env:RUSTC = $cargoAuthority.RustcPath
    $env:SUPEREXPLORER_TRUSTED_CARGO = $cargoPath
    $env:SUPEREXPLORER_TRUSTED_CARGO_SHA256 = $cargoHash
    $env:SUPEREXPLORER_TRUSTED_RUSTC = $cargoAuthority.RustcPath
    $env:SUPEREXPLORER_TRUSTED_RUSTC_SHA256 = $cargoAuthority.RustcSha256
    $env:PATH = "$cargoDirectory;$rustcDirectory;$savedPath"
    $expectedOutputParent = Join-Path $root 'target\superexplorer'
    Assert-NoReparseAncestors $expectedOutputParent 'plugin build output parent'
    New-Item -ItemType Directory -Path $temporaryTarget,$expectedOutputParent -Force | Out-Null
    Assert-NoReparseAncestors $expectedOutputParent 'plugin build output parent'
    Repair-IncompleteBuildPublication
    Assert-NoReparseAncestors $outputRoot 'plugin build output root'
    $stage = Join-Path $outputRoot ('.build-stage-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stage -ErrorAction Stop | Out-Null
    Assert-NoReparseAncestors $stage 'plugin build staging directory'
    Copy-BoundedConsumerSnapshot $root $stagedRoot | Out-Null
    if ((Get-ConsumerTreeDigest $stagedRoot) -ne $consumerTreeDigest) { throw 'consumer source changed while creating the private no-reparse build snapshot' }
    if ($env:SUPEREXPLORER_BUILD_TEST_MUTATE_AFTER_SNAPSHOT -eq '1') {
        [IO.File]::AppendAllText($manifestPath, "`n", [Text.UTF8Encoding]::new($false))
    }
    foreach ($configName in @('.cargo\config.toml','.cargo\config','rust-toolchain.toml','rust-toolchain')) { if (Test-Path -LiteralPath (Join-Path $stagedRoot $configName)) { throw 'staged consumer Cargo or Rustup config overrides are forbidden' } }
    Push-Location $sdk
    $pushed = $true
    $templateJson = & $cargoPath run --release --locked --offline --config $localCargoConfig --manifest-path (Join-Path $sdk 'tools\plugin-tooling\Cargo.toml') -- materialize-folder-size-template $stagedRoot ([string]$sdkLock.bundle_id) ([string]$sdkLock.build_policy.abi_schema_version)
    if ($LASTEXITCODE -ne 0) { throw 'private plugin template materialization failed before build' }
    $templateMaterialization = ($templateJson -join "`n") | ConvertFrom-Json
    if ($templateMaterialization.template_manifest_sha256 -notmatch '^[0-9a-f]{64}$' -or $templateMaterialization.resolved_manifest_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'template materialization emitted invalid digests' }
    Pop-Location
    $pushed = $false
    foreach ($name in $dangerous) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
    & (Join-Path $PSHOME 'powershell.exe') -NoProfile -File (Join-Path $PSScriptRoot 'validate-plugin.ps1') -PluginRoot $stagedRoot -TemplateManifestSha256 ([string]$templateMaterialization.template_manifest_sha256) -ExpectedResolvedManifestSha256 ([string]$templateMaterialization.resolved_manifest_sha256) | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'plugin validation failed before build' }
    # Only the bounded, no-reparse snapshot reaches PowerShell's JSON parser.
    # The core validator has already accepted the exact schema, so later path
    # derivation cannot use untrusted live payload paths.
    $stagedManifestPath = Join-Path $stagedRoot 'plugin-project.json'
    $stagedManifestInfo = Get-Item -LiteralPath $stagedManifestPath -Force
    if ($stagedManifestInfo.Length -gt 1MB) { throw 'plugin-project.json exceeds the 1 MiB build manifest limit' }
    $manifest = Get-Content -LiteralPath $stagedManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $crateFile = ([string]$manifest.rust.crate_name).Replace('-', '_') + '.dll'
    $env:CARGO_HOME = $localCargoHome
    $env:CARGO_TARGET_DIR = $temporaryTarget
    $env:RUSTC = $cargoAuthority.RustcPath
    $env:SUPEREXPLORER_TRUSTED_CARGO = $cargoPath
    $env:SUPEREXPLORER_TRUSTED_CARGO_SHA256 = $cargoHash
    $env:SUPEREXPLORER_TRUSTED_RUSTC = $cargoAuthority.RustcPath
    $env:SUPEREXPLORER_TRUSTED_RUSTC_SHA256 = $cargoAuthority.RustcSha256
    $env:PATH = "$cargoDirectory;$rustcDirectory;$savedPath"
    Push-Location $sdk
    $pushed = $true
    & $cargoPath build --release --locked --offline --config $localCargoConfig --target $targetTriple --manifest-path (Join-Path $stagedRoot 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw "plugin build failed ($LASTEXITCODE)" }
    Pop-Location
    $pushed = $false
    if ((Get-ConsumerTreeDigest $root) -ne $consumerTreeDigest) { throw 'consumer source changed after the private build snapshot was validated' }

    $dll = Join-Path $temporaryTarget "$targetTriple\release\$crateFile"
    if (-not (Test-Path -LiteralPath $dll)) { throw "expected cdylib was not produced: $crateFile" }
    & $cargoPath run --release --locked --offline --config $localCargoConfig --manifest-path (Join-Path $sdk 'tools\plugin-tooling\Cargo.toml') -- inspect-dll $dll | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'built DLL failed the non-loading abi_stable export inspection' }
    $buildDir = Join-Path $stage 'build'
    $reportDir = Join-Path $stage 'reports'
    New-Item -ItemType Directory -Path $buildDir,$reportDir -Force | Out-Null
    $validationReport = Join-Path $stagedRoot ('target\superexplorer\' + [string]$manifest.sdk.bundle_id + '\reports\validation.json')
    if (-not (Test-Path -LiteralPath $validationReport)) { throw 'validation report was not produced' }
    Copy-Item -LiteralPath $validationReport -Destination (Join-Path $reportDir 'validation.json')
    Copy-Item -LiteralPath $dll -Destination (Join-Path $buildDir 'plugin.dll')
    $dllHash = (Get-FileHash -LiteralPath $dll -Algorithm SHA256).Hash.ToLowerInvariant()
    $report = [ordered]@{
        schema_version = 1
        bundle_id = [string]$manifest.sdk.bundle_id
        target = $targetTriple
        profile = 'release'
        toolchain = [ordered]@{
            rustc_release = [string]$sdkLock.toolchain.rustc_release
            rustc_commit_hash = [string]$sdkLock.toolchain.rustc_commit_hash
            cargo_release = [string]$sdkLock.toolchain.cargo_release
            cargo_commit_hash = [string]$sdkLock.toolchain.cargo_commit_hash
        }
        build_policy = $sdkLock.build_policy
        plugin_dll = [ordered]@{ path = 'build/plugin.dll'; size = (Get-Item $dll).Length; sha256 = $dllHash }
        inputs = [ordered]@{
            manifest_sha256 = (Get-FileHash -LiteralPath (Join-Path $stagedRoot 'plugin-project.json') -Algorithm SHA256).Hash.ToLowerInvariant()
            template_manifest_sha256 = [string]$templateMaterialization.template_manifest_sha256
            resolved_manifest_sha256 = [string]$templateMaterialization.resolved_manifest_sha256
            cargo_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $stagedRoot 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
            consumer_tree_sha256 = $consumerTreeDigest
        }
    }
    $reportJson = $report | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText((Join-Path $reportDir 'build.json'), "$reportJson`n", [Text.UTF8Encoding]::new($false))
    if ((Get-ConsumerTreeDigest $root) -ne $consumerTreeDigest) { throw 'consumer source changed before build generation publication' }
    if (-not $outputRoot.StartsWith(($expectedOutputParent.TrimEnd('\') + '\'), [StringComparison]::OrdinalIgnoreCase)) { throw 'plugin build output escaped the authorized target root' }
    Assert-NoReparseAncestors $outputRoot 'plugin build output root'
    $publishedBuild = Join-Path $outputRoot 'build'
    $publishedReports = Join-Path $outputRoot 'reports'
    $publishedBuildReport = Join-Path $publishedReports 'build.json'
    $publishedComplete = Join-Path $publishedReports 'build.complete.json'
    if ((Test-Path -LiteralPath $publishedBuild) -or (Test-Path -LiteralPath $publishedBuildReport) -or (Test-Path -LiteralPath $publishedComplete)) { throw 'a complete or incomplete build generation already exists; refusing to overwrite it' }
    New-Item -ItemType Directory -Path $outputRoot,$publishedReports -Force | Out-Null
    Assert-NoReparseAncestors $outputRoot 'plugin build publication root'
    # Commit only a previously absent build generation. Existing validation
    # reports remain valid inputs. Consumers MUST require build.complete.json,
    # so the three preceding moves are an invisible incomplete generation.
    $publishedThisAttempt = @()
    try {
        Move-Item -LiteralPath $buildDir -Destination $publishedBuild -ErrorAction Stop
        $publishedThisAttempt += $publishedBuild
        Move-Item -LiteralPath (Join-Path $reportDir 'build.json') -Destination $publishedBuildReport -ErrorAction Stop
        $publishedThisAttempt += $publishedBuildReport
        $publishedValidation = Join-Path $publishedReports 'validation.json'
        if (-not (Test-Path -LiteralPath $publishedValidation)) {
            Move-Item -LiteralPath (Join-Path $reportDir 'validation.json') -Destination $publishedValidation -ErrorAction Stop
            $publishedThisAttempt += $publishedValidation
        }
        $complete = [ordered]@{
            schema_version = 1
            bundle_id = [string]$manifest.sdk.bundle_id
            build_report_sha256 = (Get-FileHash -LiteralPath $publishedBuildReport -Algorithm SHA256).Hash.ToLowerInvariant()
            validation_report_sha256 = (Get-FileHash -LiteralPath $publishedValidation -Algorithm SHA256).Hash.ToLowerInvariant()
            consumer_tree_sha256 = $consumerTreeDigest
        }
        $stagedComplete = Join-Path $reportDir 'build.complete.json'
        [IO.File]::WriteAllText($stagedComplete, (($complete | ConvertTo-Json -Depth 6 -Compress) + "`n"), [Text.UTF8Encoding]::new($false))
        if ($env:SUPEREXPLORER_BUILD_TEST_WAIT_BEFORE_COMPLETE_MARKER) {
            [IO.File]::WriteAllText($env:SUPEREXPLORER_BUILD_TEST_WAIT_BEFORE_COMPLETE_MARKER, 'ready', [Text.UTF8Encoding]::new($false))
            while ($true) { Start-Sleep -Seconds 1 }
        }
        Move-Item -LiteralPath $stagedComplete -Destination $publishedComplete -ErrorAction Stop
        $publishedThisAttempt += $publishedComplete
    } catch {
        foreach ($path in @($publishedThisAttempt | Sort-Object Length -Descending)) {
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
        }
        throw
    }
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    $stage = $null
    Write-Output $publishedBuildReport
} finally {
    if ($pushed) { Pop-Location }
    foreach ($name in $dangerous) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    [Environment]::SetEnvironmentVariable('PATH', $savedPath, 'Process')
    foreach ($path in @($temporaryTarget,$stage,$stagedRoot)) {
        if ($path -and (Test-Path -LiteralPath $path)) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
    Remove-SealedCargoAuthority $cargoAuthority
}
