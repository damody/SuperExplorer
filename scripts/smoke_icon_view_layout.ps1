param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('icon-view-layout-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$longCjkName = -join @(48, 50, 45, 36889, 26159, 19968, 20491, 24456, 38263, 32780, 19988, 24517, 38920, 22312, 33258, 24049, 30340, 22294, 31034, 26684, 23376, 20839, 25563, 34892, 30340, 36039, 26009, 22846, 21517, 31281 | ForEach-Object { [char]$_ })
foreach ($name in @(
    '00 common attachment archive for quarterly reports',
    '01-ThisIsAnUnbrokenFilenameThatMustNeverEnterTheNextIconCell',
    $longCjkName,
    'common_attachment', 'inetpub', 'PerfLogs', 'portable', 'Program Files', 'Program Files (x86)', 'Riot Games', 'Windows'
)) {
    New-Item -ItemType Directory -Force -Path (Join-Path $fixture $name) | Out-Null
}
foreach ($name in @('appverifUI.dll', 'DumpStack.log', 'vfcompat.dll')) {
    Set-Content -LiteralPath (Join-Path $fixture $name) -Value 'icon view fixture' -Encoding utf8
}
if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

function New-AspectFixture([string]$Path, [int]$Width, [int]$Height, [Drawing.Color]$Color) {
    $bitmap = [Drawing.Bitmap]::new($Width, $Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.Clear($Color)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

New-AspectFixture (Join-Path $fixture 'A.png') 120 480 ([Drawing.Color]::Magenta)
New-AspectFixture (Join-Path $fixture 'B.png') 480 120 ([Drawing.Color]::Lime)
New-AspectFixture (Join-Path $fixture 'C.png') 240 240 ([Drawing.Color]::Cyan)
if (-not ('IconViewLayoutSmoke.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace IconViewLayoutSmoke {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
        [DllImport("user32.dll", SetLastError=true)] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    }
}
'@
}

function Wait-Element([Windows.Automation.AutomationElement]$Root, [string]$Name, [Windows.Automation.ControlType]$Type) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $Name)
        $elements = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        foreach ($element in $elements) {
            if ($element.Current.ControlType -eq $Type) { return $element }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $Name ($($Type.ProgrammaticName))"
}

function Click-Element([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
        return
    }
    $bounds = $Element.Current.BoundingRectangle
    [void][IconViewLayoutSmoke.Native]::SetCursorPos([int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
    [IconViewLayoutSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [IconViewLayoutSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Wait-FileRows([Windows.Automation.AutomationElement]$Root) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem)
        $all = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        $rows = @($all | Where-Object {
            $bounds = $_.Current.BoundingRectangle
            $bounds.Width -gt 0 -and $bounds.Height -gt 0
        })
        if ($rows.Count -ge 3) { return $rows }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'fewer than three visible file rows appeared'
}

function Convert-Bounds([Windows.Rect]$Bounds) {
    [ordered]@{ left=$Bounds.Left; top=$Bounds.Top; right=$Bounds.Right; bottom=$Bounds.Bottom; width=$Bounds.Width; height=$Bounds.Height }
}

function Save-WindowScreenshot([Windows.Automation.AutomationElement]$Root, [string]$Path) {
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

function Send-CtrlWheel([Windows.Automation.AutomationElement]$Root, [int]$Delta) {
    $rows = Wait-FileRows $Root
    $bounds = $rows[0].Current.BoundingRectangle
    [void][IconViewLayoutSmoke.Native]::SetCursorPos(
        [int]($bounds.Left + [Math]::Min($bounds.Width / 2, 80)),
        [int]($bounds.Top + $bounds.Height / 2))
    [IconViewLayoutSmoke.Native]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    try {
        $data = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]$Delta), 0)
        [IconViewLayoutSmoke.Native]::mouse_event(0x0800, 0, 0, $data, [UIntPtr]::Zero)
    } finally {
        [IconViewLayoutSmoke.Native]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 180
}

function Measure-FirstIcon([Windows.Automation.AutomationElement]$Root) {
    $row = (Wait-FileRows $Root)[0]
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Image)
    $images = $row.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
    $visible = @($images | Where-Object {
        $bounds = $_.Current.BoundingRectangle
        $bounds.Width -gt 0 -and $bounds.Height -gt 0
    } | Sort-Object { $_.Current.BoundingRectangle.Width })
    if ($visible.Count -eq 0) { throw 'file row did not expose its rendered icon through UIA' }
    return $visible[-1].Current.BoundingRectangle
}

function Measure-ThumbnailSeparation(
    [Windows.Automation.AutomationElement]$Root,
    [string]$FileName,
    [int]$SourceWidth,
    [int]$SourceHeight,
    [double]$DpiScale
) {
    $row = Wait-Element $Root "$FileName File" ([Windows.Automation.ControlType]::ListItem)
    $icon = Wait-Element $row "$FileName icon" ([Windows.Automation.ControlType]::Image)
    $rowBounds = $row.Current.BoundingRectangle
    $iconBounds = $icon.Current.BoundingRectangle
    $tolerance = 1.5
    $labelTop = $rowBounds.Bottom - (48.0 * $DpiScale)
    if ($iconBounds.Bottom -gt $labelTop + $tolerance) {
        throw "thumbnail overlaps its reserved filename region: file=$FileName icon=$iconBounds labelTop=$labelTop row=$rowBounds"
    }
    [ordered]@{
        file=$FileName
        source_width=$SourceWidth
        source_height=$SourceHeight
        source_aspect=[double]$SourceWidth / [double]$SourceHeight
        row=Convert-Bounds $rowBounds
        icon_host=Convert-Bounds $iconBounds
        filename_region=[ordered]@{
            top=$labelTop
            bottom=$rowBounds.Bottom
            height=48.0 * $DpiScale
        }
        non_overlapping=$true
    }
}

function Measure-LongNameCells([Windows.Automation.AutomationElement]$Root) {
    $names = @(
        '00 common attachment archive for quarterly reports',
        '01-ThisIsAnUnbrokenFilenameThatMustNeverEnterTheNextIconCell',
        $script:longCjkName
    )
    $rows = @($names | ForEach-Object {
        Wait-Element $Root "$_ Folder" ([Windows.Automation.ControlType]::ListItem)
    })
    $first = $rows[0].Current.BoundingRectangle
    $second = $rows[1].Current.BoundingRectangle
    $third = $rows[2].Current.BoundingRectangle
    $tolerance = 2.0
    if ($second.Left -lt $first.Right - $tolerance -or $third.Left -lt $second.Right - $tolerance) {
        throw "long-name icon cells overlap horizontally: first=$first second=$second third=$third"
    }
    $heightTolerance = [Math]::Max(2.0, $first.Height * 0.05)
    foreach ($row in $rows) {
        if ([Math]::Abs($row.Current.BoundingRectangle.Height - $first.Height) -gt $heightTolerance) {
            throw "selected/unselected long-name cells changed grid height: first=$first current=$($row.Current.BoundingRectangle)"
        }
    }
    $selection = $null
    if (-not $rows[0].TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
        throw 'long-name row has no SelectionItemPattern'
    }
    ([Windows.Automation.SelectionItemPattern]$selection).Select()
    Start-Sleep -Milliseconds 150
    $selectedFirst = (Wait-Element $Root "$($names[0]) Folder" ([Windows.Automation.ControlType]::ListItem)).Current.BoundingRectangle
    if ([Math]::Abs($selectedFirst.Height - $first.Height) -gt $heightTolerance) {
        throw "selected third-line expansion changed grid height: before=$first after=$selectedFirst"
    }
    [ordered]@{
        names=$names
        first=Convert-Bounds $first
        second=Convert-Bounds $second
        third=Convert-Bounds $third
        selected_first=Convert-Bounds $selectedFirst
        normal_lines=2
        selected_maximum_lines=3
        horizontally_disjoint=$true
        stable_cell_height=$true
    }
}

function Measure-Mode([Windows.Automation.AutomationElement]$Root, [string]$ViewName, [string]$MenuName, [string]$Mode, [string]$OutputDirectory) {
    $view = Wait-Element $Root $ViewName ([Windows.Automation.ControlType]::Button)
    Click-Element $view
    $item = Wait-Element $Root $MenuName ([Windows.Automation.ControlType]::MenuItem)
    Click-Element $item
    Start-Sleep -Milliseconds 350
    $rows = Wait-FileRows $Root
    $first = $rows[0].Current.BoundingRectangle
    $second = $rows[1].Current.BoundingRectangle
    $third = $rows[2].Current.BoundingRectangle
    $windowBounds = $Root.Current.BoundingRectangle
    $tolerance = [Math]::Max(3.0, $first.Height * 0.08)
    if ([Math]::Abs($first.Top - $second.Top) -gt $tolerance) {
        throw "$Mode did not advance left-to-right: first=$first second=$second"
    }
    if ($second.Left -le $first.Left) { throw "$Mode second cell is not to the right of first: first=$first second=$second" }
    if ($first.Width -ge $windowBounds.Width * 0.55) { throw "$Mode selection/hit cell still spans the viewport: row=$first window=$windowBounds" }
    $selection = $null
    if (-not $rows[0].TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
        throw "$Mode row has no SelectionItemPattern"
    }
    ([Windows.Automation.SelectionItemPattern]$selection).Select()
    Start-Sleep -Milliseconds 150
    $rows = Wait-FileRows $Root
    $selection = $null
    if (-not $rows[0].TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection) -or
        -not ([Windows.Automation.SelectionItemPattern]$selection).Current.IsSelected) {
        throw "$Mode row did not become selected"
    }
    $screenshot = Join-Path $OutputDirectory ($Mode + '.png')
    Save-WindowScreenshot $Root $screenshot
    [ordered]@{
        mode=$Mode
        first=Convert-Bounds $first
        second=Convert-Bounds $second
        third=Convert-Bounds $third
        selection_local=$true
        screenshot=$screenshot
    }
}

function Measure-AdaptiveGrid(
    [Windows.Automation.AutomationElement]$Root,
    [IntPtr]$Hwnd,
    [double]$DpiScale,
    [double]$LogicalWindowWidth,
    [string]$Label,
    [string]$OutputDirectory
) {
    $rootBounds = $Root.Current.BoundingRectangle
    $width = [int][Math]::Round($LogicalWindowWidth * $DpiScale)
    $height = [int][Math]::Min(
        [Math]::Round(700.0 * $DpiScale),
        [Windows.Forms.Screen]::FromHandle($Hwnd).WorkingArea.Height)
    if (-not [IconViewLayoutSmoke.Native]::SetWindowPos($Hwnd, [IntPtr]::Zero, 0, 0, $width, $height, 0x0006)) {
        throw "SetWindowPos failed for adaptive grid width $LogicalWindowWidth"
    }
    [void][IconViewLayoutSmoke.Native]::SetForegroundWindow($Hwnd)
    Start-Sleep -Milliseconds 500
    $rows = @(Wait-FileRows $Root)
    $firstTop = ($rows | Sort-Object { $_.Current.BoundingRectangle.Top } | Select-Object -First 1).Current.BoundingRectangle.Top
    $topTolerance = [Math]::Max(2.0, 2.0 * $DpiScale)
    $firstRow = @($rows | Where-Object {
        [Math]::Abs($_.Current.BoundingRectangle.Top - $firstTop) -le $topTolerance
    } | Sort-Object { $_.Current.BoundingRectangle.Left })
    if ($firstRow.Count -ne 5) {
        throw "$Label did not preserve five adaptive columns: count=$($firstRow.Count) width=$width rows=$($firstRow.Current.BoundingRectangle)"
    }

    $scrollbar = Wait-Element $Root 'File view vertical scroll bar' ([Windows.Automation.ControlType]::ScrollBar)
    $scrollbarBounds = $scrollbar.Current.BoundingRectangle
    if ($scrollbarBounds.Width -le 0) { throw "$Label did not expose the expected vertical scrollbar" }
    $rightmost = $firstRow[-1]
    $rightBounds = $rightmost.Current.BoundingRectangle
    $edgeTolerance = [Math]::Max(1.0, $DpiScale)
    if ($rightBounds.Right -gt $scrollbarBounds.Left + $edgeTolerance) {
        throw "$Label rightmost item entered scrollbar space: item=$rightBounds scrollbar=$scrollbarBounds"
    }

    $selection = $null
    if (-not $rightmost.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
        throw "$Label rightmost item has no SelectionItemPattern"
    }
    ([Windows.Automation.SelectionItemPattern]$selection).Select()
    Start-Sleep -Milliseconds 150
    $screenshot = Join-Path $OutputDirectory ("adaptive-grid-$Label.png")
    Save-WindowScreenshot $Root $screenshot
    [ordered]@{
        label=$Label
        logical_window_width=$LogicalWindowWidth
        physical_window_width=$width
        columns=$firstRow.Count
        cell_widths=@($firstRow | ForEach-Object { $_.Current.BoundingRectangle.Width })
        rightmost=Convert-Bounds $rightBounds
        scrollbar=Convert-Bounds $scrollbarBounds
        right_edge_clear=$true
        screenshot=$screenshot
    }
}

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $executable
$start.WorkingDirectory = $workspaceRoot
$start.UseShellExecute = $false
$start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
$start.Environment['EXPLORER_INITIAL_PATH'] = $fixture
$start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$process = [Diagnostics.Process]::Start($start)
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        $hwnd = $process.MainWindowHandle
        if ($hwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
    } while ($hwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($hwnd -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][IconViewLayoutSmoke.Native]::SetForegroundWindow($hwnd)
    $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $dpiScale = [IconViewLayoutSmoke.Native]::GetDpiForWindow($hwnd) / 96.0
    $smallName = -join ([char]0x5C0F, [char]0x5716, [char]0x793A)
    $mediumName = -join ([char]0x4E2D, [char]0x5716, [char]0x793A)
    $largeName = -join ([char]0x5927, [char]0x5716, [char]0x793A)
    $viewName = 'View'
    $measurements = @(
        Measure-Mode $root $viewName $smallName 'small-icons' $OutputDirectory
        Measure-Mode $root $viewName $mediumName 'medium-icons' $OutputDirectory
        Measure-Mode $root $viewName $largeName 'large-icons' $OutputDirectory
    )
    $adaptiveGridMeasurements = @(
        Measure-AdaptiveGrid $root $hwnd $dpiScale 1120.0 'wide' $OutputDirectory
        Measure-AdaptiveGrid $root $hwnd $dpiScale 1100.0 'narrow' $OutputDirectory
    )

    # Portrait, landscape, and square thumbnails must remain inside the icon host;
    # the independent filename region begins below that host, as in File Explorer.
    Click-Element (Wait-Element $root $viewName ([Windows.Automation.ControlType]::Button))
    Click-Element (Wait-Element $root $mediumName ([Windows.Automation.ControlType]::MenuItem))
    Start-Sleep -Milliseconds 750
    $thumbnailMeasurements = @(
        Measure-ThumbnailSeparation $root 'A.png' 120 480 $dpiScale
        Measure-ThumbnailSeparation $root 'B.png' 480 120 $dpiScale
        Measure-ThumbnailSeparation $root 'C.png' 240 240 $dpiScale
    )
    $thumbnailScreenshot = Join-Path $OutputDirectory 'thumbnail-aspect-layout.png'
    Save-WindowScreenshot $root $thumbnailScreenshot
    $longNameMeasurements = Measure-LongNameCells $root
    $longNameScreenshot = Join-Path $OutputDirectory 'medium-icon-long-name-layout.png'
    Save-WindowScreenshot $root $longNameScreenshot

    # Selecting Small Icons enters its middle 32 px notch. One downward notch
    # reaches 24, then upward Ctrl+wheel must expose every requested icon size.
    $view = Wait-Element $root $viewName ([Windows.Automation.ControlType]::Button)
    Click-Element $view
    Click-Element (Wait-Element $root $smallName ([Windows.Automation.ControlType]::MenuItem))
    Start-Sleep -Milliseconds 250
    Send-CtrlWheel $root -120
    $notchMeasurements = @()
    $nativeResolutionScreenshot = $null
    $expectedSizes = @(24, 32, 48, 64, 72, 84, 96, 108, 128, 256, 384, 512)
    for ($index = 0; $index -lt $expectedSizes.Count; $index++) {
        $iconBounds = Measure-FirstIcon $root
        $expectedPhysical = $expectedSizes[$index] * $dpiScale
        $tolerance = [Math]::Max(3.0, $expectedPhysical * 0.08)
        if ([Math]::Abs($iconBounds.Width - $expectedPhysical) -gt $tolerance -or
            [Math]::Abs($iconBounds.Height - $expectedPhysical) -gt $tolerance) {
            throw "Ctrl+wheel icon notch mismatch: logical=$($expectedSizes[$index]) expectedPhysical=$expectedPhysical actual=$iconBounds"
        }
        $notchMeasurements += [ordered]@{
            logical_size=$expectedSizes[$index]
            physical_bounds=Convert-Bounds $iconBounds
        }
        if ($expectedSizes[$index] -eq 128) {
            $nativeResolutionScreenshot = Join-Path $OutputDirectory 'shell-icons-native-resolution.png'
            Save-WindowScreenshot $root $nativeResolutionScreenshot
        }
        if ($index -lt $expectedSizes.Count - 1) { Send-CtrlWheel $root 120 }
    }
    if (-not $nativeResolutionScreenshot) { throw 'native-resolution Shell icon evidence was not captured' }

    # Explorer's downward sequence continues beyond Details to Tiles and Content.
    $detailsName = -join ([char]0x8A73, [char]0x7D30, [char]0x8CC7, [char]0x6599)
    Click-Element (Wait-Element $root $viewName ([Windows.Automation.ControlType]::Button))
    Click-Element (Wait-Element $root $detailsName ([Windows.Automation.ControlType]::MenuItem))
    Start-Sleep -Milliseconds 250
    Send-CtrlWheel $root -120
    $tilesBounds = (Wait-FileRows $root)[0].Current.BoundingRectangle
    Send-CtrlWheel $root -120
    $contentRows = Wait-FileRows $root
    $contentBounds = $contentRows[0].Current.BoundingRectangle
    $windowBounds = $root.Current.BoundingRectangle
    if ($tilesBounds.Width -ge $windowBounds.Width * 0.55) { throw "Details did not advance to Tiles: $tilesBounds" }
    if ($contentBounds.Width -lt $tilesBounds.Width * 2.0) {
        throw "Tiles did not advance to a full file-host Content row: tiles=$tilesBounds content=$contentBounds"
    }
    $rowHeightTolerance = [Math]::Max(2.0, $contentBounds.Height * 0.05)
    foreach ($row in @($contentRows | Select-Object -First 6)) {
        if ([Math]::Abs($row.Current.BoundingRectangle.Height - $contentBounds.Height) -gt $rowHeightTolerance) {
            throw "Content row vertical spacing is inconsistent: first=$contentBounds current=$($row.Current.BoundingRectangle)"
        }
    }
    $contentScreenshot = Join-Path $OutputDirectory 'content-grid.png'
    Save-WindowScreenshot $root $contentScreenshot
    [ordered]@{
        schema_version=1
        captured_utc=[DateTime]::UtcNow.ToString('o')
        fixture=$fixture
        measurements=$measurements
        adaptive_grid=$adaptiveGridMeasurements
        thumbnail_aspect_layout=[ordered]@{
            measurements=$thumbnailMeasurements
            screenshot=$thumbnailScreenshot
        }
        medium_icon_long_names=[ordered]@{
            measurements=$longNameMeasurements
            screenshot=$longNameScreenshot
        }
        ctrl_wheel_notches=$notchMeasurements
        native_resolution_shell_icons=[ordered]@{
            logical_size=128
            dpi_scale=$dpiScale
            minimum_required_source_pixels=[Math]::Ceiling(128 * $dpiScale)
            source_contract='Rust real-Shell test requires native payload dimensions to cover the rendered physical size'
            screenshot=$nativeResolutionScreenshot
        }
        downward_sequence=[ordered]@{
            tiles=Convert-Bounds $tilesBounds
            content=Convert-Bounds $contentBounds
            equal_content_row_heights=$true
            content_grid_screenshot=$contentScreenshot
        }
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    Write-Output "Icon-view headful smoke passed: $OutputDirectory"
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
