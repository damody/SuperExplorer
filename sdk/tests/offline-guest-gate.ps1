[CmdletBinding()]
param(
    [string]$SdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path,
    [Parameter(Mandatory)][string]$AttestationPath,
    [Parameter(Mandatory)][string]$ArtifactOutputRoot,
    [string]$RunNonce = ([guid]::NewGuid().ToString('N'))
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-DefaultRouteEvidence {
    $routes = @(Get-NetRoute -ErrorAction Stop | Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0', '::/0') })
    return @($routes | ForEach-Object { [ordered]@{ destination = $_.DestinationPrefix; next_hop = $_.NextHop; interface_index = $_.InterfaceIndex } })
}

function Assert-NetworkIsolated([string]$Phase) {
    $adapters = @(Get-NetAdapter -ErrorAction Stop | Where-Object { $_.Status -eq 'Up' })
    $routes = @(Get-DefaultRouteEvidence)
    if ($adapters.Count -ne 0) { throw "$Phase guest has enabled NIC(s): $($adapters.Name -join ', ')" }
    if ($routes.Count -ne 0) { throw "$Phase guest has default route(s)" }
    return [ordered]@{ nics = $adapters.Count; routes = $routes }
}

function Test-TcpEgressBlocked {
    $client = [Net.Sockets.TcpClient]::new()
    try {
        $task = $client.ConnectAsync('1.1.1.1', 443)
        return -not ($task.Wait(3000) -and $client.Connected)
    } catch {
        return $true
    } finally {
        $client.Dispose()
    }
}

function Assert-BlockedEgress {
    if (-not (Test-TcpEgressBlocked)) { throw 'direct TCP egress succeeded' }
    $child = Start-Process -FilePath powershell.exe -PassThru -Wait -NoNewWindow -ArgumentList @(
        '-NoProfile', '-NonInteractive', '-Command',
        '$c=[Net.Sockets.TcpClient]::new();try{$t=$c.ConnectAsync(''1.1.1.1'',443);if($t.Wait(3000) -and $c.Connected){exit 1};exit 0}catch{exit 0}finally{$c.Dispose()}'
    )
    if ($child.ExitCode -ne 0) { throw "child TCP egress succeeded (exit $($child.ExitCode))" }
}

function Assert-OfflineEvidence($Evidence) {
    if ($Evidence.schema_version -ne 1 -or -not $Evidence.cargo.home_initially_empty -or $Evidence.cargo.home_entry_count_after_build -lt 0 -or -not $Evidence.cargo.target_directories_distinct) { throw 'offline build evidence is incomplete' }
    foreach ($hash in @($Evidence.artifacts.host.sha256, $Evidence.artifacts.plugin.sha256)) { if ([string]$hash -notmatch '^[0-9a-f]{64}$') { throw 'artifact hash is invalid' } }
    if ($Evidence.load.mode -ne 'compatible' -or $Evidence.load.exit_code -ne 0) { throw 'fixture host did not complete a real compatible load' }
    foreach ($command in @($Evidence.cargo.host_command, $Evidence.cargo.plugin_command)) { if ([string]$command -notmatch '--locked --offline') { throw 'offline build command was not exact' } }
}

$SdkRoot = (Resolve-Path -LiteralPath $SdkRoot).Path
$RunNonce = $RunNonce.ToLowerInvariant()
if ($RunNonce -notmatch '^[0-9a-f]{32}$') { throw 'offline guest run nonce must be a GUID without separators' }
$evidencePath = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-offline-build-' + [guid]::NewGuid().ToString('N') + '.json')
try {
    $before = Assert-NetworkIsolated 'before'
    Assert-BlockedEgress
    & powershell.exe -NoProfile -File (Join-Path $SdkRoot 'tests\offline-host-plugin-contract.ps1') -EvidencePath $evidencePath -ArtifactOutputRoot $ArtifactOutputRoot
    if ($LASTEXITCODE -ne 0) { throw "offline host/plugin contract failed with exit code $LASTEXITCODE" }
    $evidence = Get-Content -LiteralPath $evidencePath -Raw | ConvertFrom-Json
    Assert-OfflineEvidence $evidence
    Assert-BlockedEgress
    $after = Assert-NetworkIsolated 'after'
    $bundle = Join-Path $SdkRoot 'bundle-manifest.json'
    $bundleManifest = Get-Content -LiteralPath $bundle -Raw | ConvertFrom-Json
    if ([string]$bundleManifest.inventory_root_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'copied bundle manifest has no valid inventory root hash' }
    $attestation = [ordered]@{
        schema_version = 2
        producer = 'sdk/tests/offline-guest-gate.ps1'
        guest_run_nonce = $RunNonce
        bundle_sha256 = (Get-FileHash -LiteralPath $bundle -Algorithm SHA256).Hash.ToLowerInvariant()
        copied_inventory_root_sha256 = [string]$bundleManifest.inventory_root_sha256
        network = [ordered]@{ before_nics = $before.nics; after_nics = $after.nics; routes = @($before.routes + $after.routes) }
        egress_attempts = [ordered]@{ direct = 'blocked'; child = 'blocked' }
        cargo = $evidence.cargo
        artifacts = $evidence.artifacts
        load = $evidence.load
    }
    $parent = Split-Path -Parent $AttestationPath
    if ($parent -and -not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    [IO.File]::WriteAllText($AttestationPath, ($attestation | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
    Write-Output 'offline guest gate passed'
} finally {
    if (Test-Path -LiteralPath $evidencePath) { Remove-Item -LiteralPath $evidencePath -Force }
}
