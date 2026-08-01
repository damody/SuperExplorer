[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repoRoot = (Resolve-Path (Join-Path $sdkRoot '..')).Path
$hostManifest = Join-Path $sdkRoot 'fixtures\abi-root-host\Cargo.toml'
$pluginManifest = Join-Path $sdkRoot 'fixtures\abi-root-plugin\Cargo.toml'
$cargoHome = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fixture-cargo-' + [guid]::NewGuid().ToString('N'))
$hostTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fixture-host-' + [guid]::NewGuid().ToString('N'))
$pluginTarget = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fixture-plugin-' + [guid]::NewGuid().ToString('N'))
$oldCargoHome = $env:CARGO_HOME; $oldTarget = $env:CARGO_TARGET_DIR
function Remove-VerifiedTempDirectory([string]$Path, [string]$Label) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw "Cannot clean ${Label}: target does not exist." }
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or $resolved -eq $tempRoot.TrimEnd('\')) { throw "Refusing to clean $Label outside temp root: $resolved" }
    try { Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction Stop } catch { throw "Failed to clean $Label '$resolved': $($_.Exception.Message)" }
}
$locationPushed = $false; $createdTemp = @()
try {
    Push-Location $sdkRoot; $locationPushed = $true
    New-Item -ItemType Directory -Path $cargoHome | Out-Null; $createdTemp += $cargoHome
    New-Item -ItemType Directory -Path $hostTarget | Out-Null; $createdTemp += $hostTarget
    New-Item -ItemType Directory -Path $pluginTarget | Out-Null; $createdTemp += $pluginTarget
    $env:CARGO_HOME = $cargoHome
    $env:CARGO_TARGET_DIR = $hostTarget
    & cargo build --manifest-path $hostManifest --locked --offline | Out-Host
    if ($LASTEXITCODE) { throw "host fixture build failed with exit code $LASTEXITCODE" }
    $env:CARGO_TARGET_DIR = $pluginTarget
    & cargo build --manifest-path $pluginManifest --locked --offline | Out-Host
    if ($LASTEXITCODE) { throw "plugin fixture build failed with exit code $LASTEXITCODE" }
    $targetTriple = 'x86_64-pc-windows-msvc'
    $hostExe = Join-Path $hostTarget "$targetTriple\debug\abi-root-fixture-host.exe"
    $pluginDll = Join-Path $pluginTarget "$targetTriple\debug\abi_root_fixture_plugin.dll"
    if (-not (Test-Path $hostExe) -or -not (Test-Path $pluginDll)) { throw 'isolated fixture artifacts were not produced.' }
    & $hostExe compatible $pluginDll
    if ($LASTEXITCODE) { throw "isolated host failed to load plugin (exit code $LASTEXITCODE)" }
    [pscustomobject]@{ Status='ok'; Offline='verified'; HostTarget=$hostTarget; PluginTarget=$pluginTarget; Plugin=$pluginDll }
} finally {
    if ($null -eq $oldCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME=$oldCargoHome }
    if ($null -eq $oldTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR=$oldTarget }
    if ($locationPushed) { Pop-Location }
    $cleanupErrors = @()
    foreach ($entry in @(@($cargoHome,'CARGO_HOME'),@($hostTarget,'host target'),@($pluginTarget,'plugin target'))) {
        if ($entry[0] -notin $createdTemp) { continue }
        try { Remove-VerifiedTempDirectory $entry[0] $entry[1] } catch { $cleanupErrors += $_.Exception.Message }
    }
    if ($cleanupErrors.Count) { throw ($cleanupErrors -join '; ') }
}
