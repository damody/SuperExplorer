param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path (Join-Path $fixture 'Alpha') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $fixture 'Beta') | Out-Null
$context = $null

function Get-FolderChildrenLabel([string]$Name) {
    $prefix = -join ([char[]]@(0x5217, 0x51FA))
    $suffix = -join ([char[]]@(0x7684, 0x5B50, 0x8CC7, 0x6599, 0x593E))
    "$prefix $Name $suffix"
}

function Find-MenuItems([Windows.Automation.AutomationElement]$Root, [int]$MinimumCount) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::MenuItem)
    do {
        $items = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($items.Count -lt $MinimumCount) { Start-Sleep -Milliseconds 80 }
    } while ($items.Count -lt $MinimumCount -and [DateTime]::UtcNow -lt $deadline)
    if ($items.Count -lt $MinimumCount) { throw "expected $MinimumCount breadcrumb rows, got $($items.Count)" }
    @($items | ForEach-Object { $_ })
}

function Get-CapturedPixel(
    [string]$Path,
    [Windows.Rect]$WindowBounds,
    [int]$ScreenX,
    [int]$ScreenY
) {
    $bitmap = [Drawing.Bitmap]::FromFile($Path)
    try {
        $x = $ScreenX - [int]$WindowBounds.Left
        $y = $ScreenY - [int]$WindowBounds.Top
        if ($x -lt 0 -or $y -lt 0 -or $x -ge $bitmap.Width -or $y -ge $bitmap.Height) {
            throw "pixel outside capture: screen=($ScreenX,$ScreenY) local=($x,$y) size=$($bitmap.Width)x$($bitmap.Height)"
        }
        $color = $bitmap.GetPixel($x, $y)
        [ordered]@{ r=[int]$color.R; g=[int]$color.G; b=[int]$color.B; x=$ScreenX; y=$ScreenY }
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

function Get-PlainAddGlyphEvidence(
    [string]$Path,
    [Windows.Rect]$WindowBounds,
    [Windows.Rect]$ButtonBounds,
    $Background
) {
    $bitmap = [Drawing.Bitmap]::FromFile($Path)
    try {
        $centerX = [int][Math]::Round(($ButtonBounds.Left + $ButtonBounds.Right) / 2 - $WindowBounds.Left)
        $centerY = [int][Math]::Round(($ButtonBounds.Top + $ButtonBounds.Bottom) / 2 - $WindowBounds.Top)
        $diameter = [Math]::Min($ButtonBounds.Width, $ButtonBounds.Height)
        $radius = [int][Math]::Floor($diameter * 0.31)
        $axisBand = [Math]::Max(2, [int][Math]::Round($diameter * 0.06))
        $ink = 0
        $axisInk = 0
        $diagonalInk = 0
        foreach ($dy in (-$radius)..$radius) {
            foreach ($dx in (-$radius)..$radius) {
                $color = $bitmap.GetPixel($centerX + $dx, $centerY + $dy)
                $distance = [Math]::Max(
                    [Math]::Abs([int]$color.R - [int]$Background.r),
                    [Math]::Max(
                        [Math]::Abs([int]$color.G - [int]$Background.g),
                        [Math]::Abs([int]$color.B - [int]$Background.b)))
                if ($distance -le 40) { continue }
                $ink++
                if ([Math]::Abs($dx) -le $axisBand -or [Math]::Abs($dy) -le $axisBand) {
                    $axisInk++
                } elseif ([Math]::Abs($dx) -gt ($axisBand + 1) -and [Math]::Abs($dy) -gt ($axisBand + 1)) {
                    $diagonalInk++
                }
            }
        }
        if ($ink -lt 8 -or $axisInk -lt 8) { throw "new-tab Add glyph is visually empty: ink=$ink axis=$axisInk" }
        if ($diagonalInk -gt 2) { throw "new-tab glyph contains an enclosing ring: diagonal_ink=$diagonalInk" }
        [ordered]@{ ink=$ink; axis_ink=$axisInk; diagonal_ink=$diagonalInk; plain_add=$true }
    } finally {
        $bitmap.Dispose()
    }
}

function Get-ElementEvidence([Windows.Automation.AutomationElement]$Element) {
    $bounds = $Element.Current.BoundingRectangle
    [ordered]@{
        name=$Element.Current.Name
        automation_id=$Element.Current.AutomationId
        bounds=[ordered]@{ left=$bounds.Left; top=$bounds.Top; right=$bounds.Right; bottom=$bounds.Bottom }
    }
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 20, 20, 1440, 880, 0x0040)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)

    Send-UitestKey -Key 0x54 -Modifiers @(0x11) -DelayMilliseconds 500 # Ctrl+T
    $root = [Windows.Automation.AutomationElement]::FromHandle($context.Hwnd)
    $tabCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::TabItem)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $tabs = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, $tabCondition) | ForEach-Object { $_ })
        if ($tabs.Count -lt 2) { Start-Sleep -Milliseconds 80 }
    } while ($tabs.Count -lt 2 -and [DateTime]::UtcNow -lt $deadline)
    if ($tabs.Count -lt 2) { throw "expected two tabs, got $($tabs.Count)" }
    $activeTab = $tabs | Where-Object {
        $_.GetCurrentPropertyValue([Windows.Automation.SelectionItemPattern]::IsSelectedProperty, $true) -eq $true
    } | Select-Object -First 1
    if ($null -eq $activeTab) { throw 'active tab did not expose selected state' }
    $inactiveTab = $tabs | Where-Object { $_ -ne $activeTab } | Select-Object -First 1
    $activeBounds = $activeTab.Current.BoundingRectangle
    $inactiveBounds = $inactiveTab.Current.BoundingRectangle
    if ($activeBounds.Width -le 20 -or $inactiveBounds.Width -le 20) { throw 'tab bounds are too small for pixel evidence' }
    $newTabButton = Find-UitestElement -Root $root -Description 'new tab button' -Predicate {
        param($element)
        $element.Current.AutomationId -eq 'new-tab-button' -or $element.Current.Name -eq 'New tab'
    }
    $newTabBounds = $newTabButton.Current.BoundingRectangle

    $windowBounds = $root.Current.BoundingRectangle
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware(
        [int][Math]::Round($windowBounds.Right - 80),
        [int][Math]::Round($windowBounds.Bottom - 80))) {
        throw 'could not move pointer away from the tab strip before idle pixel capture'
    }
    Start-Sleep -Milliseconds 250
    $tabCapture = Join-Path $output 'tab-surface.png'
    Save-UitestScreenshot -Root $root -Path $tabCapture
    $activeX = [int][Math]::Round($activeBounds.Left + 9)
    $activeY = [int][Math]::Round($activeBounds.Top + $activeBounds.Height / 2)
    $inactiveX = [int][Math]::Round($inactiveBounds.Left + 9)
    $inactiveY = [int][Math]::Round($inactiveBounds.Top + $inactiveBounds.Height / 2)
    $stripX = [int][Math]::Round(($inactiveBounds.Right + $activeBounds.Left) / 2)
    $stripY = $inactiveY
    $activeFill = Get-CapturedPixel $tabCapture $windowBounds $activeX $activeY
    $inactiveFill = Get-CapturedPixel $tabCapture $windowBounds $inactiveX $inactiveY
    $stripFill = Get-CapturedPixel $tabCapture $windowBounds $stripX $stripY
    $activeBottom = Get-CapturedPixel $tabCapture $windowBounds $activeX ([int][Math]::Round($activeBounds.Bottom - 1))
    $contentTop = Get-CapturedPixel $tabCapture $windowBounds $activeX ([int][Math]::Round($activeBounds.Bottom + 2))
    $activeTopX = [int][Math]::Round($activeBounds.Left + $activeBounds.Width / 2)
    $activeTop = Get-CapturedPixel $tabCapture $windowBounds $activeTopX ([int][Math]::Round($activeBounds.Top + 2))
    $activeBody = Get-CapturedPixel $tabCapture $windowBounds $activeTopX ([int][Math]::Round($activeBounds.Bottom - 10))
    $newTabBackground = Get-CapturedPixel $tabCapture $windowBounds ([int][Math]::Round($newTabBounds.Left + 5)) ([int][Math]::Round($newTabBounds.Top + $newTabBounds.Height / 2))

    $activeContentDistance = Get-ColorDistance $activeFill $contentTop
    $activeBoundaryDistance = Get-ColorDistance $activeBottom $contentTop
    $inactiveStripDistance = Get-ColorDistance $inactiveFill $stripFill
    $activeInactiveDistance = Get-ColorDistance $activeFill $inactiveFill
    $activeTopBodyDistance = Get-ColorDistance $activeTop $activeBody
    $newTabStripDistance = Get-ColorDistance $newTabBackground $stripFill
    $newTabGlyph = Get-PlainAddGlyphEvidence $tabCapture $windowBounds $newTabBounds $newTabBackground
    if ($activeContentDistance -gt 3) { throw "active tab does not match content: distance=$activeContentDistance" }
    if ($activeBoundaryDistance -gt 3) { throw "active tab bottom divider remains visible: distance=$activeBoundaryDistance" }
    if ($inactiveStripDistance -gt 3) { throw "inactive tab does not match strip: distance=$inactiveStripDistance" }
    if ($activeInactiveDistance -lt 10) { throw "active and inactive tabs are not visually distinct: distance=$activeInactiveDistance" }
    if ($activeTopBodyDistance -gt 3) { throw "focused active tab has a top focus line: distance=$activeTopBodyDistance" }
    if ($newTabStripDistance -gt 3) { throw "new-tab button has a distinct idle background: distance=$newTabStripDistance" }
    if ($activeFill.r -lt 250 -or $activeFill.g -lt 250 -or $activeFill.b -lt 250) {
        throw "active tab is not white: rgb=$($activeFill.r),$($activeFill.g),$($activeFill.b)"
    }

    $chevronLabel = Get-FolderChildrenLabel 'fixture'
    $chevron = Find-UitestElement -Root $root -Description "breadcrumb chevron '$chevronLabel'" -Predicate {
        param($element)
        $element.Current.Name -eq $chevronLabel
    }
    Invoke-UitestClick -Element $chevron
    $rows = @(Find-MenuItems $root 2 | Where-Object { $_.Current.Name -in @('Alpha','Beta') })
    if ($rows.Count -ne 2) { throw "expected Alpha and Beta menu rows, got $($rows.Current.Name -join ', ')" }
    $first = $rows | Where-Object { $_.Current.Name -eq 'Alpha' } | Select-Object -First 1
    $second = $rows | Where-Object { $_.Current.Name -eq 'Beta' } | Select-Object -First 1
    $firstBounds = $first.Current.BoundingRectangle
    $secondBounds = $second.Current.BoundingRectangle
    $sampleX = [int][Math]::Round([Math]::Min($firstBounds.Right, $secondBounds.Right) - 12)
    $firstY = [int][Math]::Round($firstBounds.Top + $firstBounds.Height / 2)
    $secondY = [int][Math]::Round($secondBounds.Top + $secondBounds.Height / 2)

    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($sampleX, $firstY)) { throw 'could not move pointer to first breadcrumb row' }
    Start-Sleep -Milliseconds 350
    $firstHoverCapture = Join-Path $output 'breadcrumb-hover-first.png'
    Save-UitestScreenshot -Root $root -Path $firstHoverCapture
    $firstHovered = Get-CapturedPixel $firstHoverCapture $windowBounds $sampleX $firstY
    $secondIdle = Get-CapturedPixel $firstHoverCapture $windowBounds $sampleX $secondY

    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($sampleX, $secondY)) { throw 'could not move pointer to second breadcrumb row' }
    Start-Sleep -Milliseconds 350
    $secondHoverCapture = Join-Path $output 'breadcrumb-hover-second.png'
    Save-UitestScreenshot -Root $root -Path $secondHoverCapture
    $firstIdle = Get-CapturedPixel $secondHoverCapture $windowBounds $sampleX $firstY
    $secondHovered = Get-CapturedPixel $secondHoverCapture $windowBounds $sampleX $secondY

    $highlightSwapDistance = Get-ColorDistance $firstHovered $secondHovered
    $idleRestoreDistance = Get-ColorDistance $secondIdle $firstIdle
    $highlightContrast = Get-ColorDistance $firstHovered $secondIdle
    if ($highlightSwapDistance -gt 3) { throw "breadcrumb highlight color did not follow pointer: distance=$highlightSwapDistance" }
    if ($idleRestoreDistance -gt 3) { throw "previous breadcrumb row did not return to menu fill: distance=$idleRestoreDistance" }
    if ($highlightContrast -lt 5) { throw "breadcrumb hover gray is not visually distinguishable: distance=$highlightContrast" }

    [ordered]@{
        schema='superexplorer.tab-breadcrumb-hover.v1'
        physical_pointer_input=$true
        tab=[ordered]@{
            active=$(Get-ElementEvidence $activeTab)
            inactive=$(Get-ElementEvidence $inactiveTab)
            new_tab=$(Get-ElementEvidence $newTabButton)
            colors=[ordered]@{ active=$activeFill; inactive=$inactiveFill; strip=$stripFill; active_bottom=$activeBottom; content_top=$contentTop; active_top=$activeTop; active_body=$activeBody; new_tab_background=$newTabBackground }
            distance=[ordered]@{ active_content=$activeContentDistance; active_boundary=$activeBoundaryDistance; inactive_strip=$inactiveStripDistance; active_inactive=$activeInactiveDistance; active_top_body=$activeTopBodyDistance; new_tab_strip=$newTabStripDistance }
            continuous_surface=$true
            no_top_focus_line=$true
            new_tab_matches_strip=$true
            new_tab_glyph=$newTabGlyph
        }
        breadcrumb=[ordered]@{
            first=$(Get-ElementEvidence $first)
            second=$(Get-ElementEvidence $second)
            colors=[ordered]@{ first_hovered=$firstHovered; second_idle=$secondIdle; first_idle=$firstIdle; second_hovered=$secondHovered }
            distance=[ordered]@{ highlight_swap=$highlightSwapDistance; idle_restore=$idleRestoreDistance; highlight_contrast=$highlightContrast }
            highlight_followed_pointer=$true
        }
        artifacts=@('tab-surface.png','breadcrumb-hover-first.png','breadcrumb-hover-second.png')
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')

    Write-Host "Tab and breadcrumb hover visual smoke passed: $output"
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}
