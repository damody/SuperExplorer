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

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile `
        -SkipBuild:$SkipBuild -AdditionalEnvironment @{ SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION = '1' }
    [void](Find-UitestFileItem -Root $context.Root -Name 'alpha.txt')
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr]::Zero, 80, 80, 540, 340, 0x0040)
    Start-Sleep -Milliseconds 300
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
    $menu = [RustExplorerUitest.DetailsPopupNative]::SendMessage($popup, 0x01E1, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($menu -eq [IntPtr]::Zero) { throw 'Details popup did not expose its menu model' }
    $count = [RustExplorerUitest.DetailsPopupNative]::GetMenuItemCount($menu)
    if ($count -lt 8) { throw "Details popup omitted columns: item_count=$count" }
    $labels = foreach ($position in 0..($count - 1)) {
        $label = [Text.StringBuilder]::new(256)
        [void][RustExplorerUitest.DetailsPopupNative]::GetMenuString($menu, [uint32]$position, $label, $label.Capacity, 0x00000400)
        if ($label.Length -gt 0) { $label.ToString() }
    }
    $lastLayout = [RustExplorerUitest.DetailsPopupNative]::SendMessage($popup, 0x0451, [IntPtr]($count - 1), [IntPtr]::Zero).ToInt64()
    if ($lastLayout -lt 0) { throw 'Last Details column row is not materialized' }
    $lastBottom = [int]($lastLayout -band 0xffff) + [int](($lastLayout -shr 16) -band 0xffff)
    if ($lastBottom -gt ($popupRect.Bottom - $popupRect.Top)) {
        throw "All Details rows did not fit despite available screen space: last_bottom=$lastBottom popup=$popupRect"
    }
    $width = $popupRect.Right - $popupRect.Left
    $height = $popupRect.Bottom - $popupRect.Top
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($popupRect.Left, $popupRect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $output 'details-column-popup.png'), [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
    Send-UitestKey -Key 0x1B
    [ordered]@{
        schema = 'superexplorer.details-column-popup.v1'
        status = 'PASS'
        popup_class = 'SuperExplorer.ImmersivePopup.v1'
        anchored_near_pointer = $true
        independent_top_level_popup = $true
        not_clipped_by_small_main_window = $true
        all_rows_materialized = $true
        item_count = $count
        labels = @($labels)
        screenshot = 'details-column-popup.png'
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Get-Content -Raw (Join-Path $output 'report.json')
