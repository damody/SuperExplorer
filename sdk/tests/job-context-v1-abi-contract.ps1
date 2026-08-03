$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $sdkRoot 'fixtures\job-context-v1-contract'
$newPluginRoot = Join-Path $fixtureRoot 'new-plugin'
$hostRoot = Join-Path $fixtureRoot 'current-host'
$oldPluginRoot = Join-Path $sdkRoot 'fixtures\extension-api-contract\old-v1-plugin'
$vendor = Join-Path $sdkRoot 'vendor\cargo-sources'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-job-context-v1-' + [Guid]::NewGuid().ToString('N'))
$cargoHome = Join-Path $tempRoot 'cargo-home'
$newPluginTarget = Join-Path $tempRoot 'target-new-plugin'
$oldPluginTarget = Join-Path $tempRoot 'target-old-plugin'
$hostTarget = Join-Path $tempRoot 'target-host'
$savedCargoHome = $env:CARGO_HOME
$savedCargoTargetDir = $env:CARGO_TARGET_DIR

function Fail([string] $Message) { throw $Message }

function Invoke-CargoBuild([string] $Project, [string] $TargetDir) {
    $env:CARGO_HOME = $cargoHome
    $env:CARGO_TARGET_DIR = $TargetDir
    Push-Location $Project
    try {
        & cargo.exe build --locked --offline --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { Fail "cargo build failed for $Project (exit $LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
}

function Find-Artifact([string] $TargetDir, [string] $Leaf) {
    $path = Join-Path $TargetDir ('x86_64-pc-windows-msvc\debug\' + $Leaf)
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "missing build artifact: $path" }
    return (Resolve-Path -LiteralPath $path).Path
}

function Invoke-Host([string[]] $Arguments) {
    & $hostExe @Arguments
    if ($LASTEXITCODE -ne 0) { Fail "job-context fixture host failed: $($Arguments -join ' ') (exit $LASTEXITCODE)" }
}

try {
    New-Item -ItemType Directory -Path $cargoHome, $newPluginTarget, $oldPluginTarget, $hostTarget -Force | Out-Null
    $cargoConfig = @(
        '[source.crates-io]',
        'replace-with = "cargo-sources"',
        '[source.cargo-sources]',
        ('directory = "' + ($vendor -replace '\\', '/') + '"')
    ) -join [Environment]::NewLine
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), $cargoConfig, [Text.UTF8Encoding]::new($false))

    Invoke-CargoBuild $oldPluginRoot $oldPluginTarget
    Invoke-CargoBuild $newPluginRoot $newPluginTarget
    Invoke-CargoBuild $hostRoot $hostTarget

    $oldPlugin = Find-Artifact $oldPluginTarget 'extension_api_contract_old_v1_plugin.dll'
    $newPlugin = Find-Artifact $newPluginTarget 'job_context_v1_contract_new_plugin.dll'
    $hostExe = Find-Artifact $hostTarget 'job-context-v1-contract-host.exe'
    $layoutOutput = & $hostExe layout
    if ($LASTEXITCODE -ne 0) { Fail "job-context fixture host layout gate failed (exit $LASTEXITCODE)" }
    $layoutText = $layoutOutput -join "`n"
    $layoutBytes = [Text.Encoding]::UTF8.GetBytes($layoutText)
    $layoutHasher = [Security.Cryptography.SHA256]::Create()
    try {
        $layoutHash = ([BitConverter]::ToString($layoutHasher.ComputeHash($layoutBytes))).Replace('-', '').ToLowerInvariant()
    } finally {
        $layoutHasher.Dispose()
    }
    if ($layoutHash -ne '4d5dcc819c91ce1bac2160bd1d5d4f73befaa17bca971dcfacf9df394fffb703') {
        Fail "job-context v1 ABI layout/numeric output changed: $layoutHash"
    }
    Invoke-Host @('transport', $newPlugin)
    Invoke-Host @('old', $oldPlugin)
    Invoke-Host @('new', $newPlugin)
    Write-Output 'job context v1 ABI contract: PASS'
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    if ($null -eq $savedCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedCargoHome }
    if ($null -eq $savedCargoTargetDir) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedCargoTargetDir }
}
