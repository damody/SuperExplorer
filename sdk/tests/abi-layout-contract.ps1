[CmdletBinding()]
param(
    [string]$TargetDirectory = ""
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$plugin = Join-Path $workspace 'sdk\fixtures\abi-root-plugin'
$hostFixture = Join-Path $workspace 'sdk\fixtures\abi-root-host'
$target = if ([string]::IsNullOrWhiteSpace($TargetDirectory)) {
    Join-Path $workspace 'target\abi-root-fixture'
} else {
    [IO.Path]::GetFullPath($TargetDirectory)
}

New-Item -ItemType Directory -Force -Path $target | Out-Null
$buildOutput = Join-Path $target 'x86_64-pc-windows-msvc\debug'

function Invoke-Cargo {
    param([string]$Directory, [string[]]$Arguments)
    & cargo.exe @Arguments --target-dir $target
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments -join ' ') failed in $Directory with exit code $LASTEXITCODE"
    }
}

function Assert-PanicAbortRejected {
    $previousRustFlags = $env:RUSTFLAGS
    $env:RUSTFLAGS = if ([string]::IsNullOrWhiteSpace($previousRustFlags)) {
        '-C panic=abort'
    } else {
        "$previousRustFlags -C panic=abort"
    }
    try {
        & cargo.exe check --locked --target-dir $target
        if ($LASTEXITCODE -eq 0) {
            throw 'plugin fixture unexpectedly accepted RUSTFLAGS panic=abort'
        }
    } finally {
        if ($null -eq $previousRustFlags) {
            Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
        } else {
            $env:RUSTFLAGS = $previousRustFlags
        }
    }
}

Push-Location $plugin
try {
    Invoke-Cargo $plugin @('test', '--locked')
    Assert-PanicAbortRejected
    Invoke-Cargo $plugin @('build', '--locked')
    $compatiblePlugin = Join-Path $buildOutput 'abi_root_fixture_plugin.dll'
    if (-not (Test-Path -LiteralPath $compatiblePlugin)) {
        throw "compatible plugin DLL was not produced: $compatiblePlugin"
    }
    $compatibleCopy = Join-Path $buildOutput 'abi_root_fixture_plugin_compatible.dll'
    Copy-Item -LiteralPath $compatiblePlugin -Destination $compatibleCopy -Force

    Invoke-Cargo $plugin @('build', '--locked', '--features', 'layout-mismatch')
    $mismatchPlugin = Join-Path $buildOutput 'abi_root_fixture_plugin.dll'
    if (-not (Test-Path -LiteralPath $mismatchPlugin)) {
        throw "mismatch plugin DLL was not produced: $mismatchPlugin"
    }
    $mismatchCopy = Join-Path $buildOutput 'abi_root_fixture_plugin_layout_mismatch.dll'
    Copy-Item -LiteralPath $mismatchPlugin -Destination $mismatchCopy -Force
} finally {
    Pop-Location
}

Push-Location $hostFixture
try {
    Invoke-Cargo $hostFixture @('test', '--locked')
    & cargo.exe run --locked --target-dir $target -- compatible $compatibleCopy
    if ($LASTEXITCODE -ne 0) { throw 'compatible host/plugin load failed' }
    $marker = Join-Path $target ("abi-root-mismatch-marker-" + [guid]::NewGuid().ToString('N'))
    $env:ABI_ROOT_FIXTURE_MARKER = $marker
    try {
        & cargo.exe run --locked --target-dir $target -- mismatch $mismatchCopy
        if ($LASTEXITCODE -ne 0) { throw 'layout mismatch was not rejected before the registrar callback' }
    } finally {
        Remove-Item Env:ABI_ROOT_FIXTURE_MARKER -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $marker) {
        throw "layout mismatch wrote registrar marker: $marker"
    }
} finally {
    Pop-Location
}

Write-Host 'ABI root-module fixture contract passed.'
