$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$document = Get-Content -LiteralPath (Join-Path $repo 'sdk\PACKAGE_LIFECYCLE.md') -Raw -Encoding UTF8
$example = Get-Content -LiteralPath (Join-Path $repo 'sdk\fixtures\package-resolution-v1\example-manifest.json') -Raw | ConvertFrom-Json
$traditionalEdgeBound = '512 ' + [char]0x689D + ' dependency'

foreach ($requiredText in @(
    'source.discover', 'PackageValidatorV1', 'PackageValidationResultV1',
    'PackageResolverV1', 'activation_guard', 'parse_json',
    '128 candidates', '65,536 search states',
    'manifest_version', 'publisher.contacts', 'output_protocol', 'data_version',
    'PackageSourceV1', 'EntitlementProviderV1', 'PackageValidationBudgetV1',
    'PackageValidationCancellationV1', 'activation_guard_with_budget',
    $traditionalEdgeBound, 'optional edge'
)) {
    if (-not $document.Contains($requiredText)) { throw "lifecycle document is missing: $requiredText" }
}
if ($document -notmatch '512\s+total dependency edges') {
    throw 'lifecycle document must state the 512 total dependency-edge bound'
}
if ($example.manifest_version -ne 1 -or $example.dependencies.Count -ne 2) {
    throw 'example manifest no longer describes the V1 resolution fixture'
}
Write-Output 'package lifecycle documentation contract: PASS'
