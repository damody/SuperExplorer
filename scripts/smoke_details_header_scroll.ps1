param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('details-header-scroll-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
$fixtureRoot = Join-Path $targetRoot ('details-header-scroll-fixture-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
1..240 | ForEach-Object {
    [IO.File]::WriteAllText((Join-Path $fixtureRoot ('item-{0:D3}.txt' -f $_)), "fixture $_")
}
[IO.File]::WriteAllText((Join-Path $fixtureRoot 'Alpha.txt'), 'alpha')
[IO.File]::WriteAllText((Join-Path $fixtureRoot 'Zebra.log'), 'zebra')

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ('DetailsHeaderScroll.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace DetailsHeaderScroll {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    }
}
'@
}

function Find-Element([Windows.Automation.AutomationElement]$Root, [Windows.Automation.ControlType]$Type, [string]$Name) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
            if ($element.Current.ControlType -eq $Type -and $element.Current.Name -like "*$Name*") { return $element }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $Name"
}

function Find-ById([Windows.Automation.AutomationElement]$Root, [string]$Id) {
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}

function Find-OptionalElement([Windows.Automation.AutomationElement]$Root, [Windows.Automation.ControlType]$Type, [string]$Name) {
    foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
        if ($element.Current.ControlType -eq $Type -and $element.Current.Name -like "*$Name*") { return $element }
    }
    return $null
}

function Invoke-Element([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        $bounds = $Element.Current.BoundingRectangle
        [void][DetailsHeaderScroll.Native]::SetCursorPos([int](($bounds.Left + $bounds.Right) / 2), [int](($bounds.Top + $bounds.Bottom) / 2))
        [DetailsHeaderScroll.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [DetailsHeaderScroll.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 200
        return
    }
    ([Windows.Automation.InvokePattern]$pattern).Invoke()
    Start-Sleep -Milliseconds 200
}

function Read-Scroll([Windows.Automation.AutomationElement]$Root) {
    $scrollbar = Find-Element $Root ([Windows.Automation.ControlType]::ScrollBar) 'File view vertical scroll bar'
    $pattern = $null
    if (-not $scrollbar.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        throw 'file-view scrollbar does not expose RangeValuePattern'
    }
    return [double]([Windows.Automation.RangeValuePattern]$pattern).Current.Value
}

function Save-WindowCapture([Windows.Automation.AutomationElement]$Root, [string]$Path) {
    $bounds = $Root.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::new([int]$bounds.Width, [int]$bounds.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen([int]$bounds.Left, [int]$bounds.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

$process = $null
try {
    $start = [Diagnostics.ProcessStartInfo]::new($executable)
    $start.WorkingDirectory = $workspaceRoot
    $start.UseShellExecute = $false
    $start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
    $start.Environment['EXPLORER_INITIAL_PATH'] = $fixtureRoot
    $start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        if ($process.MainWindowHandle -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][DetailsHeaderScroll.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 20, 20, 1100, 800, 0x0040)
    [void][DetailsHeaderScroll.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 1000
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $nameLabel = -join ([char]0x540D, [char]0x7A31)
    $header = Find-Element $root ([Windows.Automation.ControlType]::Button) $nameLabel
    $filterButtons = @($root.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button)) |
        Where-Object { $_.Current.Name -like 'Filter *' })
    if ($filterButtons.Count -lt 4) { throw "expected filter buttons on every visible details column, found $($filterButtons.Count)" }
    $nameFilter = $filterButtons | Sort-Object { $_.Current.BoundingRectangle.Left } | Select-Object -First 1
    if ($null -eq $nameFilter) { throw 'Name filter button not found' }
    Invoke-Element $nameFilter
    $filterMenu = Find-Element $root ([Windows.Automation.ControlType]::Menu) 'Filter Name'
    $nameGroups = @($filterMenu.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::MenuItem)))
    if ($nameGroups.Count -lt 3) { throw "Name filter did not expose A-H, I-P and Q-Z groups: $($nameGroups.Count)" }
    Invoke-Element $nameGroups[0]
    $filterMenu = Find-Element $root ([Windows.Automation.ControlType]::Menu) 'Filter Name'
    $nameGroups = @($filterMenu.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::MenuItem)))
    Invoke-Element $nameGroups[1]
    if ($null -eq (Find-OptionalElement $root ([Windows.Automation.ControlType]::Menu) 'Filter Name')) {
        throw 'filter menu closed while selecting multiple options'
    }

    $outside = Find-Element $root ([Windows.Automation.ControlType]::ListItem) 'item-'
    Invoke-Element $outside
    if ($null -ne (Find-OptionalElement $root ([Windows.Automation.ControlType]::Menu) 'Filter Name')) {
        throw 'filter menu remained open after focus left the popup'
    }

    $row = Find-Element $root ([Windows.Automation.ControlType]::ListItem) 'item-'
    $headerTopBefore = $header.Current.BoundingRectangle.Top
    $scrollBefore = Read-Scroll $root
    Save-WindowCapture $root (Join-Path $OutputDirectory 'before.png')

    $rowBounds = $row.Current.BoundingRectangle
    [void][DetailsHeaderScroll.Native]::SetCursorPos(
        [int]($rowBounds.Left + [Math]::Min(160, $rowBounds.Width / 2)),
        [int]($rowBounds.Top + $rowBounds.Height / 2)
    )
    foreach ($tick in 1..8) {
        [DetailsHeaderScroll.Native]::mouse_event(0x0800, 0, 0, [uint32]4294967176, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 50
    }
    Start-Sleep -Milliseconds 300
    $scrollAfterWheel = Read-Scroll $root
    $headerTopAfterWheel = (Find-Element $root ([Windows.Automation.ControlType]::Button) $nameLabel).Current.BoundingRectangle.Top
    Save-WindowCapture $root (Join-Path $OutputDirectory 'after-wheel.png')
    if ($scrollAfterWheel -le $scrollBefore) { throw "wheel did not scroll rows: before=$scrollBefore after=$scrollAfterWheel" }
    if ([Math]::Abs($headerTopAfterWheel - $headerTopBefore) -gt 2) {
        throw "header moved after wheel: before=$headerTopBefore after=$headerTopAfterWheel"
    }

    [DetailsHeaderScroll.Native]::keybd_event(0x23, 0, 0, [UIntPtr]::Zero)
    [DetailsHeaderScroll.Native]::keybd_event(0x23, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    $headerTopAfterEnd = (Find-Element $root ([Windows.Automation.ControlType]::Button) $nameLabel).Current.BoundingRectangle.Top
    $scrollAfterEnd = Read-Scroll $root
    if ([Math]::Abs($headerTopAfterEnd - $headerTopBefore) -gt 2) {
        throw "header moved after End: before=$headerTopBefore after=$headerTopAfterEnd"
    }
    [ordered]@{
        schema_version=1
        captured_utc=[DateTime]::UtcNow.ToString('o')
        fixture_item_count=242
        scroll=[ordered]@{ before=$scrollBefore; after_wheel=$scrollAfterWheel; after_end=$scrollAfterEnd }
        header_top=[ordered]@{ before=$headerTopBefore; after_wheel=$headerTopAfterWheel; after_end=$headerTopAfterEnd }
        details_filter_buttons=$filterButtons.Count
        details_filter_multi_select_persisted=$true
        details_filter_closed_on_focus_leave=$true
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Details header scroll smoke passed: $OutputDirectory"
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        [void][DetailsHeaderScroll.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    if (Test-Path -LiteralPath $fixtureRoot) {
        $resolved = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $fixtureRoot).Path)
        $allowedPrefix = [IO.Path]::GetFullPath($targetRoot).TrimEnd('\') + '\details-header-scroll-fixture-'
        if (-not $resolved.StartsWith($allowedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing unsafe fixture cleanup: $resolved"
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
