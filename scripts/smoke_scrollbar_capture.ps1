param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('scrollbar-capture-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
} elseif (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "missing app: $executable" }

$fixtureRoot = [IO.Path]::GetFullPath((Join-Path $targetRoot ('scrollbar-capture-fixture-' + [guid]::NewGuid().ToString('N'))))
$shortFixtureRoot = [IO.Path]::GetFullPath((Join-Path $targetRoot ('scrollbar-capture-fixture-' + [guid]::NewGuid().ToString('N'))))
$emptyFixtureRoot = [IO.Path]::GetFullPath((Join-Path $targetRoot ('scrollbar-capture-fixture-' + [guid]::NewGuid().ToString('N'))))
$allowedFixturePrefix = [IO.Path]::GetFullPath($targetRoot).TrimEnd('\') + '\scrollbar-capture-fixture-'
$fixtureRoots = @($fixtureRoot, $shortFixtureRoot, $emptyFixtureRoot)
foreach ($fixturePath in $fixtureRoots) {
    if (-not $fixturePath.StartsWith($allowedFixturePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "unsafe fixture path: $fixturePath"
    }
    New-Item -ItemType Directory -Path $fixturePath | Out-Null
}
1..240 | ForEach-Object {
    [IO.File]::WriteAllText((Join-Path $fixtureRoot ('item-{0:D3}.txt' -f $_)), "fixture $_")
}
1..3 | ForEach-Object {
    [IO.File]::WriteAllText((Join-Path $shortFixtureRoot ('short-{0:D3}.txt' -f $_)), "short fixture $_")
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ('ExplorerScrollbarCapture.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerScrollbarCapture {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
        [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
        [StructLayout(LayoutKind.Sequential)] public struct GuiThreadInfo {
            public uint cbSize;
            public uint flags;
            public IntPtr hwndActive;
            public IntPtr hwndFocus;
            public IntPtr hwndCapture;
            public IntPtr hwndMenuOwner;
            public IntPtr hwndMoveSize;
            public IntPtr hwndCaret;
            public Rect rcCaret;
        }
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetWindowPos(IntPtr window, IntPtr insertAfter, int x, int y, int width, int height, uint flags);
        [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, IntPtr processId);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool GetGUIThreadInfo(uint threadId, ref GuiThreadInfo info);
    }
}
'@
}

function Get-AppCapture([IntPtr]$Window) {
    $threadId = [ExplorerScrollbarCapture.Native]::GetWindowThreadProcessId($Window, [IntPtr]::Zero)
    if ($threadId -eq 0) { throw 'GetWindowThreadProcessId failed' }
    $info = [ExplorerScrollbarCapture.Native+GuiThreadInfo]::new()
    $info.cbSize = [Runtime.InteropServices.Marshal]::SizeOf([type][ExplorerScrollbarCapture.Native+GuiThreadInfo])
    if (-not [ExplorerScrollbarCapture.Native]::GetGUIThreadInfo($threadId, [ref]$info)) {
        throw 'GetGUIThreadInfo failed'
    }
    return $info.hwndCapture
}

function Save-WindowEvidence([IntPtr]$Window, [string]$Path) {
    $rect = [ExplorerScrollbarCapture.Native+Rect]::new()
    if (-not [ExplorerScrollbarCapture.Native]::GetWindowRect($Window, [ref]$rect)) {
        throw 'GetWindowRect failed while capturing evidence'
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) { throw "invalid evidence bounds: ${width}x${height}" }
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Find-Scrollbar([Windows.Automation.AutomationElement]$Root, [string]$Name) {
    $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $Name)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $element = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -eq $element) { Start-Sleep -Milliseconds 100 }
    } while ($null -eq $element -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $element) { throw "missing scrollbar: $Name" }
    return $element
}

function Find-DetailsColumnSeparator([Windows.Automation.AutomationElement]$Root) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $element = $null
        $bestValue = [double]::NegativeInfinity
        $candidates = $Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Separator
            )
        )
        for ($index = 0; $index -lt $candidates.Count; $index++) {
            $candidate = $candidates.Item($index)
            $range = $null
            if ($candidate.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$range)) {
                $current = ([Windows.Automation.RangeValuePattern]$range).Current
                # Details columns have a distinct 48..1200 contract; the navigation splitter is 180..440.
                if ([Math]::Abs($current.Minimum - 48) -lt 0.01 -and [Math]::Abs($current.Maximum - 1200) -lt 0.01) {
                    if ($current.Value -gt $bestValue) {
                        $element = $candidate
                        $bestValue = $current.Value
                    }
                }
            }
        }
        if ($null -eq $element) { Start-Sleep -Milliseconds 100 }
    } while ($null -eq $element -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $element) {
        throw 'missing Details column separator with the 48..1200 RangeValue contract'
    }
    return $element
}

function Read-RangeValue([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        throw "scrollbar does not expose RangeValuePattern: $($Element.Current.Name)"
    }
    return ([Windows.Automation.RangeValuePattern]$pattern).Current.Value
}

function Read-RangeMaximum([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        throw "scrollbar does not expose RangeValuePattern: $($Element.Current.Name)"
    }
    return ([Windows.Automation.RangeValuePattern]$pattern).Current.Maximum
}

function Get-ScrollbarRatioExpectation(
    [double]$TrackPhysicalLength,
    [double]$Maximum,
    [double]$Before,
    [double]$PointerPhysicalDelta,
    [uint32]$Dpi
) {
    if ($Dpi -eq 0) { $Dpi = 96 }
    $scale = $Dpi / 96.0
    $viewportLogical = $TrackPhysicalLength / $scale
    $thumbLogical = [Math]::Min(
        $viewportLogical,
        [Math]::Max(32.0, $viewportLogical * $viewportLogical / ($viewportLogical + $Maximum))
    )
    $thumbTrackLogical = [Math]::Max(1.0, $viewportLogical - $thumbLogical)
    $pointerLogicalDelta = $PointerPhysicalDelta / $scale
    $expected = [Math]::Min(
        $Maximum,
        [Math]::Max(0.0, $Before + $pointerLogicalDelta / $thumbTrackLogical * $Maximum)
    )
    # Permit two physical pixels of cursor/UIA rounding expressed in RangeValue units.
    $tolerance = [Math]::Max(1.0, $Maximum / $thumbTrackLogical * (2.0 / $scale))
    return [pscustomobject]@{
        dpi = $Dpi
        scale = $scale
        viewport_logical = $viewportLogical
        thumb_logical = $thumbLogical
        thumb_track_logical = $thumbTrackLogical
        pointer_physical_delta = $PointerPhysicalDelta
        pointer_logical_delta = $pointerLogicalDelta
        expected = $expected
        tolerance = $tolerance
    }
}

function Assert-ScrollbarRatio([string]$Name, [double]$Observed, $Expectation) {
    $error = [Math]::Abs($Observed - $Expectation.expected)
    if ($error -gt $Expectation.tolerance) {
        throw "scrollbar drag ratio mismatch: $Name observed=$Observed expected=$($Expectation.expected) error=$error tolerance=$($Expectation.tolerance) dpi=$($Expectation.dpi) physicalDelta=$($Expectation.pointer_physical_delta) logicalDelta=$($Expectation.pointer_logical_delta)"
    }
    return $error
}

function Test-CapturedDrag(
    [Diagnostics.Process]$Process,
    [Windows.Automation.AutomationElement]$Root,
    [string]$Name,
    [ExplorerScrollbarCapture.Native+Rect]$WindowRect
) {
    $element = Find-Scrollbar $Root $Name
    $bounds = $element.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 80) { throw "invalid scrollbar bounds: $Name $bounds" }
    $startX = [int][Math]::Round($bounds.Left + $bounds.Width / 2)
    $startY = [int][Math]::Round($bounds.Top + 10)
    $middleX = [int][Math]::Round(($WindowRect.Left + $WindowRect.Right) / 2)
    $middleY = [int][Math]::Round($bounds.Top + $bounds.Height * 0.45)
    $outsideX = $WindowRect.Right + 80
    $before = Read-RangeValue $element
    $maximum = Read-RangeMaximum $element
    $dpi = [ExplorerScrollbarCapture.Native]::GetDpiForWindow($Process.MainWindowHandle)

    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($startX, $startY)
    $point = [ExplorerScrollbarCapture.Native+Point]::new()
    $point.X = $startX
    $point.Y = $startY
    $hitWindow = [ExplorerScrollbarCapture.Native]::WindowFromPoint($point)
    if ($hitWindow -ne $Process.MainWindowHandle) {
        throw "scrollbar coordinate is occluded: $Name point=$startX,$startY hit=$hitWindow hwnd=$($Process.MainWindowHandle)"
    }
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
    $captureAfterDown = Get-AppCapture $Process.MainWindowHandle
    if ($captureAfterDown -ne $Process.MainWindowHandle) {
        $afterDown = Read-RangeValue (Find-Scrollbar $Root $Name)
        throw "native capture not owned after thumb down: $Name bounds=$bounds start=$startX,$startY before=$before afterDown=$afterDown capture=$captureAfterDown hwnd=$($Process.MainWindowHandle)"
    }

    $ratioDelta = [int][Math]::Round([Math]::Min(96.0, $bounds.Height * 0.12))
    $ratioY = $startY + $ratioDelta
    $ratioExpectation = Get-ScrollbarRatioExpectation $bounds.Height $maximum $before $ratioDelta $dpi
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($middleX, $ratioY)
    Start-Sleep -Milliseconds 300
    $ratioObserved = Read-RangeValue (Find-Scrollbar $Root $Name)
    $ratioError = Assert-ScrollbarRatio $Name $ratioObserved $ratioExpectation

    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($middleX, $middleY)
    Start-Sleep -Milliseconds 300
    $inside = Read-RangeValue (Find-Scrollbar $Root $Name)
    if ($inside -le $before) { throw "offset did not advance after leaving scrollbar into content: $Name before=$before inside=$inside" }

    $outsideY = if ($inside -ge ($maximum - 0.01)) {
        [int][Math]::Round($bounds.Top - 100)
    } else {
        [int][Math]::Round($bounds.Bottom + 100)
    }
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($outsideX, $outsideY)
    Start-Sleep -Milliseconds 300
    $outside = Read-RangeValue (Find-Scrollbar $Root $Name)
    if ([Math]::Abs($outside - $inside) -le 0.01) { throw "offset did not change outside HWND: $Name inside=$inside outside=$outside maximum=$maximum" }
    if ((Get-AppCapture $Process.MainWindowHandle) -ne $Process.MainWindowHandle) {
        throw "capture was lost while pointer was outside HWND: $Name"
    }

    [ExplorerScrollbarCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    if ((Get-AppCapture $Process.MainWindowHandle) -eq $Process.MainWindowHandle) {
        throw "capture was not released after outside mouse-up: $Name"
    }
    $released = Read-RangeValue (Find-Scrollbar $Root $Name)
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($outsideX, $startY)
    Start-Sleep -Milliseconds 250
    $afterRelease = Read-RangeValue (Find-Scrollbar $Root $Name)
    if ([Math]::Abs($afterRelease - $released) -gt 0.01) {
        throw "offset changed after capture release: $Name released=$released after=$afterRelease"
    }

    return [ordered]@{
        name = $Name
        bounds = [ordered]@{ left=$bounds.Left; top=$bounds.Top; width=$bounds.Width; height=$bounds.Height }
        before = $before
        maximum = $maximum
        ratio = [ordered]@{
            dpi = $ratioExpectation.dpi
            scale = $ratioExpectation.scale
            pointer_physical_delta = $ratioExpectation.pointer_physical_delta
            pointer_logical_delta = $ratioExpectation.pointer_logical_delta
            viewport_logical = $ratioExpectation.viewport_logical
            thumb_logical = $ratioExpectation.thumb_logical
            thumb_track_logical = $ratioExpectation.thumb_track_logical
            expected = $ratioExpectation.expected
            observed = $ratioObserved
            error = $ratioError
            tolerance = $ratioExpectation.tolerance
        }
        content_area = $inside
        outside_hwnd = $outside
        released = $released
        after_release_move = $afterRelease
        capture_after_down = $captureAfterDown.ToInt64()
    }
}

function Test-CapturedColumnResize(
    [Diagnostics.Process]$Process,
    [Windows.Automation.AutomationElement]$Root,
    [ExplorerScrollbarCapture.Native+Rect]$WindowRect
) {
    $element = Find-DetailsColumnSeparator $Root
    $script:DetailsNameSeparator = $element
    $name = $element.Current.Name
    $bounds = $element.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 10) { throw "invalid column splitter bounds: $bounds" }
    $startX = [int][Math]::Round($bounds.Left + $bounds.Width / 2)
    $startY = [int][Math]::Round($bounds.Top + $bounds.Height / 2)
    $before = Read-RangeValue $element
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($startX, $startY)
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 200
    $captureAfterDown = Get-AppCapture $Process.MainWindowHandle
    if ($captureAfterDown -ne $Process.MainWindowHandle) {
        throw "column resize did not own native capture: capture=$captureAfterDown hwnd=$($Process.MainWindowHandle)"
    }
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos(($startX + 100), $startY)
    Start-Sleep -Milliseconds 250
    $inside = Read-RangeValue $element
    if ($inside -le $before) { throw "column width did not grow inside client: before=$before inside=$inside" }
    # Use the reachable left side. The test window may extend beyond the physical desktop on
    # multi-monitor CI, causing Windows to clamp a requested right-side point back inside.
    $outsideX = $WindowRect.Left - 100
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($outsideX, $startY)
    Start-Sleep -Milliseconds 250
    $outside = Read-RangeValue $element
    if ([Math]::Abs($outside - $inside) -le 0.01) {
        throw "column width did not continue changing outside HWND: inside=$inside outside=$outside"
    }
    if ((Get-AppCapture $Process.MainWindowHandle) -ne $Process.MainWindowHandle) {
        throw 'column capture was lost outside HWND'
    }
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
    $released = Read-RangeValue $element
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($startX, $startY)
    Start-Sleep -Milliseconds 200
    $afterRelease = Read-RangeValue $element
    if ([Math]::Abs($afterRelease - $released) -gt 0.01) {
        throw "column changed after release: released=$released after=$afterRelease"
    }
    $autoBounds = $element.Current.BoundingRectangle
    $autoX = [int][Math]::Round($autoBounds.Left + $autoBounds.Width / 2)
    $autoY = [int][Math]::Round($autoBounds.Top + $autoBounds.Height / 2)
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($autoX, $autoY)
    foreach ($click in 1..2) {
        [ExplorerScrollbarCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ExplorerScrollbarCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
    Start-Sleep -Milliseconds 250
    $autoSized = Read-RangeValue $element
    if ([Math]::Abs($autoSized - $released) -le 0.01) {
        throw "column double-click did not auto-size: released=$released autoSized=$autoSized"
    }
    return [ordered]@{
        name=$name
        before=$before
        content_area=$inside
        outside_hwnd=$outside
        released=$released
        after_release_move=$afterRelease
        auto_sized=$autoSized
        capture_after_down=$captureAfterDown.ToInt64()
    }
}

function Find-FirstListItem([Windows.Automation.AutomationElement]$Root) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem
    )
    $element = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $element) { throw 'missing first file ListItem' }
    return $element
}

function Test-HorizontalOverflow(
    [Diagnostics.Process]$Process,
    [Windows.Automation.AutomationElement]$Root,
    [ExplorerScrollbarCapture.Native+Rect]$WindowRect
) {
    $separator = Find-Scrollbar $Root $script:DetailsNameSeparator.Current.Name
    if ($null -eq $separator) { throw 'Name column separator was not captured by the resize test' }
    $script:DetailsNameSeparator = $separator
    $expanded = Read-RangeValue $separator

    $scrollbar = Find-Scrollbar $Root 'File view horizontal scroll bar'
    $bounds = $scrollbar.Current.BoundingRectangle
    $maximum = Read-RangeMaximum $scrollbar
    if ($bounds.Width -le 100 -or $bounds.Height -le 0 -or $maximum -le 0) {
        throw "invalid horizontal scrollbar bounds/range: bounds=$bounds maximum=$maximum"
    }
    $row = Find-FirstListItem $Root
    $headerLeftBefore = $separator.Current.BoundingRectangle.Left
    $rowLeftBefore = $row.Current.BoundingRectangle.Left
    $dragX = [int][Math]::Round($bounds.Left + 10)
    $dragY = [int][Math]::Round($bounds.Top + $bounds.Height / 2)
    $before = Read-RangeValue $scrollbar
    $dpi = [ExplorerScrollbarCapture.Native]::GetDpiForWindow($Process.MainWindowHandle)
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($dragX, $dragY)
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
    if ((Get-AppCapture $Process.MainWindowHandle) -ne $Process.MainWindowHandle) {
        throw 'horizontal scrollbar did not own native capture'
    }
    $ratioDelta = [int][Math]::Round([Math]::Min(120.0, $bounds.Width * 0.12))
    $ratioExpectation = Get-ScrollbarRatioExpectation $bounds.Width $maximum $before $ratioDelta $dpi
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos(($dragX + $ratioDelta), $dragY)
    Start-Sleep -Milliseconds 250
    $ratioObserved = Read-RangeValue $scrollbar
    $ratioError = Assert-ScrollbarRatio 'File view horizontal scroll bar' $ratioObserved $ratioExpectation

    [void][ExplorerScrollbarCapture.Native]::SetCursorPos(
        ([int][Math]::Round($bounds.Left + $bounds.Width * 0.65)),
        ([int][Math]::Round($bounds.Top - 180))
    )
    Start-Sleep -Milliseconds 250
    $inside = Read-RangeValue $scrollbar
    if ($inside -le 0) { throw "horizontal offset did not advance in content: $inside" }
    $headerLeftAfter = $separator.Current.BoundingRectangle.Left
    $rowLeftAfter = $row.Current.BoundingRectangle.Left
    $headerDelta = $headerLeftAfter - $headerLeftBefore
    $rowDelta = $rowLeftAfter - $rowLeftBefore
    # RangeValue is expressed in GPUI logical pixels, while UIA bounding rectangles are
    # physical screen pixels. Compare them after the same per-window DPI transform.
    # UIA also clips ListItem bounds to the viewport, so its delta may be shorter.
    if ($dpi -eq 0) { $dpi = 96 }
    $expectedPhysicalDelta = $inside * $dpi / 96.0
    if ([Math]::Abs([Math]::Abs($headerDelta) - $expectedPhysicalDelta) -gt 2) {
        throw "header did not follow horizontal offset: headerDelta=$headerDelta logicalOffset=$inside dpi=$dpi expectedPhysical=$expectedPhysicalDelta"
    }
    if ($rowDelta -gt 0 -or [Math]::Abs($rowDelta) -gt [Math]::Abs($headerDelta) + 2) {
        throw "clipped row moved inconsistently: headerDelta=$headerDelta rowDelta=$rowDelta"
    }
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos(($WindowRect.Left - 80), $dragY)
    Start-Sleep -Milliseconds 200
    $outside = Read-RangeValue $scrollbar
    if ([Math]::Abs($outside - $inside) -le 0.01) {
        throw "horizontal offset did not change outside HWND: inside=$inside outside=$outside"
    }
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 200
    $released = Read-RangeValue $scrollbar
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($dragX, $dragY)
    Start-Sleep -Milliseconds 150
    $afterRelease = Read-RangeValue $scrollbar
    if ([Math]::Abs($released - $afterRelease) -gt 0.01) {
        throw "horizontal offset changed after release: released=$released after=$afterRelease"
    }
    # Put the horizontal view at a non-zero offset, then remove overflow by widening the window.
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos($dragX, $dragY)
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    [void][ExplorerScrollbarCapture.Native]::SetCursorPos(
        ([int][Math]::Round($bounds.Right - 30)),
        $dragY
    )
    Start-Sleep -Milliseconds 150
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
    $beforeResize = Read-RangeValue $scrollbar
    if ($beforeResize -le 0) { throw 'failed to establish horizontal offset before resize clamp' }
    if (-not [ExplorerScrollbarCapture.Native]::SetWindowPos(
        $Process.MainWindowHandle, [IntPtr](-1), 20, 20, 3000, 900, 0x0040
    )) { throw 'failed to widen app for horizontal clamp' }
    Start-Sleep -Milliseconds 350
    $resizedScrollbar = Find-Scrollbar $Root 'File view horizontal scroll bar'
    $afterResizeMaximum = Read-RangeMaximum $resizedScrollbar
    $afterResize = Read-RangeValue $resizedScrollbar
    if ($afterResizeMaximum -gt 0.01 -or [Math]::Abs($afterResize) -gt 0.01) {
        throw "horizontal offset was not clamped after overflow disappeared: maximum=$afterResizeMaximum value=$afterResize"
    }
    if (-not [ExplorerScrollbarCapture.Native]::SetWindowPos(
        $Process.MainWindowHandle, [IntPtr](-1), 20, 20, 800, 900, 0x0040
    )) { throw 'failed to restore app after horizontal clamp' }
    Start-Sleep -Milliseconds 300
    return [ordered]@{
        expanded_column=$expanded
        ratio=[ordered]@{
            dpi=$ratioExpectation.dpi
            scale=$ratioExpectation.scale
            pointer_physical_delta=$ratioExpectation.pointer_physical_delta
            pointer_logical_delta=$ratioExpectation.pointer_logical_delta
            viewport_logical=$ratioExpectation.viewport_logical
            thumb_logical=$ratioExpectation.thumb_logical
            thumb_track_logical=$ratioExpectation.thumb_track_logical
            expected=$ratioExpectation.expected
            observed=$ratioObserved
            error=$ratioError
            tolerance=$ratioExpectation.tolerance
        }
        maximum=$maximum
        content_area=$inside
        outside_hwnd=$outside
        released=$released
        after_release_move=$afterRelease
        header_delta=$headerDelta
        row_delta=$rowDelta
        dpi=$dpi
        before_resize=$beforeResize
        after_resize_maximum=$afterResizeMaximum
        after_resize=$afterResize
    }
}

function Test-HiddenFileScrollbar([string]$FixturePath, [string]$CaseName) {
    $hiddenProcess = $null
    try {
        $hiddenStart = [Diagnostics.ProcessStartInfo]::new($executable)
        $hiddenStart.WorkingDirectory = $workspaceRoot
        $hiddenStart.UseShellExecute = $false
        $hiddenStart.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory ("localappdata-" + $CaseName))
        $hiddenStart.Environment['EXPLORER_INITIAL_PATH'] = $FixturePath
        $hiddenStart.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
        $hiddenProcess = [Diagnostics.Process]::Start($hiddenStart)
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            $hiddenProcess.Refresh()
            if ($hiddenProcess.HasExited) { throw "$CaseName app exited early: $($hiddenProcess.ExitCode)" }
            $ready = $hiddenProcess.MainWindowHandle -ne [IntPtr]::Zero
            if (-not $ready) { Start-Sleep -Milliseconds 50 }
        } while (-not $ready -and [DateTime]::UtcNow -lt $deadline)
        if (-not $ready) { throw "$CaseName timed out waiting for app HWND" }
        Start-Sleep -Milliseconds 900
        $hiddenRoot = [Windows.Automation.AutomationElement]::FromHandle($hiddenProcess.MainWindowHandle)
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty,
            'File view vertical scroll bar'
        )
        $element = $hiddenRoot.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        $maximum = 0.0
        $visible = $false
        if ($null -ne $element) {
            $maximum = Read-RangeMaximum $element
            $bounds = $element.Current.BoundingRectangle
            $visible = -not $element.Current.IsOffscreen -and $bounds.Width -gt 0 -and $bounds.Height -gt 0
        }
        if ($maximum -gt 0.01 -or $visible) {
            throw "$CaseName unexpectedly exposes a visible file scrollbar: maximum=$maximum visible=$visible"
        }
        $hiddenProcess.CloseMainWindow() | Out-Null
        if (-not $hiddenProcess.WaitForExit(10000)) { throw "$CaseName app did not close" }
        return [ordered]@{ name=$CaseName; item_count=(Get-ChildItem -LiteralPath $FixturePath -File).Count; maximum=$maximum; visible=$visible }
    } finally {
        if ($null -ne $hiddenProcess) {
            if (-not $hiddenProcess.HasExited) { $hiddenProcess.Kill(); $hiddenProcess.WaitForExit() }
            $hiddenProcess.Dispose()
        }
    }
}

$process = $null
try {
    $startInfo = [Diagnostics.ProcessStartInfo]::new($executable)
    $startInfo.WorkingDirectory = $workspaceRoot
    $startInfo.UseShellExecute = $false
    $startInfo.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata-main')
    $startInfo.Environment['EXPLORER_INITIAL_PATH'] = $fixtureRoot
    $startInfo.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    $process = [Diagnostics.Process]::Start($startInfo)
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "app exited early: $($process.ExitCode)" }
        $ready = $process.MainWindowHandle -ne [IntPtr]::Zero
        if (-not $ready) { Start-Sleep -Milliseconds 50 }
    } while (-not $ready -and [DateTime]::UtcNow -lt $deadline)
    if (-not $ready) { throw 'timed out waiting for app HWND' }
    Start-Sleep -Milliseconds 1200
    if (-not [ExplorerScrollbarCapture.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 20, 20, 800, 900, 0x0040)) {
        throw 'SetWindowPos(HWND_TOPMOST) failed'
    }
    [void][ExplorerScrollbarCapture.Native]::SetForegroundWindow($process.MainWindowHandle)
    # Normalize synthetic input after any previously interrupted smoke run.
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    $windowRect = [ExplorerScrollbarCapture.Native+Rect]::new()
    if (-not [ExplorerScrollbarCapture.Native]::GetWindowRect($process.MainWindowHandle, [ref]$windowRect)) {
        throw 'GetWindowRect failed'
    }
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    # The narrow window makes the default Details columns overflow without changing user data.
    $script:DetailsNameSeparator = Find-DetailsColumnSeparator $root
    $horizontal = Test-HorizontalOverflow $process $root $windowRect
    # Column headers share the file-view accessibility surface. Exercise them before
    # vertical scrolling so their physical bounds remain directly hit-testable.
    $columnResize = Test-CapturedColumnResize $process $root $windowRect
    $headerTopBeforeVerticalScroll = $script:DetailsNameSeparator.Current.BoundingRectangle.Top
    $results = @(
        Test-CapturedDrag $process $root 'File view vertical scroll bar' $windowRect
        Test-CapturedDrag $process $root 'Navigation pane vertical scroll bar' $windowRect
    )
    $headerTopAfterVerticalScroll = $script:DetailsNameSeparator.Current.BoundingRectangle.Top
    if ([Math]::Abs($headerTopAfterVerticalScroll - $headerTopBeforeVerticalScroll) -gt 2) {
        throw "Details header moved vertically: before=$headerTopBeforeVerticalScroll after=$headerTopAfterVerticalScroll"
    }
    Save-WindowEvidence $process.MainWindowHandle (Join-Path $OutputDirectory 'scrollbars-final.png')
    $process.CloseMainWindow() | Out-Null
    if (-not $process.WaitForExit(10000)) { throw 'app did not close after scrollbar drag smoke' }
    $hiddenResults = @(
        Test-HiddenFileScrollbar $shortFixtureRoot 'short-folder'
        Test-HiddenFileScrollbar $emptyFixtureRoot 'empty-folder'
    )
    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        fixture_item_count = 240
        results = $results
        column_resize = $columnResize
        horizontal_overflow = $horizontal
        fixed_header = [ordered]@{ before=$headerTopBeforeVerticalScroll; after=$headerTopAfterVerticalScroll }
        hidden_results = $hiddenResults
    } | ConvertTo-Json -Depth 7 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Scrollbar capture smoke passed: $OutputDirectory"
} catch {
    ($_ | Format-List * -Force | Out-String) | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'failure.txt')
    throw
} finally {
    # Never leave the real desktop's left button logically pressed after an assertion failure.
    [ExplorerScrollbarCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    if ($null -ne $process) {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
    }
    foreach ($fixturePath in $fixtureRoots) {
        if (Test-Path -LiteralPath $fixturePath) {
            $resolvedFixture = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $fixturePath).Path)
            if (-not $resolvedFixture.StartsWith($allowedFixturePrefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw "refusing unsafe fixture cleanup: $resolvedFixture"
            }
            Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
        }
    }
}
