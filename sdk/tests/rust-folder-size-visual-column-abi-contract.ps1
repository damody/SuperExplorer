[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}

$repository = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$fixture = Join-Path $repository 'sdk\fixtures\rust-folder-size-visual-column'
$sdk = Join-Path $repository 'sdk'
$artifact = Get-Content -LiteralPath (Join-Path $sdk 'ui-abi-fingerprint.json') -Raw | ConvertFrom-Json
$lock = Get-Content -LiteralPath (Join-Path $sdk 'sdk-lock.json') -Raw | ConvertFrom-Json

if ($artifact.bundle_id -ne $lock.bundle_id -or [string]$artifact.fingerprint -notmatch '^[0-9a-f]{64}$') {
    throw 'canonical SDK bundle or UI ABI fingerprint is malformed'
}

$scratch = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-rust-folder-size-visual-column-' + [guid]::NewGuid().ToString('N'))
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
    Copy-Item -LiteralPath (Join-Path $fixture 'vendor') -Destination $consumer -Recurse

    # Materialize the two public SDK crates beside the isolated consumer. The
    # fixture intentionally references these crates by path, but the clean
    # scratch tree has no repository workspace to provide inherited metadata.
    # Keep the materialized manifests standalone and patch the fixture paths so
    # Cargo never resolves a dependency outside this temporary tree.
    $materializedCrates = Join-Path $scratch 'crates'
    New-Item -ItemType Directory -Path $materializedCrates -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repository 'crates\explorer-extension-api') -Destination $materializedCrates -Recurse
    Copy-Item -LiteralPath (Join-Path $repository 'crates\explorer-extension-ui-api') -Destination $materializedCrates -Recurse
    $consumerManifest = Get-Content -LiteralPath (Join-Path $consumer 'Cargo.toml') -Raw
    $consumerManifest = $consumerManifest.Replace('../../../crates/explorer-extension-api', '../crates/explorer-extension-api').Replace('../../../crates/explorer-extension-ui-api', '../crates/explorer-extension-ui-api')
    Write-Utf8NoBom (Join-Path $consumer 'Cargo.toml') $consumerManifest

    $apiManifest = @'
[package]
name = "explorer-extension-api"
version = "1.2.0"
edition = "2024"
rust-version = "1.97.1"
publish = false

[dependencies]
abi_stable = { version = "=0.11.3", default-features = false }
'@
    Write-Utf8NoBom (Join-Path $materializedCrates 'explorer-extension-api\Cargo.toml') $apiManifest
    $uiManifest = @'
[package]
name = "explorer-extension-ui-api"
version = "0.1.0"
edition = "2024"
rust-version = "1.97.1"
publish = false

[dependencies]
explorer-extension-api = { path = "../explorer-extension-api" }
'@
    Write-Utf8NoBom (Join-Path $materializedCrates 'explorer-extension-ui-api\Cargo.toml') $uiManifest

    $sourcePath = Join-Path $consumer 'src\lib.rs'
    $sourceBytes = [IO.File]::ReadAllBytes($sourcePath)
    $sourceHash = (Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestTemplate = Get-Content -LiteralPath (Join-Path $fixture 'plugin-project.json') -Raw
    $manifest = $manifestTemplate.Replace('@SDK_BUNDLE_ID@', [string]$lock.bundle_id).Replace('@ABI_SCHEMA@', [string]$lock.build_policy.abi_schema_version).Replace('@UI_ABI_FINGERPRINT@', [string]$artifact.fingerprint).Replace('@SOURCE_SIZE@', [string]$sourceBytes.Length).Replace('@SOURCE_SHA256@', $sourceHash)
    if ($manifest -match '@[A-Z0-9_]+@') { throw 'consumer template placeholder was not materialized' }
    Write-Utf8NoBom (Join-Path $consumer 'plugin-project.json') $manifest

    $env:CARGO_HOME = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) '.cargo'
    Write-Utf8NoBom (Join-Path $contractHost 'Cargo.toml') @"
[package]
name = "rust-folder-size-visual-column-contract-host"
version = "0.1.0"
edition = "2021"
rust-version = "1.97.1"
publish = false

[dependencies]
abi_stable = { version = "=0.11.3", default-features = false }
explorer-extension-api = { path = "../crates/explorer-extension-api" }

[workspace]
"@
    $hostSource = @'
#![allow(non_camel_case_types)]

use std::{env, path::Path};

use abi_stable::{library::RootModule, std_types::ROption};
use explorer_extension_api::{
    ABI_SCHEMA_V1, AbiErrorCodeV1, AbiSchemaIdV1, DESCRIPTOR_CONTRACT_REVISION_V1,
    ExtensionRootModuleV1_Ref, ROOT_MODULE_CONTRACT_ID_V1, RegistrationOutcomeV1,
    SDK_MAJOR_VERSION_V1, registrar_request_v1,
};

fn run(path: &Path, marker: &Path) -> Result<(), String> {
    let root = ExtensionRootModuleV1_Ref::load_from_file(path)
        .map_err(|error| format!("folder-size visual column root export could not load: {error}"))?;
    if root.abi_schema() != ABI_SCHEMA_V1
        || root.root_contract_id() != ROOT_MODULE_CONTRACT_ID_V1
        || root.sdk_major() != SDK_MAJOR_VERSION_V1
        || root.reserved() != 0
        || root.descriptor_contract_revision() != DESCRIPTOR_CONTRACT_REVISION_V1
        || root.ui_abi_fingerprint_sha256() != ROption::RNone
    {
        return Err("folder-size visual column root data is not the fixed data-only V1 contract".into());
    }
    let registrar = root.create_registrar().create().into_result()
        .map_err(|error| format!("P0 registrar factory failed: {error:?}"))?;
    let mut mismatched = registrar_request_v1();
    mismatched.abi_schema = AbiSchemaIdV1::new(0x5345, 2);
    match registrar.register(mismatched).into_result() {
        Err(error) if error.code == AbiErrorCodeV1::SCHEMA_MISMATCH => {}
        Err(error) => return Err(format!("schema mismatch returned code {}", error.code.into_raw())),
        Ok(_) => return Err("schema mismatch unexpectedly registered".into()),
    }
    if marker.exists() {
        return Err("schema mismatch invoked the registrar marker".into());
    }
    let output = registrar.register(registrar_request_v1()).into_result()
        .map_err(|error| format!("matching registrar failed: {error:?}"))?;
    if output.outcome != RegistrationOutcomeV1::accepted(2) || output.contributions.len() != 2 {
        return Err("matching registrar returned the wrong descriptor batch".into());
    }
    if !marker.exists() {
        return Err("matching fingerprint did not invoke the registrar marker".into());
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let plugin = arguments.next().ok_or("usage: rust-folder-size-visual-column-contract-host <plugin.dll> <marker>")?;
    let marker = arguments.next().ok_or("usage: rust-folder-size-visual-column-contract-host <plugin.dll> <marker>")?;
    if arguments.next().is_some() {
        return Err("too many arguments".into());
    }
    run(Path::new(&plugin), Path::new(&marker))
}
'@
    Write-Utf8NoBom (Join-Path $contractHost 'src\main.rs') $hostSource

    # CARGO_HOME is a fresh, isolated directory containing only this explicit
    # offline source policy; it has no inherited registry cache or credentials.
    $env:CARGO_HOME = $cargoHome
    Push-Location $consumer
    try {
        & cargo.exe build --release --locked --offline --target ([string]$lock.toolchain.target)
        if ($LASTEXITCODE -ne 0) { throw 'materialized folder-size visual column did not build locked and offline' }
    } finally { Pop-Location }

    Push-Location $contractHost
    try {
        & cargo.exe generate-lockfile --offline
        if ($LASTEXITCODE -ne 0) { throw 'folder-size visual column ABI contract host lock generation failed offline' }
        & cargo.exe build --locked --offline
        if ($LASTEXITCODE -ne 0) { throw 'folder-size visual column ABI contract host did not build locked and offline' }
    } finally { Pop-Location }

    $pluginDll = Join-Path $consumer 'target\x86_64-pc-windows-msvc\release\rust_folder_size_visual_column.dll'
    $hostExe = Join-Path $contractHost 'target\debug\rust-folder-size-visual-column-contract-host.exe'
    if (-not (Test-Path -LiteralPath $pluginDll)) { throw 'folder-size visual column did not export the expected cdylib' }
    if (-not (Test-Path -LiteralPath $hostExe)) { throw 'contract host executable was not produced' }
    $env:RUST_FOLDER_SIZE_REGISTRAR_MARKER = $marker
    & $hostExe $pluginDll $marker
    if ($LASTEXITCODE -ne 0) { throw 'folder-size visual column ABI root/data pre-callback contract failed' }
    if ((Get-Content -LiteralPath $marker -Raw).Trim() -ne 'rust folder-size visual column registrar invoked') {
        throw 'folder-size visual column registrar marker had unexpected content'
    }
    Write-Host 'Rust folder-size visual column standalone Rust-first abi_stable root contract passed.'
} finally {
    [Environment]::SetEnvironmentVariable('CARGO_HOME', $savedCargoHome, 'Process')
    [Environment]::SetEnvironmentVariable('RUST_FOLDER_SIZE_REGISTRAR_MARKER', $null, 'Process')
    if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Recurse -Force }
}
