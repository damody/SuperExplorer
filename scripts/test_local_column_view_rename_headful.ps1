param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = 'D:\SuperExplorer\target\debug\SuperExplorer.exe'
)

$ErrorActionPreference = 'Stop'
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$original = Join-Path $fixture 'rename-me.txt'
$renamed = Join-Path $fixture 'renamed-by-columns.txt'
Set-Content -LiteralPath $original -Value 'column-rename-test' -Encoding utf8
$profile = Join-Path $OutputDirectory 'localappdata'
New-Item -ItemType Directory -Force -Path $profile | Out-Null

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
if (-not ('ColumnRename.Native' -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ColumnRename {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    }
}
"@
}

function Find-Name([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Wait-Name([Windows.Automation.AutomationElement] $Root, [string] $Name, [int] $TimeoutMs = 4000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $match = Find-Name $Root $Name
        if ($null -ne $match) { return $match }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Find-Row([Windows.Automation.AutomationElement] $Root, [string] $Filename) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem)
    $rows = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
    foreach ($row in $rows) {
        if ($row.Current.Name -match [regex]::Escape($Filename)) { return $row }
    }
    return $null
}

function Click([Windows.Automation.AutomationElement] $Element) {
    $rect = $Element.Current.BoundingRectangle
    $x = [int]($rect.Left + [Math]::Min(80, $rect.Width / 2))
    $y = [int]($rect.Top + $rect.Height / 2)
    [System.Windows.Forms.Cursor]::Position = New-Object Drawing.Point $x, $y
    Start-Sleep -Milliseconds 80
    [ColumnRename.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [ColumnRename.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Report([string] $Status, [string] $Reason, [System.Collections.Generic.List[string]] $Observed) {
    [ordered]@{
        status = $Status
        reason = $Reason
        fixture = $fixture
        observed = @($Observed)
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
}

$observed = New-Object System.Collections.Generic.List[string]
$process = $null
try {
    if (-not (Test-Path -LiteralPath $Executable)) { throw "app executable missing: $Executable" }
    $start = [Diagnostics.ProcessStartInfo]::new($Executable)
    $start.WorkingDirectory = 'D:\SuperExplorer'
    $start.UseShellExecute = $false
    $start.Environment['LOCALAPPDATA'] = $profile
    $start.Environment['EXPLORER_INITIAL_PATH'] = $fixture
    $start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    $start.Environment['SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION'] = '1'
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if ($process.HasExited) { throw "app exited early: $($process.ExitCode)" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'window not found' }
    [void][ColumnRename.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 40, 40, 1100, 760, 0x0040)
    [void][ColumnRename.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $row = Find-Row $root 'rename-me.txt'
    if ($null -eq $row) { throw 'runner fixture row missing before mode switch' }
    $view = Find-Name $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-Name $root 'View' }
    if ($null -eq $view) { throw 'View control missing' }
    try { ($view.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke() } catch { Click $view }
    Start-Sleep -Milliseconds 650
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $columns = Wait-Name $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    if ($null -eq $columns) { $columns = Wait-Name $root 'Columns' 1000 }
    if ($null -eq $columns) {
        Add-Type -AssemblyName System.Drawing
        $bounds = ([Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)).Current.BoundingRectangle
        $shot = New-Object System.Drawing.Bitmap ([int]$bounds.Width), ([int]$bounds.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($shot)
        $graphics.CopyFromScreen(
            ([System.Drawing.Point]::new([int]$bounds.Left, [int]$bounds.Top)),
            [System.Drawing.Point]::Empty,
            ([System.Drawing.Size]::new([int]$bounds.Width, [int]$bounds.Height)))
        $shot.Save((Join-Path $OutputDirectory 'menu-missing.png'))
        $graphics.Dispose()
        $shot.Dispose()
        throw 'Columns option missing'
    }
    Click $columns
    Start-Sleep -Milliseconds 650
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $surface = Find-Name $root ([string]::new(@([char]0x5206, [char]0x6B04, [char]0x6AA2, [char]0x8996)))
    if ($null -eq $surface) { $surface = Find-Name $root 'Column view' }
    if ($null -eq $surface) { throw 'Columns surface missing' }
    $row = Find-Row $root 'rename-me.txt'
    if ($null -eq $row) { throw 'runner row missing in Columns mode' }
    $rowRect = $row.Current.BoundingRectangle
    Click $row
    Start-Sleep -Milliseconds 180
    [void][ColumnRename.Native]::SetForegroundWindow($process.MainWindowHandle)
    [ColumnRename.Native]::keybd_event(0x71, 0, 0, [UIntPtr]::Zero)
    [ColumnRename.Native]::keybd_event(0x71, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    Add-Type -AssemblyName System.Drawing
    $bounds = ([Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)).Current.BoundingRectangle
    $shot = New-Object System.Drawing.Bitmap ([int]$bounds.Width), ([int]$bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($shot)
    $graphics.CopyFromScreen(
        ([System.Drawing.Point]::new([int]$bounds.Left, [int]$bounds.Top)),
        [System.Drawing.Point]::Empty,
        ([System.Drawing.Size]::new([int]$bounds.Width, [int]$bounds.Height)))
    $shot.Save((Join-Path $OutputDirectory 'after-f2.png'))
    $graphics.Dispose()
    $shot.Dispose()
    $observed.Add("F2 screenshot captured; selected row bounds: $rowRect")
    [System.Windows.Forms.SendKeys]::SendWait('^a')
    [System.Windows.Forms.SendKeys]::SendWait('renamed-by-columns.txt')
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    $renamedDeadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        if (Test-Path -LiteralPath $renamed) { break }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $renamedDeadline)
    if (-not (Test-Path -LiteralPath $renamed)) { throw 'F2 editor appeared, but rename did not commit to the runner fixture' }
    if (Test-Path -LiteralPath $original) { throw 'old fixture filename still exists after rename' }
    $observed.Add('F2 committed runner-owned file rename')
    Report 'PASS' 'Columns F2 showed an inline editor and renamed the selected fixture file.' $observed
    exit 0
} catch {
    $observed.Add($_.Exception.Message)
    Report 'FAIL' $_.Exception.Message $observed
    exit 1
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
