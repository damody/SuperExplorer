param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [string]$InitialPath = '',
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('sort-column-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixtureRoot = $null
if ([string]::IsNullOrWhiteSpace($InitialPath)) {
    $fixtureRoot = [IO.Path]::GetFullPath((Join-Path $targetRoot ('column-fixture-' + [guid]::NewGuid().ToString('N'))))
    $allowedFixturePrefix = [IO.Path]::GetFullPath($targetRoot).TrimEnd('\') + '\column-fixture-'
    if (-not $fixtureRoot.StartsWith($allowedFixturePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "unsafe sort fixture path: $fixtureRoot"
    }
    New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
    1..12 | ForEach-Object {
        [IO.File]::WriteAllText((Join-Path $fixtureRoot ('item-{0:D2}.txt' -f $_)), ('fixture-' + $_))
    }
    New-Item -ItemType Directory -Path (Join-Path $fixtureRoot 'folder-a') | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $fixtureRoot 'folder-z') | Out-Null
    $InitialPath = $fixtureRoot
}
if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ('SortColumnSmoke.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace SortColumnSmoke {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
    }
}
'@
}

function Get-All([Windows.Automation.AutomationElement]$Root) {
    return $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
}
function Wait-Match(
    [Windows.Automation.AutomationElement]$Root,
    [Windows.Automation.ControlType]$Type,
    [string]$NamePrefix,
    [int]$TimeoutSeconds = 10
) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        foreach ($element in (Get-All $Root)) {
            if ($element.Current.ControlType -eq $Type -and $element.Current.Name -like "*$NamePrefix*") { return $element }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $($Type.ProgrammaticName) $NamePrefix"
}
function Get-Rows([Windows.Automation.AutomationElement]$Root) {
    $rows = @()
    foreach ($element in (Get-All $Root)) {
        if ($element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem) { $rows += $element }
    }
    return @($rows)
}
function Get-FileScrollValue([Windows.Automation.AutomationElement]$Root) {
    $scroll = Wait-Match $Root ([Windows.Automation.ControlType]::ScrollBar) 'File view vertical scroll bar'
    $pattern = $null
    if (-not $scroll.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        throw 'file-view scrollbar does not expose RangeValuePattern'
    }
    return [double]([Windows.Automation.RangeValuePattern]$pattern).Current.Value
}
function Click-Element([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke(); return
    }
    $bounds = $Element.Current.BoundingRectangle
    [void][SortColumnSmoke.Native]::SetCursorPos([int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
    [SortColumnSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [SortColumnSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}
function Click-ElementByPointer([Windows.Automation.AutomationElement]$Element) {
    $bounds = $Element.Current.BoundingRectangle
    [void][SortColumnSmoke.Native]::SetCursorPos(
        [int]($bounds.Left + $bounds.Width / 2),
        [int]($bounds.Top + $bounds.Height / 2))
    [SortColumnSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [SortColumnSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}
function Convert-Bounds([Windows.Rect]$Bounds) {
    return [ordered]@{
        left = $Bounds.Left; top = $Bounds.Top
        right = $Bounds.Right; bottom = $Bounds.Bottom
        width = $Bounds.Width; height = $Bounds.Height
    }
}
function Assert-PopupAnchored(
    [Windows.Automation.AutomationElement]$Button,
    [Windows.Automation.AutomationElement]$FirstItem,
    [Windows.Automation.AutomationElement]$Window,
    [string]$MenuName
) {
    $buttonBounds = $Button.Current.BoundingRectangle
    $itemBounds = $FirstItem.Current.BoundingRectangle
    $windowBounds = $Window.Current.BoundingRectangle
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
    $popup = $FirstItem
    while ($null -ne $popup -and $popup.Current.ControlType -ne [Windows.Automation.ControlType]::Menu) {
        $popup = $walker.GetParent($popup)
    }
    if ($null -eq $popup) { throw "$MenuName popup did not expose a Menu ancestor" }
    $popupBounds = $popup.Current.BoundingRectangle
    $tolerance = [Math]::Max(4.0, $buttonBounds.Height * 0.20)
    $opensBelow = $popupBounds.Top -ge $buttonBounds.Bottom - $tolerance
    $notEnoughSpaceBelow = ($windowBounds.Bottom - $buttonBounds.Bottom) -lt ($popupBounds.Height - $tolerance)
    if (-not $opensBelow -and -not $notEnoughSpaceBelow) {
        throw "$MenuName popup moved above its button without an edge constraint: button=$buttonBounds popup=$popupBounds window=$windowBounds"
    }
    if ($opensBelow -and $popupBounds.Top -gt $buttonBounds.Bottom + $buttonBounds.Height * 2.0) {
        throw "$MenuName popup is too far below its button: button=$buttonBounds popup=$popupBounds"
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
    return [ordered]@{
        menu = $MenuName
        window = Convert-Bounds $windowBounds
        button = Convert-Bounds $buttonBounds
        popup = Convert-Bounds $popupBounds
        first_item = Convert-Bounds $itemBounds
        top_delta_from_button_bottom = $popupBounds.Top - $buttonBounds.Bottom
        edge_shifted = -not $opensBelow
        horizontally_overlaps = $true
        inside_window = $true
    }
}
function Drag-Element([Windows.Automation.AutomationElement]$Element, [int]$DeltaX, [int]$ReleaseY) {
    $bounds = $Element.Current.BoundingRectangle
    $startX = [int]($bounds.Left + $bounds.Width / 2)
    $startY = [int]($bounds.Top + $bounds.Height / 2)
    [void][SortColumnSmoke.Native]::SetCursorPos($startX, $startY)
    [SortColumnSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($step in 1..10) {
        $x = [int]($startX + $DeltaX * $step / 10)
        $y = [int]($startY + ($ReleaseY - $startY) * $step / 10)
        [void][SortColumnSmoke.Native]::SetCursorPos($x, $y)
        Start-Sleep -Milliseconds 25
    }
    [SortColumnSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 200
}
function DoubleClick-Element([Windows.Automation.AutomationElement]$Element) {
    $bounds = $Element.Current.BoundingRectangle
    [void][SortColumnSmoke.Native]::SetCursorPos([int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
    foreach ($click in 1..2) {
        [SortColumnSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [SortColumnSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 60
    }
    Start-Sleep -Milliseconds 200
}

$nameLabel = -join ([char]0x540D, [char]0x7A31)
$dateLabel = -join ([char]0x4FEE, [char]0x6539, [char]0x65E5, [char]0x671F)
$typeLabel = -join ([char]0x985E, [char]0x578B)
$sizeLabel = -join ([char]0x5927, [char]0x5C0F)
$descendingLabel = -join ([char]0x905E, [char]0x6E1B)
$labels = @($nameLabel, $dateLabel, $typeLabel, $sizeLabel)

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $executable
$start.WorkingDirectory = $workspaceRoot
$start.UseShellExecute = $false
$start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
$start.Environment['EXPLORER_INITIAL_PATH'] = $InitialPath
$start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$process = [Diagnostics.Process]::Start($start)
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh(); $hwnd = $process.MainWindowHandle
        if ($hwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
    } while ($hwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($hwnd -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][SortColumnSmoke.Native]::SetForegroundWindow($hwnd)
    [SortColumnSmoke.Native]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    [SortColumnSmoke.Native]::keybd_event(0x46, 0, 0, [UIntPtr]::Zero)
    [SortColumnSmoke.Native]::keybd_event(0x46, 0, 2, [UIntPtr]::Zero)
    [SortColumnSmoke.Native]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
    [SortColumnSmoke.Native]::keybd_event(0x1b, 0, 0, [UIntPtr]::Zero)
    [SortColumnSmoke.Native]::keybd_event(0x1b, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
    $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $nameHeader = Wait-Match $root ([Windows.Automation.ControlType]::Button) $nameLabel
    $rows = @(Get-Rows $root)
    if ($rows.Count -lt 2) { throw "real folder did not expose enough rows: $($rows.Count)" }
    $initialNames = @($rows | ForEach-Object { $_.Current.Name })
    $selectedName = $initialNames[0]
    $selection = $null
    if ($rows[0].TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
        ([Windows.Automation.SelectionItemPattern]$selection).Select()
    } else { Click-Element $rows[0] }

    $sortEvidence = @()
    foreach ($label in $labels) {
        $header = Wait-Match $root ([Windows.Automation.ControlType]::Button) $label
        $scrollBefore = Get-FileScrollValue $root
        Click-Element $header; Start-Sleep -Milliseconds 200
        $scrollAfterFirst = Get-FileScrollValue $root
        $firstHeader = Wait-Match $root ([Windows.Automation.ControlType]::Button) $label
        $firstHeaderName = $firstHeader.Current.Name
        $firstNames = @((Get-Rows $root) | ForEach-Object { $_.Current.Name })
        Click-Element $firstHeader; Start-Sleep -Milliseconds 200
        $scrollAfterSecond = Get-FileScrollValue $root
        $secondHeader = Wait-Match $root ([Windows.Automation.ControlType]::Button) $label
        $secondHeaderName = $secondHeader.Current.Name
        $secondNames = @((Get-Rows $root) | ForEach-Object { $_.Current.Name })
        if ($firstNames.Count -ne $initialNames.Count -or $secondNames.Count -ne $initialNames.Count) { throw "sort changed item count: $label" }
        if (@(Compare-Object ($firstNames | Sort-Object) ($initialNames | Sort-Object)).Count -ne 0) { throw "sort changed identities: $label" }
        if ($firstHeaderName -eq $secondHeaderName) { throw "sort direction did not toggle: $label" }
        if ([Math]::Abs($scrollAfterFirst-$scrollBefore) -gt 0.5 -or [Math]::Abs($scrollAfterSecond-$scrollBefore) -gt 0.5) {
            throw "header click changed file scroll offset: $label before=$scrollBefore first=$scrollAfterFirst second=$scrollAfterSecond"
        }
        $sortEvidence += [ordered]@{ column=$label; first_header=$firstHeaderName; second_header=$secondHeaderName; first_order=$firstNames; second_order=$secondNames; scroll_before=$scrollBefore; scroll_after_first=$scrollAfterFirst; scroll_after_second=$scrollAfterSecond }
    }
    $commandScrollBefore = Get-FileScrollValue $root
    $sortButton = Wait-Match $root ([Windows.Automation.ControlType]::Button) 'Sort'
    # UIA reports physical bounds while the app can run at a non-100% scale;
    # use the control's Invoke pattern so a scaled pointer coordinate cannot
    # accidentally hit the breadcrumb row above the command bar.
    Click-Element $sortButton
    $nameMenuItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $nameLabel
    $sortMenuAnchor = Assert-PopupAnchored $sortButton $nameMenuItem $root 'Sort'
    Click-Element $nameMenuItem; Start-Sleep -Milliseconds 150
    $sortButton = Wait-Match $root ([Windows.Automation.ControlType]::Button) 'Sort'
    Click-Element $sortButton
    $descendingMenuItem = Wait-Match $root ([Windows.Automation.ControlType]::MenuItem) $descendingLabel
    Click-Element $descendingMenuItem; Start-Sleep -Milliseconds 150
    $commandScrollAfter = Get-FileScrollValue $root
    if ([Math]::Abs($commandScrollAfter-$commandScrollBefore) -gt 0.5) { throw "command Sort menu changed file scroll offset: before=$commandScrollBefore after=$commandScrollAfter" }
    $commandHeader = (Wait-Match $root ([Windows.Automation.ControlType]::Button) $nameLabel).Current.Name
    if ($commandHeader -notlike "*$nameLabel*" -or $commandHeader -notlike '*↓*') { throw "command Sort menu did not apply name descending: $commandHeader" }
    $selected = @(Get-Rows $root | Where-Object { $_.Current.Name -eq $selectedName } | Select-Object -First 1)
    if ($selected.Count -ne 1) { throw 'selected identity disappeared after sorting' }
    $selectedPattern = $null
    if ($selected[0].TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selectedPattern) -and -not ([Windows.Automation.SelectionItemPattern]$selectedPattern).Current.IsSelected) {
        throw 'selection identity was not preserved after sorting'
    }

    $windowBounds = $root.Current.BoundingRectangle
    $columnEvidence = @()
    foreach ($label in $labels) {
        $header = Wait-Match $root ([Windows.Automation.ControlType]::Button) $label
        $separator = Wait-Match $root ([Windows.Automation.ControlType]::Separator) "Resize $label column"
        $separatorBounds = $separator.Current.BoundingRectangle
        $before = $header.Current.BoundingRectangle.Width
        Drag-Element $separator 70 ([int]($windowBounds.Bottom + 30))
        $after = (Wait-Match $root ([Windows.Automation.ControlType]::Button) $label).Current.BoundingRectangle.Width
        if ($after -le $before + 20) { throw "outside-release resize failed: $label before=$before after=$after separator=$separatorBounds window=$windowBounds" }
        $physicalDelta = $after - $before
        if ([Math]::Abs($physicalDelta - 70) -gt 8) {
            throw "column resize was not pointer 1:1: $label pointer_delta=70 width_delta=$physicalDelta before=$before after=$after"
        }
        $separator = Wait-Match $root ([Windows.Automation.ControlType]::Separator) "Resize $label column"
        DoubleClick-Element $separator
        $auto = (Wait-Match $root ([Windows.Automation.ControlType]::Button) $label).Current.BoundingRectangle.Width
        if ($auto -le 0) { throw "auto-size produced invalid width: $label" }
        $columnEvidence += [ordered]@{ column=$label; before=$before; after_drag=$after; physical_pointer_delta=70; physical_width_delta=$physicalDelta; after_double_click=$auto }
    }

    [ordered]@{
        schema_version=1
        captured_utc=[DateTime]::UtcNow.ToString('o')
        initial_path=$InitialPath
        item_count=$initialNames.Count
        selected_identity=$selectedName
        sort=$sortEvidence
        command_sort_header=$commandHeader
        sort_menu_anchor=$sortMenuAnchor
        command_scroll_before=$commandScrollBefore
        command_scroll_after=$commandScrollAfter
        columns=$columnEvidence
        release_outside_verified=$true
        exit_code=0
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
} finally {
    if (-not $process.HasExited) {
        [void][SortColumnSmoke.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    if ($null -ne $fixtureRoot -and (Test-Path -LiteralPath $fixtureRoot)) {
        $resolvedFixture = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $fixtureRoot).Path)
        $allowedFixturePrefix = [IO.Path]::GetFullPath($targetRoot).TrimEnd('\') + '\column-fixture-'
        if (-not $resolvedFixture.StartsWith($allowedFixturePrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing unsafe sort fixture cleanup: $resolvedFixture"
        }
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}
Write-Output "Sort/column smoke passed: $OutputDirectory"
