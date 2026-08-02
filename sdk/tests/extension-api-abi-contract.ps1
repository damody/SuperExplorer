$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $sdkRoot 'fixtures\extension-api-contract'
$oldPluginRoot = Join-Path $fixtureRoot 'old-v1-plugin'
$hostRoot = Join-Path $fixtureRoot 'current-host'
$vendor = Join-Path $sdkRoot 'vendor\cargo-sources'
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('superexplorer-extension-api-' + [Guid]::NewGuid().ToString('N'))
$cargoHome = Join-Path $tempRoot 'cargo-home'; $pluginTarget = Join-Path $tempRoot 'target-old-v1-plugin'; $hostTarget = Join-Path $tempRoot 'target-current-host'; $markerRoot = Join-Path $tempRoot 'markers'
$savedCargoHome = $env:CARGO_HOME; $savedCargoTargetDir = $env:CARGO_TARGET_DIR
function Fail([string] $Message) { throw $Message }
function Invoke-CargoBuild([string] $Project, [string] $TargetDir) {
    $env:CARGO_HOME = $cargoHome; $env:CARGO_TARGET_DIR = $TargetDir; Push-Location $Project
    try { & cargo build --locked --offline --target x86_64-pc-windows-msvc; if ($LASTEXITCODE -ne 0) { Fail "cargo build failed for $Project (exit $LASTEXITCODE)" } } finally { Pop-Location }
}
function Find-Artifact([string] $TargetDir, [string] $Leaf) {
    $path = Join-Path $TargetDir ('x86_64-pc-windows-msvc\debug\' + $Leaf); if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "missing build artifact: $path" }; return (Resolve-Path -LiteralPath $path).Path
}
function Invoke-Host([string] $Mode, [string] $Plugin, [bool] $ExpectProcessSuccess, [bool] $ExpectMarker) {
    $marker = Join-Path $markerRoot ($Mode + '.marker'); Remove-Item -LiteralPath $marker -Force -ErrorAction SilentlyContinue
    $psi = [System.Diagnostics.ProcessStartInfo]::new(); $psi.FileName = $hostExe; $psi.UseShellExecute = $false
    foreach ($argument in @($Mode, $Plugin, $marker)) { if ($argument.Contains('"')) { Fail 'contract argument contains an unsupported quote' } }
    $psi.Arguments = ('"{0}" "{1}" "{2}"' -f $Mode, $Plugin, $marker)
    $savedMode = $env:EXTENSION_API_CONTRACT_MODE; $savedMarker = $env:EXTENSION_API_CONTRACT_MARKER
    try {
        $env:EXTENSION_API_CONTRACT_MODE = $Mode; $env:EXTENSION_API_CONTRACT_MARKER = $marker
        $proc = [System.Diagnostics.Process]::Start($psi); $proc.WaitForExit(); $ok = $proc.ExitCode -eq 0
    } finally {
        if ($null -eq $savedMode) { Remove-Item Env:EXTENSION_API_CONTRACT_MODE -ErrorAction SilentlyContinue } else { $env:EXTENSION_API_CONTRACT_MODE = $savedMode }
        if ($null -eq $savedMarker) { Remove-Item Env:EXTENSION_API_CONTRACT_MARKER -ErrorAction SilentlyContinue } else { $env:EXTENSION_API_CONTRACT_MARKER = $savedMarker }
    }
    if ($ok -ne $ExpectProcessSuccess) { Fail "$Mode process success was $ok, expected $ExpectProcessSuccess (exit $($proc.ExitCode))" }
    $markerExists = Test-Path -LiteralPath $marker
    if ($markerExists -ne $ExpectMarker) { Fail "$Mode marker presence was $markerExists, expected $ExpectMarker" }
}
try {
    New-Item -ItemType Directory -Path $cargoHome, $pluginTarget, $hostTarget, $markerRoot -Force | Out-Null
    $cargoConfig = @('[source.crates-io]','replace-with = "cargo-sources"','[source.cargo-sources]',('directory = "' + ($vendor -replace '\\','/') + '"')) -join [Environment]::NewLine
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), $cargoConfig, [Text.UTF8Encoding]::new($false))
    Invoke-CargoBuild $oldPluginRoot $pluginTarget; Invoke-CargoBuild $hostRoot $hostTarget
    $pluginDll = Find-Artifact $pluginTarget 'extension_api_contract_old_v1_plugin.dll'; $hostExe = Find-Artifact $hostTarget 'extension-api-contract-host.exe'
    Invoke-Host 'compatible' $pluginDll $true $true
    Invoke-Host 'schema-mismatch' $pluginDll $true $false
    Invoke-Host 'root-contract-mismatch' $pluginDll $true $false
    Invoke-Host 'sdk-major-mismatch' $pluginDll $true $false
    Invoke-Host 'panic' $pluginDll $true $true
    Invoke-Host 'raw-panic' $pluginDll $false $false
    Write-Output 'extension API ABI contract: PASS'
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    if ($null -eq $savedCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedCargoHome }
    if ($null -eq $savedCargoTargetDir) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedCargoTargetDir }
}
