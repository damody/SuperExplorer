[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string]$InstallDirectory,
    [ValidateRange(0, 30000)] [int]$GracefulTimeoutMilliseconds = 5000,
    [ValidateRange(0, 30000)] [int]$ForceTimeoutMilliseconds = 5000
)

$ErrorActionPreference = 'Stop'

function Get-NormalizedFullPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
}

$targetExecutable = Get-NormalizedFullPath (Join-Path $InstallDirectory 'SuperExplorer.exe')

function Get-TargetProcesses {
    $matches = @()
    foreach ($candidate in @(Get-CimInstance Win32_Process -Filter "Name = 'SuperExplorer.exe'")) {
        if ([string]::IsNullOrWhiteSpace($candidate.ExecutablePath)) { continue }
        $candidatePath = Get-NormalizedFullPath $candidate.ExecutablePath
        if ([string]::Equals($candidatePath, $targetExecutable, [System.StringComparison]::OrdinalIgnoreCase)) {
            $matches += $candidate
        }
    }
    @($matches)
}

function Wait-ForTargetExit {
    param([Parameter(Mandatory = $true)][int]$TimeoutMilliseconds)
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        if (@(Get-TargetProcesses).Count -eq 0) { return $true }
        if ([DateTime]::UtcNow -ge $deadline) { return $false }
        Start-Sleep -Milliseconds 100
    } while ($true)
}

try {
    $initial = @(Get-TargetProcesses)
    foreach ($candidate in $initial) {
        $process = Get-Process -Id ([int]$candidate.ProcessId) -ErrorAction SilentlyContinue
        if ($null -ne $process) { [void]$process.CloseMainWindow() }
    }
    if (-not (Wait-ForTargetExit -TimeoutMilliseconds $GracefulTimeoutMilliseconds)) {
        foreach ($candidate in @(Get-TargetProcesses)) {
            Stop-Process -Id ([int]$candidate.ProcessId) -Force -ErrorAction Stop
        }
        if (-not (Wait-ForTargetExit -TimeoutMilliseconds $ForceTimeoutMilliseconds)) {
            throw 'target SuperExplorer processes remained after bounded force termination'
        }
    }
    if (@(Get-TargetProcesses).Count -ne 0) { throw 'final target-process absence could not be proven' }
    Write-Output "SuperExplorer quiescence verified: target=$targetExecutable initial=$($initial.Count)"
    exit 0
} catch {
    Write-Error "SuperExplorer quiescence failed for target '$targetExecutable': $($_.Exception.Message)"
    exit 1
}
