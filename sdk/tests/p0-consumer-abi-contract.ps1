[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}

$repository = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$fixture = Join-Path $repository 'sdk\fixtures\p0-consumer'
$sdk = Join-Path $repository 'sdk'
$artifact = Get-Content -LiteralPath (Join-Path $sdk 'ui-abi-fingerprint.json') -Raw | ConvertFrom-Json
$lock = Get-Content -LiteralPath (Join-Path $sdk 'sdk-lock.json') -Raw | ConvertFrom-Json

if ($artifact.bundle_id -ne $lock.bundle_id -or [string]$artifact.fingerprint -notmatch '^[0-9a-f]{64}$') {
    throw 'canonical SDK bundle or UI ABI fingerprint is malformed'
}

$scratch = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-p0-consumer-' + [guid]::NewGuid().ToString('N'))
$consumer = Join-Path $scratch 'consumer'
$contractHost = Join-Path $scratch 'host'
$cargoHome = Join-Path $scratch 'empty-cargo-home'
$marker = Join-Path $scratch 'registrar-marker.txt'
$savedCargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME', 'Process')

try {
    New-Item -ItemType Directory -Path $consumer,(Join-Path $consumer 'src'),$contractHost,(Join-Path $contractHost 'src'),$cargoHome -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $fixture 'Cargo.toml') -Destination (Join-Path $consumer 'Cargo.toml')
    Copy-Item -LiteralPath (Join-Path $fixture 'Cargo.lock') -Destination (Join-Path $consumer 'Cargo.lock')
    Copy-Item -LiteralPath (Join-Path $fixture 'src\lib.rs') -Destination (Join-Path $consumer 'src\lib.rs')

    $sourcePath = Join-Path $consumer 'src\lib.rs'
    $sourceBytes = [IO.File]::ReadAllBytes($sourcePath)
    $sourceHash = (Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $source = [Text.Encoding]::UTF8.GetString($sourceBytes)
    if ($source -notmatch ([regex]::Escape([string]$artifact.fingerprint))) {
        throw 'consumer source does not embed the canonical immutable UI ABI fingerprint'
    }

    $manifestTemplate = Get-Content -LiteralPath (Join-Path $fixture 'plugin-project.json') -Raw
    $manifest = $manifestTemplate.Replace('@SDK_BUNDLE_ID@', [string]$lock.bundle_id).Replace('@ABI_SCHEMA@', [string]$lock.build_policy.abi_schema_version).Replace('@UI_ABI_FINGERPRINT@', [string]$artifact.fingerprint).Replace('@SOURCE_SIZE@', [string]$sourceBytes.Length).Replace('@SOURCE_SHA256@', $sourceHash)
    if ($manifest -match '@[A-Z0-9_]+@') { throw 'consumer template placeholder was not materialized' }
    Write-Utf8NoBom (Join-Path $consumer 'plugin-project.json') $manifest

    $vendor = (Resolve-Path (Join-Path $sdk 'vendor\cargo-sources')).Path.Replace('\', '/')
    $isolatedCargoConfig = @"
[net]
offline = true

[source.crates-io]
replace-with = "cargo-sources"

[source.cargo-sources]
directory = "$vendor"
"@
    Write-Utf8NoBom (Join-Path $cargoHome 'config.toml') $isolatedCargoConfig
    Write-Utf8NoBom (Join-Path $contractHost 'Cargo.toml') @"
[package]
name = "p0-consumer-contract-host"
version = "0.1.0"
edition = "2021"
rust-version = "1.97.1"
publish = false

[dependencies]
abi_stable = { version = "=0.11.3", default-features = false }
p0-consumer = { path = "../consumer" }

[workspace]
"@
    $hostSource = @'
#![allow(non_camel_case_types)]

use std::{env, path::Path};

use abi_stable::{library::RootModule, std_types::{RResult, RStr}};
use p0_consumer::{P0ConsumerRoot_Ref, ABI_SCHEMA_VERSION, RegistrarResult};

const EXPECTED_FINGERPRINT: &str = "__EXPECTED_FINGERPRINT__";
const MISMATCHED_FINGERPRINT: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn terminal(result: RegistrarResult, expected: bool) -> Result<(), String> {
    match (expected, result) {
        (true, RResult::ROk(7)) | (false, RResult::RErr(_)) => Ok(()),
        (true, RResult::ROk(value)) => Err(format!("matching registrar returned {value}, expected 7")),
        (true, RResult::RErr(error)) => Err(format!("matching registrar failed: {error}")),
        (false, RResult::ROk(value)) => Err(format!("mismatched registrar unexpectedly succeeded with {value}")),
    }
}

fn run(path: &Path, marker: &Path) -> Result<(), String> {
    let root = P0ConsumerRoot_Ref::load_from_file(path)
        .map_err(|error| format!("P0 consumer root export could not load: {error}"))?;
    if root.abi_schema() != ABI_SCHEMA_VERSION {
        return Err("P0 consumer root ABI schema is not the P0 layout".into());
    }
    if (root.ui_abi_fingerprint())().as_str() != EXPECTED_FINGERPRINT {
        return Err("P0 consumer root did not expose the canonical immutable UI ABI fingerprint".into());
    }
    terminal((root.registrar())(ABI_SCHEMA_VERSION, RStr::from_str(MISMATCHED_FINGERPRINT)), false)?;
    if marker.exists() {
        return Err("fingerprint mismatch invoked the registrar marker".into());
    }
    terminal((root.registrar())(ABI_SCHEMA_VERSION, RStr::from_str(EXPECTED_FINGERPRINT)), true)?;
    if !marker.exists() {
        return Err("matching fingerprint did not invoke the registrar marker".into());
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let plugin = arguments.next().ok_or("usage: p0-consumer-contract-host <plugin.dll> <marker>")?;
    let marker = arguments.next().ok_or("usage: p0-consumer-contract-host <plugin.dll> <marker>")?;
    if arguments.next().is_some() {
        return Err("too many arguments".into());
    }
    run(Path::new(&plugin), Path::new(&marker))
}
'@.Replace('__EXPECTED_FINGERPRINT__', [string]$artifact.fingerprint)
    Write-Utf8NoBom (Join-Path $contractHost 'src\main.rs') $hostSource

    # CARGO_HOME is a fresh, isolated directory containing only this explicit
    # offline source policy; it has no inherited registry cache or credentials.
    $env:CARGO_HOME = $cargoHome
    Push-Location $consumer
    try {
        & cargo.exe build --release --locked --offline --target ([string]$lock.toolchain.target)
        if ($LASTEXITCODE -ne 0) { throw 'materialized P0 consumer did not build locked and offline' }
    } finally { Pop-Location }

    Push-Location $contractHost
    try {
        & cargo.exe generate-lockfile --offline
        if ($LASTEXITCODE -ne 0) { throw 'P0 ABI contract host lock generation failed offline' }
        & cargo.exe build --locked --offline
        if ($LASTEXITCODE -ne 0) { throw 'P0 ABI contract host did not build locked and offline' }
    } finally { Pop-Location }

    $pluginDll = Join-Path $consumer 'target\x86_64-pc-windows-msvc\release\p0_consumer.dll'
    $hostExe = Join-Path $contractHost 'target\debug\p0-consumer-contract-host.exe'
    if (-not (Test-Path -LiteralPath $pluginDll)) { throw 'consumer did not export the expected p0_consumer cdylib' }
    if (-not (Test-Path -LiteralPath $hostExe)) { throw 'contract host executable was not produced' }
    $env:P0_CONSUMER_REGISTRAR_MARKER = $marker
    & $hostExe $pluginDll $marker
    if ($LASTEXITCODE -ne 0) { throw 'P0 consumer ABI root/fingerprint pre-callback contract failed' }
    if ((Get-Content -LiteralPath $marker -Raw).Trim() -ne 'p0 consumer registrar invoked') {
        throw 'P0 consumer registrar marker had unexpected content'
    }
    Write-Host 'P0 consumer standalone abi_stable root and fingerprint pre-callback contract passed.'
} finally {
    [Environment]::SetEnvironmentVariable('CARGO_HOME', $savedCargoHome, 'Process')
    [Environment]::SetEnvironmentVariable('P0_CONSUMER_REGISTRAR_MARKER', $null, 'Process')
    if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Recurse -Force }
}
