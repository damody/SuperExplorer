[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'protected-dependency-test-support.psm1') -Force
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $sdk
try {
    $m = (& cargo metadata --locked --offline --format-version 1 | Out-String) | ConvertFrom-Json
    $closure = Get-Content snapshot\protected-dependency-closure.json -Raw | ConvertFrom-Json
    $snapshot = Get-Content snapshot\approved-gpui.json -Raw | ConvertFrom-Json
    $lockText = Get-Content Cargo.lock -Raw
    $good = Assert-ProtectedDependencyMetadata $m $closure ((Resolve-Path '..').Path) $snapshot $lockText
    if ($good.Status -ne 'ok') { throw 'positive closure contract failed.' }
    $bad = $closure.PSObject.Copy(); $bad.edge_digest = ('0' * 64)
    $rejected = $false; try { Assert-ProtectedDependencyMetadata $m $bad ((Resolve-Path '..').Path) $snapshot $lockText } catch { $rejected = $true }
    if (!$rejected) { throw 'edge digest mismatch was accepted.' }; Write-Output 'rejected edge digest mismatch'
    $relocated = $m | ConvertTo-Json -Depth 100 | ConvertFrom-Json
    $idMap = @{}; $oldRoot = ((Resolve-Path '..').Path).Replace('\','/'); foreach ($p in $relocated.packages) { $old = [string]$p.id; $p.manifest_path = ([string]$p.manifest_path).Replace((Resolve-Path '..').Path, 'C:\relocated-sdk'); $p.dependencies | ForEach-Object { if ($_.path) { $_.path = ([string]$_.path).Replace((Resolve-Path '..').Path, 'C:\relocated-sdk') } }; $p.id = $old.Replace($oldRoot,'C:/relocated-sdk'); $idMap[$old]=$p.id }
    foreach ($n in $relocated.resolve.nodes) { $n.id = $idMap[[string]$n.id]; foreach ($d in $n.deps) { $d.pkg = $idMap[[string]$d.pkg] } }
    $relocatedResult = Assert-ProtectedDependencyMetadata $relocated $closure 'C:\relocated-sdk' $snapshot $lockText
    if ($relocatedResult.Status -ne 'ok') { throw 'relocated Assert did not pass.' }; Write-Output 'path relocation preserved digest and Assert status'
    $featureDrift = $m | ConvertTo-Json -Depth 100 | ConvertFrom-Json; $featureDrift.resolve.nodes[0].features = @('drift')
    $rejected=$false; try { Assert-ProtectedDependencyMetadata $featureDrift $closure ((Resolve-Path '..').Path) $snapshot $lockText } catch { $rejected=$true }; if(!$rejected){throw 'feature drift accepted.'}; Write-Output 'rejected feature drift'
    $kindDrift = $m | ConvertTo-Json -Depth 100 | ConvertFrom-Json; $kindDrift.resolve.nodes[0].deps[0].dep_kinds[0].target = 'cfg(drift)'
    $rejected=$false; try { Assert-ProtectedDependencyMetadata $kindDrift $closure ((Resolve-Path '..').Path) $snapshot $lockText } catch { $rejected=$true }; if(!$rejected){throw 'kind/target drift accepted.'}; Write-Output 'rejected kind/target drift'
    $schemaDrift = $closure.PSObject.Copy(); $schemaDrift.schema_version = 1
    $rejected=$false; try { Assert-ProtectedDependencyMetadata $m $schemaDrift ((Resolve-Path '..').Path) $snapshot $lockText } catch { $rejected=$true }; if(!$rejected){throw 'schema drift accepted.'}; Write-Output 'rejected schema drift'
    $rootDrift = $closure.PSObject.Copy(); $rootDrift.required_roots = @('abi_stable@0.11.3')
    $rejected=$false; try { Assert-ProtectedDependencyMetadata $m $rootDrift ((Resolve-Path '..').Path) $snapshot $lockText } catch { $rejected=$true }; if(!$rejected){throw 'root drift accepted.'}; Write-Output 'rejected root drift'
    foreach ($property in @('abi_stable','gpui')) { $defaultDrift = $closure.PSObject.Copy(); $defaultDrift.$property = $closure.$property.PSObject.Copy(); $defaultDrift.$property.default_features = $true; $rejected=$false; try { Assert-ProtectedDependencyMetadata $m $defaultDrift ((Resolve-Path '..').Path) $snapshot $lockText } catch { $rejected=$true }; if(!$rejected){throw "${property} default feature drift accepted."}; Write-Output "rejected ${property} default feature drift" }
    $featureClosure = $closure.PSObject.Copy(); $featureClosure.gpui = $closure.gpui.PSObject.Copy(); $featureClosure.gpui.features = @('drift'); $rejected=$false; try { Assert-ProtectedDependencyMetadata $m $featureClosure ((Resolve-Path '..').Path) $snapshot $lockText } catch { $rejected=$true }; if(!$rejected){throw 'closure GPUI feature drift accepted.'}; Write-Output 'rejected closure GPUI feature drift'
    $alternate = $m | ConvertTo-Json -Depth 100 | ConvertFrom-Json; $altPkg = @($alternate.packages | Where-Object name -eq 'abi_stable')[0]; $altOld = [string]$altPkg.id; $altPkg.source = 'registry+https://alternate.invalid/index'; $altPkg.id = $altOld -replace 'registry\+https://github.com/rust-lang/crates.io-index','registry+https://alternate.invalid/index'; foreach($n in $alternate.resolve.nodes){if($n.id -eq $altOld){$n.id=$altPkg.id};foreach($d in $n.deps){if($d.pkg -eq $altOld){$d.pkg=$altPkg.id}}}; $altLock = [regex]::Replace($lockText,'(?ms)(name = "abi_stable".*?source = ")registry\+https://github.com/rust-lang/crates.io-index','$1registry+https://alternate.invalid/index',1); if((Get-DependencyEdgeDigest $alternate ((Resolve-Path '..').Path) $snapshot $altLock) -eq $closure.edge_digest){throw 'alternate registry canonicalization did not change digest.'}; Write-Output 'alternate registry source/checksum identity distinguished'
    $gitFixture = $m | ConvertTo-Json -Depth 100 | ConvertFrom-Json; $gitPkg = @($gitFixture.packages | Where-Object source -like 'registry+*')[0]; $gitOld=[string]$gitPkg.id; $gitPkg.source='git+https://example.invalid/repo?rev=0000000000000000000000000000000000000001#0000000000000000000000000000000000000001'; $gitPkg.id='git+https://example.invalid/repo?rev=0000000000000000000000000000000000000001#0000000000000000000000000000000000000001'; foreach($n in $gitFixture.resolve.nodes){if($n.id -eq $gitOld){$n.id=$gitPkg.id};foreach($d in $n.deps){if($d.pkg -eq $gitOld){$d.pkg=$gitPkg.id}}}; [void](Get-DependencyEdgeDigest $gitFixture ((Resolve-Path '..').Path) $snapshot $lockText); Write-Output 'git full-revision canonicalization accepted'
} finally { Pop-Location }
