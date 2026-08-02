$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$scripts = Join-Path $sdk 'scripts'
$fixture = Join-Path $sdk 'fixtures\p0-consumer'
foreach ($script in @('build-plugin.ps1','validate-plugin.ps1','package-plugin.ps1')) {
    [scriptblock]::Create((Get-Content (Join-Path $scripts $script) -Raw)) | Out-Null
}

function Assert-Fails([scriptblock]$Action, [string]$Case) {
    $failed = $false
    try { & $Action } catch { $failed = $true }
    if (-not $failed) { throw "$Case unexpectedly succeeded" }
}

Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot (Join-Path $fixture 'missing') } 'missing root'

$temp = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-p0-consumer-' + [guid]::NewGuid().ToString('N'))
Copy-Item -LiteralPath $fixture -Destination $temp -Recurse
if (Test-Path -LiteralPath (Join-Path $temp 'target')) { Remove-Item -LiteralPath (Join-Path $temp 'target') -Recurse -Force }
try {
    $lock = Get-Content (Join-Path $sdk 'sdk-lock.json') -Raw | ConvertFrom-Json
    $fingerprint = Get-Content (Join-Path $sdk 'ui-abi-fingerprint.json') -Raw | ConvertFrom-Json
    $source = Join-Path $temp 'src\lib.rs'
    $manifestPath = Join-Path $temp 'plugin-project.json'
    $positive = (Get-Content $manifestPath -Raw).
        Replace('@SDK_BUNDLE_ID@', [string]$lock.bundle_id).
        Replace('@UI_ABI_FINGERPRINT@', [string]$fingerprint.fingerprint).
        Replace('@ABI_SCHEMA@', [string]$lock.build_policy.abi_schema_version).
        Replace('@SOURCE_SIZE@', [string](Get-Item $source).Length).
        Replace('@SOURCE_SHA256@', (Get-FileHash $source -Algorithm SHA256).Hash.ToLowerInvariant())
    [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))

    & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp

    $manifest = $positive | ConvertFrom-Json
    $manifest | Add-Member -NotePropertyName program -NotePropertyValue 'cmd.exe'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'unknown command injection field'

    $manifest = $positive | ConvertFrom-Json
    $manifest.payloads[0].path = '../escape.dll'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'unsafe payload path'

    $manifest = $positive | ConvertFrom-Json
    $manifest.sdk.bundle_id = 'wrong-bundle'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'wrong SDK bundle'

    $manifest = $positive | ConvertFrom-Json
    $manifest.payloads[0].sha256 = '0' * 64
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'payload hash drift'

    $manifest = $positive | ConvertFrom-Json
    $manifest.verification.requirements[0].requirement_id = 'unknown/requirement'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'unknown trusted gate mapping'

    $originalSource = Get-Content -LiteralPath $source -Raw
    $missingRootSource = $originalSource.Replace('#[export_root_module]', '')
    if ($missingRootSource -eq $originalSource) { throw 'P0 fixture no longer contains the expected root export attribute' }
    [IO.File]::WriteAllText($source, $missingRootSource, [Text.UTF8Encoding]::new($false))
    $missingRootManifest = $positive | ConvertFrom-Json
    $missingRootManifest.payloads[0].size = (Get-Item -LiteralPath $source).Length
    $missingRootManifest.payloads[0].sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($manifestPath, ($missingRootManifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp } 'cdylib without abi_stable loader export'
    $unexpectedBuild = Join-Path $temp ("target\superexplorer\$($lock.bundle_id)\reports\build.json")
    if (Test-Path -LiteralPath $unexpectedBuild) { throw 'failed ABI inspection published a build report' }

    [IO.File]::WriteAllText($source, $originalSource, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))
    & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp | Out-Null
    $buildReport = Join-Path $temp ("target\superexplorer\$($lock.bundle_id)\reports\build.json")
    if (-not (Test-Path -LiteralPath $buildReport)) { throw 'build report was not retained' }
    $package = & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp
    $firstPackageHash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash
    $secondPackage = & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp
    if ($package -ne $secondPackage -or $firstPackageHash -ne (Get-FileHash -LiteralPath $secondPackage -Algorithm SHA256).Hash) {
        throw 'repeated packaging was not byte-identical'
    }
    [IO.File]::AppendAllText((Join-Path $temp "target\superexplorer\$($lock.bundle_id)\build\plugin.dll"), 'tamper')
    Assert-Fails { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp } 'changed DLL after build'
    if ($firstPackageHash -ne (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash) {
        throw 'failed packaging changed the existing package'
    }
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}

Write-Output 'plugin tooling wrapper self-test passed'
