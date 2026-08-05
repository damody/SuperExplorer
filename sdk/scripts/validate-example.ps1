[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ExampleRoot,
    [switch]$RequireCompleteArtifacts
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $ExampleRoot).Path
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$manifest = Join-Path $root 'Cargo.toml'

foreach ($relative in @(
    'Cargo.toml', 'Cargo.lock', 'src/lib.rs', 'plugin-project.json',
    'README.md', 'README.zh-TW.md', 'LICENSE', 'NOTICE', 'locales'
)) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $relative))) {
        throw "example artifact is missing: $relative"
    }
}

$cargoText = Get-Content -LiteralPath $manifest -Raw
if ($cargoText -notmatch '(?m)^\[workspace\]\s*$') {
    throw 'example must terminate workspace discovery with its own [workspace] table'
}
if ($cargoText -match '(?m)^\s*(version|edition|rust-version)\s*=\s*\{?\s*workspace\s*=') {
    throw 'example may not inherit private root workspace package metadata'
}
if ($cargoText -match '(?m)^\s*[A-Za-z0-9_-]+\s*=\s*\{[^\r\n]*path\s*=\s*"(?!\.\.\/\.\.\/\.\.\/crates\/explorer-extension-(api|ui-api)")') {
    throw 'example contains a path dependency outside the two public SDK crates'
}
if ($cargoText -match 'explorer-(app|model|ui|shell-win|extension-host|common|jobs)\s*=') {
    throw 'example references a private product crate'
}
foreach ($publicCrate in @('explorer-extension-api', 'explorer-extension-ui-api')) {
    $pattern = [regex]::Escape($publicCrate) + '\s*=\s*\{[^\r\n]*path\s*=\s*"\.\.\/\.\.\/\.\.\/crates\/' + [regex]::Escape($publicCrate) + '"[^\r\n]*version\s*=\s*"=[^"]+"'
    if ($cargoText -notmatch $pattern) {
        throw "$publicCrate must use the first-party relative path and an exact version"
    }
}

$metadataText = & cargo metadata --manifest-path $manifest --locked --offline --format-version 1 --no-deps 2>&1
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed: $metadataText" }
$metadata = $metadataText | ConvertFrom-Json
if (@($metadata.workspace_members).Count -ne 1) {
    throw 'example metadata must describe exactly one independent workspace member'
}
$package = @($metadata.packages)[0]
if (-not $package -or [IO.Path]::GetFullPath($package.manifest_path) -ne [IO.Path]::GetFullPath($manifest)) {
    throw 'example metadata resolved through another composition root'
}

$projectText = Get-Content -LiteralPath (Join-Path $root 'plugin-project.json') -Raw
if ($projectText -notmatch '"contributions"\s*:') { throw 'plugin project has no production contributions' }
if ($projectText -notmatch '"payload"\s*:\s*"src/lib.rs"') { throw 'contributions are not wired to production src/lib.rs' }
if ((Get-Content -LiteralPath (Join-Path $root 'src/lib.rs') -Raw) -notmatch 'register|registrar') {
    throw 'production composition root has no registrar implementation'
}

if ($RequireCompleteArtifacts) {
    foreach ($relative in @('fixtures', 'screenshots', 'SBOM.json', 'provenance.json')) {
        if (-not (Test-Path -LiteralPath (Join-Path $root $relative))) {
            throw "complete example artifact is missing: $relative"
        }
    }
    if (@(Get-ChildItem -LiteralPath (Join-Path $root 'screenshots') -File).Count -eq 0) {
        throw 'complete example has no screenshot evidence'
    }
    if ($projectText -notmatch '"verification"\s*:') { throw 'complete example has no verification mapping' }
}

[pscustomobject]@{
    schema_version = 1
    package = $package.name
    manifest = $manifest.Substring($repo.Length).TrimStart('\')
    independent_workspace = $true
    public_path_dependencies = @('explorer-extension-api', 'explorer-extension-ui-api')
    complete_artifacts = [bool]$RequireCompleteArtifacts
} | ConvertTo-Json -Depth 4
