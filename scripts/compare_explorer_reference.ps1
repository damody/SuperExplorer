param(
    [Parameter(Mandatory = $true)]
    [string]$ExplorerDirectory,
    [Parameter(Mandatory = $true)]
    [string]$ApplicationDirectory,
    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory,
    [string]$PythonExecutable = 'python',
    [string]$ExplorerRegions,
    [string]$ApplicationDiagnostics,
    [double]$RegionTolerancePercent = 10.0,
    [switch]$RequireRegionPass,
    [switch]$RequireSameImageSize
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
foreach ($name in @('ExplorerDirectory', 'ApplicationDirectory')) {
    Set-Variable -Name $name -Value (Resolve-Path -LiteralPath (Get-Variable -Name $name -ValueOnly)).Path
}
$OutputDirectory = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
    [System.IO.Path]::GetFullPath($OutputDirectory)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}

$arguments = @(
    (Join-Path $PSScriptRoot 'compare_explorer_reference.py'),
    '--explorer', $ExplorerDirectory,
    '--application', $ApplicationDirectory,
    '--output', $OutputDirectory,
    '--region-tolerance-percent', $RegionTolerancePercent
)
if ($ExplorerRegions) { $arguments += @('--explorer-regions', $ExplorerRegions) }
if ($ApplicationDiagnostics) { $arguments += @('--application-diagnostics', $ApplicationDiagnostics) }
if ($RequireRegionPass) { $arguments += '--require-region-pass' }
if ($RequireSameImageSize) { $arguments += '--require-same-image-size' }

& $PythonExecutable @arguments
if ($LASTEXITCODE -ne 0) {
    throw "Explorer reference comparison failed with exit code $LASTEXITCODE"
}
