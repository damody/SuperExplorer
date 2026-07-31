param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [string]$InitialPath = 'D:\test',
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('command-menu-anchor-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ('CommandMenuAnchorSmoke.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace CommandMenuAnchorSmoke {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);
        public static bool SetCursorPosDpiAware(int x, int y) {
            IntPtr previous = SetThreadDpiAwarenessContext(new IntPtr(-4));
            try { return SetCursorPos(x, y); }
            finally { if (previous != IntPtr.Zero) SetThreadDpiAwarenessContext(previous); }
        }
    }
}
'@
}

function Wait-Match(
    [Windows.Automation.AutomationElement]$Root,
    [Windows.Automation.ControlType]$Type,
    [string]$Name,
    [int]$TimeoutSeconds = 10
) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $condition = [Windows.Automation.AndCondition]::new(
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty, $Type),
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::NameProperty, $Name))
        $element = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $element) { return $element }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $($Type.ProgrammaticName) $Name"
}

function Invoke-Element([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        throw "element does not expose InvokePattern: $($Element.Current.Name)"
    }
    ([Windows.Automation.InvokePattern]$pattern).Invoke()
}

function Send-Escape([IntPtr]$WindowHandle) {
    [void][CommandMenuAnchorSmoke.Native]::PostMessage($WindowHandle, 0x0100, [IntPtr]0x1B, [IntPtr]::Zero)
    [void][CommandMenuAnchorSmoke.Native]::PostMessage($WindowHandle, 0x0101, [IntPtr]0x1B, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 180
}

function Click-Element([Windows.Automation.AutomationElement]$Element) {
    $bounds = $Element.Current.BoundingRectangle
    $x = [int][Math]::Round($bounds.Left + $bounds.Width / 2.0)
    $y = [int][Math]::Round($bounds.Top + $bounds.Height / 2.0)
    [void][CommandMenuAnchorSmoke.Native]::SetCursorPosDpiAware($x, $y)
    [CommandMenuAnchorSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [CommandMenuAnchorSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Convert-Bounds([Windows.Rect]$Bounds) {
    return [ordered]@{
        left = $Bounds.Left; top = $Bounds.Top
        right = $Bounds.Right; bottom = $Bounds.Bottom
        width = $Bounds.Width; height = $Bounds.Height
    }
}

function Wait-FirstVisibleMenuItem(
    [Windows.Automation.AutomationElement]$Root,
    [int]$TimeoutSeconds = 10
) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::MenuItem)
    do {
        $items = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        foreach ($item in $items) {
            $bounds = $item.Current.BoundingRectangle
            if (-not $item.Current.IsOffscreen -and $bounds.Width -gt 0 -and $bounds.Height -gt 0) {
                return $item
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'visible menu item not found'
}

function Capture-WindowAndPixel(
    [Windows.Rect]$WindowBounds,
    [Windows.Rect]$ItemBounds,
    [string]$Path
) {
    $width = [Math]::Max(1, [int][Math]::Round($WindowBounds.Width))
    $height = [Math]::Max(1, [int][Math]::Round($WindowBounds.Height))
    $previousDpi = [CommandMenuAnchorSmoke.Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen(
            [int][Math]::Round($WindowBounds.Left),
            [int][Math]::Round($WindowBounds.Top),
            0,
            0,
            $bitmap.Size)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
        $sampleX = [int][Math]::Round($ItemBounds.Right - $WindowBounds.Left - 14)
        $sampleY = [int][Math]::Round($ItemBounds.Top - $WindowBounds.Top + ($ItemBounds.Height / 2.0))
        $sampleX = [Math]::Max(0, [Math]::Min($width - 1, $sampleX))
        $sampleY = [Math]::Max(0, [Math]::Min($height - 1, $sampleY))
        $color = $bitmap.GetPixel($sampleX, $sampleY)
        return [ordered]@{ r = $color.R; g = $color.G; b = $color.B; hex = ('#{0:X2}{1:X2}{2:X2}' -f $color.R, $color.G, $color.B) }
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
        if ($previousDpi -ne [IntPtr]::Zero) {
            [void][CommandMenuAnchorSmoke.Native]::SetThreadDpiAwarenessContext($previousDpi)
        }
    }
}

function Get-ColorDistance($Left, $Right) {
    $red = [double]$Left.r - [double]$Right.r
    $green = [double]$Left.g - [double]$Right.g
    $blue = [double]$Left.b - [double]$Right.b
    return [Math]::Sqrt(($red * $red) + ($green * $green) + ($blue * $blue))
}

function Move-ToMenuItem([Windows.Automation.AutomationElement]$Item) {
    $bounds = $Item.Current.BoundingRectangle
    $x = [int][Math]::Round($bounds.Right - 14)
    $y = [int][Math]::Round($bounds.Top + ($bounds.Height / 2.0))
    [void][CommandMenuAnchorSmoke.Native]::SetCursorPosDpiAware($x, $y)
    Start-Sleep -Milliseconds 250
}

function Assert-HoverFollowsPointer(
    [Windows.Automation.AutomationElement]$Window,
    [Windows.Automation.AutomationElement]$FirstItem,
    [Windows.Automation.AutomationElement]$SecondItem,
    [string]$MenuName,
    [string]$FilePrefix
) {
    $windowBounds = $Window.Current.BoundingRectangle
    $firstBounds = $FirstItem.Current.BoundingRectangle
    $secondBounds = $SecondItem.Current.BoundingRectangle
    Move-ToMenuItem $FirstItem
    $firstHover = Capture-WindowAndPixel $windowBounds $firstBounds (Join-Path $OutputDirectory "$FilePrefix-hover-first.png")
    $secondIdle = Capture-WindowAndPixel $windowBounds $secondBounds (Join-Path $OutputDirectory "$FilePrefix-hover-first-idle-sample.png")
    Move-ToMenuItem $SecondItem
    $firstIdle = Capture-WindowAndPixel $windowBounds $firstBounds (Join-Path $OutputDirectory "$FilePrefix-hover-second-idle-sample.png")
    $secondHover = Capture-WindowAndPixel $windowBounds $secondBounds (Join-Path $OutputDirectory "$FilePrefix-hover-second.png")

    $hoverMatch = Get-ColorDistance $firstHover $secondHover
    $idleMatch = Get-ColorDistance $secondIdle $firstIdle
    $firstContrast = Get-ColorDistance $firstHover $firstIdle
    $secondContrast = Get-ColorDistance $secondHover $secondIdle
    if ($hoverMatch -gt 4.0 -or $idleMatch -gt 4.0 -or $firstContrast -lt 5.0 -or $secondContrast -lt 5.0) {
        throw "$MenuName hover did not follow the pointer: hoverMatch=$hoverMatch idleMatch=$idleMatch firstContrast=$firstContrast secondContrast=$secondContrast"
    }
    return [ordered]@{
        first_hover = $firstHover; first_idle = $firstIdle
        second_hover = $secondHover; second_idle = $secondIdle
        hover_color_distance = $hoverMatch; idle_color_distance = $idleMatch
        first_contrast = $firstContrast; second_contrast = $secondContrast
    }
}

function Assert-SingleItemHover(
    [Windows.Automation.AutomationElement]$Window,
    [Windows.Automation.AutomationElement]$Item,
    [string]$MenuName,
    [string]$FilePrefix
) {
    $windowBounds = $Window.Current.BoundingRectangle
    $itemBounds = $Item.Current.BoundingRectangle
    $idle = Capture-WindowAndPixel $windowBounds $itemBounds (Join-Path $OutputDirectory "$FilePrefix-idle.png")
    Move-ToMenuItem $Item
    $hover = Capture-WindowAndPixel $windowBounds $itemBounds (Join-Path $OutputDirectory "$FilePrefix-hover.png")
    $contrast = Get-ColorDistance $hover $idle
    if ($contrast -lt 5.0) { throw "$MenuName item did not gain a gray hover fill: contrast=$contrast" }
    return [ordered]@{ idle = $idle; hover = $hover; contrast = $contrast }
}

function Get-MenuAncestor([Windows.Automation.AutomationElement]$Item) {
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
    $current = $Item
    while ($null -ne $current -and $current.Current.ControlType -ne [Windows.Automation.ControlType]::Menu) {
        $current = $walker.GetParent($current)
    }
    if ($null -eq $current) { throw 'menu item does not expose a Menu ancestor' }
    return $current
}

function Assert-CommandMenuAnchor(
    [Windows.Automation.AutomationElement]$Window,
    [Windows.Automation.AutomationElement]$Button,
    [Windows.Automation.AutomationElement]$FirstItem,
    [string]$MenuName
) {
    $windowBounds = $Window.Current.BoundingRectangle
    $buttonBounds = $Button.Current.BoundingRectangle
    $itemBounds = $FirstItem.Current.BoundingRectangle
    $popupBounds = (Get-MenuAncestor $FirstItem).Current.BoundingRectangle
    $tolerance = [Math]::Max(4.0, $buttonBounds.Height * 0.20)

    if ([Math]::Abs($popupBounds.Top - $buttonBounds.Bottom) -gt $tolerance) {
        throw "$MenuName popup top is not anchored to button bottom: button=$buttonBounds popup=$popupBounds"
    }
    if ([Math]::Abs($popupBounds.Right - $buttonBounds.Right) -gt $tolerance) {
        throw "$MenuName popup right edge is not anchored to button right edge: button=$buttonBounds popup=$popupBounds"
    }
    if ($popupBounds.Left -gt $buttonBounds.Right -or $popupBounds.Right -lt $buttonBounds.Left) {
        throw "$MenuName popup does not horizontally overlap its button: button=$buttonBounds popup=$popupBounds"
    }
    if ($popupBounds.Left -lt $windowBounds.Left - $tolerance -or
        $popupBounds.Right -gt $windowBounds.Right + $tolerance -or
        $popupBounds.Top -lt $windowBounds.Top - $tolerance -or
        $popupBounds.Bottom -gt $windowBounds.Bottom + $tolerance) {
        throw "$MenuName popup is outside the window: window=$windowBounds popup=$popupBounds"
    }
    if ($popupBounds.Left -le $windowBounds.Left + 2.0 -and $popupBounds.Top -le $windowBounds.Top + 2.0) {
        throw "$MenuName popup regressed to the window origin: window=$windowBounds popup=$popupBounds"
    }
    if ($itemBounds.Left -lt $popupBounds.Left - $tolerance -or
        $itemBounds.Right -gt $popupBounds.Right + $tolerance -or
        $itemBounds.Top -lt $popupBounds.Top - $tolerance -or
        $itemBounds.Bottom -gt $popupBounds.Bottom + $tolerance) {
        throw "$MenuName first item is outside its popup: popup=$popupBounds item=$itemBounds"
    }

    return [ordered]@{
        menu = $MenuName
        window = Convert-Bounds $windowBounds
        button = Convert-Bounds $buttonBounds
        popup = Convert-Bounds $popupBounds
        first_item = Convert-Bounds $itemBounds
        top_delta_from_button_bottom = $popupBounds.Top - $buttonBounds.Bottom
        right_delta_from_button_right = $popupBounds.Right - $buttonBounds.Right
        horizontally_overlaps = $true
        inside_window = $true
        origin_regression = $false
    }
}

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $executable
$start.WorkingDirectory = $workspaceRoot
$start.UseShellExecute = $false
$start.Environment['EXPLORER_INITIAL_PATH'] = (Resolve-Path -LiteralPath $InitialPath).Path
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

    $previousDpi = [CommandMenuAnchorSmoke.Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    [void][CommandMenuAnchorSmoke.Native]::SetWindowPos($hwnd, [IntPtr](-1), 20, 20, 1440, 1040, 0x0040)
    if ($previousDpi -ne [IntPtr]::Zero) {
        [void][CommandMenuAnchorSmoke.Native]::SetThreadDpiAwarenessContext($previousDpi)
    }
    [void][CommandMenuAnchorSmoke.Native]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 300
    $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $nameLabel = -join ([char]0x540D, [char]0x7A31)
    $dateModifiedLabel = -join ([char]0x4FEE, [char]0x6539, [char]0x65E5, [char]0x671F)
    $extraLargeIconsLabel = -join ([char]0x8D85, [char]0x5927, [char]0x5716, [char]0x793A)
    $largeIconsLabel = -join ([char]0x5927, [char]0x5716, [char]0x793A)

    $sortButton = Wait-Match $root ([Windows.Automation.ControlType]::Button) 'Sort'
    Invoke-Element $sortButton
    $sortFirstItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $nameLabel
    $sortEvidence = Assert-CommandMenuAnchor $root $sortButton $sortFirstItem 'Sort'
    $sortSecondItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $dateModifiedLabel
    $sortHoverEvidence = Assert-HoverFollowsPointer $root $sortFirstItem $sortSecondItem 'Sort' 'sort'
    Send-Escape $hwnd

    $viewButton = Wait-Match $root ([Windows.Automation.ControlType]::Button) 'View'
    Invoke-Element $viewButton
    $viewFirstItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $extraLargeIconsLabel
    $viewEvidence = Assert-CommandMenuAnchor $root $viewButton $viewFirstItem 'View'
    $viewSecondItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $largeIconsLabel
    $viewHoverEvidence = Assert-HoverFollowsPointer $root $viewFirstItem $viewSecondItem 'View' 'view'
    Send-Escape $hwnd

    $undoLabel = -join ([char]0x5FA9, [char]0x539F)
    $selectAllLabel = -join ([char]0x5168, [char]0x9078)
    $invertSelectionLabel = -join ([char]0x53CD, [char]0x5411, [char]0x9078, [char]0x64C7)
    $optionsLabel = -join ([char]0x9078, [char]0x9805)
    $generalLabel = -join ([char]0x4E00, [char]0x822C)
    $viewTabLabel = -join ([char]0x6AA2, [char]0x8996)
    $otherLabel = -join ([char]0x5176, [char]0x5B83)
    $moreButton = Wait-Match $root ([Windows.Automation.ControlType]::Button) $otherLabel
    Invoke-Element $moreButton
    $moreFirstItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $undoLabel
    $moreEvidence = Assert-CommandMenuAnchor $root $moreButton $moreFirstItem 'More'
    $selectAllItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $selectAllLabel
    $invertSelectionItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $invertSelectionLabel
    $moreHoverEvidence = Assert-HoverFollowsPointer $root $selectAllItem $invertSelectionItem 'More' 'more'
    Send-Escape $hwnd

    $extensionsLabel = -join ([char]0x64F4, [char]0x5145, [char]0x529F, [char]0x80FD)
    $extensionsButton = Wait-Match $root ([Windows.Automation.ControlType]::Button) $extensionsLabel
    Invoke-Element $extensionsButton
    $extensionsItem = Wait-FirstVisibleMenuItem $root
    $extensionsEvidence = Assert-CommandMenuAnchor $root $extensionsButton $extensionsItem 'Extensions'
    $extensionsHoverEvidence = Assert-SingleItemHover $root $extensionsItem 'Extensions' 'extensions'
    Send-Escape $hwnd

    $moreButton = Wait-Match $root ([Windows.Automation.ControlType]::Button) $otherLabel
    Invoke-Element $moreButton
    $optionsItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $optionsLabel
    Click-Element $optionsItem
    $generalTab = Wait-Match $root ([Windows.Automation.ControlType]::TabItem) $generalLabel
    $viewTab = Wait-Match $root ([Windows.Automation.ControlType]::TabItem) $viewTabLabel
    $searchCondition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::TabItem),
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty,
            (-join ([char]0x641C, [char]0x5C0B))))
    if ($null -ne $root.FindFirst([Windows.Automation.TreeScope]::Descendants, $searchCondition)) {
        throw 'Folder Options unexpectedly exposes the excluded Search tab'
    }

    Send-Escape $hwnd

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        initial_path = (Resolve-Path -LiteralPath $InitialPath).Path
        sort = $sortEvidence
        sort_hover = $sortHoverEvidence
        view = $viewEvidence
        view_hover = $viewHoverEvidence
        more = $moreEvidence
        more_hover = $moreHoverEvidence
        extensions = $extensionsEvidence
        extensions_hover = $extensionsHoverEvidence
        folder_options = [ordered]@{
            general_tab = Convert-Bounds $generalTab.Current.BoundingRectangle
            view_tab = Convert-Bounds $viewTab.Current.BoundingRectangle
            search_tab_present = $false
        }
        exit_code = 0
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
} finally {
    if (-not $process.HasExited) {
        [void][CommandMenuAnchorSmoke.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
}
Write-Output "Command menu anchor smoke passed: $OutputDirectory"
