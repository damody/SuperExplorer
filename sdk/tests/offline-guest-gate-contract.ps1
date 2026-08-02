[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$gate = Get-Content -LiteralPath (Join-Path $repo 'sdk\tests\offline-guest-gate.ps1') -Raw
$fixture = Get-Content -LiteralPath (Join-Path $repo 'sdk\tests\offline-host-plugin-contract.ps1') -Raw
$template = Get-Content -LiteralPath (Join-Path $repo 'sdk\ci\Invoke-OfflineSdkGuest.template.ps1') -Raw
$workflow = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\sdk-offline-windows.yml') -Raw

foreach ($required in @('Assert-NetworkIsolated', 'Assert-BlockedEgress', 'offline-host-plugin-contract.ps1', 'ArtifactOutputRoot', 'home_initially_empty', 'target_directories_distinct', 'Get-FileHash', 'copied_inventory_root_sha256', 'guest_run_nonce', "producer = 'sdk/tests/offline-guest-gate.ps1'")) {
    if ($gate -notlike "*$required*") { throw "offline guest gate lost required evidence: $required" }
}
foreach ($required in @('--locked --offline --target x86_64-pc-windows-msvc', 'Get-FileHash', 'compatible $pluginDll', 'home_initially_empty', 'target_directories_distinct')) {
    if ($fixture -notlike "*$required*") { throw "offline fixture contract lost required proof: $required" }
}
foreach ($required in @("Join-Path `$root 'vendor'", "Join-Path `$RepositoryRoot 'sdk'", "Join-Path `$RepositoryRoot 'vendor\gpui-ce'", "@('Cargo.toml','rust-toolchain.toml')", 'bundle-generator', 'offline-guest-gate.ps1', 'ArtifactOutputRoot $artifacts', 'retained host/plugin compatible load failed', 'independent guest artifact hashes do not match repo attestation', 'RunNonce $runNonce', 'guest_run_nonce -ne $nonce', 'copied guest artifact binding failed', 'Copy-Item -FromSession', 'do not synthesize success here')) {
    if ($template -notlike "*$required*") { throw "offline guest template lost repository-owned attestation boundary: $required" }
}

# Exercise PowerShell's directory-copy semantics used by the Hyper-V template.
# The destination parent must already exist or gpui-ce is renamed to vendor.
$shapeRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-guest-shape-' + [guid]::NewGuid().ToString('N'))
try {
    $source = Join-Path $shapeRoot 'source\vendor\gpui-ce'
    $destination = Join-Path $shapeRoot 'guest\repo\vendor'
    New-Item -ItemType Directory -Path $source,$destination -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $source 'shape-marker.txt'), 'gpui-ce', [Text.UTF8Encoding]::new($false))
    Copy-Item -Path $source -Destination $destination -Recurse -Force
    if (-not (Test-Path -LiteralPath (Join-Path $destination 'gpui-ce\shape-marker.txt'))) {
        throw 'offline guest copy does not preserve vendor/gpui-ce tree shape'
    }
} finally {
    if (Test-Path -LiteralPath $shapeRoot) { Remove-Item -LiteralPath $shapeRoot -Recurse -Force }
}
foreach ($required in @("schema_version -ne 2", "producer -ne 'sdk/tests/offline-guest-gate.ps1'", 'guest_run_nonce -notmatch', 'copied_inventory_root_sha256', 'cargo.home_initially_empty', 'artifacts.host.sha256', 'Offline copied artifact binding failed', 'load.exit_code')) {
    if ($workflow -notlike "*$required*") { throw "offline workflow lost fail-closed validation: $required" }
}

Write-Output 'offline guest gate contract passed'
