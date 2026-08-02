[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = (Resolve-Path -LiteralPath $PluginRoot).Path
$core = Join-Path $sdk 'tools\plugin-tooling'
$manifestPath = Join-Path $root 'plugin-project.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'plugin-project.json required' }
if (-not (Test-Path -LiteralPath (Join-Path $core 'Cargo.toml'))) { throw 'plugin Rust core missing' }
foreach ($configName in @('.cargo\config.toml','.cargo\config')) {
    if (Test-Path -LiteralPath (Join-Path $root $configName)) { throw 'consumer Cargo config overrides are forbidden' }
}

$temporaryCargo = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-validate-cargo-' + [guid]::NewGuid().ToString('N'))
$temporaryTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-validate-target-' + [guid]::NewGuid().ToString('N'))
$savedCargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME','Process')
$savedTargetDir = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR','Process')
try {
    New-Item -ItemType Directory -Path $temporaryCargo,$temporaryTarget -Force | Out-Null
    $vendor = (Join-Path $sdk 'vendor\cargo-sources').Replace('\','/')
    $cargoConfig = "[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$vendor'`n"
    [IO.File]::WriteAllText((Join-Path $temporaryCargo 'config.toml'), $cargoConfig, [Text.UTF8Encoding]::new($false))
    $env:CARGO_HOME = $temporaryCargo
    $env:CARGO_TARGET_DIR = $temporaryTarget
    $savedErrorAction = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $reportJson = & cargo.exe run --release --manifest-path (Join-Path $core 'Cargo.toml') --locked --offline -- validate $root
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $savedErrorAction
} finally {
    $ErrorActionPreference = 'Stop'
    [Environment]::SetEnvironmentVariable('CARGO_HOME',$savedCargoHome,'Process')
    [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR',$savedTargetDir,'Process')
    foreach ($path in @($temporaryCargo,$temporaryTarget)) {
        if ((Test-Path -LiteralPath $path) -and $path.StartsWith([IO.Path]::GetTempPath(),[StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
}
$reportText = ($reportJson -join "`n")
Write-Output $reportText
if ($exitCode -ne 0) { throw 'plugin validation failed' }

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$reportDir = Join-Path $root ("target\superexplorer\$($manifest.sdk.bundle_id)\reports")
New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
[IO.File]::WriteAllText((Join-Path $reportDir 'validation.json'), "$reportText`n", [Text.UTF8Encoding]::new($false))
