$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $sdkRoot 'fixtures\extension-author-jobs-v1'
$vendor = Join-Path $sdkRoot 'vendor\cargo-sources'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-extension-author-jobs-' + [Guid]::NewGuid().ToString('N'))
$cargoHome = Join-Path $tempRoot 'cargo-home'
$target = Join-Path $tempRoot 'target'
$savedCargoHome = $env:CARGO_HOME
$savedCargoTargetDir = $env:CARGO_TARGET_DIR

try {
    New-Item -ItemType Directory -Force -Path $cargoHome, $target | Out-Null
    $config = @('[source.crates-io]', 'replace-with = "cargo-sources"', '[source.cargo-sources]', ('directory = "' + ($vendor -replace '\\', '/') + '"')) -join [Environment]::NewLine
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), $config, [Text.UTF8Encoding]::new($false))
    $env:CARGO_HOME = $cargoHome
    $env:CARGO_TARGET_DIR = $target
    Push-Location $fixtureRoot
    try {
        & cargo.exe run --locked --offline
        if ($LASTEXITCODE -ne 0) { throw "extension author jobs fixture failed (exit $LASTEXITCODE)" }
    } finally { Pop-Location }
    Write-Output 'extension author jobs v1 contract: PASS'
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    if ($null -eq $savedCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedCargoHome }
    if ($null -eq $savedCargoTargetDir) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedCargoTargetDir }
}
