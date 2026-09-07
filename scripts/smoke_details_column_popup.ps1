param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

if (-not ('RustExplorerUitest.DetailsPopupNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace RustExplorerUitest {
    public static class DetailsPopupNative {
        [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int count, uint flags);
        [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr menu, uint item, uint flags);
        public const uint MfByPosition = 0x00000400;
        public const uint MfChecked = 0x00000008;
        public const uint MfDisabled = 0x00000002;
        public const uint MfGrayed = 0x00000001;
    }
}
'@
}

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'alpha.txt') -Value 'alpha' -Encoding utf8
$context = $null

function Get-ProcessTreeIds {
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add([int]$context.Process.Id)
    do {
        $changed = $false
        foreach ($process in @(Get-CimInstance Win32_Process)) {
            if ($ids.Contains([int]$process.ParentProcessId) -and $ids.Add([int]$process.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    return ,$ids
}

function Get-DetailsPopup {
    $allowed = Get-ProcessTreeIds
    $handles = [Collections.Generic.List[IntPtr]]::new()
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
            $className = [Text.StringBuilder]::new(96)
            [void][RustExplorerUitest.Native]::GetClassName($hwnd, $className, $className.Capacity)
            [uint32]$processId = 0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($hwnd, [ref]$processId)
            if ($className.ToString() -eq 'SuperExplorer.ImmersivePopup.v1' -and $allowed.Contains([int]$processId)) {
                $handles.Add($hwnd)
            }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    $handles | Select-Object -First 1
}

function Wait-DetailsPopup {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $popup = Get-DetailsPopup
        if ($null -ne $popup) { return $popup }
        Start-Sleep -Milliseconds 80
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Details column popup did not appear'
}

function Wait-DetailsPopupGone {
    $deadline = [DateTime]::UtcNow.AddSeconds(4)
    do {
        if ($null -eq (Get-DetailsPopup)) { return }
        Start-Sleep -Milliseconds 80
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Details column popup remained visible'
}

function Get-DetailsPopupMenu($popup) {
    $menu = [RustExplorerUitest.DetailsPopupNative]::SendMessage($popup, 0x01E1, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($menu -eq [IntPtr]::Zero) { throw 'Details popup did not expose its menu model' }
    return $menu
}

function Get-DetailsPopupLabels($menu, $count) {
    foreach ($position in 0..($count - 1)) {
        $label = [Text.StringBuilder]::new(256)
        [void][RustExplorerUitest.DetailsPopupNative]::GetMenuString(
            $menu, [uint32]$position, $label, $label.Capacity, 0x00000400)
        if ($label.Length -gt 0) { $label.ToString() }
    }
}

function Get-DetailsPopupRowChecked($menu, [int]$position) {
    $state = [RustExplorerUitest.DetailsPopupNative]::GetMenuState(
        $menu, [uint32]$position, [RustExplorerUitest.DetailsPopupNative]::MfByPosition)
    return ($state -band [RustExplorerUitest.DetailsPopupNative]::MfChecked) -ne 0
}

function Invoke-DetailsPopupRow($popup, [int]$position) {
    $layout = [RustExplorerUitest.DetailsPopupNative]::SendMessage(
        $popup, 0x0451, [IntPtr]$position, [IntPtr]::Zero).ToInt64()
    if ($layout -lt 0) { throw "Details popup row $position is not materialized" }
    $top = [int]($layout -band 0xffff)
    $height = [int](($layout -shr 16) -band 0xffff)
    $rect = [RustExplorerUitest.Native+RECT]::new()
    if (-not [RustExplorerUitest.Native]::GetWindowRect($popup, [ref]$rect)) {
        throw 'Unable to read Details popup bounds for row click'
    }
    $x = [int](($rect.Left + $rect.Right) / 2)
    $y = $rect.Top + $top + [Math]::Max(2, [int]($height / 2))
    [void][RustExplorerUitest.Native]::SetPhysicalCursorPos($x, $y)
    Start-Sleep -Milliseconds 40
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 120
}

function Save-DetailsPopupScreenshot($popup, [string]$name) {
    $rect = [RustExplorerUitest.Native+RECT]::new()
    if (-not [RustExplorerUitest.Native]::GetWindowRect($popup, [ref]$rect)) {
        throw "Unable to read Details popup bounds for $name"
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $output $name), [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Open-DetailsPopupFromHeader {
    $header = Find-UitestElement -Root $context.Root -Description 'Details header' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            ($element.Current.Name -like 'Sort by *' -or $element.Current.Name -like '*sorted*') -and
            $element.Current.BoundingRectangle.Width -gt 40
    }
    $click = Get-UitestPhysicalPoint -Element $header -HorizontalOffset 30
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    [void][RustExplorerUitest.Native]::SetPhysicalCursorPos($click.X, $click.Y)
    [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    $popup = Wait-DetailsPopup
    return @{ Popup = $popup; Click = $click }
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile `
        -SkipBuild:$SkipBuild -AdditionalEnvironment @{ SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION = '1' }
    [void](Find-UitestFileItem -Root $context.Root -Name 'alpha.txt')
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr]::Zero, 80, 80, 540, 340, 0x0040)
    Start-Sleep -Milliseconds 300
    $opened = Open-DetailsPopupFromHeader
    $popup = $opened.Popup
    $click = $opened.Click
    $popupRect = [RustExplorerUitest.Native+RECT]::new()
    if (-not [RustExplorerUitest.Native]::GetWindowRect($popup, [ref]$popupRect)) {
        throw 'Unable to read Details popup bounds'
    }
    $windowRect = [RustExplorerUitest.Native+RECT]::new()
    if (-not [RustExplorerUitest.Native]::GetWindowRect($context.Hwnd, [ref]$windowRect)) {
        throw 'Unable to read main window bounds'
    }
    if ([Math]::Abs($popupRect.Left - $click.X) -gt 40 -or [Math]::Abs($popupRect.Top - $click.Y) -gt 40) {
        throw "Details popup is not near the pointer: click=($($click.X),$($click.Y)) popup=$popupRect"
    }
    $menu = Get-DetailsPopupMenu $popup
    $count = [RustExplorerUitest.DetailsPopupNative]::GetMenuItemCount($menu)
    if ($count -lt 8) { throw "Details popup omitted columns: item_count=$count" }
    $labels = @(Get-DetailsPopupLabels $menu $count)
    $lastLayout = [RustExplorerUitest.DetailsPopupNative]::SendMessage($popup, 0x0451, [IntPtr]($count - 1), [IntPtr]::Zero).ToInt64()
    if ($lastLayout -lt 0) { throw 'Last Details column row is not materialized' }
    $lastBottom = [int]($lastLayout -band 0xffff) + [int](($lastLayout -shr 16) -band 0xffff)
    if ($lastBottom -gt ($popupRect.Bottom - $popupRect.Top)) {
        throw "All Details rows did not fit despite available screen space: last_bottom=$lastBottom popup=$popupRect"
    }
    Save-DetailsPopupScreenshot $popup 'details-column-popup.png'

    $sizeIndex = -1
    $nameIndex = -1
    for ($position = 0; $position -lt $count; $position++) {
        $label = [Text.StringBuilder]::new(256)
        [void][RustExplorerUitest.DetailsPopupNative]::GetMenuString(
            $menu, [uint32]$position, $label, $label.Capacity, 0x00000400)
        switch ($label.ToString()) {
            'Size' { $sizeIndex = $position }
            'Name' { $nameIndex = $position }
        }
    }
    if ($sizeIndex -lt 0) { throw "Size row was not found in $($labels -join ', ')" }
    if ($nameIndex -lt 0) { throw 'Name row was not found' }
    if (-not (Get-DetailsPopupRowChecked $menu $nameIndex)) {
        throw 'Name row must start checked'
    }
    $initialSizeChecked = Get-DetailsPopupRowChecked $menu $sizeIndex
    $hwndBefore = [int64]$popup
    $scrollBefore = [RustExplorerUitest.DetailsPopupNative]::SendMessage(
        $popup, 0x0451, [IntPtr]$sizeIndex, [IntPtr]::Zero).ToInt64()
    Invoke-DetailsPopupRow $popup $sizeIndex
    $popupAfter = Get-DetailsPopup
    if ($null -eq $popupAfter) { throw 'Size toggle dismissed the Details popup' }
    if ([int64]$popupAfter -ne $hwndBefore) { throw 'Size toggle replaced the Details popup HWND' }
    $menu = Get-DetailsPopupMenu $popupAfter
    $afterFirst = Get-DetailsPopupRowChecked $menu $sizeIndex
    if ($afterFirst -eq $initialSizeChecked) {
        throw "Size check state did not change after the first click: checked=$afterFirst"
    }
    Save-DetailsPopupScreenshot $popupAfter 'details-column-popup-toggled.png'
    Invoke-DetailsPopupRow $popupAfter $sizeIndex
    $popupAfterSecond = Get-DetailsPopup
    if ($null -eq $popupAfterSecond) { throw 'Second Size toggle dismissed the Details popup' }
    if ([int64]$popupAfterSecond -ne $hwndBefore) { throw 'Second Size toggle replaced the Details popup HWND' }
    $menu = Get-DetailsPopupMenu $popupAfterSecond
    $afterSecond = Get-DetailsPopupRowChecked $menu $sizeIndex
    if ($afterSecond -ne $initialSizeChecked) {
        throw "Size check state did not restore after the second click: checked=$afterSecond"
    }
    $scrollAfter = [RustExplorerUitest.DetailsPopupNative]::SendMessage(
        $popupAfterSecond, 0x0451, [IntPtr]$sizeIndex, [IntPtr]::Zero).ToInt64()
    if ($scrollAfter -ne $scrollBefore) {
        throw "Size toggle changed popup scroll/layout: before=$scrollBefore after=$scrollAfter"
    }
    Invoke-DetailsPopupRow $popupAfterSecond $nameIndex
    $popupAfterName = Get-DetailsPopup
    if ($null -eq $popupAfterName) { throw 'Name click dismissed the Details popup' }
    $menu = Get-DetailsPopupMenu $popupAfterName
    if (-not (Get-DetailsPopupRowChecked $menu $nameIndex)) {
        throw 'Name click must not uncheck the required column'
    }
    Save-DetailsPopupScreenshot $popupAfterName 'details-column-popup-checked.png'

    Send-UitestKey -Key 0x1B
    Wait-DetailsPopupGone

    $reopened = Open-DetailsPopupFromHeader
    $outsideX = $windowRect.Left + 20
    $outsideY = $windowRect.Bottom - 12
    [void][RustExplorerUitest.Native]::SetPhysicalCursorPos($outsideX, $outsideY)
    Start-Sleep -Milliseconds 40
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Wait-DetailsPopupGone

    $terminal = Open-DetailsPopupFromHeader
    Invoke-DetailsPopupRow $terminal.Popup 0
    Wait-DetailsPopupGone

    [ordered]@{
        schema = 'superexplorer.details-column-popup.v2'
        status = 'PASS'
        popup_class = 'SuperExplorer.ImmersivePopup.v1'
        anchored_near_pointer = $true
        independent_top_level_popup = $true
        not_clipped_by_small_main_window = $true
        all_rows_materialized = $true
        item_count = $count
        labels = @($labels)
        size_index = $sizeIndex
        name_index = $nameIndex
        size_initial_checked = $initialSizeChecked
        size_after_first_toggle = $afterFirst
        size_after_second_toggle = $afterSecond
        hwnd_unchanged = $true
        name_remained_checked = $true
        escape_dismissed = $true
        outside_click_dismissed = $true
        auto_size_dismissed = $true
        screenshot = 'details-column-popup.png'
        screenshot_toggled = 'details-column-popup-toggled.png'
        screenshot_checked = 'details-column-popup-checked.png'
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Get-Content -Raw (Join-Path $output 'report.json')
