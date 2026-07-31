param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [string]$OutputDirectory = 'target\mouse-evidence\all-controls',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not [IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory = [IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory)) }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) { throw "artifact finalization failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"

if (-not ('ExplorerMouse.Native' -as [type])) {
    Add-Type -AssemblyName System.Drawing
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerMouse {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool IsIconic(IntPtr window);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool IsZoomed(IntPtr window);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool ShowWindow(IntPtr window, int command);
        [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
        [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    }
}
'@
}

$diagnostics = Join-Path $OutputDirectory 'diagnostics.json'
$startInfo = [Diagnostics.ProcessStartInfo]::new($executable)
$startInfo.WorkingDirectory = $workspaceRoot
$startInfo.UseShellExecute = $false
$startInfo.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$startInfo.Environment['EXPLORER_VISUAL_FIXTURE'] = '1'
$startInfo.Environment['EXPLORER_VISUAL_WIDTH'] = '1120'
$startInfo.Environment['EXPLORER_VISUAL_HEIGHT'] = '720'
$startInfo.Environment['EXPLORER_VISUAL_DPI'] = '175'
$startInfo.Environment['EXPLORER_VISUAL_THEME'] = 'light'
$startInfo.Environment['EXPLORER_VISUAL_FONT'] = 'Microsoft JhengHei UI'
$startInfo.Environment['EXPLORER_VISUAL_STATE'] = 'populated'
$startInfo.Environment['EXPLORER_VISUAL_DIAGNOSTICS'] = $diagnostics
$process = [Diagnostics.Process]::Start($startInfo)

function Point-LParam([int]$X, [int]$Y) { return [IntPtr](($Y -shl 16) -bor ($X -band 0xffff)) }
function Move-PhysicalMouse([IntPtr]$ClientPoint) {
    $packed = $ClientPoint.ToInt64()
    $x = [int]($packed -band 0xffff)
    $y = [int](($packed -shr 16) -band 0xffff)
    [void][ExplorerMouse.Native]::SetCursorPos($windowRect.Left + $x, $windowRect.Top + $y)
}
function Capture-Window([IntPtr]$Window, [string]$Path) {
    [void][ExplorerMouse.Native]::DwmFlush()
    $rect = [ExplorerMouse.Native+Rect]::new()
    if (-not [ExplorerMouse.Native]::GetWindowRect($Window, [ref]$rect)) { throw 'GetWindowRect failed' }
    $bitmap = [Drawing.Bitmap]::new($rect.Right-$rect.Left, $rect.Bottom-$rect.Top, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try { if (-not [ExplorerMouse.Native]::PrintWindow($Window, $dc, 2)) { throw 'PrintWindow failed' } }
            finally { $graphics.ReleaseHdc($dc) }
        } finally { $graphics.Dispose() }
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
}

function Capture-StableWindow([IntPtr]$Window, [string]$Path) {
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    $previous = $null
    do {
        Capture-Window $Window $Path
        $current = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash
        if ($current -eq $previous) { return $current }
        $previous = $current
        Start-Sleep -Milliseconds 120
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "window did not reach a stable rendered frame: $Path"
}

$controls = @(
    @{ name='close-tab'; id='active-tab-close'; x=172; y=32 }, @{ name='new-tab'; id='new-tab-button'; x=222; y=32 },
    @{ name='navigation-up'; id='navigation-up'; x=135; y=74 },
    @{ name='command-new'; id='command-new'; x=50; y=123 }, @{ name='command-cut'; id='command-cut'; x=125; y=123 }, @{ name='command-copy'; id='command-copy'; x=188; y=123 },
    @{ name='command-rename'; id='command-rename'; x=256; y=123 }, @{ name='command-delete'; id='command-delete'; x=345; y=123 }, @{ name='command-sort'; id='command-sort'; x=416; y=123 }, @{ name='command-view'; id='command-view'; x=496; y=123 },
    @{ name='command-menu'; id='command-more-menu'; x=560; y=123 }
)
$disabled = @(
    @{ name='navigation-back'; id='navigation-back'; x=40; y=74 }, @{ name='navigation-forward'; id='navigation-forward'; x=86; y=74 }
)

try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $process.Refresh(); $hwnd = $process.MainWindowHandle
        $ready = $hwnd -ne [IntPtr]::Zero -and (Test-Path $diagnostics) -and (Test-Path (Join-Path $OutputDirectory 'explorer.log'))
        if (-not $ready) { Start-Sleep -Milliseconds 50 }
    } while (-not $ready -and [DateTime]::UtcNow -lt $deadline)
    if (-not $ready) { throw 'timed out waiting for mouse fixture' }
    [void][ExplorerMouse.Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    [void][ExplorerMouse.Native]::SetForegroundWindow($hwnd)
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $scale = [double][ExplorerMouse.Native]::GetDpiForWindow($hwnd) / 96
    $automationRoot = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $windowRect = [ExplorerMouse.Native+Rect]::new()
    [void][ExplorerMouse.Native]::GetWindowRect($hwnd, [ref]$windowRect)
    # The populated fixture starts without a selection, so Cut/Copy/Delete are truthfully
    # disabled. Select the first real row before exercising those enabled button states.
    $listItemTypeCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem)
    $fileRowNameCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty,
        'Archive Folder')
    $fileRowCondition = [Windows.Automation.AndCondition]::new(
        $listItemTypeCondition,
        $fileRowNameCondition)
    $firstRow = $automationRoot.FindFirst([Windows.Automation.TreeScope]::Descendants, $fileRowCondition)
    if ($null -eq $firstRow) { throw 'populated fixture exposed no selectable file row' }
    $rowBounds = $firstRow.Current.BoundingRectangle
    $rowPoint = Point-LParam `
        ([int][Math]::Round($rowBounds.Left + $rowBounds.Width / 2 - $windowRect.Left)) `
        ([int][Math]::Round($rowBounds.Top + $rowBounds.Height / 2 - $windowRect.Top))
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x0201, [IntPtr]1, $rowPoint)
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x0202, [IntPtr]::Zero, $rowPoint)
    Start-Sleep -Milliseconds 150
    $diagnosticRegions = (Get-Content -Raw -Encoding utf8 $diagnostics | ConvertFrom-Json).region_diagnostics.regions
    $regionById = @{}
    foreach ($region in $diagnosticRegions) { $regionById[$region.id] = $region }
    function Resolve-ControlPoint($control, [double]$currentScale) {
        if ($control.id -and $regionById.ContainsKey($control.id)) {
            $rect = $regionById[$control.id].logical_rect
            return Point-LParam `
                ([int][Math]::Round(($rect.x + $rect.width / 2) * $currentScale)) `
                ([int][Math]::Round(($rect.y + $rect.height / 2) * $currentScale))
        }
        return Point-LParam ([int]($control.x*$currentScale)) ([int]($control.y*$currentScale))
    }
    $outside = Point-LParam ([int](700*$scale)) ([int](300*$scale))
    $results = @()
    foreach ($control in $controls) {
        $point = Resolve-ControlPoint $control $scale
        Move-PhysicalMouse $outside
        Move-PhysicalMouse $point
        Start-Sleep -Milliseconds 80
        $hoverPath = Join-Path $OutputDirectory ($control.name + '-hover.png')
        Capture-Window $hwnd $hoverPath
        [ExplorerMouse.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
        $pressedPath = Join-Path $OutputDirectory ($control.name + '-pressed.png')
        Capture-Window $hwnd $pressedPath
        [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x001F, [IntPtr]::Zero, [IntPtr]::Zero)
        Move-PhysicalMouse $outside
        [ExplorerMouse.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        $hoverHash = (Get-FileHash $hoverPath -Algorithm SHA256).Hash
        $pressedHash = (Get-FileHash $pressedPath -Algorithm SHA256).Hash
        if ($hoverHash -eq $pressedHash) { throw "$($control.name) did not expose distinct hover/pressed rendering" }
        $results += [ordered]@{ name=$control.name; hover_sha256=$hoverHash; pressed_sha256=$pressedHash }
    }
    foreach ($control in $disabled) {
        Move-PhysicalMouse $outside
        Start-Sleep -Milliseconds 80
        $normalPath = Join-Path $OutputDirectory ($control.name + '-normal.png')
        $normalHash = Capture-StableWindow $hwnd $normalPath
        $point = Resolve-ControlPoint $control $scale
        Move-PhysicalMouse $point
        Start-Sleep -Milliseconds 80
        $hoverPath = Join-Path $OutputDirectory ($control.name + '-hover.png')
        $hoverHash = Capture-StableWindow $hwnd $hoverPath
        if ($normalHash -ne $hoverHash) { throw "$($control.name) changed on disabled hover" }
    }

    $dividerStart = Point-LParam ([int](240*$scale)) ([int](300*$scale))
    $dividerEnd = Point-LParam ([int](280*$scale)) ([int](300*$scale))
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x0200, [IntPtr]::Zero, $dividerStart)
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x0201, [IntPtr]1, $dividerStart)
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x0200, [IntPtr]1, $dividerEnd)
    Start-Sleep -Milliseconds 100
    Capture-Window $hwnd (Join-Path $OutputDirectory 'divider-drag.png')
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x0202, [IntPtr]::Zero, $dividerEnd)

    [void][ExplorerMouse.Native]::GetWindowRect($hwnd, [ref]$windowRect)
    $captionResults = @()
    $captionDefinitions = @(
        @{ name='minimize'; accessible='Minimize'; expected=8 },
        @{ name='maximize'; accessible='Maximize or restore; Windows Snap Layout available'; expected=9 },
        @{ name='close'; accessible='Close'; expected=20 }
    )
    foreach ($caption in $captionDefinitions) {
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty, $caption.accessible)
        $element = $null
        $captionDeadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            $element = $automationRoot.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
            if ($null -eq $element) { Start-Sleep -Milliseconds 100 }
        } while ($null -eq $element -and [DateTime]::UtcNow -lt $captionDeadline)
        if ($null -eq $element) { throw "$($caption.name) is missing from UI Automation" }
        $bounds = $element.Current.BoundingRectangle
        $screenX = [int][Math]::Round($bounds.Left + $bounds.Width / 2)
        $screenY = [int][Math]::Round($bounds.Top + $bounds.Height / 2)
        $captionX = [int][Math]::Round(($screenX - $windowRect.Left) / $scale)
        $screenPoint = Point-LParam $screenX $screenY
        [void][ExplorerMouse.Native]::SetCursorPos($screenX, $screenY)
        $clientPoint = Point-LParam ($screenX - $windowRect.Left) ($screenY - $windowRect.Top)
        [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x0200, [IntPtr]::Zero, $clientPoint)
        Start-Sleep -Milliseconds 50
        $hit = [int][ExplorerMouse.Native]::SendMessage($hwnd, 0x0084, [IntPtr]::Zero, $screenPoint)
        if ($hit -ne $caption.expected) { throw "$($caption.name) returned HT code $hit instead of $($caption.expected); bounds=$bounds" }
        $insidePoints = @(
            @{ name='top-left'; x=$bounds.Left+2; y=$bounds.Top+2 },
            @{ name='top-right'; x=$bounds.Right-2; y=$bounds.Top+2 },
            @{ name='bottom-left'; x=$bounds.Left+2; y=$bounds.Bottom-2 },
            @{ name='bottom-right'; x=$bounds.Right-2; y=$bounds.Bottom-2 },
            @{ name='center'; x=$bounds.Left+$bounds.Width/2; y=$bounds.Top+$bounds.Height/2 },
            @{ name='top-center'; x=$bounds.Left+$bounds.Width/2; y=$bounds.Top+2 },
            @{ name='bottom-center'; x=$bounds.Left+$bounds.Width/2; y=$bounds.Bottom-2 },
            @{ name='left-center'; x=$bounds.Left+2; y=$bounds.Top+$bounds.Height/2 },
            @{ name='right-center'; x=$bounds.Right-2; y=$bounds.Top+$bounds.Height/2 }
        )
        $hitGrid = foreach ($point in $insidePoints) {
            $gridX = [int][Math]::Round($point.x); $gridY = [int][Math]::Round($point.y)
            $gridHit = [int][ExplorerMouse.Native]::SendMessage(
                $hwnd, 0x0084, [IntPtr]::Zero, (Point-LParam $gridX $gridY))
            if ($gridHit -ne $caption.expected) {
                throw "$($caption.name) $($point.name) returned HT code $gridHit instead of $($caption.expected); bounds=$bounds"
            }
            [ordered]@{ point=$point.name; x=$gridX; y=$gridY; hit_test=$gridHit }
        }
        $outsidePoints = @(
            @{ name='above'; x=$bounds.Left+$bounds.Width/2; y=$bounds.Top-2 },
            @{ name='below'; x=$bounds.Left+$bounds.Width/2; y=$bounds.Bottom+2 },
            @{ name='left-outside'; x=$bounds.Left-2; y=$bounds.Top+$bounds.Height/2 },
            @{ name='right-outside'; x=$bounds.Right+2; y=$bounds.Top+$bounds.Height/2 }
        )
        $outsideGrid = foreach ($point in $outsidePoints) {
            $gridX = [int][Math]::Round($point.x); $gridY = [int][Math]::Round($point.y)
            $gridHit = [int][ExplorerMouse.Native]::SendMessage(
                $hwnd, 0x0084, [IntPtr]::Zero, (Point-LParam $gridX $gridY))
            [ordered]@{ point=$point.name; x=$gridX; y=$gridY; hit_test=$gridHit; outside_current_button=($gridHit -ne $caption.expected) }
        }
        $captionResult = [ordered]@{
            name=$caption.name; hit_test=$hit; logical_center_x=$captionX
            width=[Math]::Round($bounds.Width, 2); height=[Math]::Round($bounds.Height, 2)
            inside_hit_grid=$hitGrid; outside_hit_grid=$outsideGrid
            uia_bounds=$bounds.ToString()
            diagnostic_bounds=$regionById["caption-$($caption.name)"].logical_rect
            bounds_contract='button element owns WindowControlArea, pointer styling, UIA role and diagnostics; glyph child is pointer-transparent visual content'
        }
        if ($caption.name -eq 'close') {
            $captionResults += $captionResult
            continue
        }
        [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x00A1, [IntPtr]$hit, $screenPoint)
        [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x00A2, [IntPtr]$hit, $screenPoint)
        Start-Sleep -Milliseconds 300
        if ($caption.name -eq 'minimize' -and -not [ExplorerMouse.Native]::IsIconic($hwnd)) { throw 'native minimize mouse flow did not minimize' }
        if ($caption.name -eq 'maximize') {
            if (-not [ExplorerMouse.Native]::IsZoomed($hwnd)) { throw 'native maximize mouse flow did not maximize' }
            $maximizedBounds = $element.Current.BoundingRectangle
            if ([Math]::Abs($maximizedBounds.Width - $bounds.Width) -gt 1 -or [Math]::Abs($maximizedBounds.Height - $bounds.Height) -gt 1) {
                throw "maximize changed the caption interaction box from $bounds to $maximizedBounds"
            }
            $captionResult.maximized_width = [Math]::Round($maximizedBounds.Width, 2)
            $captionResult.maximized_height = [Math]::Round($maximizedBounds.Height, 2)
            Capture-Window $hwnd (Join-Path $OutputDirectory 'caption-restore-glyph.png')
        }
        [void][ExplorerMouse.Native]::ShowWindow($hwnd, 9)
        Start-Sleep -Milliseconds 300
        [void][ExplorerMouse.Native]::GetWindowRect($hwnd, [ref]$windowRect)
        if ($caption.name -eq 'maximize') {
            $restoredBounds = $element.Current.BoundingRectangle
            if ([Math]::Abs($restoredBounds.Width - $bounds.Width) -gt 1 -or [Math]::Abs($restoredBounds.Height - $bounds.Height) -gt 1) {
                throw "restore changed the caption interaction box from $bounds to $restoredBounds"
            }
            $captionResult.restored_width = [Math]::Round($restoredBounds.Width, 2)
            $captionResult.restored_height = [Math]::Round($restoredBounds.Height, 2)
            Capture-Window $hwnd (Join-Path $OutputDirectory 'caption-maximize-glyph.png')
        }
        $captionResults += $captionResult
    }

    # Exercise the real non-client title-drag region rather than calling a model action.
    # A Windows title-bar double click must toggle maximization and preserve caption geometry.
    $dragRect = $regionById['window-drag-region'].logical_rect
    $dragHit = 0
    $dragScreenPoint = [IntPtr]::Zero
    foreach ($fraction in @(0.85, 0.70, 0.55, 0.40)) {
        $dragX = $windowRect.Left + [int][Math]::Round(($dragRect.x + $dragRect.width * $fraction) * $scale)
        $dragY = $windowRect.Top + [int][Math]::Round(($dragRect.y + $dragRect.height / 2) * $scale)
        $candidate = Point-LParam $dragX $dragY
        [void][ExplorerMouse.Native]::SetCursorPos($dragX, $dragY)
        Start-Sleep -Milliseconds 30
        $candidateHit = [int][ExplorerMouse.Native]::SendMessage($hwnd, 0x0084, [IntPtr]::Zero, $candidate)
        if ($candidateHit -eq 2) { $dragHit = $candidateHit; $dragScreenPoint = $candidate; break }
    }
    if ($dragHit -ne 2) { throw 'window drag region exposed no HTCAPTION point for title double-click' }
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x00A3, [IntPtr]2, $dragScreenPoint)
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x00A2, [IntPtr]2, $dragScreenPoint)
    Start-Sleep -Milliseconds 300
    if (-not [ExplorerMouse.Native]::IsZoomed($hwnd)) { throw 'title drag region double-click did not maximize' }
    [void][ExplorerMouse.Native]::ShowWindow($hwnd, 9)
    Start-Sleep -Milliseconds 300
    if ([ExplorerMouse.Native]::IsZoomed($hwnd)) { throw 'title drag region did not restore after maximize' }

    [ordered]@{
        schema_version=1; captured_utc=[DateTime]::UtcNow.ToString('o'); dpi=[int](96*$scale)
        enabled_control_count=$controls.Count; disabled_control_count=$disabled.Count
        controls=$results; divider='real WM_LBUTTONDOWN/MOVE/UP capture'; caption=$captionResults
        title_double_click='real HTCAPTION WM_NCLBUTTONDBLCLK maximize/restore passed'
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
    $close = $captionResults | Where-Object name -eq 'close'
    $closeX = $windowRect.Left + [int]($close.logical_center_x*$scale)
    $closeY = $windowRect.Top + [int](25*$scale)
    $closePoint = Point-LParam $closeX $closeY
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x00A1, [IntPtr]$close.hit_test, $closePoint)
    [void][ExplorerMouse.Native]::PostMessage($hwnd, 0x00A2, [IntPtr]$close.hit_test, $closePoint)
    if (-not $process.WaitForExit(10000) -or $process.ExitCode -ne 0) { throw 'mouse fixture did not exit cleanly' }
    Write-Output "Mouse control smoke passed: $OutputDirectory"
} catch {
    ($_ | Format-List * -Force | Out-String) | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'failure.txt')
    throw
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    $process.Dispose()
}
