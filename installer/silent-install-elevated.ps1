[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string]$InstallerPath
)

$ErrorActionPreference = 'Stop'

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

$installer = [IO.Path]::GetFullPath($InstallerPath)
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
    throw "installer does not exist: $installer"
}

$start = @{
    FilePath = $installer
    ArgumentList = @('/S')
    Wait = $true
    PassThru = $true
}
if (-not (Test-Administrator)) {
    $start['Verb'] = 'RunAs'
}

$process = Start-Process @start
if ($null -eq $process) {
    throw "silent SuperExplorer install did not start (UAC cancelled?): $installer"
}
if ($process.ExitCode -ne 0) {
    throw "silent SuperExplorer install failed with exit $($process.ExitCode): $installer"
}
Write-Output "silent SuperExplorer install completed: $installer exit=0"
exit 0
