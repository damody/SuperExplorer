$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$sdkRoot = Join-Path $repo 'sdk'
$fixtureRoot = Join-Path $sdkRoot 'fixtures\package-resolution-v1'
$vendor = (Join-Path $sdkRoot 'vendor\cargo-sources').Replace('\', '/')
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-resolution-v1-' + [Guid]::NewGuid().ToString('N'))
$savedHome = $env:CARGO_HOME; $savedTarget = $env:CARGO_TARGET_DIR
try {
    $workspace = Join-Path $tempRoot 'workspace'
    $cargoHome = Join-Path $tempRoot 'cargo-home'; $target = Join-Path $tempRoot 'target'
    New-Item -ItemType Directory -Path $workspace, $cargoHome, $target -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $fixtureRoot 'Cargo.toml') -Destination (Join-Path $workspace 'Cargo.toml')
    Copy-Item -LiteralPath (Join-Path $fixtureRoot 'Cargo.lock') -Destination (Join-Path $workspace 'Cargo.lock')
    Copy-Item -LiteralPath (Join-Path $fixtureRoot 'validator') -Destination (Join-Path $workspace 'validator') -Recurse
    Copy-Item -LiteralPath (Join-Path $fixtureRoot 'example-manifest.json') -Destination (Join-Path $workspace 'example-manifest.json')
    $crateRoot = Join-Path $workspace 'crates'
    New-Item -ItemType Directory -Path $crateRoot -Force | Out-Null
    foreach ($crate in @('explorer-extension-api', 'explorer-extension-ui-api', 'explorer-extension-host')) {
        Copy-Item -LiteralPath (Join-Path $repo "crates\\$crate") -Destination (Join-Path $crateRoot $crate) -Recurse
    }
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), "[build]`ntarget = 'x86_64-pc-windows-msvc'`n`n[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$vendor'`n", [Text.UTF8Encoding]::new($false))
    $env:CARGO_HOME = $cargoHome; $env:CARGO_TARGET_DIR = $target
    & cargo.exe test --manifest-path (Join-Path $workspace 'Cargo.toml') -p explorer-extension-host --locked --offline --test package_lifecycle -- --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'package resolver integration contract failed' }
    & cargo.exe test --manifest-path (Join-Path $workspace 'Cargo.toml') -p explorer-extension-host --locked --offline package_source -- --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'package source contract failed' }
    & cargo.exe run --manifest-path (Join-Path $workspace 'Cargo.toml') -p package-manifest-example-validator --locked --offline -- (Join-Path $workspace 'example-manifest.json')
    if ($LASTEXITCODE -ne 0) { throw 'production PackageManifestV1 parser rejected the example manifest' }
} finally {
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
    if ($null -eq $savedHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedHome }
    if ($null -eq $savedTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedTarget }
}
Write-Output 'package resolution v1 contract: PASS'
