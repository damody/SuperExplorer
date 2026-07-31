param(
    [Parameter(Mandatory = $true)]
    [string]$BaselineDirectory,
    [Parameter(Mandatory = $true)]
    [string]$ActualDirectory,
    [string]$OutputDirectory,
    [string]$PythonExecutable = 'python'
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$BaselineDirectory = (Resolve-Path -LiteralPath $BaselineDirectory).Path
$ActualDirectory = (Resolve-Path -LiteralPath $ActualDirectory).Path
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $workspaceRoot ('target\visual-diff\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
} else {
    $OutputDirectory = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
        [System.IO.Path]::GetFullPath($OutputDirectory)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
    }
}

& $PythonExecutable (Join-Path $PSScriptRoot 'compare_visual_baseline.py') `
    --baseline $BaselineDirectory `
    --actual $ActualDirectory `
    --output $OutputDirectory `
    --config (Join-Path $workspaceRoot 'docs\visual\comparison-config.json')
if ($LASTEXITCODE -ne 0) {
    throw "visual comparison failed; artifacts preserved at $OutputDirectory"
}
