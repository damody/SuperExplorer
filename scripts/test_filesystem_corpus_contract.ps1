param(
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestFilesystemCorpus.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$first = Join-Path $output 'fixture-a'
$second = Join-Path $output 'fixture-b'

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Get-StableShape([object[]]$Items) {
    return @($Items | ForEach-Object {
        [pscustomobject]@{
            relative_path = $_.relative_path
            kind = $_.kind
            length = $_.length
            sha256 = $_.sha256
            attributes = $_.attributes
        }
    })
}

try {
    New-UitestFilesystemCorpus -FixtureRoot $first -OwnedRoot $output -Profile small | Out-Null
    New-UitestFilesystemCorpus -FixtureRoot $second -OwnedRoot $output -Profile small | Out-Null
    $itemsA = @(Write-UitestCorpusManifest -FixtureRoot $first -Path (Join-Path $output 'fixture-manifest.json') -Profile small)
    $itemsB = @(Get-UitestFilesystemSnapshot -Root $second)
    Get-StableShape $itemsA | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'before.json')

    $byPath = @{}
    foreach ($item in $itemsA) { $byPath[$item.relative_path] = $item }
    Assert-True ($itemsA.Count -ge 55) "corpus is unexpectedly small: $($itemsA.Count)"
    Assert-True ($byPath.ContainsKey('00-empty-folder')) 'missing empty directory'
    Assert-True ($byPath.ContainsKey('01-nested-empty/level-a/level-b/level-c')) 'missing nested empty directory'
    Assert-True ($byPath['03-content/empty.bin'].length -eq 0) 'empty file is not empty'
    Assert-True ($byPath['03-content/one-byte.bin'].length -eq 1) 'one-byte file has wrong length'
    Assert-True ($byPath['03-content/duplicate-a.bin'].sha256 -eq $byPath['03-content/duplicate-b.bin'].sha256) 'duplicate files have different hashes'
    Assert-True ($byPath['03-content/same-size-different-a.bin'].length -eq $byPath['03-content/same-size-different-b.bin'].length) 'same-size controls differ in length'
    Assert-True ($byPath['03-content/same-size-different-a.bin'].sha256 -ne $byPath['03-content/same-size-different-b.bin'].sha256) 'same-size controls unexpectedly match'
    Assert-True (@($itemsA | Where-Object relative_path -like '02-unicode/*').Count -ge 7) 'Unicode corpus is incomplete'
    Assert-True (@($itemsA | Where-Object relative_path -like '06-deep/*/deep-leaf.txt').Count -eq 1) 'deep path leaf is missing'
    Assert-True (($byPath['05-mutation/readonly-source.txt'].attributes -like '*ReadOnly*')) 'read-only attribute is missing'
    Assert-True (($byPath['08-attributes/hidden-item.txt'].attributes -like '*Hidden*')) 'hidden attribute is missing'
    Assert-True (($byPath['08-attributes/system-item.txt'].attributes -like '*System*')) 'system attribute is missing'

    $jsonA = (Get-StableShape $itemsA | ConvertTo-Json -Depth 5 -Compress)
    $jsonB = (Get-StableShape $itemsB | ConvertTo-Json -Depth 5 -Compress)
    Assert-True ($jsonA -ceq $jsonB) 'two independently generated corpora are not deterministic'

    $outsideRejected = $false
    try { Assert-UitestOwnedPath -Path (Split-Path $output -Parent) -OwnedRoot $output | Out-Null } catch { $outsideRejected = $true }
    Assert-True $outsideRejected 'owned-path guard accepted an outside path'

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        profile = 'small'
        item_count = $itemsA.Count
        oracles = [ordered]@{
            deterministic_generation = $true
            empty_and_nested_items = $true
            duplicate_and_same_size_controls = $true
            unicode_and_long_paths = $true
            windows_attributes = $true
            cleanup_scope_guard = $true
        }
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    foreach ($fixture in @($first, $second)) {
        if (Test-Path -LiteralPath $fixture) {
            Remove-UitestOwnedFixture -FixtureRoot $fixture -OwnedRoot $output
        }
    }
}

Write-Output "Filesystem corpus contract passed: $OutputDirectory"
