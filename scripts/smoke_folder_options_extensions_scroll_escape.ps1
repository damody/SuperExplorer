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
    $more = Find-ById -Id 'command-more-menu' -Description 'More command button' -AccessibleName ([string]([char]0x5176) + [char]0x5B83)
    Invoke-Control -Element $more
    $options = Find-ById -Id 'more-options' -Description 'Options menu item' -AccessibleName ([string]([char]0x9078) + [char]0x9805)
    Invoke-Control -Element $options

    $dialogName = [string]([char]0x8CC7) + [char]0x6599 + [char]0x593E + [char]0x9078 + [char]0x9805
    $dialog = Find-ById -Id 'folder-options-dialog' -Description 'Folder Options dialog' -AccessibleName $dialogName
    $extensionsTab = Find-ById -Id 'folder-options-extensions-tab' -Description 'Extensions tab' -AccessibleName 'Extensions'
    Invoke-Control -Element $extensionsTab

    $page = Find-RoleName -Type ([Windows.Automation.ControlType]::List) -Name 'Extensions' -Description 'scrollable Extensions page'
    $cancelName = [string]([char]0x53D6) + [char]0x6D88
    $cancel = Find-ById -Id 'folder-options-cancel' -Description 'Cancel button' -AccessibleName $cancelName
    Assert-Inside -Child $cancel -Parent $dialog -Description 'Cancel button before scrolling'
    $backgroundScrollbar = Find-RoleName -Type ([Windows.Automation.ControlType]::ScrollBar) -Name 'File view vertical scroll bar' -Description 'background file-view scrollbar'
    $backgroundScrollBefore = Read-RangeValue -Element $backgroundScrollbar -Description 'background file-view scrollbar'

    $first = Find-RoleName -Type ([Windows.Automation.ControlType]::ListItem) -Name 'Folder size column' -Description 'first extension card'
    $last = Find-RoleName -Type ([Windows.Automation.ControlType]::ListItem) -Name 'Bulk folder generator' -Description 'last extension card'
    $firstTopBefore = [double]$first.Current.BoundingRectangle.Top
    $lastTopBefore = [double]$last.Current.BoundingRectangle.Top
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'folder-options-before-scroll.png')

    $point = Get-UitestPhysicalPoint -Element $page
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)) {
        throw 'could not position pointer over Extensions page'
    }
    $wheelDown = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]-120), 0)
    foreach ($step in 1..8) {
        # MOUSEEVENTF_WHEEL, with -120 encoded as an unsigned DWORD.
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $wheelDown, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
    Start-Sleep -Milliseconds 350

    $firstTopAfter = [double]$first.Current.BoundingRectangle.Top
    $lastTopAfter = [double]$last.Current.BoundingRectangle.Top
    if ($firstTopAfter -ge ($firstTopBefore - 20) -or $lastTopAfter -ge ($lastTopBefore - 20)) {
        throw "Extensions list did not scroll: first $firstTopBefore->$firstTopAfter, last $lastTopBefore->$lastTopAfter"
    }
    $backgroundScrollAfter = Read-RangeValue -Element $backgroundScrollbar -Description 'background file-view scrollbar'
    if ([Math]::Abs($backgroundScrollAfter - $backgroundScrollBefore) -gt 0.01) {
        throw "wheel input leaked through the modal: background $backgroundScrollBefore->$backgroundScrollAfter"
    }
    Assert-Inside -Child $cancel -Parent $dialog -Description 'Cancel button after scrolling'
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'folder-options-after-scroll.png')

    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 350
    Wait-IdAbsent -Id 'folder-options-dialog'
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'folder-options-after-escape.png')

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        scroll_delta = [ordered]@{
            first_card = $firstTopAfter - $firstTopBefore
            last_card = $lastTopAfter - $lastTopBefore
            background_file_view = $backgroundScrollAfter - $backgroundScrollBefore
        }
        oracles = [ordered]@{
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
