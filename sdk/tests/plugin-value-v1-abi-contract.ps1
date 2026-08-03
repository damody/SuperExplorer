$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $sdkRoot 'fixtures\plugin-value-v1-contract'
$pluginRoot = Join-Path $fixtureRoot 'new-plugin'
$hostRoot = Join-Path $fixtureRoot 'current-host'
$vendor = Join-Path $sdkRoot 'vendor\cargo-sources'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-plugin-value-v1-' + [Guid]::NewGuid().ToString('N'))
$cargoHome = Join-Path $tempRoot 'cargo-home'
$pluginTarget = Join-Path $tempRoot 'target-plugin'
$hostTarget = Join-Path $tempRoot 'target-host'
$savedCargoHome = $env:CARGO_HOME
$savedCargoTargetDir = $env:CARGO_TARGET_DIR

function Fail([string] $Message) { throw $Message }
function Invoke-Build([string] $Project, [string] $Target) {
    $env:CARGO_HOME = $cargoHome
    $env:CARGO_TARGET_DIR = $Target
    Push-Location $Project
    try {
        & cargo.exe build --locked --offline --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { Fail "cargo build failed for $Project (exit $LASTEXITCODE)" }
    } finally { Pop-Location }
}
function Artifact([string] $Target, [string] $Name) {
    $path = Join-Path $Target ('x86_64-pc-windows-msvc\debug\' + $Name)
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "missing artifact: $path" }
    (Resolve-Path -LiteralPath $path).Path
}

try {
    New-Item -ItemType Directory -Force -Path $cargoHome, $pluginTarget, $hostTarget | Out-Null
    $config = @('[source.crates-io]', 'replace-with = "cargo-sources"', '[source.cargo-sources]', ('directory = "' + ($vendor -replace '\\', '/') + '"')) -join [Environment]::NewLine
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), $config, [Text.UTF8Encoding]::new($false))
    Invoke-Build $pluginRoot $pluginTarget
    Invoke-Build $hostRoot $hostTarget
    $plugin = Artifact $pluginTarget 'plugin_value_v1_contract_new_plugin.dll'
    $hostExe = Artifact $hostTarget 'plugin-value-v1-contract-host.exe'
    & $hostExe $plugin
    if ($LASTEXITCODE -ne 0) { Fail "plugin-value v1 contract host failed (exit $LASTEXITCODE)" }
    Write-Output 'plugin value v1 ABI contract: PASS'
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    if ($null -eq $savedCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedCargoHome }
    if ($null -eq $savedCargoTargetDir) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedCargoTargetDir }
}
