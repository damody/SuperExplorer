param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [string]$OutputDirectory = 'build/bookmark-manager-view-verify'
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '..\scripts\UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$output = [IO.Path]::GetFullPath((Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..')).Path $OutputDirectory))
New-Item -ItemType Directory -Force -Path $output | Out-Null
$context = $null

function Get-AppElements {
    param([int]$ProcessId)
    @([Windows.Automation.AutomationElement]::RootElement.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    ) | Where-Object { $_.Current.ProcessId -eq $ProcessId })
}

function Find-AppElement {
    param([int]$ProcessId, [string]$Name, [int]$TimeoutSeconds = 12)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $hit = Get-AppElements -ProcessId $ProcessId | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1
        if ($null -ne $hit) { return $hit }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    $names = Get-AppElements -ProcessId $ProcessId |
        ForEach-Object { "$($_.Current.ControlType.ProgrammaticName)|$($_.Current.AutomationId)|$($_.Current.Name)" } |
        Select-Object -Unique
    $names | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'uia-dump.txt')
    throw "UIA element not found: $Name"
}

$viewLabel = -join @([char]0x6AA2, [char]0x8996, ' (V)')
$showColumnsLabel = -join @([char]0x986F, [char]0x793A, [char]0x6B04, [char]0x4F4D, ' (C)')
$sortLabel = -join @([char]0x6392, [char]0x5E8F, ' (S)')

function Save-WindowShot {
    param([Windows.Automation.AutomationElement]$Window, [string]$Name)
    Save-UitestScreenshot -Root $Window -Path (Join-Path $output $Name)
}

try {
    $context = Start-UitestExplorer -InitialPath $env:USERPROFILE -OutputDirectory $output -Profile $Profile -SkipBuild
    $appPid = [int]$context.Process.Id
    Invoke-UitestClick -Element (Find-AppElement -ProcessId $appPid -Name 'Manage bookmarks')
    $manager = Find-AppElement -ProcessId $appPid -Name 'Bookmark manager window'
    Save-WindowShot -Window $manager -Name 'manager-toolbar.png'
    Invoke-UitestClick -Element (Find-AppElement -ProcessId $appPid -Name $viewLabel)
    Start-Sleep -Milliseconds 400
    Save-WindowShot -Window $manager -Name 'manager-view-open.png'
    $columns = Get-AppElements -ProcessId $appPid | Where-Object { $_.Current.Name -eq $showColumnsLabel } | Select-Object -First 1
    if ($null -eq $columns) { $columns = Find-AppElement -ProcessId $appPid -Name 'Show columns (C)' }
    Invoke-UitestClick -Element $columns
    Start-Sleep -Milliseconds 350
    Save-WindowShot -Window $manager -Name 'manager-view-columns.png'
    $visited = Find-AppElement -ProcessId $appPid -Name (-join @([char]0x4E0A, [char]0x6B21, [char]0x700F, [char]0x89BD, [char]0x6642, [char]0x9593))
    Invoke-UitestClick -Element $visited
    Start-Sleep -Milliseconds 350
    Save-WindowShot -Window $manager -Name 'manager-view-columns-toggled.png'
    [ordered]@{
        schema = 'bookmark-manager-view-verify-v1'
        status = 'PASS'
        artifacts = @('manager-toolbar.png', 'manager-view-open.png', 'manager-view-columns.png')
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
    Write-Output "PASS $output"
} catch {
    if ($null -ne $context) {
        try {
            $appPid = [int]$context.Process.Id
            Get-AppElements -ProcessId $appPid |
                ForEach-Object { "$($_.Current.ControlType.ProgrammaticName)|$($_.Current.Name)" } |
                Select-Object -Unique |
                Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'uia-dump.txt')
            $win = Get-AppElements -ProcessId $appPid | Where-Object { $_.Current.NativeWindowHandle -ne 0 } | Select-Object -Last 1
            if ($null -ne $win) { Save-WindowShot -Window $win -Name 'failure.png' }
        } catch { }
    }
    $_ | Out-String | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'error.txt')
    throw
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}
