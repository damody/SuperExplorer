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

$context = Start-UitestExplorer -InitialPath $workspace -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

try {
    $mainRoot = $context.Root
    $script:dialogName = [string]([char]0x8CC7) + [char]0x6599 + [char]0x593E + [char]0x9078 + [char]0x9805
    $more = Find-ById -Id 'command-more-menu' -Description 'More command button' -AccessibleName ([string]([char]0x5176) + [char]0x5B83)
    Invoke-Control -Element $more
    $options = Find-ById -Id 'more-options' -Description 'Options menu item' -AccessibleName ([string]([char]0x9078) + [char]0x9805)
    Invoke-Control -Element $options

    $optionsRoot = Wait-FolderOptionsWindow
    $optionsHwnd = [IntPtr]$optionsRoot.Current.NativeWindowHandle
    if ($optionsHwnd -eq $context.Hwnd) { throw 'Folder Options reused the Explorer HWND instead of opening a separate native window' }
    $context.Root = $optionsRoot
    $dialog = Find-ById -Id 'folder-options-dialog' -Description 'Folder Options dialog' -AccessibleName $script:dialogName
    $extensionsTab = Find-ById -Id 'folder-options-extensions-tab' -Description 'Extensions tab' -AccessibleName 'Extensions'
    Invoke-Control -Element $extensionsTab

    $page = Find-ById -Id 'folder-options-page' -Description 'scrollable Extensions page' -AccessibleName 'Extensions'
    $cancelName = [string]([char]0x53D6) + [char]0x6D88
    $cancel = Find-ById -Id 'folder-options-cancel' -Description 'Cancel button' -AccessibleName $cancelName
    Assert-Inside -Child $cancel -Parent $dialog -Description 'Cancel button before scrolling'
    $scrollbarName = [string]([char]0x8CC7) + [char]0x6599 + [char]0x593E + [char]0x9078 + [char]0x9805 + [char]0x5782 + [char]0x76F4 + [char]0x6372 + [char]0x52D5 + [char]0x5217
    $scrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name $scrollbarName -Description 'Folder Options right-side scrollbar'
    $context.Root = $mainRoot
    $backgroundScrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name 'File view vertical scroll bar' -Description 'background file-view scrollbar'
    $backgroundScrollBefore = Read-RangeValue -Element $backgroundScrollbar -Description 'background file-view scrollbar'
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
    $backgroundScrollAfter = Read-RangeValue -Element $backgroundScrollbar -Description 'background file-view scrollbar'
    if ([Math]::Abs($backgroundScrollAfter - $backgroundScrollBefore) -gt 0.01) {
        throw "wheel input leaked through the modal: background $backgroundScrollBefore->$backgroundScrollAfter"
    }
    Assert-Inside -Child $cancel -Parent $dialog -Description 'Cancel button after scrolling'
    if ($scrollbar.Current.BoundingRectangle.Right -lt ($dialog.Current.BoundingRectangle.Right - 24)) {
        throw 'Folder Options scrollbar is not positioned at the right edge of the content viewport'
    }
    Save-UitestScreenshot -Root $optionsRoot -Path (Join-Path $output 'folder-options-after-scroll.png')

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

    [void][RustExplorerUitest.Native]::SetForegroundWindow($optionsHwnd)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 350
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
        }
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Folder Options extension scrolling and Escape smoke passed: $OutputDirectory"
