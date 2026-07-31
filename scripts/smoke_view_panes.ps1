param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [string]$InitialPath = 'D:\test',
    [string]$ExpectedPreviewFile,
    [string]$SecondaryPath,
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('view-pane-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
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
Add-Type -AssemblyName System.Windows.Forms
if (-not ('ViewPaneSmoke.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ViewPaneSmoke {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr LoadKeyboardLayout(string id, uint flags);
        [DllImport("user32.dll")] public static extern IntPtr ActivateKeyboardLayout(IntPtr layout, uint flags);
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, IntPtr processId);
        [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint threadId);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    }
}
'@
}

function Set-EnglishInput([IntPtr]$WindowHandle) {
    $english = [ViewPaneSmoke.Native]::LoadKeyboardLayout('00000409', 1)
    if ($english -eq [IntPtr]::Zero) { throw 'failed to load English (US) keyboard layout' }
    [void][ViewPaneSmoke.Native]::ActivateKeyboardLayout($english, 0)
    if (-not [ViewPaneSmoke.Native]::PostMessage($WindowHandle, 0x0050, [IntPtr]::Zero, $english)) {
        throw 'failed to request English (US) input for explorer window'
    }
    $threadId = [ViewPaneSmoke.Native]::GetWindowThreadProcessId($WindowHandle, [IntPtr]::Zero)
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        $active = [ViewPaneSmoke.Native]::GetKeyboardLayout($threadId).ToInt64() -band 0xFFFF
        if ($active -eq 0x0409) { return }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw ('explorer input language did not switch to English (US); active LANGID=0x{0:X4}' -f $active)
}

function Send-Key([byte]$Key, [byte[]]$Modifiers = @()) {
    foreach ($modifier in $Modifiers) {
        [ViewPaneSmoke.Native]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero)
    }
    [ViewPaneSmoke.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [ViewPaneSmoke.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    for ($index = $Modifiers.Count - 1; $index -ge 0; $index--) {
        [ViewPaneSmoke.Native]::keybd_event($Modifiers[$index], 0, 2, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 180
}

function Wait-Element(
    [Windows.Automation.AutomationElement]$Root,
    [string]$Name,
    [bool]$Present = $true,
    [Windows.Automation.ControlType]$ControlType = $null,
    [Windows.Automation.ControlType]$ExcludedControlType = $null
) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty, $Name)
        $elements = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        $element = $null
        foreach ($candidate in $elements) {
            $matchesRequired = $null -eq $ControlType -or $candidate.Current.ControlType -eq $ControlType
            $matchesExcluded = $null -ne $ExcludedControlType -and $candidate.Current.ControlType -eq $ExcludedControlType
            if ($matchesRequired -and -not $matchesExcluded) {
                $element = $candidate
                break
            }
        }
        if ($Present -and $null -ne $element) { return $element }
        if (-not $Present -and $null -eq $element) { return $null }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $typeSuffix = if ($null -eq $ControlType) { '' } else { " ($($ControlType.ProgrammaticName))" }
    if ($Present) { throw "UIA element not found: $Name$typeSuffix" }
    throw "UIA element remained present: $Name$typeSuffix"
}

function Wait-AutomationId(
    [Windows.Automation.AutomationElement]$Root,
    [string]$AutomationId,
    [bool]$Present = $true
) {
    $deadline = [DateTime]::UtcNow.AddSeconds(12)
    do {
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::AutomationIdProperty, $AutomationId)
        $element = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($Present -and $null -ne $element) { return $element }
        if (-not $Present -and $null -eq $element) { return $null }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($Present) { throw "UIA automation id not found: $AutomationId" }
    throw "UIA automation id remained present: $AutomationId"
}

function Click-Element([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
        return 'InvokePattern'
    }
    $bounds = $Element.Current.BoundingRectangle
    $x = [int]($bounds.Left + $bounds.Width / 2)
    $y = [int]($bounds.Top + $bounds.Height / 2)
    [void][ViewPaneSmoke.Native]::SetCursorPos($x, $y)
    [ViewPaneSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [ViewPaneSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    return 'bounds-pointer'
}

function Click-ElementByPointer([Windows.Automation.AutomationElement]$Element) {
    $bounds = $Element.Current.BoundingRectangle
    $x = [int]($bounds.Left + $bounds.Width / 2)
    $y = [int]($bounds.Top + $bounds.Height / 2)
    [void][ViewPaneSmoke.Native]::SetCursorPos($x, $y)
    [ViewPaneSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [ViewPaneSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    return 'bounds-pointer'
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

function Wait-FirstFileRow([Windows.Automation.AutomationElement]$Root) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem)
        $row = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $row) { return $row }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'UIA file row did not appear'
}

function Wait-FileRowByDisplayName(
    [Windows.Automation.AutomationElement]$Root,
    [string]$DisplayName
) {
    $fileViewBounds = (Wait-FirstFileRow $Root).Current.BoundingRectangle
    $wheelData = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]-720), 0)
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem)
        $rows = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        foreach ($row in $rows) {
            if ($row.Current.Name -eq $DisplayName -or $row.Current.Name.StartsWith("$DisplayName ")) {
                return $row
            }
        }
        [void][ViewPaneSmoke.Native]::SetCursorPos(
            [int]($fileViewBounds.Left + $fileViewBounds.Width / 2),
            [int]($fileViewBounds.Top + $fileViewBounds.Height / 2))
        [ViewPaneSmoke.Native]::mouse_event(0x0800, 0, 0, $wheelData, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA file row not found: $DisplayName"
}

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $executable
$start.WorkingDirectory = $workspaceRoot
$start.UseShellExecute = $false
$start.Environment['EXPLORER_INITIAL_PATH'] = (Resolve-Path -LiteralPath $InitialPath).Path
$start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
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
    [void][ViewPaneSmoke.Native]::SetWindowPos($hwnd, [IntPtr]::Zero, 80, 60, 1440, 900, 0x0040)
    Start-Sleep -Milliseconds 250
    [void][ViewPaneSmoke.Native]::SetForegroundWindow($hwnd)
    $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    # Windows PowerShell 5 reads UTF-8-without-BOM scripts through the active ANSI code page.
    # Construct localized UIA names from code points so the committed script is encoding-stable.
    $viewName = 'View'
    $detailsName = -join ([char]0x8A73, [char]0x7D30, [char]0x8CC7, [char]0x6599, [char]0x7A97, [char]0x683C)
    $previewName = -join ([char]0x9810, [char]0x89BD, [char]0x7A97, [char]0x683C)
    $splitterName = -join ([char]0x8ABF, [char]0x6574, [char]0x5074, [char]0x908A, [char]0x7A97, [char]0x683C, [char]0x5927, [char]0x5C0F)
    $largeIconsName = -join ([char]0x5927, [char]0x5716, [char]0x793A)
    $detailsModeName = -join ([char]0x8A73, [char]0x7D30, [char]0x8CC7, [char]0x6599)

    if ($ExpectedPreviewFile) {
        $expectedPath = [IO.Path]::GetFullPath($ExpectedPreviewFile)
        if (-not (Test-Path -LiteralPath $expectedPath -PathType Leaf)) {
            throw "preview fixture does not exist: $expectedPath"
        }
        $tabEvidence = $null
        if ($SecondaryPath) {
            $secondary = (Resolve-Path -LiteralPath $SecondaryPath).Path
            $newTab = Wait-Element $root 'New tab' $true ([Windows.Automation.ControlType]::Button)
            [void](Click-Element $newTab)
            Set-EnglishInput $hwnd
            Send-Key 0x4C @(0x11)
            $rootTop = $root.Current.BoundingRectangle.Top
            $addressEditor = $null
            $deadline = [DateTime]::UtcNow.AddSeconds(5)
            do {
                $editCondition = [Windows.Automation.PropertyCondition]::new(
                    [Windows.Automation.AutomationElement]::ControlTypeProperty,
                    [Windows.Automation.ControlType]::Edit)
                foreach ($candidate in $root.FindAll([Windows.Automation.TreeScope]::Descendants, $editCondition)) {
                    if ($candidate.Current.BoundingRectangle.Top -lt ($rootTop + 260)) {
                        $addressEditor = $candidate
                        break
                    }
                }
                if ($null -eq $addressEditor) { Start-Sleep -Milliseconds 100 }
            } while ($null -eq $addressEditor -and [DateTime]::UtcNow -lt $deadline)
            if ($null -eq $addressEditor) { throw 'address editor did not appear' }
            $addressEditor.SetFocus()
            Send-Key 0x41 @(0x11)
            [Windows.Forms.SendKeys]::SendWait($secondary)
            Send-Key 0x0D
            Start-Sleep -Milliseconds 500
            $secondaryTabName = Split-Path -Leaf $secondary
            $primaryTabName = Split-Path -Leaf ([IO.Path]::GetDirectoryName($expectedPath))
            $tabCondition = [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::TabItem)
            $tabs = $root.FindAll([Windows.Automation.TreeScope]::Descendants, $tabCondition)
            if ($tabs.Count -ne 2) { throw "expected two tabs after navigation, got $($tabs.Count)" }
            $primaryTab = $tabs.Item(0)
            [void](Click-ElementByPointer $primaryTab)
            Start-Sleep -Milliseconds 300
            $tabEvidence = [ordered]@{
                count = 2
                primary = $primaryTabName
                secondary = $secondaryTabName
                secondary_path = $secondary
                switched_back = $true
            }
        }
        $viewButton = Wait-Element $root $viewName $true ([Windows.Automation.ControlType]::Button)
        [void](Click-Element $viewButton)
        $previewMenuItem = Wait-Element $root $previewName $true ([Windows.Automation.ControlType]::MenuItem)
        [void](Click-Element $previewMenuItem)
        [void](Wait-Element $root $previewName $true $null ([Windows.Automation.ControlType]::MenuItem))
        $expectedName = [IO.Path]::GetFileName($expectedPath)
        $fileRow = Wait-FileRowByDisplayName $root $expectedName
        $fileRowInvoke = Click-ElementByPointer $fileRow
        $previewImage = Wait-Element $root 'Preview image loaded' $true ([Windows.Automation.ControlType]::Image)
        if ($previewImage.Current.ControlType -ne [Windows.Automation.ControlType]::Image) {
            throw "preview image host has the wrong control type: $($previewImage.Current.ControlType.ProgrammaticName)"
        }
        [ordered]@{
            schema_version = 1
            captured_utc = [DateTime]::UtcNow.ToString('o')
            initial_path = (Resolve-Path -LiteralPath $InitialPath).Path
            selected_image_preview = [ordered]@{
                path = $expectedPath
                file_row_invocation = $fileRowInvoke
                file_name = $expectedName
                image_automation_id = $previewImage.Current.AutomationId
                image_control_type = $previewImage.Current.ControlType.ProgrammaticName
                image_bounds = Convert-Bounds $previewImage.Current.BoundingRectangle
                loaded = $true
            }
            tabs = $tabEvidence
            exit_code = 0
        } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
        return
    }

    $detailsRowBefore = (Wait-FirstFileRow $root).Current.BoundingRectangle
    $view = Wait-Element $root $viewName $true ([Windows.Automation.ControlType]::Button)
    $viewAnchorInvoke = Click-Element $view
    $largeIconsItem = Wait-Element $root $largeIconsName $true ([Windows.Automation.ControlType]::MenuItem)
    $viewMenuAnchor = Assert-PopupAnchored $view $largeIconsItem $root 'View'
    $largeIconsInvoke = Click-Element $largeIconsItem
    Start-Sleep -Milliseconds 250
    $largeIconRow = (Wait-FirstFileRow $root).Current.BoundingRectangle
    if ($largeIconRow.Height -le $detailsRowBefore.Height) {
        throw "large-icon view did not enlarge item height: details=$($detailsRowBefore.Height), large=$($largeIconRow.Height)"
    }

    $view = Wait-Element $root $viewName $true ([Windows.Automation.ControlType]::Button)
    [void](Click-Element $view)
    $detailsModeItem = Wait-Element $root $detailsModeName $true ([Windows.Automation.ControlType]::MenuItem)
    $detailsModeInvoke = Click-Element $detailsModeItem
    Start-Sleep -Milliseconds 250
    $detailsRowAfter = (Wait-FirstFileRow $root).Current.BoundingRectangle
    if ([Math]::Abs($detailsRowAfter.Height - $detailsRowBefore.Height) -gt 1.0) {
        throw "details view row height did not restore: before=$($detailsRowBefore.Height), after=$($detailsRowAfter.Height)"
    }

    $view = Wait-Element $root $viewName $true ([Windows.Automation.ControlType]::Button)
    $viewInvoke = Click-Element $view
    $detailsItem = Wait-Element $root $detailsName
    $detailsInvoke = Click-Element $detailsItem
    $details = Wait-Element $root $detailsName $true $null ([Windows.Automation.ControlType]::MenuItem)
    $splitter = Wait-Element $root $splitterName $true ([Windows.Automation.ControlType]::Separator)
    $before = $details.Current.BoundingRectangle
    $splitterBounds = $splitter.Current.BoundingRectangle
    $startX = [int]($splitterBounds.Left + $splitterBounds.Width / 2)
    $startY = [int]($splitterBounds.Top + $splitterBounds.Height / 2)
    [void][ViewPaneSmoke.Native]::SetCursorPos($startX, $startY)
    [ViewPaneSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($offset in 10..100 | Where-Object { $_ % 10 -eq 0 }) {
        [void][ViewPaneSmoke.Native]::SetCursorPos($startX - $offset, $startY)
        Start-Sleep -Milliseconds 25
    }
    [ViewPaneSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    $details = Wait-Element $root $detailsName $true $null ([Windows.Automation.ControlType]::MenuItem)
    $after = $details.Current.BoundingRectangle
    if ($after.Width -le $before.Width + 50) {
        throw "side pane drag did not resize: before=$($before.Width), after=$($after.Width)"
    }

    $view = Wait-Element $root $viewName $true ([Windows.Automation.ControlType]::Button)
    [void](Click-Element $view)
    $previewItem = Wait-Element $root $previewName
    $previewInvoke = Click-Element $previewItem
    $preview = Wait-Element $root $previewName $true $null ([Windows.Automation.ControlType]::MenuItem)
    [void](Wait-Element $root $detailsName $false $null ([Windows.Automation.ControlType]::MenuItem))

    $previewEvidence = $null
    if ($ExpectedPreviewFile) {
        $expectedPath = [IO.Path]::GetFullPath($ExpectedPreviewFile)
        if (-not (Test-Path -LiteralPath $expectedPath -PathType Leaf)) {
            throw "preview fixture does not exist: $expectedPath"
        }
        $expectedName = [IO.Path]::GetFileName($expectedPath)
        $fileRow = Wait-FileRowByDisplayName $root $expectedName
        $fileRowInvoke = Click-ElementByPointer $fileRow
        $previewImage = Wait-AutomationId $root 'preview-image-host'
        $previewFileName = Wait-AutomationId $root 'preview-file-name'
        if ($previewFileName.Current.Name -notlike "*$expectedName*") {
            throw "preview pane selected a different file: expected=$expectedName actual=$($previewFileName.Current.Name)"
        }
        if ($previewImage.Current.ControlType -ne [Windows.Automation.ControlType]::Image) {
            throw "preview image host has the wrong control type: $($previewImage.Current.ControlType.ProgrammaticName)"
        }
        $previewEvidence = [ordered]@{
            path = $expectedPath
            file_row_invocation = $fileRowInvoke
            file_name = $previewFileName.Current.Name
            image_automation_id = $previewImage.Current.AutomationId
            image_control_type = $previewImage.Current.ControlType.ProgrammaticName
            image_bounds = Convert-Bounds $previewImage.Current.BoundingRectangle
            loaded = $true
        }
    }

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        initial_path = (Resolve-Path -LiteralPath $InitialPath).Path
        view_invocation = $viewInvoke
        view_anchor_invocation = $viewAnchorInvoke
        view_menu_anchor = $viewMenuAnchor
        details_invocation = $detailsInvoke
        preview_invocation = $previewInvoke
        large_icons_invocation = $largeIconsInvoke
        details_mode_invocation = $detailsModeInvoke
        details_row_height = $detailsRowBefore.Height
        large_icon_row_height = $largeIconRow.Height
        details_restored_row_height = $detailsRowAfter.Height
        details_width_before = $before.Width
        details_width_after = $after.Width
        splitter_control_type = $splitter.Current.ControlType.ProgrammaticName
        splitter_name = $splitter.Current.Name
        preview_control_type = $preview.Current.ControlType.ProgrammaticName
        mutual_exclusion_verified = $true
        selected_image_preview = $previewEvidence
        exit_code = 0
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
} finally {
    if (-not $process.HasExited) {
        [void][ViewPaneSmoke.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
}
Write-Output "View pane smoke passed: $OutputDirectory"
