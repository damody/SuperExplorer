Set-StrictMode -Version Latest

function Get-DependencyEdgeDigest {
    param([Parameter(Mandatory)]$Metadata,[Parameter(Mandatory)][string]$RepoRoot,[Parameter(Mandatory)]$Snapshot,[Parameter(Mandatory)][string]$LockText)
    $checksums = @{}
    foreach ($block in [regex]::Matches($LockText, '(?ms)^\[\[package\]\].*?(?=^\[\[package\]\]|\z)')) {
        $nm = [regex]::Match($block.Value, 'name = "([^"]+)"'); $vm = [regex]::Match($block.Value, 'version = "([^"]+)"'); $cm = [regex]::Match($block.Value, 'checksum = "([^"]+)"')
        $sm = [regex]::Match($block.Value, 'source = "([^"]+)"')
        if ($nm.Success -and $vm.Success -and $sm.Success -and $cm.Success) { $checksums["$($nm.Groups[1].Value)|$($vm.Groups[1].Value)|$($sm.Groups[1].Value)"] = $cm.Groups[1].Value }
    }
    $packages = @{}
    foreach ($p in $Metadata.packages) {
        if ($p.source -like 'registry+*') {
            $key = "$($p.name)|$($p.version)|$($p.source)"; if (-not $checksums.ContainsKey($key)) { throw "Registry package lacks canonical checksum: $key" }
            $packages[$p.id] = "registry|$($p.source)|$($p.name)|$($p.version)|checksum=$($checksums[$key])"
        } elseif ($p.source -like 'git+*') {
            if ($p.source -notmatch '#[0-9a-f]{40}$') { throw "Git package lacks canonical full revision: $($p.id)" }
            $packages[$p.id] = "git|$($p.name)|$($p.version)|$($p.source)"
        } else {
            $baseUri = [Uri]($RepoRoot.TrimEnd('\') + '\')
            $relative = $baseUri.MakeRelativeUri([Uri]$p.manifest_path).ToString()
            if ($p.name -eq 'gpui') { $relative = 'vendor/gpui-ce/crates/gpui'; $packages[$p.id] = "path|gpui|$($p.version)|$relative|rev=$($Snapshot.source.revision)|tree=$($Snapshot.source.tree)" }
            else { $packages[$p.id] = "path|$($p.name)|$($p.version)|$relative" }
        }
    }
    $lines = @($Metadata.resolve.nodes | ForEach-Object {
        $nodeId = $packages[$_.id]
        $features = ((@($_.features) | Sort-Object) -join ',')
        $edges = @($_.deps | ForEach-Object {
            $kinds = @($_.dep_kinds | ForEach-Object { "kind=$($_.kind);target=$($_.target)" } | Sort-Object) -join '&'
            "$($packages[$_.pkg])[$kinds]"
        } | Sort-Object) -join ','
        "$nodeId|features=$features|deps=$edges"
    } | Sort-Object)
    $bytes = [Text.Encoding]::UTF8.GetBytes(($lines -join "`n"))
    (([Security.Cryptography.SHA256]::Create().ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join '')
}

function Assert-ProtectedDependencyMetadata {
    param(
        [Parameter(Mandatory)]$Metadata,
        [Parameter(Mandatory)]$Closure,
        [Parameter(Mandatory)][string]$RepoRoot,
        [Parameter(Mandatory)]$Snapshot,
        [Parameter(Mandatory)][string]$LockText
    )
    if ($Closure.schema_version -ne 2 -or $Closure.algorithm -ne 'normalized-package-edges-v2') { throw 'protected closure schema/algorithm drifted.' }
    if ($Closure.abi_stable.default_features -ne $false -or $Closure.gpui.default_features -ne $false) { throw 'protected closure default_features must be false.' }
    if (@($Closure.gpui.features).Count -ne 0) { throw 'protected closure GPUI features must be empty.' }
    $abi = @($Metadata.packages | Where-Object name -eq 'abi_stable')
    $gpui = @($Metadata.packages | Where-Object name -eq 'gpui')
    if ($abi.Count -ne 1) { throw "abi_stable package count drifted: $($abi.Count)" }
    if ($gpui.Count -ne 1) { throw "gpui package count drifted: $($gpui.Count)" }
    if ($abi[0].version -ne $Closure.abi_stable.version -or $abi[0].source -ne $Closure.abi_stable.source) { throw 'abi_stable version/source drifted.' }
    if ($gpui[0].version -ne $Closure.gpui.version) { throw 'gpui version drifted.' }
    $abiId = [string]$abi[0].id; $gpuiId = [string]$gpui[0].id
    $abiNode = @($Metadata.resolve.nodes | Where-Object id -eq $abiId)
    $gpuiNode = @($Metadata.resolve.nodes | Where-Object id -eq $gpuiId)
    if ($abiNode.Count -ne 1 -or $gpuiNode.Count -ne 1) { throw 'protected package resolve node identity/count drifted.' }
    $abiNode = $abiNode[0]; $gpuiNode = $gpuiNode[0]
    if (@($abiNode.features).Count -ne 0 -or @($gpuiNode.features).Count -ne 0) { throw 'protected production features must be empty.' }
    $root = @($Metadata.packages | Where-Object { $_.name -eq 'superexplorer-sdk-bootstrap' })[0]
    $abiDep = @($root.dependencies | Where-Object name -eq 'abi_stable')[0]; $gpuiDep = @($root.dependencies | Where-Object name -eq 'gpui')[0]
    if (-not $abiDep -or -not $gpuiDep -or $abiDep.uses_default_features -or $gpuiDep.uses_default_features) { throw 'protected root default_features drifted.' }
    $roots = @($Closure.required_roots | Sort-Object) -join ','
    if ($roots -ne 'abi_stable@0.11.3,gpui@0.2.2') { throw 'protected closure required roots drifted.' }
    if ((Get-DependencyEdgeDigest $Metadata $RepoRoot $Snapshot $LockText) -ne $Closure.edge_digest) { throw 'protected dependency edge digest drifted.' }
    [pscustomobject]@{ Status = 'ok'; EdgeDigest = $Closure.edge_digest; PackageCount = $Metadata.packages.Count }
}

Export-ModuleMember -Function Assert-ProtectedDependencyMetadata,Get-DependencyEdgeDigest
