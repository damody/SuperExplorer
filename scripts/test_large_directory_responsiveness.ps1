param(
    [string]$Executable = 'C:\Program Files\SuperExplorer\SuperExplorer.exe',
    [string]$OutputDirectory = 'D:\SuperExplorer\build\large-directory-responsiveness',
    [int]$Cycles = 3,
    [string]$TargetName = '',
    [switch]$IncludeDenseChildren,
    [switch]$SortBySize
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class LargeDirectoryProbe {
    [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
}
'@
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$targets = @(
    @{ path = 'C:\portable\OpenKoikatsu'; child = 'Content'; name = 'openkoikatsu' },
    @{ path = 'C:\portable\KoikatuSunshine\UserData\chara\female'; child = '作者'; name = 'female' },
    @{ path = 'D:\SuperExplorer'; child = 'crates'; name = 'superexplorer' }
)
if ($IncludeDenseChildren) {
    $targets += @(
        @{ path = 'C:\portable\OpenKoikatsu\Saved\NativeImportSources\147151CE45EAE64B0668C286F24D35DB'; child = 'mods'; name = 'openkoikatsu-7283' },
        @{ path = 'C:\portable\KoikatuSunshine\UserData\chara\female\其它'; child = '尚未分类'; name = 'female-990' },
        @{ path = 'D:\SuperExplorer\build\fluent-svg-icons-1.1.339-audit\package'; child = 'icons'; name = 'superexplorer-20643' }
    )
}
$reports = [Collections.Generic.List[object]]::new()

function Send-ProbeKey {
    param([byte]$Key, [byte[]]$Modifiers = @())
    foreach ($modifier in $Modifiers) { [RustExplorerUitest.Native]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 60
    [RustExplorerUitest.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [RustExplorerUitest.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    for ($index = $Modifiers.Count - 1; $index -ge 0; $index--) { [RustExplorerUitest.Native]::keybd_event($Modifiers[$index], 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 180
}
function Assert-Responsive($Context) {
    $response = [IntPtr]::Zero
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $sent = [LargeDirectoryProbe]::SendMessageTimeout($Context.Hwnd, 0, [IntPtr]::Zero, [IntPtr]::Zero, 2, 1000, [ref]$response)
    if ($sent -eq [IntPtr]::Zero) { throw 'The application did not respond within one second.' }
    $timer.ElapsedMilliseconds
}

function Set-SizeSort($Context) {
    $button = Find-UitestElement -Root $Context.Root -Description 'Sort toolbar' -Predicate {
        param($e) $e.Current.Name -in @('排序', 'Sort') -and $e.Current.BoundingRectangle.Width -gt 0
    }
    Invoke-UitestClick -Element $button
    $size = Find-UitestElement -Root $Context.Root -Description 'Size sort menu item' -Predicate {
        param($e) $e.Current.Name -in @('大小', 'Size') -and
            $e.Current.ControlType -eq [Windows.Automation.ControlType]::Button
    }
    Invoke-UitestClick -Element $size
    # Return keyboard focus from the command bar to the file list before paging.
    $row = Find-VisibleFileRow $Context
    Invoke-UitestClick -Element $row
}

function Wait-Directory($Context, [string]$Path) {
    $expected = @(Get-ChildItem -Force -LiteralPath $Path)
    $expectedLabels = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $expected) {
        [void]$expectedLabels.Add($entry.Name)
        [void]$expectedLabels.Add($entry.Name + ' File')
        [void]$expectedLabels.Add($entry.Name + ' Folder')
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $maxResponseMs = 0
    do {
        $maxResponseMs = [Math]::Max($maxResponseMs, (Assert-Responsive $Context))
        $elements = $Context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
        $status = @($elements | Where-Object {
            ($_.Current.Name -replace '[\u2066-\u2069,]', '') -match "(?<!\d)$($expected.Count) 個項目"
        } | ForEach-Object { $_.Current.Name })
        $rows = @(Get-UitestFileItems -Root $Context.Root)
        $validRow = @($rows | Where-Object {
            $expectedLabels.Contains($_.Current.Name)
        }).Count -gt 0
        if ($validRow -and $status.Count -gt 0) {
            return @{ expected_count = $expected.Count; visible_rows = $rows.Count; status = $status[0]; max_response_ms = $maxResponseMs }
        }
        Start-Sleep -Milliseconds 40
    } while ($timer.Elapsed.TotalSeconds -lt 30)
    @($elements | ForEach-Object { $_.Current.Name }) | ConvertTo-Json | Set-Content (Join-Path $OutputDirectory 'failed-elements.json')
    Save-UitestScreenshot -Root $Context.Root -Path (Join-Path $OutputDirectory 'failed.png')
    throw "Directory did not finish listing within 30 seconds: $Path; expected=$($expected.Count)"
}

function Get-Popups {
    $handles = [Collections.Generic.List[IntPtr]]::new()
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
            $class = [Text.StringBuilder]::new(64)
            [void][RustExplorerUitest.Native]::GetClassName($hwnd, $class, 64)
            if ($class.ToString() -in @('#32768', 'SuperExplorer.ImmersivePopup.v1')) { $handles.Add($hwnd) }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    @($handles)
}

function Measure-Menu($Context, $Row) {
    if (@(Get-Popups).Count -ne 0) { throw 'A popup was already open before the gesture.' }
    [void][RustExplorerUitest.Native]::SetForegroundWindow($Context.Hwnd)
    $point = Get-UitestPhysicalPoint -Element $Row -HorizontalOffset 80
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    [RustExplorerUitest.Native]::mouse_event(8, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(16, 0, 0, 0, [UIntPtr]::Zero)
    do {
        $popups = @(Get-Popups)
        if ($popups.Count -gt 0) { break }
        Start-Sleep -Milliseconds 10
    } while ($timer.Elapsed.TotalSeconds -lt 10)
    $menuMs = $timer.ElapsedMilliseconds
    if ($popups.Count -ne 1) { throw "Expected one context menu, got $($popups.Count)." }
    $popupPid = [uint32]0
    [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($popups[0], [ref]$popupPid)
    $processes = @(Get-CimInstance Win32_Process)
    $ancestor = [int]$popupPid
    $owned = $false
    foreach ($step in 1..12) {
        if ($ancestor -eq $Context.Process.Id) { $owned = $true; break }
        $process = $processes | Where-Object ProcessId -eq $ancestor | Select-Object -First 1
        if ($null -eq $process) { break }
        $ancestor = [int]$process.ParentProcessId
    }
    if (-not $owned) { throw 'Popup did not belong to the tested application.' }
    $responseMs = Assert-Responsive $Context
    Send-ProbeKey -Key 0x1B
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (@(Get-Popups).Count -gt 0 -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 20 }
    if (@(Get-Popups).Count -gt 0) { throw 'Escape did not dismiss the menu.' }
    [void][RustExplorerUitest.Native]::SetForegroundWindow($Context.Hwnd)
    Start-Sleep -Milliseconds 150
    @{ menu_ms = $menuMs; response_ms = $responseMs }
}

function Find-VisibleFileRow($Context, [string]$Name = '') {
    $window = $Context.Root.Current.BoundingRectangle
    foreach ($page in 0..60) {
        $rows = @(Get-UitestFileItems -Root $Context.Root | Where-Object {
            try {
                $bounds = $_.Current.BoundingRectangle
                ($Name -eq '' -or $_.Current.Name -eq $Name -or $_.Current.Name.StartsWith($Name + ' ')) -and
                    $bounds.Top -ge ($window.Top + 300) -and $bounds.Bottom -lt ($window.Bottom - 50)
            } catch { $false }
        })
        if ($rows.Count -gt 0) { return $rows[0] }
        Send-ProbeKey -Key 0x22
    }
    throw "Could not scroll the requested row into view: $Name"
}

foreach ($target in $targets) {
    if ($TargetName -and $target.name -ne $TargetName) { continue }
    $output = Join-Path $OutputDirectory $target.name
    $context = Start-UitestExplorer -InitialPath $target.path -OutputDirectory $output -Executable $Executable -SkipBuild `
        -AdditionalEnvironment @{ EXPLORER_AUTO_CLOSE_MS = '180000' }
    try {
        [void][LargeDirectoryProbe]::ShowWindow($context.Hwnd, 9)
        [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 20, 20, 1440, 880, 0x0040)
        [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 0, 0, 0, 0, 3)
        [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
        Start-Sleep -Milliseconds 200
        $listing = Wait-Directory $context $target.path
        if ($SortBySize) { Set-SizeSort $context }
        $measurements = @()
        foreach ($cycle in 1..$Cycles) {
            Send-ProbeKey -Key 0x24 -Modifiers @(0x11)
            $row = Find-VisibleFileRow $context $target.child
            Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'before-menu.png')
            $parentMenu = Measure-Menu $context $row

            $row = Find-VisibleFileRow $context $target.child
            $timer = [Diagnostics.Stopwatch]::StartNew()
            Invoke-UitestClick -Element $row -Double
            $childListing = Wait-Directory $context (Join-Path $target.path $target.child)
            $openMs = $timer.ElapsedMilliseconds
            $childRow = Find-VisibleFileRow $context
            $childMenu = Measure-Menu $context $childRow
            $timer.Restart()
            [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
            $up = Find-UitestElement -Root $context.Root -Description 'Up toolbar button' -Predicate { param($e) $e.Current.Name -eq '上一層' }; Invoke-UitestClick -Element $up
            $listing = Wait-Directory $context $target.path
            $backMs = $timer.ElapsedMilliseconds
            Send-ProbeKey -Key 0x23 -Modifiers @(0x11)
            $scrollMs = Assert-Responsive $context
            Send-ProbeKey -Key 0x24 -Modifiers @(0x11)
            $measurements += @{ cycle = $cycle; menu_ms = $parentMenu.menu_ms; popup_response_ms = $parentMenu.response_ms; child_menu_ms = $childMenu.menu_ms; child_popup_response_ms = $childMenu.response_ms; open_ms = $openMs; back_ms = $backMs; scroll_response_ms = $scrollMs; child_count = $childListing.expected_count }
        }
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'ready.png')
        $report = @{ status = 'PASS'; path = $target.path; child = $target.child; listing = $listing; cycles = $measurements; executable = $Executable; size_sort = [bool]$SortBySize }
        $report | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $output 'report.json') -Encoding utf8
        $reports.Add($report)
        $report | ConvertTo-Json -Depth 6
    } catch {
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'failed.png')
        throw
    } finally {
        Stop-UitestExplorer -Context $context
    }
}
$reports | ConvertTo-Json -Depth 7 | Set-Content (Join-Path $OutputDirectory 'report.json') -Encoding utf8


