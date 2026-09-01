param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$RemotePath = '',
    [switch]$UseCurrentProfile,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
$source = Join-Path $fixture 'source'
$destination = Join-Path $fixture 'destination'
New-Item -ItemType Directory -Force -Path $source,$destination | Out-Null
$externalName = 'clipboard-external.txt'
$localName = 'clipboard-superexplorer.txt'
$externalPath = Join-Path $source $externalName
$localPath = Join-Path $source $localName
Set-Content -Encoding utf8 -LiteralPath $externalPath -Value 'external CF_HDROP fixture'
Set-Content -Encoding utf8 -LiteralPath $localPath -Value 'SuperExplorer CF_HDROP fixture'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $destination 'focus-anchor.txt') -Value 'focus fixture'
$context = $null

function Set-FileClipboard([string]$Path) {
    $files = [Collections.Specialized.StringCollection]::new()
    [void]$files.Add($Path)
    foreach ($attempt in 1..20) {
        try {
            [Windows.Forms.Clipboard]::SetFileDropList($files)
            return
        } catch {
            Start-Sleep -Milliseconds 50
        }
    }
    throw 'Windows file clipboard remained busy'
}

function Read-FileClipboard {
    foreach ($attempt in 1..20) {
        try {
            return @([Windows.Forms.Clipboard]::GetFileDropList() | ForEach-Object { [string]$_ })
        } catch {
            Start-Sleep -Milliseconds 50
        }
    }
    throw 'Windows file clipboard remained busy while reading'
}

function Wait-VisibleItem([object]$Root, [string]$Name, [int]$Seconds = 20) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        try { return Find-UitestFileItem -Root $Root -Name $Name } catch {}
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "file item did not appear: $Name"
}

try {
    # A standard external CF_HDROP must paste into a local SuperExplorer destination.
    $context = Start-UitestExplorer -InitialPath $destination -OutputDirectory (Join-Path $output 'local-paste') -Profile $Profile -SkipBuild:$SkipBuild
    Invoke-UitestClick -Element (Wait-VisibleItem -Root $context.Root -Name 'focus-anchor.txt')
    Set-FileClipboard $externalPath
    Start-Sleep -Milliseconds 750
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 500
    Wait-UitestPath -Path (Join-Path $destination $externalName)
    Stop-UitestExplorer -Context $context
    $context = $null

    # SuperExplorer local Ctrl+C must publish a standard CF_HDROP readable by Explorer.
    $context = Start-UitestExplorer -InitialPath $source -OutputDirectory (Join-Path $output 'local-copy') -Profile $Profile -SkipBuild
    $item = Wait-VisibleItem -Root $context.Root -Name $localName
    Invoke-UitestClick -Element $item
    Send-UitestKey -Key 0x43 -Modifiers @(0x11) -DelayMilliseconds 300
    $paths = @(Read-FileClipboard)
    if ($paths.Count -ne 1 -or -not ([IO.Path]::GetFullPath($paths[0])).Equals([IO.Path]::GetFullPath($localPath), [StringComparison]::OrdinalIgnoreCase)) {
        throw "SuperExplorer Ctrl+C did not publish the selected local path: $($paths -join ', ')"
    }
    Stop-UitestExplorer -Context $context
    $context = $null

    $remotePassed = $true
    if (-not [string]::IsNullOrWhiteSpace($RemotePath)) {
        $context = Start-UitestExplorer -InitialPath $source -OutputDirectory (Join-Path $output 'remote-paste') -Profile $Profile -UseCurrentProfile:$UseCurrentProfile -SkipBuild
        Set-UitestAddress -Context $context -Path $RemotePath
        Start-Sleep -Milliseconds 1200
        $firstRemoteItem = @(Get-UitestFileItems -Root $context.Root | Select-Object -First 1)
        if ($firstRemoteItem.Count -gt 0) { Invoke-UitestClick -Element $firstRemoteItem[0] }
        Set-FileClipboard $externalPath
        Start-Sleep -Milliseconds 750
        [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
        Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 800
        $remoteItem = Wait-VisibleItem -Root $context.Root -Name $externalName -Seconds 30
        Invoke-UitestClick -Element $remoteItem
        Send-UitestKey -Key 0x2E -DelayMilliseconds 500
        $remotePassed = $true
    }

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        external_to_local = (Test-Path -LiteralPath (Join-Path $destination $externalName))
        superexplorer_local_published_standard_file_drop = $true
        remote_path = $RemotePath
        external_to_remote = $remotePassed
    } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}

Write-Output "Windows file clipboard interoperability smoke passed: $output"
