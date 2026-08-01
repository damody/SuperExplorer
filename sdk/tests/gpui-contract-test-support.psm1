function Assert-ExactGpuiFeatureSet {
    param(
        [string[]]$Actual,
        [string[]]$Expected
    )

    $actualSet = @($Actual | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
    $expectedSet = @($Expected | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
    if (($actualSet -join "`n") -ne ($expectedSet -join "`n")) {
        throw "GPUI production feature set mismatch: expected [$($expectedSet -join ', ')], got [$($actualSet -join ', ')]."
    }
}

function Assert-ApprovedGpuiMetadata {
    param(
        [Parameter(Mandatory)]$Metadata,
        [Parameter(Mandatory)][string]$ExpectedVersion,
        [Parameter(Mandatory)][string]$ExpectedManifestPath
    )
    $gpuiPackages = @($Metadata.packages | Where-Object name -eq 'gpui')
    if ($gpuiPackages.Count -ne 1) { throw "Production metadata must contain exactly one GPUI package; found $($gpuiPackages.Count)." }
    $gpui = $gpuiPackages[0]
    if ($gpui.version -ne $ExpectedVersion) { throw "GPUI version mismatch: expected $ExpectedVersion, got $($gpui.version)." }
    if ([IO.Path]::GetFullPath($gpui.manifest_path) -ne [IO.Path]::GetFullPath($ExpectedManifestPath)) {
        throw "GPUI manifest path is not the approved vendored package: $($gpui.manifest_path)."
    }
    $gpuiNodes = @($Metadata.resolve.nodes | Where-Object id -eq $gpui.id)
    if ($gpuiNodes.Count -ne 1) { throw "Production metadata must contain exactly one resolved GPUI node; found $($gpuiNodes.Count)." }
    $apps = @($Metadata.packages | Where-Object name -eq 'explorer-app')
    if ($apps.Count -ne 1) { throw "Production metadata must contain exactly one explorer-app package; found $($apps.Count)." }
    $nodesById = @{}
    foreach ($node in $Metadata.resolve.nodes) { $nodesById[$node.id] = $node }
    $pending = [Collections.Generic.Queue[string]]::new()
    $pending.Enqueue($apps[0].id)
    $visited = @{}
    while ($pending.Count -gt 0) {
        $id = $pending.Dequeue()
        if ($visited.ContainsKey($id)) { continue }
        $visited[$id] = $true
        if ($nodesById.ContainsKey($id)) {
            foreach ($dependency in $nodesById[$id].dependencies) { $pending.Enqueue($dependency) }
        }
    }
    if (-not $visited.ContainsKey($gpui.id)) { throw 'The approved GPUI package is not reachable from explorer-app.' }
    $gpuiNodes[0]
}

function Get-GpuiProductionFeatures {
    param(
        [Parameter(Mandatory)][string]$CargoTreeOutput,
        [Parameter(Mandatory)][string]$ExpectedVersion
    )
    $versionPattern = [regex]::Escape($ExpectedVersion)
    $lines = @($CargoTreeOutput -split '\r?\n' | Where-Object { $_ -match "^gpui v$versionPattern .*\|" })
    if ($lines.Count -eq 0) { throw 'cargo tree did not contain the approved GPUI package in explorer-app normal/build dependencies.' }
    @($lines | ForEach-Object {
        $featureText = $_.Substring($_.LastIndexOf('|') + 1)
        $featureText -split ',' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | ForEach-Object Trim
    })
}

Export-ModuleMember -Function Assert-ExactGpuiFeatureSet,Assert-ApprovedGpuiMetadata,Get-GpuiProductionFeatures
