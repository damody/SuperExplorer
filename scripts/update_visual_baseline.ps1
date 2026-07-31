param(
    [Parameter(Mandatory = $true)]
    [string]$ActualDirectory,
    [Parameter(Mandatory = $true)]
    [string]$BaselineDirectory,
    [switch]$Approve
)

$ErrorActionPreference = 'Stop'
if (-not $Approve) {
    throw 'baseline update requires -Approve after manual screenshot/metadata/diff review'
}
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$ActualDirectory = (Resolve-Path -LiteralPath $ActualDirectory).Path
$BaselineDirectory = if ([System.IO.Path]::IsPathRooted($BaselineDirectory)) {
    [System.IO.Path]::GetFullPath($BaselineDirectory)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $BaselineDirectory))
}
if ($ActualDirectory -eq $BaselineDirectory) {
    throw 'actual and baseline directories must be different'
}
foreach ($name in @('screenshot.png', 'diagnostics.json', 'metadata.json')) {
    if (-not (Test-Path -LiteralPath (Join-Path $ActualDirectory $name) -PathType Leaf)) {
        throw "actual capture is missing $name"
    }
}
$metadata = Get-Content -Raw -Encoding utf8 -LiteralPath (Join-Path $ActualDirectory 'metadata.json') | ConvertFrom-Json
if (-not $metadata.dpi.matches_expectation) {
    throw 'baseline update rejected: actual capture DPI does not match its declared expectation'
}
New-Item -ItemType Directory -Path $BaselineDirectory -Force | Out-Null
foreach ($name in @('screenshot.png', 'diagnostics.json', 'metadata.json')) {
    Copy-Item -LiteralPath (Join-Path $ActualDirectory $name) -Destination (Join-Path $BaselineDirectory $name) -Force
}
$review = [ordered]@{
    schema_version = 1
    source_actual = $ActualDirectory
    approved = $true
    approved_utc = [DateTime]::UtcNow.ToString('o')
    baseline_files = @('screenshot.png', 'diagnostics.json', 'metadata.json')
}
$review | ConvertTo-Json -Depth 3 | Set-Content -Encoding utf8 (Join-Path $BaselineDirectory 'review.json')
Write-Output "Visual baseline updated after explicit approval: $BaselineDirectory"
