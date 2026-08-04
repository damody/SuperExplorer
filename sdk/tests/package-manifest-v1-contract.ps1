$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $sdkRoot 'fixtures\package-manifest-v1'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-manifest-v1-' + [Guid]::NewGuid().ToString('N'))
$cargoHome = Join-Path $tempRoot 'cargo-home'; $targetDir = Join-Path $tempRoot 'target'
$savedCargoHome = $env:CARGO_HOME; $savedTarget = $env:CARGO_TARGET_DIR
try {
    New-Item -ItemType Directory -Path $cargoHome, $targetDir -Force | Out-Null
    $configPath = & powershell.exe -NoProfile -File (Join-Path $sdkRoot 'scripts\prepare-local-cargo-source.ps1') -PluginRoot $fixtureRoot
    $config = Get-Content -LiteralPath $configPath -Raw
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), $config, [Text.UTF8Encoding]::new($false))
    $env:CARGO_HOME = $cargoHome; $env:CARGO_TARGET_DIR = $targetDir
    Push-Location $fixtureRoot
    try {
        & cargo build --locked --offline --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    } finally { Pop-Location }
    $exe = Join-Path $targetDir 'x86_64-pc-windows-msvc\debug\package-manifest-v1-contract.exe'
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "missing fixture executable: $exe" }
    $env:PACKAGE_MANIFEST_FIXTURE_ROOT = $fixtureRoot
    & $exe
    if ($LASTEXITCODE -ne 0) { throw "manifest contract failed (exit $LASTEXITCODE)" }
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    if ($null -eq $savedCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedCargoHome }
    if ($null -eq $savedTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedTarget }
    Remove-Item Env:PACKAGE_MANIFEST_FIXTURE_ROOT -ErrorAction SilentlyContinue
}
Write-Output 'package manifest v1 contract: PASS'
