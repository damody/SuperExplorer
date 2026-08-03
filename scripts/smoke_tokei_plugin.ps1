param(
    [string]$FixtureRoot = 'sdk\fixtures\rust-tokei-code-lines-column',
    [string]$OutputDirectory = 'target\tokei-smoke'
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not [IO.Path]::IsPathRooted($FixtureRoot)) { $FixtureRoot = Join-Path $workspace $FixtureRoot }
if (-not [IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory = Join-Path $workspace $OutputDirectory }
$FixtureRoot = (Resolve-Path -LiteralPath $FixtureRoot).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

$cargoHome = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-tokei-cargo-' + [guid]::NewGuid().ToString('N'))
$target = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-tokei-target-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $cargoHome,$target | Out-Null
$oldHome = $env:CARGO_HOME; $oldTarget = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_HOME = $cargoHome
    $env:CARGO_TARGET_DIR = $target
    Push-Location $FixtureRoot
    try {
        & cargo.exe test --lib --locked --offline
        if ($LASTEXITCODE) { throw "tokei fixture tests failed ($LASTEXITCODE)" }
        & cargo.exe build --release --locked --offline --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE) { throw "tokei fixture build failed ($LASTEXITCODE)" }
    } finally { Pop-Location }
    $dll = Join-Path $target 'x86_64-pc-windows-msvc\release\rust_tokei_code_lines_column.dll'
    if (-not (Test-Path -LiteralPath $dll)) { throw "missing built plugin DLL: $dll" }
    $source = Get-ChildItem -LiteralPath (Join-Path $FixtureRoot 'src') -Filter '*.rs' -File -Recurse | Get-Content -Raw
    if ($source -match 'Command(::|\s*::\s*)new|std::process') { throw 'plugin must not spawn an external process' }
    $dllName = Split-Path -Leaf $dll
    [ordered]@{
        schema_version = 1
        fixture = 'rust-tokei-code-lines-column'
        tokei = '14.0.0'
        target = 'x86_64-pc-windows-msvc'
        locked = $true
        offline = $true
        empty_cargo_home = $true
        source_process_api_scan = 'passed'
        dll = $dllName
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    Write-Output "PASS: $dll"
} finally {
    $env:CARGO_HOME = $oldHome; $env:CARGO_TARGET_DIR = $oldTarget
    if (Test-Path -LiteralPath $cargoHome) { Remove-Item -LiteralPath $cargoHome -Recurse -Force }
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
}
