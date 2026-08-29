param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful
if (-not ('RustExplorerUitest.ReplacementMenuNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace RustExplorerUitest {
    public static class ReplacementMenuNative {
        [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
        [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll", SetLastError = true)] public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out IntPtr result);
        [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int count, uint flags);
        [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr hwnd, IntPtr menu, uint item, out RECT rect);
    }
}
'@
}

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture '00-first-sentinel.txt') -Value 'sentinel'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'Alpha.txt') -Value 'alpha'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'Beta.txt') -Value 'beta'
$context = $null

function Get-UitestProcessTreeIds {
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add([int]$context.Process.Id)
    do {
        $changed = $false
        foreach ($process in @(Get-CimInstance Win32_Process)) {
            if ($ids.Contains([int]$process.ParentProcessId) -and $ids.Add([int]$process.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    return ,$ids
}

function Invoke-RightClick(
    [Windows.Automation.AutomationElement]$Element,
    [switch]$FromRightEdge
) {
    $bounds = $Element.Current.BoundingRectangle
    $x = if ($FromRightEdge) {
        # UI Automation may report only the Name cell even though the Details row spans all
        # visible columns. Choose a point beyond the active popup and inside the file viewport so
        # a wider application-owned menu cannot accidentally consume the replacement gesture.
        $candidate = $bounds.Right - [Math]::Min(80, $bounds.Width / 4)
        $popups = @(Get-NativePopupMenus)
        if ($popups.Count -eq 1) {
            $popupRect = [RustExplorerUitest.Native+RECT]::new()
            if ([RustExplorerUitest.Native]::GetWindowRect($popups[0], [ref]$popupRect)) {
                $rootBounds = $context.Root.Current.BoundingRectangle
                $candidate = [Math]::Min($rootBounds.Right - 120, $popupRect.Right + 40)
            }
        }
        $candidate
    } else {
        $bounds.Left + [Math]::Min(80, $bounds.Width / 2)
    }
    [void][RustExplorerUitest.Native]::SetCursorPos(
        [int]$x,
        [int]($bounds.Top + $bounds.Height / 2)
    )
    [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

function Invoke-PhysicalClickPoint([int]$X, [int]$Y, [switch]$Right) {
    [void][RustExplorerUitest.Native]::SetCursorPos($X, $Y)
    if ($Right) {
        [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    } else {
        [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 250
}

function Get-NativePopupMenus {
    $handles = [Collections.Generic.List[IntPtr]]::new()
    $allowedProcessIds = Get-UitestProcessTreeIds
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
            $className = [Text.StringBuilder]::new(64)
            [void][RustExplorerUitest.Native]::GetClassName($hwnd, $className, $className.Capacity)
            $processId = [uint32]0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($hwnd, [ref]$processId)
            if (($className.ToString() -eq '#32768' -or
                 $className.ToString() -eq 'SuperExplorer.ImmersivePopup.v1') -and
                $allowedProcessIds.Contains([int]$processId)) {
                $handles.Add($hwnd)
            }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    @($handles | Select-Object -Unique)
}

function Get-PopupSession([IntPtr]$Hwnd) {
    [uint32]$processId = 0
    [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($Hwnd, [ref]$processId)
    $menu = [RustExplorerUitest.ReplacementMenuNative]::SendMessage($Hwnd, 0x01E1, [IntPtr]::Zero, [IntPtr]::Zero)
    $className = [Text.StringBuilder]::new(64)
    [void][RustExplorerUitest.Native]::GetClassName($Hwnd, $className, $className.Capacity)
    [pscustomobject]@{
        Hwnd = $Hwnd
        ProcessId = [int]$processId
        Menu = $menu
        ApplicationOwned = $className.ToString() -eq 'SuperExplorer.ImmersivePopup.v1'
    }
}

function Wait-SinglePopup([string]$Description, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $popups = @(Get-NativePopupMenus)
        if ($popups.Count -eq 1) { return Get-PopupSession $popups[0] }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "timed out waiting for one process-bound popup: $Description"
}

function Wait-Replacement([Windows.Automation.AutomationElement]$Element, $OriginalSession, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $pattern = $null
        $selected = $Element.TryGetCurrentPattern(
            [Windows.Automation.SelectionItemPattern]::Pattern,
            [ref]$pattern
        ) -and ([Windows.Automation.SelectionItemPattern]$pattern).Current.IsSelected
        $popups = @(Get-NativePopupMenus)
        if ($selected -and $popups.Count -eq 1) {
            $session = Get-PopupSession $popups[0]
            # HWND and HMENU values may be reused immediately after the old popup is destroyed.
            # Exact target is proved below by physically invoking Copy and checking the clipboard,
            # so handle inequality is diagnostic rather than a pass/fail oracle.
            if ($session.Menu -ne [IntPtr]::Zero) {
                return $session
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'right-click replacement did not select the second item and open its native menu'
}

function Wait-ReplacementSession($OriginalSession, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $popups = @(Get-NativePopupMenus)
        if ($popups.Count -eq 1) {
            $session = Get-PopupSession $popups[0]
            if ($session.Menu -ne [IntPtr]::Zero -and
                ($session.Hwnd -ne $OriginalSession.Hwnd -or $session.Menu -ne $OriginalSession.Menu -or $session.ProcessId -ne $OriginalSession.ProcessId)) {
                return $session
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'right-click replacement did not produce a distinct process-bound popup session'
}

function Wait-NoPopup([string]$Description, [int]$TimeoutSeconds = 5) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if (@(Get-NativePopupMenus).Count -eq 0) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "native popup remained visible: $Description"
}

function Assert-ExplorerResponsive([int]$Cycle) {
    if ($context.Process.HasExited) {
        throw "cycle $Cycle application exited during context-menu replacement"
    }
    $result = [IntPtr]::Zero
    $started = [Diagnostics.Stopwatch]::StartNew()
    $sent = [RustExplorerUitest.ReplacementMenuNative]::SendMessageTimeout(
        $context.Hwnd,
        0,
        [IntPtr]::Zero,
        [IntPtr]::Zero,
        0x0002,
        1000,
        [ref]$result
    )
    $started.Stop()
    if ($sent -eq [IntPtr]::Zero) {
        throw "cycle $Cycle SuperExplorer window stopped responding during second right-click"
    }
    return [int]$started.ElapsedMilliseconds
}

function Invoke-PopupCopy($Session) {
    $menu = $Session.Menu
    if ($menu -eq [IntPtr]::Zero) { throw 'replacement popup did not expose an HMENU' }
    for ($position = 0; $position -lt [RustExplorerUitest.ReplacementMenuNative]::GetMenuItemCount($menu); $position++) {
        $label = [Text.StringBuilder]::new(512)
        [void][RustExplorerUitest.ReplacementMenuNative]::GetMenuString($menu, [uint32]$position, $label, $label.Capacity, 0x00000400)
        if ($label.ToString() -match '\(&?C\)$' -or $label.ToString() -match '^&?Copy(\t|$)') {
            if ($Session.ApplicationOwned) {
                # The clean-room popup deliberately does not mutate Shell menu geometry. Its
                # documented test seam returns the row top/height while the HWND supplies the
                # screen origin, allowing the same physical single-dispatch check as #32768.
                $packed = [RustExplorerUitest.ReplacementMenuNative]::SendMessage(
                    $Session.Hwnd, 0x0451, [IntPtr]$position, [IntPtr]::Zero
                ).ToInt64()
                if ($packed -lt 0) { throw 'application-owned popup did not expose Copy geometry' }
                $windowRect = [RustExplorerUitest.Native+RECT]::new()
                if (-not [RustExplorerUitest.Native]::GetWindowRect($Session.Hwnd, [ref]$windowRect)) {
                    throw 'GetWindowRect failed for application-owned replacement Copy'
                }
                $top = [int]($packed -band 0xffff)
                $height = [int](($packed -shr 16) -band 0xffff)
                [void][RustExplorerUitest.Native]::SetCursorPos(
                    [int](($windowRect.Left + $windowRect.Right) / 2),
                    [int]($windowRect.Top + $top + ($height / 2))
                )
            } else {
                $rect = [RustExplorerUitest.ReplacementMenuNative+RECT]::new()
                if (-not [RustExplorerUitest.ReplacementMenuNative]::GetMenuItemRect([IntPtr]::Zero, $menu, [uint32]$position, [ref]$rect)) {
                    throw 'GetMenuItemRect failed for replacement Copy'
                }
                [void][RustExplorerUitest.Native]::SetCursorPos([int](($rect.Left + $rect.Right) / 2), [int](($rect.Top + $rect.Bottom) / 2))
            }
            [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
            [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
            return
        }
    }
    throw 'Copy was not found in the replacement native menu'
}

function Wait-ClipboardTarget([string]$ExpectedName, [string]$UnexpectedName, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $paths = @([Windows.Forms.Clipboard]::GetFileDropList() | ForEach-Object { [string]$_ })
            $names = @($paths | ForEach-Object { [IO.Path]::GetFileName($_) })
            if ($names -contains $ExpectedName -and $names -notcontains $UnexpectedName) { return }
        } catch {}
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "replacement Copy targeted the wrong item: expected=$ExpectedName unexpected=$UnexpectedName"
}

function Wait-ClipboardNames([string[]]$ExpectedNames, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $names = @([Windows.Forms.Clipboard]::GetFileDropList() | ForEach-Object { [IO.Path]::GetFileName([string]$_) })
            $missing = @($ExpectedNames | Where-Object { $names -notcontains $_ })
            if ($missing.Count -eq 0 -and $names.Count -eq $ExpectedNames.Count) { return }
        } catch {}
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "replacement Copy did not contain the expected selection: $($ExpectedNames -join ', ')"
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    # Keep physical replacement gestures bound to this isolated process even when the developer
    # has another SuperExplorer window open on the same desktop.
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0003)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 250
    $alpha = Find-UitestFileItem -Root $context.Root -Name 'Alpha.txt'
    $beta = Find-UitestFileItem -Root $context.Root -Name 'Beta.txt'
    $context.Process.Refresh()
    $baselineHandles = $context.Process.HandleCount
    $baselineThreads = $context.Process.Threads.Count
    $sessions = [Collections.Generic.List[object]]::new()
    $responsivenessMilliseconds = [Collections.Generic.List[int]]::new()

    for ($cycle = 1; $cycle -le 10; $cycle++) {
        $source = if (($cycle % 2) -eq 1) { $alpha } else { $beta }
        $target = if (($cycle % 2) -eq 1) { $beta } else { $alpha }
        $sourceName = if (($cycle % 2) -eq 1) { 'Alpha.txt' } else { 'Beta.txt' }
        $targetName = if (($cycle % 2) -eq 1) { 'Beta.txt' } else { 'Alpha.txt' }

        Invoke-RightClick -Element $source
        $original = Wait-SinglePopup -Description "cycle $cycle original $sourceName"

        # The popup may cover the name cells below it. Explorer can retarget only a secondary
        # click that lands on a visible part of the other row, so use the unobscured right edge.
        Invoke-RightClick -Element $target -FromRightEdge
        $responsivenessMilliseconds.Add((Assert-ExplorerResponsive -Cycle $cycle))
        $replacement = Wait-Replacement -Element $target -OriginalSession $original
        $sessions.Add([pscustomobject]@{
            cycle = $cycle
            source = $sourceName
            target = $targetName
            original_hwnd = $original.Hwnd.ToInt64()
            original_menu = $original.Menu.ToInt64()
            original_pid = $original.ProcessId
            replacement_hwnd = $replacement.Hwnd.ToInt64()
            replacement_menu = $replacement.Menu.ToInt64()
            replacement_pid = $replacement.ProcessId
            native_identity_changed = ($replacement.Hwnd -ne $original.Hwnd -or $replacement.Menu -ne $original.Menu -or $replacement.ProcessId -ne $original.ProcessId)
        })

        if ($cycle -eq 1) {
            Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'context-menu-replacement.png')
        }
        Invoke-PopupCopy $replacement
        Wait-ClipboardTarget -ExpectedName $targetName -UnexpectedName $sourceName

        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (@(Get-NativePopupMenus).Count -gt 0 -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 100
        }
        if (@(Get-NativePopupMenus).Count -gt 0) { throw "cycle $cycle replacement popup did not close after Copy" }
    }

    # Let the final delegated Copy terminal event settle before starting unrelated compatibility
    # scenarios, then reacquire UIA rows so no stale provider element influences hit-testing.
    Start-Sleep -Milliseconds 300
    $alpha = Find-UitestFileItem -Root $context.Root -Name 'Alpha.txt'
    $beta = Find-UitestFileItem -Root $context.Root -Name 'Beta.txt'
    $rootBounds = $context.Root.Current.BoundingRectangle
    $betaBounds = $beta.Current.BoundingRectangle
    # Stay in the file-view column instead of using the application right edge, which may be a
    # preview/details pane depending on restored layout settings.
    $backgroundX = [int](($betaBounds.Left + $betaBounds.Right) / 2)
    $backgroundY = [int][Math]::Min($rootBounds.Bottom - 110, $betaBounds.Bottom + 120)

    # Right-clicking another member of a compatible multi-selection keeps the selection intact.
    Invoke-UitestClick -Element $alpha
    Invoke-UitestClick -Element $beta -Control
    if ((Get-UitestSelectedCount -Root $context.Root) -ne 2) {
        throw 'failed to establish replacement multi-selection'
    }
    Invoke-RightClick -Element $alpha
    $multiOriginal = Wait-SinglePopup -Description 'multi-selection original popup'
    Invoke-RightClick -Element $beta -FromRightEdge
    $multiReplacement = Wait-Replacement -Element $beta -OriginalSession $multiOriginal
    if ((Get-UitestSelectedCount -Root $context.Root) -ne 2) {
        throw 'replacement collapsed a compatible multi-selection'
    }
    Invoke-PopupCopy $multiReplacement
    Wait-ClipboardNames -ExpectedNames @('Alpha.txt', 'Beta.txt')
    Wait-NoPopup 'multi-selection Copy'

    # A secondary click inside the current popup remains a native-menu gesture; it must never be
    # replayed onto a file row behind the popup.
    Invoke-UitestClick -Element $alpha
    Invoke-RightClick -Element $alpha
    $popupInteraction = Wait-SinglePopup -Description 'popup interaction'
    $popupRect = [RustExplorerUitest.Native+RECT]::new()
    if (-not [RustExplorerUitest.Native]::GetWindowRect($popupInteraction.Hwnd, [ref]$popupRect)) {
        throw 'GetWindowRect failed for popup interaction'
    }
    Invoke-PhysicalClickPoint -X ([int](($popupRect.Left + $popupRect.Right) / 2)) -Y ([int](($popupRect.Top + $popupRect.Bottom) / 2)) -Right
    if ((Get-UitestSelectedCount -Root $context.Root) -ne 1) {
        throw 'right-click inside the popup was replayed onto file content'
    }
    Send-UitestKey -Key 0x1B -DelayMilliseconds 250
    Wait-NoPopup 'popup interaction Escape'

    # A physical outside left-click is ordinary dismissal, followed by a fresh usable right-click.
    Invoke-RightClick -Element $alpha
    $null = Wait-SinglePopup -Description 'outside-left cancellation popup'
    Invoke-PhysicalClickPoint -X $backgroundX -Y $backgroundY
    Wait-NoPopup 'outside left click dismissal'
    Invoke-RightClick -Element $beta
    $null = Wait-SinglePopup -Description 'fresh popup after outside left dismissal'
    Send-UitestKey -Key 0x1B -DelayMilliseconds 250
    Wait-NoPopup 'fresh popup Escape'

    # Escape remains ordinary cancellation and must not replay another right-click.
    Invoke-RightClick -Element $alpha
    $null = Wait-SinglePopup -Description 'Escape cancellation popup'
    Send-UitestKey -Key 0x1B -DelayMilliseconds 350
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (@(Get-NativePopupMenus).Count -gt 0 -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (@(Get-NativePopupMenus).Count -gt 0) { throw 'Escape did not close the native menu' }

    $context.Process.Refresh()
    if ($context.Process.HandleCount -gt ($baselineHandles + 40)) {
        throw "replacement handle growth exceeded bound: baseline=$baselineHandles actual=$($context.Process.HandleCount)"
    }
    if ($context.Process.Threads.Count -gt ($baselineThreads + 4)) {
        throw "replacement thread growth exceeded bound: baseline=$baselineThreads actual=$($context.Process.Threads.Count)"
    }
    $treeIds = Get-UitestProcessTreeIds
    if (@(Get-Process | Where-Object { $treeIds.Contains([int]$_.Id) -and $_.ProcessName -eq 'explorer-extension-broker' }).Count -gt 1) {
        throw 'replacement flow left more than one broker in the launched process tree'
    }
    $sessions | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'popup-sessions.json')
    if (-not (Test-Path -LiteralPath (Join-Path $fixture 'Alpha.txt') -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $fixture 'Beta.txt') -PathType Leaf)) {
        throw 'context-menu replacement invoked a destructive command'
    }

    [pscustomobject]@{
        schema = 'superexplorer.context-menu-replacement.v2'
        replacement_cycles = 10
        exact_clipboard_target_each_cycle = $true
        process_tree_bound_popups = $true
        popup_session_identity_changed_each_cycle = (@($sessions | Where-Object { -not $_.native_identity_changed }).Count -eq 0)
        replacement_session_result_verified = $true
        background_dismissal_preserved = $true
        multi_selection_preserved = $true
        popup_input_not_replayed = $true
        outside_left_dismissal = $true
        one_broker = $true
        resources_bounded = $true
        responsive_each_cycle = $true
        maximum_responsiveness_probe_ms = ($responsivenessMilliseconds | Measure-Object -Maximum).Maximum
        escape_closed = $true
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Context-menu replacement smoke passed: $OutputDirectory"
