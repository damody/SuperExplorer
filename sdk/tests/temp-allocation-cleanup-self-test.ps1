[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-cleanup-selftest-' + [guid]::NewGuid().ToString('N'))
$first = Join-Path $tempRoot 'first'
$blocker = Join-Path $tempRoot 'blocker'
$second = Join-Path $blocker 'second'
$createdTemp = @()
function Remove-TrackedTemp([string]$Path) {
    $root = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if (-not $resolved.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw "cleanup escaped temp root: $resolved" }
    Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction Stop
}
try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    New-Item -ItemType Directory -Path $first | Out-Null; $createdTemp += $first
    New-Item -ItemType File -Path $blocker | Out-Null
    $failed = $false
    try { New-Item -ItemType Directory -Path $second -ErrorAction Stop | Out-Null; $createdTemp += $second } catch { $failed = $true }
    if (-not $failed) { throw 'second allocation unexpectedly succeeded.' }
} finally {
    foreach ($path in $createdTemp) { if (Test-Path -LiteralPath $path) { Remove-TrackedTemp $path } }
    if (Test-Path -LiteralPath $tempRoot) { Remove-TrackedTemp $tempRoot }
}
if (Test-Path -LiteralPath $first) { throw 'tracked first allocation was not cleaned.' }
if (Test-Path -LiteralPath $tempRoot) { throw 'self-test temp root was not cleaned.' }
Write-Output 'temp allocation cleanup self-test passed'
