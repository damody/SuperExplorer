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

$target = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-tokei-target-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $target | Out-Null
$oldTarget = $env:CARGO_TARGET_DIR
try {
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
    $cargoText = Get-Content (Join-Path $FixtureRoot 'Cargo.toml') -Raw
    $tokeiMatch = [regex]::Match($cargoText, '(?m)^tokei\s*=\s*\{\s*version\s*=\s*"=(?<version>[^"]+)"')
    if (-not $tokeiMatch.Success) { throw 'Cargo.toml does not declare an exact tokei version' }
    $sbom = Get-Content (Join-Path $FixtureRoot 'SBOM.json') -Raw | ConvertFrom-Json
    $sbomTokei = @($sbom.components | Where-Object name -eq 'tokei' | Select-Object -First 1)
    if ($sbomTokei.Count -ne 1 -or $sbomTokei[0].version -ne $tokeiMatch.Groups['version'].Value) { throw 'SBOM tokei version differs from Cargo.toml' }
    $cargoTomlSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $FixtureRoot 'Cargo.toml')).Hash.ToLowerInvariant()
    $cargoLockSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $FixtureRoot 'Cargo.lock')).Hash.ToLowerInvariant()
    if ($sbom.cargo_toml_sha256 -ne $cargoTomlSha -or $sbom.lockfile.sha256 -ne $cargoLockSha) { throw 'SBOM source hashes differ from Cargo.toml or Cargo.lock' }
    $metadata = & cargo.exe metadata --manifest-path (Join-Path $FixtureRoot 'Cargo.toml') --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
    if ($LASTEXITCODE) { throw "tokei fixture metadata failed ($LASTEXITCODE)" }
    $expectedComponents = @($metadata.packages | Where-Object id -ne $metadata.resolve.root | ForEach-Object { "$($_.name)|$($_.version)" } | Sort-Object)
    $actualComponents = @($sbom.components | ForEach-Object {
        if ([string]::IsNullOrWhiteSpace($_.checksum) -or [string]::IsNullOrWhiteSpace($_.license)) { throw "incomplete SBOM component: $($_.name)@$($_.version)" }
        "$($_.name)|$($_.version)"
    } | Sort-Object)
    if ($sbom.component_count -ne $expectedComponents.Count -or (Compare-Object $expectedComponents $actualComponents)) { throw 'SBOM does not match the active Cargo metadata closure' }
    $licenses = Get-Content (Join-Path $FixtureRoot 'LICENSES.json') -Raw | ConvertFrom-Json
    $licenseComponents = @($licenses.inventory | ForEach-Object { "$($_.name)|$($_.version)" } | Sort-Object)
    if ($licenses.component_count -ne $expectedComponents.Count -or (Compare-Object $expectedComponents $licenseComponents)) { throw 'license inventory does not match the active Cargo metadata closure' }
    [ordered]@{
        schema_version = 1
        fixture = 'rust-tokei-code-lines-column'
        tokei = $tokeiMatch.Groups['version'].Value
        target = 'x86_64-pc-windows-msvc'
        locked = $true
        offline = $true
        standard_registry_cache = $true
        source_process_api_scan = 'passed'
        dll = $dllName
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    Write-Output "PASS: $dll"
} finally {
    $env:CARGO_TARGET_DIR = $oldTarget
    if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target -Recurse -Force }
}
