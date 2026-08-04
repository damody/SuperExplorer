$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $sdkRoot 'fixtures\job-context-v1-contract'
$newPluginRoot = Join-Path $fixtureRoot 'new-plugin'
$hostRoot = Join-Path $fixtureRoot 'current-host'
# ABI v1 is intentionally unpublished at this stage. The stateful registrar
# object supersedes the former raw callback root, so this transport fixture
# verifies only the current root/provider shape; legacy-root coverage belongs
# to the separate pre-publication migration fixture.
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-job-context-v1-' + [Guid]::NewGuid().ToString('N'))
$cargoHome = if ([string]::IsNullOrWhiteSpace($env:CARGO_HOME)) {
    Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) '.cargo'
} else {
    $env:CARGO_HOME
}
$newPluginTarget = Join-Path $tempRoot 'target-new-plugin'
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

function Invoke-PanicLifecycle([string] $Case, [string] $ExpectedMarker) {
    $marker = Join-Path $tempRoot ($Case + '.marker')
    $savedMode = $env:JOB_CONTEXT_V1_MODE
    $savedMarker = $env:JOB_CONTEXT_V1_MARKER
    try {
        $env:JOB_CONTEXT_V1_MODE = $Case
        $env:JOB_CONTEXT_V1_MARKER = $marker
        & $hostExe 'panic-lifecycle' $Case $newPlugin $marker
        if ($LASTEXITCODE -ne 0) { Fail "panic lifecycle fixture failed: $Case (exit $LASTEXITCODE)" }
        if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { Fail "panic lifecycle marker missing: $Case" }
        if ((Get-Content -LiteralPath $marker -Raw) -ne $ExpectedMarker) { Fail "panic lifecycle marker mismatch: $Case" }
    } finally {
        if ($null -eq $savedMode) { Remove-Item Env:JOB_CONTEXT_V1_MODE -ErrorAction SilentlyContinue } else { $env:JOB_CONTEXT_V1_MODE = $savedMode }
        if ($null -eq $savedMarker) { Remove-Item Env:JOB_CONTEXT_V1_MARKER -ErrorAction SilentlyContinue } else { $env:JOB_CONTEXT_V1_MARKER = $savedMarker }
    }
}

try {
    New-Item -ItemType Directory -Path $newPluginTarget, $hostTarget -Force | Out-Null
    if (-not (Test-Path -LiteralPath (Join-Path $cargoHome 'registry') -PathType Container)) {
        Fail 'Prefilled local Cargo registry cache is unavailable; bootstrap it before offline validation.'
    }

    Invoke-CargoBuild $newPluginRoot $newPluginTarget
    Invoke-CargoBuild $hostRoot $hostTarget

    $newPlugin = Find-Artifact $newPluginTarget 'job_context_v1_contract_new_plugin.dll'
    $hostExe = Find-Artifact $hostTarget 'job-context-v1-contract-host.exe'
    $layoutOutput = & $hostExe layout
    if ($LASTEXITCODE -ne 0 -or ($layoutOutput -join "`n") -notmatch 'JobHostServicesV1') {
        Fail 'job-context v1 Rust-first host-services baseline marker missing'
    }
    Invoke-Host @('transport', $newPlugin)
    Invoke-Host @('new', $newPlugin)
    Invoke-PanicLifecycle 'factory-panic' 'factory'
    Invoke-PanicLifecycle 'register-panic' 'register'
    Invoke-PanicLifecycle 'registrar-drop-panic' 'registrar-drop'
    Invoke-PanicLifecycle 'provider-drop-panic' 'provider-drop'
    Write-Output 'job context v1 ABI contract: PASS'
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    if ($null -eq $savedCargoHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedCargoHome }
    if ($null -eq $savedCargoTargetDir) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedCargoTargetDir }
}
