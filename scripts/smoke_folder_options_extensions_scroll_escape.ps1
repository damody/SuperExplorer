param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$context = $null

function Find-ById([string]$Id, [string]$Description, [string]$AccessibleName = '', [int]$TimeoutSeconds = 10) {
    Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        $element.Current.AutomationId -eq $Id -or
            ($AccessibleName -and $element.Current.Name -eq $AccessibleName)
    }
}

function Find-OptionalById([string]$Id) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
    $context.Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Get-FolderOptionsWindows {
    $processCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ProcessIdProperty, $context.Process.Id)
    @([Windows.Automation.AutomationElement]::RootElement.FindAll(
        [Windows.Automation.TreeScope]::Children, $processCondition) | Where-Object {
        $_.Current.NativeWindowHandle -ne 0 -and
            $_.Current.NativeWindowHandle -ne $context.Hwnd -and
            ($_.Current.Name -eq $script:dialogName -or $null -ne $_.FindFirst(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.PropertyCondition]::new(
                    [Windows.Automation.AutomationElement]::AutomationIdProperty,
                    'folder-options-window')))
    })
}

function Wait-FolderOptionsWindow([int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $windows = @(Get-FolderOptionsWindows)
        if ($windows.Count -eq 1) { return $windows[0] }
        if ($windows.Count -gt 1) { throw "multiple Folder Options native windows were created: $($windows.Count)" }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Folder Options native window did not appear'
}

function Find-RoleName([Windows.Automation.ControlType]$Type, [string]$Name, [string]$Description) {
    Find-UitestElement -Root $context.Root -Description $Description -Predicate {
        param($element)
        $element.Current.ControlType -eq $Type -and $element.Current.Name -eq $Name
    }
}

function Wait-IdAbsent([string]$Id, [int]$TimeoutSeconds = 5) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ($null -eq (Find-OptionalById -Id $Id)) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element remained present: $Id"
}

function Assert-Inside([Windows.Automation.AutomationElement]$Child, [Windows.Automation.AutomationElement]$Parent, [string]$Description) {
    $childBounds = $Child.Current.BoundingRectangle
    $parentBounds = $Parent.Current.BoundingRectangle
    if ($childBounds.Width -le 0 -or $childBounds.Height -le 0 -or
        $childBounds.Left -lt $parentBounds.Left -or $childBounds.Right -gt $parentBounds.Right -or
        $childBounds.Top -lt $parentBounds.Top -or $childBounds.Bottom -gt $parentBounds.Bottom) {
        throw "$Description is not fully reachable inside the dialog: child=$childBounds parent=$parentBounds"
    }
}

function Invoke-Control([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
        Start-Sleep -Milliseconds 450
        return
    }
    Invoke-UitestClick -Element $Element
}

function Read-RangeValue([Windows.Automation.AutomationElement]$Element, [string]$Description) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        throw "$Description does not expose RangeValuePattern"
    }
    [double]([Windows.Automation.RangeValuePattern]$pattern).Current.Value
}

function Read-RangeSnapshot([Windows.Automation.AutomationElement]$Element, [string]$Description) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        throw "$Description does not expose RangeValuePattern"
    }
    $current = ([Windows.Automation.RangeValuePattern]$pattern).Current
    [pscustomobject]@{ Value = [double]$current.Value; Minimum = [double]$current.Minimum; Maximum = [double]$current.Maximum }
}

function Send-WindowKey([IntPtr]$Hwnd, [int]$Key, [int]$DelayMilliseconds = 250) {
    if (-not [RustExplorerUitest.Native]::PostMessage($Hwnd, 0x0100, [IntPtr]$Key, [IntPtr]::Zero)) {
        throw "could not post key-down $Key to Folder Options"
    }
    if (-not [RustExplorerUitest.Native]::PostMessage($Hwnd, 0x0101, [IntPtr]$Key, [IntPtr]::Zero)) {
        throw "could not post key-up $Key to Folder Options"
    }
    Start-Sleep -Milliseconds $DelayMilliseconds
}

$env:SUPEREXPLORER_UITEST_OPEN_FOLDER_OPTIONS = 'view'
$context = Start-UitestExplorer -InitialPath $workspace -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

try {
    $mainRoot = $context.Root
    $script:dialogName = [string]([char]0x8CC7) + [char]0x6599 + [char]0x593E + [char]0x9078 + [char]0x9805
    $optionsRoot = Wait-FolderOptionsWindow
    $optionsHwnd = [IntPtr]$optionsRoot.Current.NativeWindowHandle
    $initialOptionsHwnd = $optionsHwnd
    $optionsDpi = [RustExplorerUitest.Native]::GetDpiForWindow($optionsHwnd)
    $optionsScale = [double]$optionsDpi / 96.0
    if ($optionsHwnd -eq $context.Hwnd) { throw 'Folder Options reused the Explorer HWND instead of opening a separate native window' }
    $context.Root = $optionsRoot
    $dialog = Find-ById -Id 'folder-options-dialog' -Description 'Folder Options dialog' -AccessibleName $script:dialogName
    $viewTab = Find-ById -Id 'folder-options-view-tab' -Description 'View tab' -AccessibleName ([string]([char]0x6AA2) + [char]0x8996)
    Invoke-Control -Element $viewTab
    $iconCacheControl = Find-ById -Id 'cache-budget-slider-IconMemory' -Description 'independent icon cache limit' -AccessibleName 'Icon memory limit'
    [void](Find-ById -Id 'cache-budget-slider-ThumbnailMemory' -Description 'independent thumbnail cache limit' -AccessibleName 'Thumbnail memory limit')
    Save-UitestScreenshot -Root $optionsRoot -Path (Join-Path $output 'folder-options-cache-controls.png')
    $viewBounds = $iconCacheControl.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetForegroundWindow($optionsHwnd)
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware(
        [int]($viewBounds.Left + $viewBounds.Width / 2),
        [int]($viewBounds.Top + [Math]::Min($viewBounds.Height / 2, 300)))) { throw 'could not position pointer over View page' }
    $viewWheelDown = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]-120), 0)
    foreach ($step in 1..24) {
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $viewWheelDown, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 25
    }
    Start-Sleep -Milliseconds 500
    [void](Find-ById -Id 'folder-options-cache-usage' -Description 'Host cache usage section' -AccessibleName 'Cache usage')
    # The unavailable MFT pipe has a bounded two-second timeout; allow the same
    # single-flight Host snapshot to publish the remaining cache reporters.
    Start-Sleep -Seconds 3
    Save-UitestScreenshot -Root $optionsRoot -Path (Join-Path $output 'folder-options-cache-telemetry.png')
    foreach ($step in 1..12) {
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $viewWheelDown, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 25
    }
    Start-Sleep -Milliseconds 350
    Save-UitestScreenshot -Root $optionsRoot -Path (Join-Path $output 'folder-options-cache-mft.png')
    $extensionsTab = Find-ById -Id 'folder-options-extensions-tab' -Description 'Extensions tab' -AccessibleName 'Extensions'
    Invoke-Control -Element $extensionsTab

    $page = Find-ById -Id 'folder-options-page' -Description 'scrollable Extensions page' -AccessibleName 'Extensions'
    $cancelName = [string]([char]0x53D6) + [char]0x6D88
    $cancel = Find-ById -Id 'folder-options-cancel' -Description 'Cancel button' -AccessibleName $cancelName
    $apply = Find-ById -Id 'folder-options-apply' -Description 'Apply button' -AccessibleName ([string]([char]0x5957) + [char]0x7528)
    $ok = Find-ById -Id 'folder-options-ok' -Description 'OK button' -AccessibleName ([string]([char]0x78BA) + [char]0x5B9A)
    Assert-Inside -Child $cancel -Parent $dialog -Description 'Cancel button before scrolling'
    $scrollbarName = [string]([char]0x8CC7) + [char]0x6599 + [char]0x593E + [char]0x9078 + [char]0x9805 + [char]0x5782 + [char]0x76F4 + [char]0x6372 + [char]0x52D5 + [char]0x5217
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'Folder Options right-side scrollbar'
    $context.Root = $mainRoot
    $backgroundScrollbar = Find-OptionalById -Id 'file-view-scrollbar'
    $backgroundScrollBefore = if ($null -ne $backgroundScrollbar) {
        Read-RangeValue -Element $backgroundScrollbar -Description 'background file-view scrollbar'
    } else { $null }
    $backgroundScrollAfter = $backgroundScrollBefore
    $context.Root = $optionsRoot

    $first = Find-RoleName -Type ([Windows.Automation.ControlType]::ListItem) -Name 'Folder size column' -Description 'first extension card'
    $last = Find-RoleName -Type ([Windows.Automation.ControlType]::ListItem) -Name 'Bulk folder generator' -Description 'last extension card'
    $firstTopBefore = [double]$first.Current.BoundingRectangle.Top
    $lastTopBefore = [double]$last.Current.BoundingRectangle.Top
    $optionsScrollBefore = Read-RangeValue -Element $scrollbar -Description 'Folder Options scrollbar before wheel'
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'folder-options-before-scroll.png')

    $pageBounds = $page.Current.BoundingRectangle
    $dialogBounds = $dialog.Current.BoundingRectangle
    $firstBounds = $first.Current.BoundingRectangle
    $visibleTop = [Math]::Max($pageBounds.Top, $dialogBounds.Top + 130)
    $visibleBottom = [Math]::Min($pageBounds.Bottom, $dialogBounds.Bottom - 80)
    $point = [pscustomobject]@{
        X = [int]($firstBounds.Left + ($firstBounds.Width / 2))
        Y = [int]($firstBounds.Top + ($firstBounds.Height / 2))
    }
    [void][RustExplorerUitest.Native]::SetForegroundWindow($optionsHwnd)
    Start-Sleep -Milliseconds 120
    if ($visibleBottom -le $visibleTop -or -not [RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)) {
        throw 'could not position pointer over Extensions page'
    }
    $wheelDown = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]-120), 0)
    foreach ($step in 1..8) {
        # MOUSEEVENTF_WHEEL, with -120 encoded as an unsigned DWORD.
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $wheelDown, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
    Start-Sleep -Milliseconds 350

    $first = Find-RoleName -Type ([Windows.Automation.ControlType]::ListItem) -Name 'Folder size column' -Description 'first extension card after scroll'
    $last = Find-RoleName -Type ([Windows.Automation.ControlType]::ListItem) -Name 'Bulk folder generator' -Description 'last extension card after scroll'
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'Folder Options scrollbar after wheel'
    $firstTopAfter = [double]$first.Current.BoundingRectangle.Top
    $lastTopAfter = [double]$last.Current.BoundingRectangle.Top
    $optionsScrollAfter = Read-RangeValue -Element $scrollbar -Description 'Folder Options scrollbar after wheel'
    if ($optionsScrollAfter -le ($optionsScrollBefore + 0.01) -or
        $firstTopAfter -ge ($firstTopBefore - 20) -or $lastTopAfter -ge ($lastTopBefore - 20)) {
        throw "Extensions list did not scroll: offset $optionsScrollBefore->$optionsScrollAfter, first $firstTopBefore->$firstTopAfter, last $lastTopBefore->$lastTopAfter"
    }
    if ($null -ne $backgroundScrollbar) {
        $backgroundScrollAfter = Read-RangeValue -Element $backgroundScrollbar -Description 'background file-view scrollbar'
        if ([Math]::Abs($backgroundScrollAfter - $backgroundScrollBefore) -gt 0.01) {
            throw "wheel input leaked through the modal: background $backgroundScrollBefore->$backgroundScrollAfter"
        }
    }
    Assert-Inside -Child $cancel -Parent $dialog -Description 'Cancel button after scrolling'
    if ($scrollbar.Current.BoundingRectangle.Right -lt ($dialog.Current.BoundingRectangle.Right - 24)) {
        throw 'Folder Options scrollbar is not positioned at the right edge of the content viewport'
    }
    Save-UitestScreenshot -Root $optionsRoot -Path (Join-Path $output 'folder-options-after-scroll.png')

    # Exercise every keyboard scroll terminal and retain the Extensions offset
    # while another page owns the active ScrollHandle.
    [void][RustExplorerUitest.Native]::SetForegroundWindow($optionsHwnd)
    $optionsRoot.SetFocus()
    Send-WindowKey -Hwnd $optionsHwnd -Key 0x24 # Home
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'Folder Options scrollbar after Home'
    $homeState = Read-RangeSnapshot -Element $scrollbar -Description 'Folder Options Home state'
    if ([Math]::Abs($homeState.Value - $homeState.Minimum) -gt 0.01) { throw "Home did not reach the minimum: $($homeState.Value)" }
    Send-WindowKey -Hwnd $optionsHwnd -Key 0x23 # End
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'Folder Options scrollbar after End'
    $endState = Read-RangeSnapshot -Element $scrollbar -Description 'Folder Options End state'
    if ([Math]::Abs($endState.Value - $endState.Maximum) -gt 0.5) { throw "End did not reach the maximum: $($endState.Value)/$($endState.Maximum)" }
    Send-WindowKey -Hwnd $optionsHwnd -Key 0x21 # Page Up
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'Folder Options scrollbar after Page Up'
    $pageUpValue = Read-RangeValue -Element $scrollbar -Description 'Folder Options Page Up state'
    if ($pageUpValue -ge ($endState.Value - 0.01)) { throw 'Page Up did not reduce the options offset' }
    Send-WindowKey -Hwnd $optionsHwnd -Key 0x22 # Page Down
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'Folder Options scrollbar after Page Down'
    $pageDownValue = Read-RangeValue -Element $scrollbar -Description 'Folder Options Page Down state'
    if ($pageDownValue -le ($pageUpValue + 0.01)) { throw 'Page Down did not increase the options offset' }

    Invoke-Control -Element $viewTab
    $viewScrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'View page scrollbar'
    $viewPageOffset = Read-RangeValue -Element $viewScrollbar -Description 'View page independent offset'
    Invoke-Control -Element $extensionsTab
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'restored Extensions scrollbar'
    $restoredExtensionsOffset = Read-RangeValue -Element $scrollbar -Description 'restored Extensions offset'
    if ([Math]::Abs($restoredExtensionsOffset - $pageDownValue) -gt 0.5) {
        throw "Extensions offset was not restored: $pageDownValue->$restoredExtensionsOffset"
    }

    # Track paging and thumb dragging use physical pointer input against the
    # same right-side scrollbar that exposes the RangeValue oracle.
    $optionsRoot.SetFocus()
    Send-WindowKey -Hwnd $optionsHwnd -Key 0x24
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'scrollbar before track click'
    $trackBounds = $scrollbar.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware([int]($trackBounds.Left + $trackBounds.Width / 2), [int]($trackBounds.Bottom - 12))
    [RustExplorerUitest.Native]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'scrollbar after track click'
    $trackClickValue = Read-RangeValue -Element $scrollbar -Description 'track click offset'
    if ($trackClickValue -le 0.01) { throw 'track click did not page the options viewport' }

    $optionsRoot.SetFocus()
    Send-WindowKey -Hwnd $optionsHwnd -Key 0x24
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'scrollbar before thumb drag'
    $dragBounds = $scrollbar.Current.BoundingRectangle
    $dragX = [int]($dragBounds.Left + $dragBounds.Width / 2)
    $dragStartY = [int]($dragBounds.Top + 12)
    $dragEndY = [int]($dragBounds.Top + $dragBounds.Height / 2)
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($dragX, $dragStartY)
    [RustExplorerUitest.Native]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($dragX, $dragEndY)
    Start-Sleep -Milliseconds 180
    [RustExplorerUitest.Native]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'scrollbar after thumb drag'
    $thumbDragValue = Read-RangeValue -Element $scrollbar -Description 'thumb drag offset'
    if ($thumbDragValue -le 0.01) { throw 'thumb drag did not move the options viewport' }

    # The window must be modeless. Activate the owner, open Options again, and
    # require the same HWND and the existing Extensions draft to remain alive.
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 250
    if ([RustExplorerUitest.Native]::GetForegroundWindow() -ne $context.Hwnd) {
        throw 'Folder Options disabled its owner window'
    }
    $context.Root = $mainRoot
    $more = Find-ById -Id 'command-more-menu' -Description 'More command button while Folder Options is open' -AccessibleName ([string]([char]0x5176) + [char]0x5B83)
    Invoke-Control -Element $more
    $options = Find-ById -Id 'more-options' -Description 'Options menu item while Folder Options is open' -AccessibleName ([string]([char]0x9078) + [char]0x9805)
    Invoke-Control -Element $options
    $sameOptionsRoot = Wait-FolderOptionsWindow
    if ([IntPtr]$sameOptionsRoot.Current.NativeWindowHandle -ne $optionsHwnd) {
        throw 'Opening Folder Options again created a second window instead of activating the existing HWND'
    }
    $context.Root = $sameOptionsRoot
    [void](Find-ById -Id 'folder-options-page' -Description 'preserved Extensions draft' -AccessibleName 'Extensions')

    # Apply keeps the native window alive; OK, Cancel, title-close, and Escape
    # each close it, after which the application controller must create a fresh HWND.
    Invoke-Control -Element $apply
    Start-Sleep -Milliseconds 350
    if (@(Get-FolderOptionsWindows).Count -ne 1) { throw 'Apply closed or duplicated Folder Options' }
    Invoke-Control -Element $ok
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (@(Get-FolderOptionsWindows).Count -ne 0 -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (@(Get-FolderOptionsWindows).Count -ne 0) { throw 'OK did not close Folder Options after persistence succeeded' }

    $openFromOwner = {
        $context.Root = $mainRoot
        [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
        $more = Find-ById -Id 'command-more-menu' -Description 'More command button for reopen' -AccessibleName ([string]([char]0x5176) + [char]0x5B83)
        Invoke-Control -Element $more
        $options = Find-ById -Id 'more-options' -Description 'Options menu item for reopen' -AccessibleName ([string]([char]0x9078) + [char]0x9805)
        Invoke-Control -Element $options
        Wait-FolderOptionsWindow
    }

    $cancelRoot = & $openFromOwner
    $cancelHwnd = [IntPtr]$cancelRoot.Current.NativeWindowHandle
    if ($cancelHwnd -eq $optionsHwnd) { throw 'stale Folder Options HWND was reused after OK' }
    $context.Root = $cancelRoot
    $cancel = Find-ById -Id 'folder-options-cancel' -Description 'Cancel button after reopen' -AccessibleName $cancelName
    Invoke-Control -Element $cancel
    Start-Sleep -Milliseconds 350
    if (@(Get-FolderOptionsWindows).Count -ne 0) { throw 'Cancel did not close Folder Options' }

    $titleRoot = & $openFromOwner
    $titleHwnd = [IntPtr]$titleRoot.Current.NativeWindowHandle
    [void][RustExplorerUitest.Native]::PostMessage($titleHwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 350
    if (@(Get-FolderOptionsWindows).Count -ne 0) { throw 'native title-close did not close Folder Options' }

    $escapeRoot = & $openFromOwner
    $optionsHwnd = [IntPtr]$escapeRoot.Current.NativeWindowHandle
    $context.Root = $escapeRoot

    [void][RustExplorerUitest.Native]::SetForegroundWindow($optionsHwnd)
    Send-WindowKey -Hwnd $optionsHwnd -Key 0x1B -DelayMilliseconds 350
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (@(Get-FolderOptionsWindows).Count -ne 0 -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (@(Get-FolderOptionsWindows).Count -ne 0) { throw 'Escape did not close the Folder Options native window' }
    $context.Root = $mainRoot
    Save-UitestScreenshot -Root $mainRoot -Path (Join-Path $output 'folder-options-after-escape.png')

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        environment = [ordered]@{
            interactive_desktop = $true
            screen_count = [Windows.Forms.Screen]::AllScreens.Count
            dpi = $optionsDpi
            scale_factor = $optionsScale
        }
        window = [ordered]@{
            initial_hwnd = [long]$initialOptionsHwnd
            cancel_reopen_hwnd = [long]$cancelHwnd
            title_close_hwnd = [long]$titleHwnd
            escape_reopen_hwnd = [long]$optionsHwnd
            count = 1
            physical_bounds = [ordered]@{
                left = $dialogBounds.Left
                top = $dialogBounds.Top
                width = $dialogBounds.Width
                height = $dialogBounds.Height
            }
            logical_size = [ordered]@{
                width = $dialogBounds.Width / $optionsScale
                height = $dialogBounds.Height / $optionsScale
            }
        }
        scroll_delta = [ordered]@{
            options_page = $optionsScrollAfter - $optionsScrollBefore
            first_card = $firstTopAfter - $firstTopBefore
            last_card = $lastTopAfter - $lastTopBefore
            background_file_view = $backgroundScrollAfter - $backgroundScrollBefore
        }
        oracles = [ordered]@{
            separate_native_window = $true
            owner_remains_interactive = $true
            repeated_open_reuses_same_hwnd_and_draft = $true
            footer_button_reachable_before_scroll = $true
            genuine_mouse_wheel_scrolls_extension_list = $true
            right_side_scrollbar_visible_after_scroll = $true
            wheel_does_not_scroll_background_folder = $true
            footer_button_stays_fixed_after_scroll = $true
            escape_closes_without_requiring_footer = $true
            apply_stays_open_and_ok_closes = $true
            cancel_closes_and_stale_handle_reopens = $true
            native_title_close_clears_controller = $true
            home_end_page_up_page_down_clamp = $true
            page_offsets_restore_independently = $true
            track_click_pages_options = $true
            thumb_drag_scrolls_options = $true
        }
        scroll_actions = [ordered]@{
            home = $homeState.Value
            end = $endState.Value
            maximum = $endState.Maximum
            page_up = $pageUpValue
            page_down = $pageDownValue
            view_page = $viewPageOffset
            restored_extensions = $restoredExtensionsOffset
            track_click = $trackClickValue
            thumb_drag = $thumbDragValue
        }
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    Remove-Item Env:SUPEREXPLORER_UITEST_OPEN_FOLDER_OPTIONS -ErrorAction SilentlyContinue
}

Write-Output "Folder Options extension scrolling and Escape smoke passed: $OutputDirectory"
