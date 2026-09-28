param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = ""
)

$ErrorActionPreference = "Stop"
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory "fixture"
$leaf = Join-Path $fixture "a\b\c\d"
New-Item -ItemType Directory -Force -Path $leaf | Out-Null
Set-Content -Path (Join-Path $leaf "notes.txt") -Value "column-scroll" -Encoding utf8
$profile = Join-Path $OutputDirectory "localappdata"
New-Item -ItemType Directory -Force -Path $profile | Out-Null

if (-not $Executable) {
    $Executable = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
}

function Write-Report([string] $Status, [string] $Reason, [System.Collections.Generic.List[string]] $Observed) {
    $report = [ordered]@{
        status = $Status
        reason = $Reason
        fixture = $leaf
        executable = $Executable
        observed = @($Observed)
    }
    $report | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
}

if (-not (Test-Path -LiteralPath $Executable)) {
    $empty = New-Object System.Collections.Generic.List[string]
    Write-Report "SKIP" "SuperExplorer.exe was not built, so the narrow column strip was not shown." $empty
    exit 0
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ("ColumnHScroll.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ColumnHScroll {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
    }
}
"@
}

function Find-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Click-Element([Windows.Automation.AutomationElement] $Element) {
    try {
        $pattern = $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        return "invoke"
    } catch {
        $bounds = $Element.Current.BoundingRectangle
        $x = [int]($bounds.Left + ($bounds.Width / 2))
        $y = [int]($bounds.Top + ([Math]::Min(12, $bounds.Height / 2)))
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point $x, $y
        if (-not ("ColumnHScroll.Mouse" -as [type])) {
            Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);' -Name "Mouse" -Namespace "ColumnHScroll"
        }
        Start-Sleep -Milliseconds 80
        [ColumnHScroll.Mouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnHScroll.Mouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "pointer@$x,$y"
    }
}

function Drag-Pointer([int] $StartX, [int] $StartY, [int] $EndX, [int] $EndY) {
    Add-Type -AssemblyName System.Windows.Forms
    if (-not ("ColumnHScroll.Mouse" -as [type])) {
        Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);' -Name "Mouse" -Namespace "ColumnHScroll"
    }
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point $StartX, $StartY
    Start-Sleep -Milliseconds 100
    [ColumnHScroll.Mouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    for ($step = 1; $step -le 5; $step++) {
        $x = [int]($StartX + ($EndX - $StartX) * $step / 5)
        $y = [int]($StartY + ($EndY - $StartY) * $step / 5)
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point $x, $y
        Start-Sleep -Milliseconds 80
    }
    [ColumnHScroll.Mouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}

$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $Executable
$start.WorkingDirectory = "D:\SuperExplorer"
$start.UseShellExecute = $false
$start.Environment["EXPLORER_INITIAL_PATH"] = $leaf
$start.Environment["EXPLORER_LOG_DIR"] = $OutputDirectory
$start.Environment["LOCALAPPDATA"] = $profile
$start.Environment["SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION"] = "1"
$start.Environment["EXPLORER_AUTO_CLOSE_MS"] = "90000"
$process = [Diagnostics.Process]::Start($start)
$observations = New-Object System.Collections.Generic.List[string]
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw "application window did not appear" }
    # Narrow enough that four ancestor columns plus the preview cannot fit.
    [void][ColumnHScroll.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 40, 40, 720, 640, 0x0040)
    [void][ColumnHScroll.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 900
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $notes = $null
    $located = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $items = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::ListItem)))
        foreach ($item in $items) {
            if ($item.Current.Name -match '(^| )notes\.txt( |$)') { $notes = $item; break }
        }
        if ($null -ne $notes) { break }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $located)
    if ($null -eq $notes) {
        throw "runner fixture notes.txt was not listed; refusing to treat another folder as the column target"
    }
    $observations.Add("found fixture row: $($notes.Current.Name)")
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-ByName $root "View" }
    if ($null -eq $view) { throw "view menu button was not found" }
    $observations.Add(("view click: " + (Click-Element $view)))
    Start-Sleep -Milliseconds 400
    $columns = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    if ($null -eq $columns) { $columns = Find-ByName $root "Columns" }
    if ($null -eq $columns) { throw "columns menu item was not found" }
    $observations.Add(("columns click: " + (Click-Element $columns)))
    Start-Sleep -Milliseconds 900
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $surface = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04, [char]0x6AA2, [char]0x8996)))
    if ($null -eq $surface) { $surface = Find-ByName $root "Column view" }
    if ($null -eq $surface) { throw "column view surface was not exposed to UIA" }
    $observations.Add("column surface: $($surface.Current.Name)")
    $scroll = Find-ByName $root "File view horizontal scroll bar"
    if ($null -eq $scroll) {
        $scroll = $root.FindFirst(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::ScrollBar)))
    }
    if ($null -eq $scroll) { throw "column strip did not expose a horizontal scroll bar" }
    $scrollRect = $scroll.Current.BoundingRectangle
    $observations.Add("scrollbar: $($scroll.Current.Name) [$($scrollRect.Left),$($scrollRect.Top) $($scrollRect.Width)x$($scrollRect.Height)]")
    if ($scrollRect.Width -lt 8 -or $scrollRect.Height -lt 4) {
        throw "horizontal scroll bar is not large enough to operate"
    }
    $maximum = $null
    try {
        $range = $scroll.GetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern)
        $maximum = $range.Current.Maximum
        $observations.Add("scrollbar range: $($range.Current.Value) / $maximum")
    } catch {
        $observations.Add("scrollbar has no RangeValue pattern: $($_.Exception.Message)")
    }
    if ($null -ne $maximum -and $maximum -le 0) {
        throw "horizontal scroll range is zero, so the final column is not reachable by scrolling"
    }
    if ($null -ne $maximum) {
        $before = $range.Current.Value
        $clickX = [int]($scrollRect.Left + 6)
        $clickY = [int]($scrollRect.Top + ($scrollRect.Height / 2))
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point $clickX, $clickY
        if (-not ("ColumnHScroll.Mouse" -as [type])) {
            Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);' -Name "Mouse" -Namespace "ColumnHScroll"
        }
        Start-Sleep -Milliseconds 120
        [ColumnHScroll.Mouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnHScroll.Mouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 500
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $scroll = Find-ByName $root "File view horizontal scroll bar"
        if ($null -eq $scroll) { throw "horizontal scroll bar disappeared after the track click" }
        $range = $scroll.GetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern)
        $after = $range.Current.Value
        $observations.Add("scrollbar after track click: $before -> $after")
        if ($after -ge $before - 0.5) {
            throw "clicking the horizontal track did not move back toward the first ancestor"
        }
        # GPUI's decorative thumb has no UIA node. Its minimum logical width is 32,
        # and UIA reports the track in physical pixels (roughly 2x on this runner).
        $thumbWidth = [Math]::Max(48, $scrollRect.Width * $scrollRect.Width / ($scrollRect.Width + $maximum * 2))
        $thumbX = [int]($scrollRect.Left + $after / $maximum * ($scrollRect.Width - $thumbWidth) + $thumbWidth / 2)
        $thumbY = [int]($scrollRect.Top + $scrollRect.Height / 2)
        Drag-Pointer $thumbX $thumbY ($thumbX - 40) $thumbY
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $scroll = Find-ByName $root "File view horizontal scroll bar"
        $dragged = ($scroll.GetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern)).Current.Value
        $observations.Add("scrollbar after thumb drag: $after -> $dragged")
        if ($dragged -ge $after - 0.5) { throw "dragging the horizontal thumb did not move the columns" }
    }
    $divider = Find-ByName $root ([string]::new(@([char]0x8ABF, [char]0x6574, [char]0x5074, [char]0x908A, [char]0x7A97, [char]0x683C, [char]0x5927, [char]0x5C0F)))
    if ($null -eq $divider) { $divider = Find-ByName $root "Resize side pane" }
    if ($null -eq $divider) { throw "integrated preview divider was not exposed" }
    $dividerRect = $divider.Current.BoundingRectangle
    $observations.Add("preview divider: $($divider.Current.Name) [$($dividerRect.Width)x$($dividerRect.Height)]")
    if ($dividerRect.Height -lt 8) { throw "preview divider is not tall enough to drag" }
    $dividerX = [int]($dividerRect.Left + $dividerRect.Width / 2)
    $dividerY = [int]($dividerRect.Top + [Math]::Min(80, $dividerRect.Height / 2))
    Drag-Pointer $dividerX $dividerY ($dividerX + 75) $dividerY
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $divider = Find-ByName $root ([string]::new(@([char]0x8ABF, [char]0x6574, [char]0x5074, [char]0x908A, [char]0x7A97, [char]0x683C, [char]0x5927, [char]0x5C0F)))
    if ($null -eq $divider) { $divider = Find-ByName $root "Resize side pane" }
    if ($null -eq $divider) { throw "preview divider disappeared after drag" }
    $dividerAfter = $divider.Current.BoundingRectangle
    $observations.Add("preview divider after drag: $($dividerRect.Left) -> $($dividerAfter.Left)")
    if ($dividerAfter.Left -le $dividerRect.Left + 10) { throw "dragging the divider did not narrow the integrated preview" }
    Write-Report "PASS" "A narrow Columns window allowed horizontal track/thumb navigation and preview width dragging." $observations
    exit 0
} catch {
    $observations.Add($_.Exception.Message)
    Write-Report "FAIL" $_.Exception.Message $observations
    exit 1
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
