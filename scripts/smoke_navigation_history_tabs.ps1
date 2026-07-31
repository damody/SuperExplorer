param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
$folderA = Join-Path $fixture 'history-a'
$folderB = Join-Path $folderA 'history-b'
$folderC = Join-Path $folderB 'history-c'
New-Item -ItemType Directory -Force -Path $folderA, $folderB, $folderC | Out-Null
Set-Content -Encoding utf8 -LiteralPath (Join-Path $folderA 'marker-a.txt') -Value 'a'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $folderB 'marker-b.txt') -Value 'b'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $folderC 'marker-c.txt') -Value 'c'
$context = $null

function Find-ByAutomationId([string]$Id, [string]$Description, [string]$AccessibleName) {
    Find-UitestElement -Root $context.Root -Description $Description -Predicate {
        param($element)
        $element.Current.AutomationId -eq $Id -or $element.Current.Name -eq $AccessibleName
    }
}

function Get-TabCount {
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::TabItem))).Count
}

function Get-CurrentHistoryItems {
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::MenuItem)) | Where-Object {
                # Accessible names append the absolute location after a comma. Match only the
                # visible folder title so a test output path containing "history" cannot inflate
                # the result count.
                ($_.Current.Name -split ',', 2)[0] -like 'history-*' -and
                $_.Current.BoundingRectangle.Width -gt 0
            })
}

function Get-HistoryItems([int]$TimeoutSeconds = 5) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $items = @(Get-CurrentHistoryItems)
        if ($items.Count -gt 0) { return $items }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'navigation history menu items did not appear'
}

function Get-CapturedPixel(
    [string]$Path,
    [Windows.Rect]$WindowBounds,
    [int]$ScreenX,
    [int]$ScreenY
) {
    $bitmap = [Drawing.Bitmap]::FromFile($Path)
    try {
        $color = $bitmap.GetPixel(
            $ScreenX - [int]$WindowBounds.Left,
            $ScreenY - [int]$WindowBounds.Top)
        [ordered]@{ r=[int]$color.R; g=[int]$color.G; b=[int]$color.B }
    } finally {
        $bitmap.Dispose()
    }
}

function Get-ColorDistance($Left, $Right) {
    [Math]::Max(
        [Math]::Abs([int]$Left.r - [int]$Right.r),
        [Math]::Max(
            [Math]::Abs([int]$Left.g - [int]$Right.g),
            [Math]::Abs([int]$Left.b - [int]$Right.b)))
}

function Measure-NavigationGlyph(
    [string]$Path,
    [Windows.Rect]$WindowBounds,
    [Windows.Automation.AutomationElement]$Element
) {
    $bounds = $Element.Current.BoundingRectangle
    $centerX = [int][Math]::Round($bounds.Left + $bounds.Width / 2) - [int]$WindowBounds.Left
    $centerY = [int][Math]::Round($bounds.Top + $bounds.Height / 2) - [int]$WindowBounds.Top
    $bitmap = [Drawing.Bitmap]::FromFile($Path)
    try {
        $background = $bitmap.GetPixel(
            [int][Math]::Round($bounds.Left - $WindowBounds.Left + 3),
            [int][Math]::Round($bounds.Top - $WindowBounds.Top + 3))
        $inkPixels = 0
        $inkWeight = 0
        foreach ($x in (($centerX - 12)..($centerX + 11))) {
            foreach ($y in (($centerY - 12)..($centerY + 11))) {
                $pixel = $bitmap.GetPixel($x, $y)
                $distance = [Math]::Max(
                    [Math]::Abs([int]$pixel.R - [int]$background.R),
                    [Math]::Max(
                        [Math]::Abs([int]$pixel.G - [int]$background.G),
                        [Math]::Abs([int]$pixel.B - [int]$background.B)))
                if ($distance -ge 6) {
                    $inkPixels += 1
                    $inkWeight += $distance
                }
            }
        }
        [ordered]@{
            ink_pixels = $inkPixels
            ink_weight = $inkWeight
            mean_ink_distance = if ($inkPixels -gt 0) { [Math]::Round($inkWeight / $inkPixels, 2) } else { 0 }
            background = [ordered]@{ r=[int]$background.R; g=[int]$background.G; b=[int]$background.B }
        }
    } finally {
        $bitmap.Dispose()
    }
}

function Invoke-HistoryItem([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
    } else {
        $point = Get-UitestPhysicalPoint -Element $Element
        [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
        [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)
        [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 500
}

function Open-ChildFolder([string]$FolderName, [string]$ExpectedMarker) {
    $lastError = $null
    foreach ($attempt in 1..3) {
        try {
            Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name $FolderName) -Double
            Find-UitestFileItem -Root $context.Root -Name $ExpectedMarker -TimeoutSeconds 3 | Out-Null
            return
        } catch {
            $lastError = $_
        }
    }
    throw $lastError
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    Open-ChildFolder -FolderName 'history-a' -ExpectedMarker 'marker-a.txt'
    Open-ChildFolder -FolderName 'history-b' -ExpectedMarker 'marker-b.txt'
    Open-ChildFolder -FolderName 'history-c' -ExpectedMarker 'marker-c.txt'

    $back = Find-ByAutomationId 'navigation-back' 'Back button' 'Back'
    $forwardDisabled = Find-ByAutomationId 'navigation-forward' 'Forward button' 'Forward'
    $windowBounds = $context.Root.Current.BoundingRectangle
    $availabilityCapture = Join-Path $output 'navigation-availability.png'
    Save-UitestScreenshot -Root $context.Root -Path $availabilityCapture
    $enabledBackGlyph = Measure-NavigationGlyph $availabilityCapture $windowBounds $back
    $disabledForwardGlyph = Measure-NavigationGlyph $availabilityCapture $windowBounds $forwardDisabled
    if ($enabledBackGlyph.ink_weight -lt [int]($disabledForwardGlyph.ink_weight * 1.5)) {
        throw "enabled Back is not sufficiently darker than disabled Forward: enabled=$($enabledBackGlyph.ink_weight) disabled=$($disabledForwardGlyph.ink_weight)"
    }
    if ($enabledBackGlyph.ink_pixels -le $disabledForwardGlyph.ink_pixels) {
        throw "enabled Back is not slightly thicker than disabled Forward: enabled=$($enabledBackGlyph.ink_pixels) disabled=$($disabledForwardGlyph.ink_pixels)"
    }
    Invoke-UitestClick -Element $back -Right
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
            [ordered]@{
                name = $_.Current.Name
                type = $_.Current.ControlType.ProgrammaticName
                automation_id = $_.Current.AutomationId
                bounds = $_.Current.BoundingRectangle.ToString()
            }
        }) | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'back-history-tree.json')
    $backItems = @(Get-HistoryItems)
    # Get-CurrentHistoryItems intentionally filters to the named history-* fixture entries; the
    # older root named "fixture" is valid history but is outside this assertion's label scope.
    if ($backItems.Count -ne 2) { throw "Back history expected 2 named entries, got $($backItems.Count)" }
    if ($backItems[0].Current.Name -notlike '*history-b*' -or $backItems[1].Current.Name -notlike '*history-a*') {
        throw "Back history order is not nearest-first: $(@($backItems | ForEach-Object { $_.Current.Name }) -join ', ')"
    }
    $firstBounds = $backItems[0].Current.BoundingRectangle
    $secondBounds = $backItems[1].Current.BoundingRectangle
    $sampleX = [int][Math]::Round([Math]::Min($firstBounds.Right, $secondBounds.Right) - 12)
    $firstY = [int][Math]::Round($firstBounds.Top + $firstBounds.Height / 2)
    $secondY = [int][Math]::Round($secondBounds.Top + $secondBounds.Height / 2)

    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($sampleX, $firstY)
    Start-Sleep -Milliseconds 300
    $firstHoverCapture = Join-Path $output 'back-history-hover-first.png'
    Save-UitestScreenshot -Root $context.Root -Path $firstHoverCapture
    $firstHovered = Get-CapturedPixel $firstHoverCapture $windowBounds $sampleX $firstY
    $secondIdle = Get-CapturedPixel $firstHoverCapture $windowBounds $sampleX $secondY

    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($sampleX, $secondY)
    Start-Sleep -Milliseconds 300
    $secondHoverCapture = Join-Path $output 'back-history-hover-second.png'
    Save-UitestScreenshot -Root $context.Root -Path $secondHoverCapture
    $firstIdle = Get-CapturedPixel $secondHoverCapture $windowBounds $sampleX $firstY
    $secondHovered = Get-CapturedPixel $secondHoverCapture $windowBounds $sampleX $secondY
    $highlightSwapDistance = Get-ColorDistance $firstHovered $secondHovered
    $idleRestoreDistance = Get-ColorDistance $secondIdle $firstIdle
    $highlightContrast = Get-ColorDistance $firstHovered $secondIdle
    if ($highlightSwapDistance -gt 3) { throw "history hover color did not follow pointer: distance=$highlightSwapDistance" }
    if ($idleRestoreDistance -gt 3) { throw "previous history row did not return to menu fill: distance=$idleRestoreDistance" }
    if ($highlightContrast -lt 5) { throw "history hover gray is not visually distinguishable: distance=$highlightContrast" }
    if ($firstHovered.r -lt 200 -or $firstHovered.r -gt 250 -or
        $firstHovered.g -lt 200 -or $firstHovered.g -gt 250 -or
        $firstHovered.b -lt 200 -or $firstHovered.b -gt 250) {
        throw "history focus is not neutral gray: rgb=$($firstHovered.r),$($firstHovered.g),$($firstHovered.b)"
    }

    Copy-Item -LiteralPath $secondHoverCapture -Destination (Join-Path $output 'back-history-menu.png')
    $backItems = @(Get-HistoryItems)
    $historyA = $backItems | Where-Object {
        ($_.Current.Name -split ',', 2)[0] -eq 'history-a'
    } | Select-Object -First 1
    if ($null -eq $historyA) { throw 'history-a target disappeared after hover rerender' }
    Invoke-HistoryItem -Element $historyA
    Find-UitestFileItem -Root $context.Root -Name 'marker-a.txt' | Out-Null

    $tabsBeforePlus = Get-TabCount
    Invoke-UitestClick -Element (Find-ByAutomationId 'new-tab-button' 'new tab plus button' 'New tab')
    $tabsAfterPlus = Get-TabCount
    if ($tabsAfterPlus -ne ($tabsBeforePlus + 1)) { throw '+ did not create exactly one tab' }
    Find-UitestFileItem -Root $context.Root -Name 'marker-a.txt' | Out-Null

    $forward = Find-ByAutomationId 'navigation-forward' 'Forward button' 'Forward'
    Invoke-UitestClick -Element $forward -Right
    $forwardItems = @(Get-HistoryItems)
    if ($forwardItems.Count -ne 2) { throw "cloned tab expected 2 Forward entries, got $($forwardItems.Count)" }
    if ($forwardItems[0].Current.Name -notlike '*history-b*' -or $forwardItems[1].Current.Name -notlike '*history-c*') {
        throw "cloned Forward history order is not nearest-first: $(@($forwardItems | ForEach-Object { $_.Current.Name }) -join ', ')"
    }
    Send-UitestKey -Key 0x1B
    Start-Sleep -Milliseconds 200
    if (@(Get-CurrentHistoryItems).Count -gt 0) { throw 'Escape did not dismiss history menu' }

    $tabsBeforeCtrlT = Get-TabCount
    Send-UitestKey -Key 0x54 -Modifiers @(0x11) -DelayMilliseconds 500
    $tabsAfterCtrlT = Get-TabCount
    if ($tabsAfterCtrlT -ne ($tabsBeforeCtrlT + 1)) { throw 'Ctrl+T did not create exactly one tab' }
    Find-UitestFileItem -Root $context.Root -Name 'marker-a.txt' | Out-Null

    $tabItems = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::TabItem)) | ForEach-Object { $_ })
    $middleTarget = $tabItems | Where-Object {
        $_.GetCurrentPropertyValue([Windows.Automation.SelectionItemPattern]::IsSelectedProperty, $true) -ne $true
    } | Select-Object -First 1
    if ($null -eq $middleTarget) { throw 'middle-click test could not find an inactive tab' }
    Invoke-UitestClick -Element $middleTarget -Middle
    $middleDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $tabsAfterMiddle = Get-TabCount
        if ($tabsAfterMiddle -ne ($tabsAfterCtrlT - 1)) { Start-Sleep -Milliseconds 80 }
    } while ($tabsAfterMiddle -ne ($tabsAfterCtrlT - 1) -and [DateTime]::UtcNow -lt $middleDeadline)
    if ($tabsAfterMiddle -ne ($tabsAfterCtrlT - 1)) {
        throw "middle-click did not close exactly one hit tab: before=$tabsAfterCtrlT after=$tabsAfterMiddle"
    }
    Find-UitestFileItem -Root $context.Root -Name 'marker-a.txt' | Out-Null

    [ordered]@{
        schema = 'superexplorer.navigation-history-tabs.v1'
        back_menu_nearest_first = $true
        multi_step_back_destination = $folderA
        plus_created_one_tab = $true
        plus_inherited_forward_history = $true
        escape_closed_history_menu = $true
        history_hover_followed_pointer = $true
        navigation_availability_visual = [ordered]@{
            enabled_back=$enabledBackGlyph
            disabled_forward=$disabledForwardGlyph
        }
        history_hover_colors = [ordered]@{
            first_hovered=$firstHovered
            second_idle=$secondIdle
            first_idle=$firstIdle
            second_hovered=$secondHovered
        }
        history_hover_distance = [ordered]@{
            swap=$highlightSwapDistance
            idle_restore=$idleRestoreDistance
            contrast=$highlightContrast
        }
        ctrl_t_created_one_tab = $true
        middle_click_closed_hit_tab = $true
        tab_counts = [ordered]@{ before_plus=$tabsBeforePlus; after_plus=$tabsAfterPlus; before_ctrl_t=$tabsBeforeCtrlT; after_ctrl_t=$tabsAfterCtrlT; after_middle_click=$tabsAfterMiddle }
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Navigation history and tabs smoke passed: $OutputDirectory"
