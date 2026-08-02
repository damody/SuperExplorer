[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = (Resolve-Path -LiteralPath $PluginRoot).Path
$manifestPath = Join-Path $root 'plugin-project.json'
if (-not (Test-Path -LiteralPath (Join-Path $root 'Cargo.toml')) -or -not (Test-Path -LiteralPath $manifestPath)) {
    throw 'PluginRoot must contain Cargo.toml and plugin-project.json'
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$sdkLock = Get-Content -LiteralPath (Join-Path $sdk 'sdk-lock.json') -Raw | ConvertFrom-Json
$targetTriple = 'x86_64-pc-windows-msvc'
$crateFile = ([string]$manifest.rust.crate_name).Replace('-', '_') + '.dll'
$temporaryTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-target-' + [guid]::NewGuid().ToString('N'))
$temporaryCargo = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-cargo-' + [guid]::NewGuid().ToString('N'))
$outputRoot = Join-Path $root ('target\superexplorer\' + [string]$manifest.sdk.bundle_id)
$stage = Join-Path $root ('target\superexplorer\.stage-' + [guid]::NewGuid().ToString('N'))
$dangerous = @('RUSTFLAGS','RUSTDOCFLAGS','CARGO_BUILD_RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_TARGET_DIR','CARGO_HOME')
$forbiddenOverrides = @([Environment]::GetEnvironmentVariables('Process').GetEnumerator() | Where-Object {
    $_.Value -and ([string]$_.Key -match '^(RUSTFLAGS|RUSTDOCFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_RUST|CARGO_PROFILE_|CARGO_TARGET_.*_(RUSTFLAGS|LINKER|RUNNER))')
})
if ($forbiddenOverrides.Count -gt 0) { throw "fingerprint-affecting build environment override is forbidden: $($forbiddenOverrides[0].Key)" }
foreach ($configName in @('.cargo\config.toml','.cargo\config')) {
    if (Test-Path -LiteralPath (Join-Path $root $configName)) { throw 'consumer Cargo config overrides are forbidden' }
}
$inputPaths = @($manifestPath,(Join-Path $root 'Cargo.toml'),(Join-Path $root 'Cargo.lock'))
foreach ($payload in $manifest.payloads) { $inputPaths += Join-Path $root ([string]$payload.path) }
$inputHashes = @{}
foreach ($path in $inputPaths) { $inputHashes[$path] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash }
$saved = @{}
foreach ($name in $dangerous) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$pushed = $false
try {
    foreach ($name in $dangerous) { [Environment]::SetEnvironmentVariable($name, $null, 'Process') }
    $env:CARGO_HOME = $temporaryCargo
    $env:CARGO_TARGET_DIR = $temporaryTarget
    New-Item -ItemType Directory -Path $temporaryCargo,$temporaryTarget,$stage -Force | Out-Null
    $vendor = (Join-Path $sdk 'vendor\cargo-sources').Replace('\','/')
    $cargoConfig = "[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$vendor'`n`n[build]`ntarget = '$targetTriple'`n"
    [IO.File]::WriteAllText((Join-Path $temporaryCargo 'config.toml'), $cargoConfig, [Text.UTF8Encoding]::new($false))
    & powershell.exe -NoProfile -File (Join-Path $sdk 'tests\toolchain-contract.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'SDK toolchain contract failed' }
    & powershell.exe -NoProfile -File (Join-Path $PSScriptRoot 'validate-plugin.ps1') -PluginRoot $root | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'plugin validation failed before build' }
    Push-Location $root
    $pushed = $true
    & cargo.exe build --release --locked --offline --target $targetTriple
    if ($LASTEXITCODE -ne 0) { throw "plugin build failed ($LASTEXITCODE)" }
    Pop-Location
    $pushed = $false
    foreach ($path in $inputPaths) {
        if (-not (Test-Path -LiteralPath $path) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $inputHashes[$path]) {
            throw "build modified a protected input: $([IO.Path]::GetFileName($path))"
        }
    }

    $dll = Join-Path $temporaryTarget "$targetTriple\release\$crateFile"
    if (-not (Test-Path -LiteralPath $dll)) { throw "expected cdylib was not produced: $crateFile" }
    & cargo.exe run --release --locked --offline --manifest-path (Join-Path $sdk 'tools\plugin-tooling\Cargo.toml') -- inspect-dll $dll | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'built DLL failed the non-loading abi_stable export inspection' }
    $buildDir = Join-Path $stage 'build'
    $reportDir = Join-Path $stage 'reports'
    New-Item -ItemType Directory -Path $buildDir,$reportDir -Force | Out-Null
    $validationReport = Join-Path $outputRoot 'reports\validation.json'
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
            manifest_sha256 = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
            cargo_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $root 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $reportJson = $report | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText((Join-Path $reportDir 'build.json'), "$reportJson`n", [Text.UTF8Encoding]::new($false))
    if (Test-Path -LiteralPath $outputRoot) {
        $existingFiles = @(Get-ChildItem -LiteralPath $outputRoot -File -Recurse)
        if ($existingFiles.Count -ne 1 -or $existingFiles[0].FullName -ne $validationReport) {
            throw 'build output already exists; refusing to overwrite it'
        }
        Remove-Item -LiteralPath $outputRoot -Recurse -Force
    }
    Move-Item -LiteralPath $stage -Destination $outputRoot
    $stage = $null
    Write-Output (Join-Path $outputRoot 'reports\build.json')
} finally {
    if ($pushed) { Pop-Location }
    foreach ($name in $dangerous) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    foreach ($path in @($temporaryCargo,$temporaryTarget,$stage)) {
        if ($path -and (Test-Path -LiteralPath $path)) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
}
