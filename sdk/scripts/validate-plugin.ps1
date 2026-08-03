[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = (Resolve-Path -LiteralPath $PluginRoot).Path
$core = Join-Path $sdk 'tools\plugin-tooling'
$manifestPath = Join-Path $root 'plugin-project.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'plugin-project.json required' }
if (-not (Test-Path -LiteralPath (Join-Path $core 'Cargo.toml'))) { throw 'plugin Rust core missing' }
$sdkLock = Get-Content -LiteralPath (Join-Path $sdk 'sdk-lock.json') -Raw -Encoding UTF8 | ConvertFrom-Json
Import-Module (Join-Path $PSScriptRoot 'sealed-cargo-authority.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'consumer-snapshot.psm1') -Force
foreach ($configName in @('.cargo\config.toml','.cargo\config','rust-toolchain.toml','rust-toolchain')) {
    if (Test-Path -LiteralPath (Join-Path $root $configName)) { throw 'consumer Cargo config overrides are forbidden' }
}
$dangerous = @('RUSTC','RUSTC_BOOTSTRAP','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTFLAGS','RUSTDOCFLAGS','CARGO_BUILD_RUSTFLAGS','CARGO_BUILD_RUSTC','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER','CARGO_ENCODED_RUSTFLAGS','CARGO_INCREMENTAL','CARGO_TARGET_DIR','CARGO_HOME','RUSTUP_TOOLCHAIN','RUSTUP_HOME','CC','CXX','AR','LINKER','SUPEREXPLORER_TRUSTED_CARGO','SUPEREXPLORER_TRUSTED_CARGO_SHA256','SUPEREXPLORER_TRUSTED_RUSTC','SUPEREXPLORER_TRUSTED_RUSTC_SHA256')
$forbiddenOverrides = @([Environment]::GetEnvironmentVariables('Process').GetEnumerator() | Where-Object { $_.Value -and ([string]$_.Key -match '^(RUSTC|RUSTC_BOOTSTRAP|RUSTFLAGS|RUSTDOCFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_ENCODED_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_BUILD_RUST(FLAGS|C|C_WRAPPER|C_WORKSPACE_WRAPPER)?|CARGO_PROFILE_|CARGO_TARGET_.*_(RUSTFLAGS|LINKER|RUNNER)|RUSTUP_(TOOLCHAIN|HOME|DIST_SERVER|UPDATE_ROOT)|SUPEREXPLORER_TRUSTED_(CARGO|RUSTC)(_SHA256)?|CC|CXX|AR|LINKER|[A-Z0-9_]+_(CC|CXX|AR|LINKER))$') })
if ($forbiddenOverrides.Count -gt 0) { throw "fingerprint-affecting validation environment override is forbidden: $($forbiddenOverrides[0].Key)" }

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

$reportDir = Join-Path $root ("target\superexplorer\$($sdkLock.bundle_id)\reports")
$authorizedOutputRoot = Join-Path $root 'target\superexplorer'
$finalReport = Join-Path $reportDir 'validation.json'
function Remove-StaleValidationReport {
    if (Test-Path -LiteralPath $finalReport) {
        Assert-NoReparseAncestors $reportDir 'validation report directory'
        Remove-Item -LiteralPath $finalReport -Force
    }
}
function Assert-ValidationSnapshotIdentity {
    if ((Get-BoundedConsumerTreeDigest $root) -ne $consumerTreeDigest) {
        Remove-StaleValidationReport
        throw 'consumer source changed after the bounded validation snapshot was validated'
    }
}
$consumerTreeDigest = Get-BoundedConsumerTreeDigest $root
$temporaryCargo = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-validate-cargo-' + [guid]::NewGuid().ToString('N'))
$temporaryTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-validate-target-' + [guid]::NewGuid().ToString('N'))
$temporarySnapshot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-validate-inputs-' + [guid]::NewGuid().ToString('N'))
$savedCargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME','Process')
$savedTargetDir = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR','Process')
$savedRustc = [Environment]::GetEnvironmentVariable('RUSTC','Process')
$savedRustupToolchain = [Environment]::GetEnvironmentVariable('RUSTUP_TOOLCHAIN','Process')
$savedPath = [Environment]::GetEnvironmentVariable('PATH','Process')
$saved = @{}
foreach ($name in $dangerous) { $saved[$name] = [Environment]::GetEnvironmentVariable($name,'Process') }
$cargoAuthority = $null
$cargoPath = $null
$cargoHash = $null
$cargoDirectory = $null
$sdkPushed = $false
try {
    # The cleanup boundary starts before Cargo authority or a private source
    # snapshot exists, so errors at any later point cannot leak either one.
    $cargoAuthority = New-SealedCargoAuthority $sdkLock.toolchain
    $cargoPath = $cargoAuthority.Path
    $cargoHash = $cargoAuthority.Sha256
    $cargoDirectory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($cargoPath))
    foreach ($name in $dangerous) { [Environment]::SetEnvironmentVariable($name,$null,'Process') }
    New-Item -ItemType Directory -Path $temporaryCargo,$temporaryTarget -Force | Out-Null
    Copy-BoundedConsumerSnapshot $root $temporarySnapshot | Out-Null
    if ((Get-BoundedConsumerTreeDigest $temporarySnapshot) -ne $consumerTreeDigest) {
        Remove-StaleValidationReport
        throw 'consumer source changed while creating the bounded validation snapshot'
    }
    foreach ($configName in @('.cargo\config.toml','.cargo\config','rust-toolchain.toml','rust-toolchain')) {
        if (Test-Path -LiteralPath (Join-Path $temporarySnapshot $configName)) { throw 'consumer Cargo config overrides are forbidden' }
    }
    $vendor = (Join-Path $sdk 'vendor\cargo-sources').Replace('\','/')
    $cargoConfig = "[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$vendor'`n"
    [IO.File]::WriteAllText((Join-Path $temporaryCargo 'config.toml'), $cargoConfig, [Text.UTF8Encoding]::new($false))
    $env:CARGO_HOME = $temporaryCargo
    $env:CARGO_TARGET_DIR = $temporaryTarget
    $env:RUSTC = $cargoAuthority.RustcPath
    $env:PATH = "$cargoDirectory;$savedPath"
    $env:SUPEREXPLORER_TRUSTED_CARGO = $cargoPath
    $env:SUPEREXPLORER_TRUSTED_CARGO_SHA256 = $cargoHash
    $env:SUPEREXPLORER_TRUSTED_RUSTC = $cargoAuthority.RustcPath
    $env:SUPEREXPLORER_TRUSTED_RUSTC_SHA256 = $cargoAuthority.RustcSha256
    if ($env:SUPEREXPLORER_VALIDATE_TEST_MUTATE_AFTER_SNAPSHOT -eq '1') {
        [IO.File]::AppendAllText($manifestPath, "`n", [Text.UTF8Encoding]::new($false))
    }
    $savedErrorAction = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    Push-Location $sdk
    $sdkPushed = $true
    $reportJson = & $cargoPath run --release --manifest-path (Join-Path $core 'Cargo.toml') --locked --offline -- validate $temporarySnapshot
    $exitCode = $LASTEXITCODE
    Pop-Location
    $sdkPushed = $false
    $ErrorActionPreference = $savedErrorAction
} finally {
    if ($sdkPushed) { Pop-Location }
    $ErrorActionPreference = 'Stop'
    [Environment]::SetEnvironmentVariable('CARGO_HOME',$savedCargoHome,'Process')
    [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR',$savedTargetDir,'Process')
    [Environment]::SetEnvironmentVariable('RUSTC',$savedRustc,'Process')
    [Environment]::SetEnvironmentVariable('RUSTUP_TOOLCHAIN',$savedRustupToolchain,'Process')
    [Environment]::SetEnvironmentVariable('PATH',$savedPath,'Process')
    foreach ($name in $dangerous) { [Environment]::SetEnvironmentVariable($name,$saved[$name],'Process') }
    foreach ($path in @($temporaryCargo,$temporaryTarget,$temporarySnapshot)) {
        if ((Test-Path -LiteralPath $path) -and $path.StartsWith([IO.Path]::GetTempPath(),[StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
    Remove-SealedCargoAuthority $cargoAuthority
}
$reportText = ($reportJson -join "`n")
Assert-ValidationSnapshotIdentity
if (-not $reportDir.StartsWith(($authorizedOutputRoot.TrimEnd('\') + '\'), [StringComparison]::OrdinalIgnoreCase)) { throw 'validation report escaped the authorized target root' }
Assert-NoReparseAncestors $authorizedOutputRoot 'validation report parent'
New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
Assert-NoReparseAncestors $reportDir 'validation report directory'
try {
    $parsedReport = $reportText | ConvertFrom-Json
    if ($parsedReport.schema_version -ne 1 -or $parsedReport.valid -isnot [bool] -or $parsedReport.diagnostics -isnot [array]) { throw 'core emitted an invalid diagnostics envelope' }
    $parsedReport | Add-Member -NotePropertyName inputs -NotePropertyValue ([ordered]@{ consumer_tree_sha256 = $consumerTreeDigest }) -Force
    $reportText = $parsedReport | ConvertTo-Json -Depth 8 -Compress
} catch {
    Remove-StaleValidationReport
    throw 'core emitted no valid serialized diagnostics report'
}
Write-Output $reportText
$stagedReport = Join-Path $reportDir ('.validation-stage-' + [guid]::NewGuid().ToString('N') + '.json')
$backupReport = Join-Path $reportDir ('.validation-backup-' + [guid]::NewGuid().ToString('N') + '.json')
try {
    # Recheck immediately before writing and again before publication: this
    # report must never bind snapshot A while describing live source B.
    Assert-ValidationSnapshotIdentity
    [IO.File]::WriteAllText($stagedReport, "$reportText`n", [Text.UTF8Encoding]::new($false))
    Assert-NoReparseAncestors $reportDir 'validation report directory'
    if ($env:SUPEREXPLORER_VALIDATE_TEST_FAIL_PUBLICATION -eq '1') { throw 'injected validation report publication failure' }
    Assert-ValidationSnapshotIdentity
    if (Test-Path -LiteralPath $finalReport) { [IO.File]::Replace($stagedReport, $finalReport, $backupReport) }
    else { Move-Item -LiteralPath $stagedReport -Destination $finalReport -ErrorAction Stop }
} catch {
    # Never leave a prior valid result to describe changed invalid inputs.
    Remove-StaleValidationReport
    throw
} finally {
    if (Test-Path -LiteralPath $stagedReport) { Remove-Item -LiteralPath $stagedReport -Force }
    if (Test-Path -LiteralPath $backupReport) { Remove-Item -LiteralPath $backupReport -Force }
}
if ($exitCode -ne 0) { throw 'plugin validation failed' }
