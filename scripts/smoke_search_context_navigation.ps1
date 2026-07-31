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
New-Item -ItemType Directory -Force -Path (Join-Path $fixture 'nested') | Out-Null
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'Needle-Alpha.txt') -Value 'alpha'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'Needle-Beta.txt') -Value 'beta'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'control.dat') -Value 'control'
$context = $null

function Find-SearchEditor {
    $window = $context.Root.Current.BoundingRectangle
    Find-UitestElement -Root $context.Root -Description 'top-right search editor' -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $bounds.Top -lt ($window.Top + 180) -and
            $bounds.Left -gt ($window.Left + $window.Width * 0.58)
    }
}

function Invoke-RightClick([Windows.Automation.AutomationElement]$Element) {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 100
    $point = Get-UitestPhysicalPoint -Element $Element -HorizontalOffset 30
    if (-not [RustExplorerUitest.Native]::SetPhysicalCursorPos($point.X, $point.Y)) {
        throw "physical right-click cursor positioning failed at ($($point.X),$($point.Y))"
    }
    [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 220
}

function Get-VisibleMenus {
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Menu
        )
    ) | Where-Object {
        try {
            $_.Current.BoundingRectangle.Width -gt 0
        } catch {
            # AccessKit replaces the overlay subtree atomically. A node returned by
            # FindAll can therefore expire before its bounds are read; retry the live tree.
            $false
        }
    })
}

function Wait-VisibleMenu([bool]$Exists, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $menus = @(Get-VisibleMenus)
        if (($menus.Count -gt 0) -eq $Exists) { return $menus }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "visible context menu existence did not become $Exists"
}

function Find-DetailsColumnMenuItem([string]$Name) {
    Find-UitestElement -Root $context.Root -Description "Details column menu item '$Name'" -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::MenuItem -and
            ($element.Current.Name -eq $Name -or $element.Current.Name -like "$Name, *") -and
            $element.Current.BoundingRectangle.Width -gt 0
    }
}

function Test-VisibleNamedMenuItem([string]$Name) {
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::MenuItem
        )
    ) | Where-Object {
        try {
            ($_.Current.Name -eq $Name -or $_.Current.Name -like "$Name, *") -and
                $_.Current.BoundingRectangle.Width -gt 0
        } catch { $false }
    }).Count -gt 0
}

function Wait-DetailsColumnMenuItemSelected([string]$Name, [bool]$Selected, [int]$TimeoutSeconds = 5) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $item = Find-DetailsColumnMenuItem -Name $Name
            $expectedSuffix = if ($Selected) { ', checked' } else { ', unchecked' }
            if ($item.Current.Name.EndsWith($expectedSuffix, [StringComparison]::Ordinal)) { return }
        } catch { }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Details column menu item '$Name' selected state did not become $Selected"
}

function Get-NativePopupMenus {
    $handles = [Collections.Generic.List[IntPtr]]::new()
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
            $className = [Text.StringBuilder]::new(64)
            [void][RustExplorerUitest.Native]::GetClassName($hwnd, $className, $className.Capacity)
            if ($className.ToString() -eq '#32768') { $handles.Add($hwnd) }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    @($handles)
}

function Wait-NativePopup([IntPtr[]]$Before, [bool]$Exists, [int]$TimeoutSeconds = 10) {
    $beforeSet = [Collections.Generic.HashSet[IntPtr]]::new($Before)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $current = @(Get-NativePopupMenus | Where-Object { -not $beforeSet.Contains($_) })
        if (($current.Count -gt 0) -eq $Exists) { return $current }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "native selected-item popup existence did not become $Exists"
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    Find-UitestFileItem -Root $context.Root -Name 'control.dat' | Out-Null

    $search = Find-SearchEditor
    Invoke-UitestClick -Element $search
    Send-UitestKey -Key 0x41 -Modifiers @(0x11)
    Set-UitestClipboardText -Text 'Needle'
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 250
    Send-UitestKey -Key 0x0D -DelayMilliseconds 700
    Find-UitestFileItem -Root $context.Root -Name 'Needle-Alpha.txt' | Out-Null
    Find-UitestFileItem -Root $context.Root -Name 'Needle-Beta.txt' | Out-Null
    if (@(Get-UitestFileItems -Root $context.Root | Where-Object { $_.Current.Name -eq 'control.dat' }).Count -ne 0) {
        throw 'search retained the non-matching control item'
    }
    # Removing the query text itself must share the Clear/Escape cancellation path and restore the
    # directory snapshot; this is the user-visible regression that an Escape-only oracle missed.
    $search.SetFocus()
    Send-UitestKey -Key 0x41 -Modifiers @(0x11)
    Send-UitestKey -Key 0x2E -DelayMilliseconds 500
    Find-UitestFileItem -Root $context.Root -Name 'control.dat' | Out-Null

    $header = Find-UitestElement -Root $context.Root -Description 'details header' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            ($element.Current.Name -like '*sorted*' -or $element.Current.Name -like 'Sort by *') -and
            $element.Current.BoundingRectangle.Width -gt 20
    }
    Invoke-RightClick -Element $header
    [void](Wait-VisibleMenu -Exists $true)

    # The complete Details column list must be available immediately. There is no draft dialog:
    # every optional-column click updates the active tab while the menu stays available for more.
    $columnChoices = @('Name', 'Date modified', 'Type', 'Size', 'Date created', 'Authors', 'Tags', 'Title')
    foreach ($choice in $columnChoices) { [void](Find-DetailsColumnMenuItem -Name $choice) }
    foreach ($obsolete in @('Other...', 'OK', 'Cancel')) {
        if (Test-VisibleNamedMenuItem -Name $obsolete) {
            throw "obsolete Details column command is still visible: $obsolete"
        }
    }
    Wait-DetailsColumnMenuItemSelected -Name 'Size' -Selected $true
    $sizeColumn = Find-DetailsColumnMenuItem -Name 'Size'
    Invoke-UitestClick -Element $sizeColumn
    Wait-DetailsColumnMenuItemSelected -Name 'Size' -Selected $false
    [void](Wait-VisibleMenu -Exists $true)
    $sizeColumn = Find-DetailsColumnMenuItem -Name 'Size'
    Invoke-UitestClick -Element $sizeColumn
    Wait-DetailsColumnMenuItemSelected -Name 'Size' -Selected $true
    [void](Wait-VisibleMenu -Exists $true)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 250
    [void](Wait-VisibleMenu -Exists $false)

    Invoke-RightClick -Element $header
    [void](Wait-VisibleMenu -Exists $true)
    $window = $context.Root.Current.BoundingRectangle
    if (-not [RustExplorerUitest.Native]::SetPhysicalCursorPos([int]($window.Right - 80), [int]($window.Bottom - 80))) {
        throw 'physical outside-click cursor positioning failed'
    }
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    [void](Wait-VisibleMenu -Exists $false)

    # Exercise the real selected-item Shell popup hosted by the isolated worker, not only the
    # app-owned Details header popup. Both Escape and an activation outside the popup must return
    # Cancelled without invoking a verb or changing the selected file.
    # Changing selection on right-button down rerenders the row. GPUI can surface the matching
    # release as mouse-up-out, so prove that an unselected item still opens its Shell menu rather
    # than only exercising the easier already-selected path.
    $control = Find-UitestFileItem -Root $context.Root -Name 'control.dat'
    Invoke-UitestClick -Element $control
    $unselected = Find-UitestFileItem -Root $context.Root -Name 'Needle-Alpha.txt'
    $nativeBefore = @(Get-NativePopupMenus)
    Invoke-RightClick -Element $unselected
    [void](Wait-NativePopup -Before $nativeBefore -Exists $true)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 350
    [void](Wait-NativePopup -Before $nativeBefore -Exists $false)

    $control = Find-UitestFileItem -Root $context.Root -Name 'control.dat'
    Invoke-UitestClick -Element $control
    $nativeBefore = @(Get-NativePopupMenus)
    Invoke-RightClick -Element $control
    [void](Wait-NativePopup -Before $nativeBefore -Exists $true)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 350
    [void](Wait-NativePopup -Before $nativeBefore -Exists $false)
    if (-not (Test-Path -LiteralPath (Join-Path $fixture 'control.dat') -PathType Leaf)) {
        throw 'Escape from selected-item Shell menu invoked a destructive verb'
    }

    Invoke-RightClick -Element $control
    [void](Wait-NativePopup -Before $nativeBefore -Exists $true)
    [void][RustExplorerUitest.Native]::SetCursorPos([int]($window.Right - 40), [int]($window.Bottom - 30))
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    [void](Wait-NativePopup -Before $nativeBefore -Exists $false)
    if (-not (Test-Path -LiteralPath (Join-Path $fixture 'control.dat') -PathType Leaf)) {
        throw 'outside click from selected-item Shell menu invoked a destructive verb'
    }
    $focusDeadline = [DateTime]::UtcNow.AddSeconds(3)
    while ([RustExplorerUitest.Native]::GetForegroundWindow() -ne $context.Hwnd -and
        [DateTime]::UtcNow -lt $focusDeadline) {
        Start-Sleep -Milliseconds 100
    }
    if ([RustExplorerUitest.Native]::GetForegroundWindow() -ne $context.Hwnd) {
        throw 'selected-item Shell menu did not restore Explorer foreground focus'
    }

    # This PC is expanded by default. Shell ancestry can return the same volume roots with short
    # labels (C:) while the stable rows use localized labels (... (C:)); each canonical drive must
    # still own exactly one navigation row.
    Start-Sleep -Milliseconds 700
    $navigationButtons = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button
        )
    ) | Where-Object {
        $bounds = $_.Current.BoundingRectangle
        $bounds.Left -lt ($window.Left + 420) -and $bounds.Top -gt ($window.Top + 120)
    })
    $volumeRows = @{}
    foreach ($button in $navigationButtons) {
        $name = $button.Current.Name
        $match = [regex]::Match($name, '^(?:.*\(([A-Z]):\)|([A-Z]):)$')
        if ($match.Success) {
            $letter = if ($match.Groups[1].Success) { $match.Groups[1].Value } else { $match.Groups[2].Value }
            if (-not $volumeRows.ContainsKey($letter)) { $volumeRows[$letter] = @() }
            $volumeRows[$letter] += $name
        }
    }
    foreach ($letter in $volumeRows.Keys) {
        if ($volumeRows[$letter].Count -ne 1) {
            throw "navigation volume $letter appears $($volumeRows[$letter].Count) times: $($volumeRows[$letter] -join ', ')"
        }
    }
    if ($volumeRows.Count -eq 0) { throw 'navigation pane exposed no canonical drive rows' }

    $drive = Find-UitestElement -Root $context.Root -Description 'expandable drive navigation row' -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -match '\([A-Z]:\)' -and
            $bounds.Left -lt ($window.Left + 420) -and $bounds.Top -gt ($window.Top + 150)
    }
    $driveBounds = $drive.Current.BoundingRectangle
    $expand = Find-UitestElement -Root $context.Root -Description 'drive Expand control' -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq 'Expand' -and
            [Math]::Abs(($bounds.Top + $bounds.Height / 2) - ($driveBounds.Top + $driveBounds.Height / 2)) -lt 8 -and
            $bounds.Left -ge ($driveBounds.Left - 4) -and $bounds.Left -lt ($driveBounds.Left + 48)
    }
    if ($null -eq $expand) { throw 'drive row has no independent Expand control' }
    Invoke-UitestClick -Element $expand
    Start-Sleep -Milliseconds 700
    $collapse = Find-UitestElement -Root $context.Root -Description 'drive Collapse control' -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq 'Collapse' -and
            [Math]::Abs(($bounds.Top + $bounds.Height / 2) - ($driveBounds.Top + $driveBounds.Height / 2)) -lt 8 -and
            $bounds.Left -ge ($driveBounds.Left - 4) -and $bounds.Left -lt ($driveBounds.Left + 48)
    }
    if ($null -eq $collapse) { throw 'expanded drive did not expose Collapse state' }

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'search-context-navigation.png')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        search_submit_and_results = $true
        search_empty_text_restores_directory = $true
        context_escape_dismiss = $true
        context_outside_click_dismiss = $true
        details_column_complete_menu = $true
        details_column_immediate_show_hide = $true
        selected_item_shell_menu_escape_cancel = $true
        selected_item_shell_menu_outside_cancel = $true
        selected_item_shell_menu_focus_restored = $true
        navigation_expand_interaction = $true
        navigation_volume_roots_unique = $true
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    $resolvedFixture = [IO.Path]::GetFullPath($fixture)
    $ownedPrefix = $output.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedFixture.StartsWith($ownedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to remove fixture outside evidence directory: $resolvedFixture"
    }
    if (Test-Path -LiteralPath $resolvedFixture) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}

Write-Output "Search, context-menu, and navigation smoke passed: $OutputDirectory"
