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
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'pointer-input-sentinel.txt') -Value 'sentinel'
$context = $null

function Get-EditorValue([Windows.Automation.AutomationElement]$Editor) {
    $pattern = $null
    if ($Editor.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        return ([Windows.Automation.ValuePattern]$pattern).Current.Value
    }
    if ($Editor.TryGetCurrentPattern([Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
        return ([Windows.Automation.TextPattern]$pattern).DocumentRange.GetText(-1)
    }
    $accessibleName = $Editor.Current.Name
    if ($accessibleName -match '^[^:]+:\s*(.*)$') { return $Matches[1] }
    $Editor.SetFocus()
    Send-UitestKey -Key 0x41 -Modifiers @(0x11) -DelayMilliseconds 80
    Send-UitestKey -Key 0x43 -Modifiers @(0x11) -DelayMilliseconds 120
    $clipboardValue = Get-Clipboard -Raw
    if ($null -ne $clipboardValue) { return $clipboardValue.TrimEnd("`r", "`n") }
    throw "editable field exposes no readable value or clipboard text: name=$accessibleName help=$($Editor.Current.HelpText) status=$($Editor.Current.ItemStatus) id=$($Editor.Current.AutomationId)"
}

function Find-TopEditor([scriptblock]$Predicate, [string]$Description) {
    Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds 8 -Predicate {
        param($element)
        if ($element.Current.ControlType -ne [Windows.Automation.ControlType]::Edit) { return $false }
        & $Predicate $element
    }
}

function Set-EditorTextWithPasteRetry(
    [Windows.Automation.AutomationElement]$Editor,
    [string]$Text
) {
    $lastValue = $null
    foreach ($attempt in 1..8) {
        $Editor.SetFocus()
        Send-UitestKey -Key 0x41 -Modifiers @(0x11) -DelayMilliseconds 80
        Set-UitestClipboardText -Text $Text
        Start-Sleep -Milliseconds 80
        Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 180
        $lastValue = Get-EditorValue $Editor
        if ($lastValue -ceq $Text) { return }
        Start-Sleep -Milliseconds 100
    }
    throw "editable paste did not converge after bounded retries: expected='$Text' actual='$lastValue'"
}

function Click-EditorAt([Windows.Automation.AutomationElement]$Editor, [double]$OffsetX) {
    $bounds = $Editor.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware(
        [int]($bounds.Left + $OffsetX),
        [int]($bounds.Top + $bounds.Height / 2))) {
        throw 'DPI-aware editor cursor positioning failed'
    }
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

function Drag-EditorSelection(
    [Windows.Automation.AutomationElement]$Editor,
    [double]$StartOffsetX,
    [double]$EndOffsetX
) {
    $bounds = $Editor.Current.BoundingRectangle
    $startX = [int]($bounds.Left + $StartOffsetX)
    $endX = [int]($bounds.Left + $EndOffsetX)
    $y = [int]($bounds.Top + $bounds.Height / 2)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($startX, $y)) {
        throw 'DPI-aware editor drag start positioning failed'
    }
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    try {
        foreach ($step in 1..8) {
            $x = [int]($startX + (($endX - $startX) * $step / 8.0))
            if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($x, $y)) {
                throw "DPI-aware editor drag positioning failed at step $step"
            }
            Start-Sleep -Milliseconds 30
        }
    } finally {
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 250
}

function Measure-EditorSelectionGeometry(
    [Windows.Automation.AutomationElement]$Editor,
    [string]$CapturePath,
    [string]$Label,
    [double]$SelectionStartOffsetX,
    [double]$SelectionEndOffsetX
) {
    Save-UitestScreenshot -Root $context.Root -Path $CapturePath
        $bounds = $Editor.Current.BoundingRectangle
        $bitmap = [Drawing.Bitmap]::FromFile($CapturePath)
    try {
        $scaleX = $bitmap.Width / $window.Width
        $scaleY = $bitmap.Height / $window.Height
        $left = [Math]::Max(0, [int][Math]::Floor(($bounds.Left - $window.Left) * $scaleX))
        $top = [Math]::Max(0, [int][Math]::Floor(($bounds.Top - $window.Top) * $scaleY))
        $right = [Math]::Min($bitmap.Width - 1, [int][Math]::Ceiling(($bounds.Right - $window.Left) * $scaleX) - 1)
        $bottom = [Math]::Min($bitmap.Height - 1, [int][Math]::Ceiling(($bounds.Bottom - $window.Top) * $scaleY) - 1)

        # UIA exposes the accessibility wrapper, whose flex-row bounds can be taller than the
        # visible editor. Locate the full-width focus-stroke rows and measure against the actual
        # blue input frame the user sees.
        $minimumFocusRowPixels = [Math]::Max(12, [int][Math]::Floor(($right - $left + 1) * 0.5))
        $focusTop = [int]::MaxValue
        $focusBottom = [int]::MinValue
        for ($y = $top; $y -le $bottom; $y++) {
            $focusRowPixels = 0
            for ($x = $left; $x -le $right; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                if ($pixel.R -eq 0 -and $pixel.G -eq 120 -and $pixel.B -eq 212) {
                    $focusRowPixels++
                }
            }
            if ($focusRowPixels -ge $minimumFocusRowPixels) {
                $focusTop = [Math]::Min($focusTop, $y)
                $focusBottom = [Math]::Max($focusBottom, $y)
            }
        }
        if ($focusTop -ne [int]::MaxValue -and $focusBottom -gt $focusTop) {
            $top = $focusTop
            $bottom = $focusBottom
        }
        $controlHeight = $bottom - $top + 1
        $borderExclusion = [Math]::Max(2, [int][Math]::Floor($controlHeight * 0.08))
        # Only sample the horizontal span exercised by the real mouse drag. This excludes
        # the focused border and search clear icon, which intentionally share #0078D4.
        $dragLeft = [int][Math]::Floor([Math]::Min($SelectionStartOffsetX, $SelectionEndOffsetX) * $scaleX)
        $dragRight = [int][Math]::Ceiling([Math]::Max($SelectionStartOffsetX, $SelectionEndOffsetX) * $scaleX)
        $scanLeft = [Math]::Max($left + $borderExclusion + 2, $left + $dragLeft - 8)
        $scanRight = [Math]::Min($right - $borderExclusion - 2, $left + $dragRight + 8)
        $selectionMinX = [int]::MaxValue
        $selectionMaxX = [int]::MinValue
        $selectionMinY = [int]::MaxValue
        $selectionMaxY = [int]::MinValue
        $highlightPixels = 0

        for ($y = $top + $borderExclusion; $y -le $bottom - $borderExclusion; $y++) {
            $rowPixels = 0
            $rowMinX = [int]::MaxValue
            $rowMaxX = [int]::MinValue
            for ($x = $scanLeft; $x -le $scanRight; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                if ($pixel.R -eq 0 -and $pixel.G -eq 120 -and $pixel.B -eq 212) {
                    $rowPixels++
                    $rowMinX = [Math]::Min($rowMinX, $x)
                    $rowMaxX = [Math]::Max($rowMaxX, $x)
                }
            }
            # The focused control border is excluded vertically and horizontally; the caret is
            # only one or two pixels wide, so a wider run in the dragged span is the selection.
            if ($rowPixels -ge 8) {
                $highlightPixels += $rowPixels
                $selectionMinX = [Math]::Min($selectionMinX, $rowMinX)
                $selectionMaxX = [Math]::Max($selectionMaxX, $rowMaxX)
                $selectionMinY = [Math]::Min($selectionMinY, $y)
                $selectionMaxY = [Math]::Max($selectionMaxY, $y)
            }
        }
        if ($highlightPixels -lt 24 -or $selectionMinY -eq [int]::MaxValue) {
            throw "$Label partial selection highlight was not measurable: pixels=$highlightPixels"
        }

        $selectionHeight = $selectionMaxY - $selectionMinY + 1
        $topInset = $selectionMinY - $top
        $bottomInset = $bottom - $selectionMaxY
        $insetDifference = [Math]::Abs($topInset - $bottomInset)
        if ($insetDifference -gt 1) {
            throw "$Label selection margins are asymmetric: top=$topInset bottom=$bottomInset difference=$insetDifference control=$left,$top..$right,$bottom selection=$selectionMinX,$selectionMinY..$selectionMaxX,$selectionMaxY scale=$scaleX,$scaleY"
        }
        $minimumNearFullHeight = [int][Math]::Floor($controlHeight * 0.72)
        if ($selectionHeight -lt $minimumNearFullHeight) {
            throw "$Label selection is not near full height: selection=$selectionHeight control=$controlHeight minimum=$minimumNearFullHeight"
        }

        $selectedGlyphPixels = 0
        $unselectedDarkPixels = 0
        for ($y = $selectionMinY; $y -le $selectionMaxY; $y++) {
            for ($x = $scanLeft; $x -le $scanRight; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                if ($x -ge $selectionMinX -and $x -le $selectionMaxX) {
                    if ($pixel.R -gt 180 -and $pixel.G -gt 180 -and $pixel.B -gt 180) {
                        $selectedGlyphPixels++
                    }
                } elseif ($pixel.R -lt 100 -and $pixel.G -lt 100 -and $pixel.B -lt 100) {
                    $unselectedDarkPixels++
                }
            }
        }
        if ($selectedGlyphPixels -lt 4) {
            throw "$Label selected glyphs were not visible inside the highlight: pixels=$selectedGlyphPixels"
        }
        if ($unselectedDarkPixels -lt 4) {
            throw "$Label unselected glyphs did not retain their dark foreground: pixels=$unselectedDarkPixels"
        }

        [pscustomobject][ordered]@{
            label = $Label
            capture = [IO.Path]::GetFileName($CapturePath)
            control_height = $controlHeight
            selection_height = $selectionHeight
            top_inset = $topInset
            bottom_inset = $bottomInset
            inset_difference = $insetDifference
            highlight_pixels = $highlightPixels
            selected_glyph_pixels = $selectedGlyphPixels
            unselected_dark_pixels = $unselectedDarkPixels
        }
    } finally {
        $bitmap.Dispose()
    }
}

function Measure-EditorCaretGeometry(
    [Windows.Automation.AutomationElement]$Editor,
    [string]$CapturePath,
    [double]$ExpectedOffsetX
) {
    $bounds = $Editor.Current.BoundingRectangle
    $bestHeight = 0
    $bestTop = 0
    $controlHeight = 0
    $glyphTop = [int]::MaxValue
    $glyphBottom = [int]::MinValue
    foreach ($attempt in 1..5) {
        Save-UitestScreenshot -Root $context.Root -Path $CapturePath
        $bitmap = [Drawing.Bitmap]::FromFile($CapturePath)
        try {
            $scaleX = $bitmap.Width / $window.Width
            $scaleY = $bitmap.Height / $window.Height
            $left = [int][Math]::Floor(($bounds.Left - $window.Left) * $scaleX)
            $top = [int][Math]::Floor(($bounds.Top - $window.Top) * $scaleY)
            $right = [int][Math]::Ceiling(($bounds.Right - $window.Left) * $scaleX) - 1
            $bottom = [int][Math]::Ceiling(($bounds.Bottom - $window.Top) * $scaleY) - 1
            $controlHeight = $bottom - $top + 1
            $scanCenter = $left + [int][Math]::Round($ExpectedOffsetX * $scaleX)
            $scanLeft = [Math]::Max($left + 4, $scanCenter - 20)
            $scanRight = [Math]::Min($right - 4, $scanCenter + 20)
            for ($x = $scanLeft; $x -le $scanRight; $x++) {
                $run = 0
                $runTop = 0
                for ($y = $top + 3; $y -le $bottom - 3; $y++) {
                    $pixel = $bitmap.GetPixel($x, $y)
                    if ($pixel.R -eq 0 -and $pixel.G -eq 120 -and $pixel.B -eq 212) {
                        if ($run -eq 0) { $runTop = $y }
                        $run++
                        if ($run -gt $bestHeight) {
                            $bestHeight = $run
                            $bestTop = $runTop
                        }
                    } else {
                        $run = 0
                    }
                }
            }
            for ($y = $top + 3; $y -le $bottom - 3; $y++) {
                $darkPixels = 0
                for ($x = $left + 8; $x -le $right - 8; $x++) {
                    $pixel = $bitmap.GetPixel($x, $y)
                    if ($pixel.R -lt 80 -and $pixel.G -lt 80 -and $pixel.B -lt 80) {
                        $darkPixels++
                    }
                }
                if ($darkPixels -ge 3) {
                    $glyphTop = [Math]::Min($glyphTop, $y)
                    $glyphBottom = [Math]::Max($glyphBottom, $y)
                }
            }
        } finally {
            $bitmap.Dispose()
        }
        if ($bestHeight -ge 6) { break }
        Start-Sleep -Milliseconds 120
    }
    if ($bestHeight -lt 6) { throw "address caret was not measurable: height=$bestHeight" }
    if ($glyphTop -eq [int]::MaxValue -or $glyphBottom -le $glyphTop) {
        throw 'address glyph bounds were not measurable'
    }
    $maximumHeight = [int][Math]::Ceiling($controlHeight * 0.48)
    if ($bestHeight -gt $maximumHeight) {
        throw "address caret still uses the line box height: caret=$bestHeight control=$controlHeight maximum=$maximumHeight"
    }
    $caretBottom = $bestTop + $bestHeight - 1
    $topDifference = [Math]::Abs($bestTop - $glyphTop)
    $bottomDifference = [Math]::Abs($caretBottom - $glyphBottom)
    if ($topDifference -gt 3 -or $bottomDifference -gt 4) {
        throw "address caret is not aligned to the rendered glyph bounds: caret=$bestTop..$caretBottom glyph=$glyphTop..$glyphBottom topDifference=$topDifference bottomDifference=$bottomDifference"
    }
    [pscustomobject][ordered]@{
        capture = [IO.Path]::GetFileName($CapturePath)
        caret_height = $bestHeight
        control_height = $controlHeight
        maximum_allowed_height = $maximumHeight
        caret_top = $bestTop
        caret_bottom = $caretBottom
        glyph_top = $glyphTop
        glyph_bottom = $glyphBottom
        top_difference = $topDifference
        bottom_difference = $bottomDifference
    }
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0003)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $window = $context.Root.Current.BoundingRectangle

    Send-UitestKey -Key 0x1B -DelayMilliseconds 120
    Send-UitestKey -Key 0x4C -Modifiers @(0x11) -DelayMilliseconds 300
    try {
        $address = Find-TopEditor -Description 'address editor after Ctrl+L' -Predicate {
            param($element)
            $bounds = $element.Current.BoundingRectangle
            $bounds.Top -lt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + $window.Width * 0.58)
        }
    } catch {
        $addressSurface = Find-UitestElement -Root $context.Root -Description 'browsing address field fallback' -TimeoutSeconds 4 -Predicate {
            param($element)
            $bounds = $element.Current.BoundingRectangle
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Document -and
                $element.Current.Name -like 'Address: *' -and
                $bounds.Top -lt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + $window.Width * 0.58)
        }
        $surfaceBounds = $addressSurface.Current.BoundingRectangle
        [void][RustExplorerUitest.Native]::SetCursorPosDpiAware(
            [int]($surfaceBounds.Right - 14),
            [int]($surfaceBounds.Top + $surfaceBounds.Height / 2))
        [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 250
        $address = Find-TopEditor -Description 'clicked address editor' -Predicate {
            param($element)
            $bounds = $element.Current.BoundingRectangle
            $bounds.Top -lt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + $window.Width * 0.58)
        }
    }
    $address.SetFocus()
    Set-EditorTextWithPasteRetry -Editor $address -Text 'C:\portable\alpha'
    Click-EditorAt -Editor $address -OffsetX 72
    $addressCaretCapture = Join-Path $output 'address-caret.png'
    $addressCaretMetrics = Measure-EditorCaretGeometry -Editor $address -CapturePath $addressCaretCapture -ExpectedOffsetX 72
    Send-UitestKey -Key 0x58 -Modifiers @(0x10) -DelayMilliseconds 180
    $addressAfter = Get-EditorValue $address
    if ($addressAfter.Length -ne 'C:\portable\alpha'.Length + 1 -or
        $addressAfter.Replace('X', '').Replace('x', '') -cne 'C:\portable\alpha') {
        throw "address pointer click recreated or replaced the editor: $addressAfter"
    }

    # Prove real pointer drag selection, not only caret placement or Ctrl+A. Replacing the
    # dragged range gives a clipboard-independent result oracle and avoids racing the app's
    # own Win32 clipboard ownership on slower test hosts.
    $address.SetFocus()
    Set-EditorTextWithPasteRetry -Editor $address -Text 'C:\portable\alpha'
    Start-Sleep -Milliseconds 550
    Drag-EditorSelection -Editor $address -StartOffsetX 72 -EndOffsetX 130
    $addressSelectionCapture = Join-Path $output 'address-partial-selection.png'
    $addressSelectionMetrics = Measure-EditorSelectionGeometry -Editor $address -CapturePath $addressSelectionCapture -Label 'address' -SelectionStartOffsetX 72 -SelectionEndOffsetX 130
    $addressHighlightPixels = $addressSelectionMetrics.highlight_pixels
    $selectionTopInset = $addressSelectionMetrics.top_inset
    $selectionBottomInset = $addressSelectionMetrics.bottom_inset
    $selectionInsetDifference = $addressSelectionMetrics.inset_difference
    $unselectedDarkPixels = $addressSelectionMetrics.unselected_dark_pixels
    Send-UitestKey -Key 0x59 -Modifiers @(0x10) -DelayMilliseconds 180
    $addressAfterDrag = Get-EditorValue $address
    $addressDragRemovedCharacterCount = 'C:\portable\alpha'.Length + 1 - $addressAfterDrag.Length
    if ($addressDragRemovedCharacterCount -lt 2 -or
        $addressDragRemovedCharacterCount -ge 'C:\portable\alpha'.Length -or
        $addressAfterDrag.IndexOf('Y', [StringComparison]::OrdinalIgnoreCase) -le 0 -or
        -not $addressAfterDrag.EndsWith('\alpha', [StringComparison]::Ordinal)) {
        throw "address pointer drag did not replace an interior text range: value='$addressAfterDrag' removed=$addressDragRemovedCharacterCount"
    }

    Send-UitestKey -Key 0x1B -DelayMilliseconds 180
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
            $bounds = $_.Current.BoundingRectangle
            if ($bounds.Top -lt ($window.Top + 180) -and $bounds.Left -gt ($window.Right - 700)) {
                [pscustomobject]@{
                    name = $_.Current.Name
                    type = $_.Current.ControlType.ProgrammaticName
                    left = $bounds.Left
                    top = $bounds.Top
                    width = $bounds.Width
                    height = $bounds.Height
                }
            }
        }) | ConvertTo-Json -Depth 3 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'top-right-tree.json')
    $searchSurface = Find-UitestElement -Root $context.Root -Description 'search field surface' -TimeoutSeconds 5 -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $bounds.Top -lt ($window.Top + 180) -and
        $bounds.Left -gt ($window.Right - 700) -and
            $element.Current.ControlType -in @(
                [Windows.Automation.ControlType]::Document,
                [Windows.Automation.ControlType]::Edit)
    }
    Click-EditorAt -Editor $searchSurface -OffsetX 58
    $search = Find-TopEditor -Description 'search editor' -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $bounds.Top -lt ($window.Top + 180) -and
            $element.Current.Name -like '*;*'
    }
    Click-EditorAt -Editor $search -OffsetX 58
    Set-EditorTextWithPasteRetry -Editor $search -Text 'abcdefghij'
    Click-EditorAt -Editor $search -OffsetX 58
    Send-UitestKey -Key 0x58 -Modifiers @(0x10) -DelayMilliseconds 180
    $searchAfter = Get-EditorValue $search
    $markerIndex = $searchAfter.IndexOf('X', [StringComparison]::OrdinalIgnoreCase)
    if ($searchAfter.Length -ne 11 -or $markerIndex -lt 0 -or $markerIndex -gt 3) {
        throw "search pointer coordinate did not account for icon padding: value=$searchAfter marker=$markerIndex"
    }

    Set-EditorTextWithPasteRetry -Editor $search -Text 'abcdefghij'
    Drag-EditorSelection -Editor $search -StartOffsetX 72 -EndOffsetX 130
    $searchCapturePath = Join-Path $output 'search-partial-selection.png'
    $searchSelectionMetrics = Measure-EditorSelectionGeometry -Editor $search -CapturePath $searchCapturePath -Label 'search' -SelectionStartOffsetX 72 -SelectionEndOffsetX 130

    Send-UitestKey -Key 0x1B -DelayMilliseconds 250
    $fileItem = Find-UitestFileItem -Root $context.Root -Name 'pointer-input-sentinel.txt'
    Invoke-UitestClick -Element $fileItem
    Send-UitestKey -Key 0x71 -DelayMilliseconds 250
    $rename = Find-UitestElement -Root $context.Root -Description 'inline rename editor' -TimeoutSeconds 8 -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $element.Current.Name -like 'Rename*'
    }
    $rename.SetFocus()
    Drag-EditorSelection -Editor $rename -StartOffsetX 24 -EndOffsetX 82
    $renameCapturePath = Join-Path $output 'rename-partial-selection.png'
    $renameSelectionMetrics = Measure-EditorSelectionGeometry -Editor $rename -CapturePath $renameCapturePath -Label 'rename' -SelectionStartOffsetX 24 -SelectionEndOffsetX 82
    Send-UitestKey -Key 0x1B -DelayMilliseconds 180

    [ordered]@{
        schema = 'superexplorer.editable-pointer-input.v2'
        genuine_pointer_input = $true
        address_editor_entity_preserved = $true
        address_caret_geometry = $addressCaretMetrics
        address_value = $addressAfter
        address_pointer_drag_selection = $true
        address_drag_removed_character_count = $addressDragRemovedCharacterCount
        address_drag_replacement_value = $addressAfterDrag
        address_partial_selection_highlight_pixels = $addressHighlightPixels
        address_partial_selection_top_inset = $selectionTopInset
        address_partial_selection_bottom_inset = $selectionBottomInset
        address_partial_selection_inset_difference = $selectionInsetDifference
        address_unselected_dark_pixels = $unselectedDarkPixels
        search_padding_hit_test = $true
        search_value = $searchAfter
        search_marker_index = $markerIndex
        selection_geometry = @(
            $addressSelectionMetrics
            $searchSelectionMetrics
            $renameSelectionMetrics
        )
        opaque_highlight_pixels = $searchSelectionMetrics.highlight_pixels
        highlight_rgb = '#0078D4'
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Editable pointer input smoke passed: $OutputDirectory"
