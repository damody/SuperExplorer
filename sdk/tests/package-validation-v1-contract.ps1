$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $sdkRoot 'fixtures\package-validation-v1'
$vendor = Join-Path $sdkRoot 'vendor\cargo-sources'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-validation-v1-' + [Guid]::NewGuid().ToString('N'))
$cargoHome = Join-Path $tempRoot 'cargo-home'; $targetDir = Join-Path $tempRoot 'target'
$savedCargoHome = $env:CARGO_HOME; $savedTarget = $env:CARGO_TARGET_DIR
try {
    New-Item -ItemType Directory -Path $cargoHome, $targetDir -Force | Out-Null
    $config = @('[source.crates-io]','replace-with = "cargo-sources"','[source.cargo-sources]',('directory = "' + ($vendor -replace '\\','/') + '"')) -join [Environment]::NewLine
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), $config, [Text.UTF8Encoding]::new($false))
    $env:CARGO_HOME = $cargoHome; $env:CARGO_TARGET_DIR = $targetDir; $env:PACKAGE_VALIDATION_FIXTURE_ROOT = $fixtureRoot
    Push-Location $fixtureRoot
    try { & cargo build --locked --offline --target x86_64-pc-windows-msvc; if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" } } finally { Pop-Location }
    $exe = Join-Path $targetDir 'x86_64-pc-windows-msvc\debug\package-validation-v1-contract.exe'
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "missing fixture executable: $exe" }
    & $exe; if ($LASTEXITCODE -ne 0) { throw "validation contract failed (exit $LASTEXITCODE)" }
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item Env:PACKAGE_VALIDATION_FIXTURE_ROOT -ErrorAction SilentlyContinue
    if ($null -eq $savedCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedCargoHome }
    if ($null -eq $savedTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedTarget }
}
Write-Output 'package validation v1 contract: PASS'
