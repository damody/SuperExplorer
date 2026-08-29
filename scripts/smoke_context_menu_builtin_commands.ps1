param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful
if (-not ('RustExplorerUitest.MenuNative' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace RustExplorerUitest {
    public static class MenuNative {
        [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
        [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public uint Size; public RECT Monitor; public RECT Work; public uint Flags; }
        [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int count, uint flags);
        [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr hwnd, IntPtr menu, uint item, out RECT rect);
        [DllImport("user32.dll")] public static extern uint GetClipboardSequenceNumber();
        [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool GetMonitorInfo(IntPtr monitor, ref MONITORINFO info);
    }
}
'@
}

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
foreach ($name in @('00-first-sentinel.txt', '01-copy.txt', '02-cut.txt', '03-link.txt', '04-rename.txt', '05-properties.txt', '06-delete.txt')) {
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture $name) -Value $name
}
New-Item -ItemType Directory -Force -Path (Join-Path $fixture '07-properties-folder') | Out-Null
foreach ($name in @('08-properties-multi-a.txt', '09-properties-multi-b.txt')) {
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture $name) -Value $name
}
Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\where.exe') -Destination (Join-Path $fixture '10-properties-app.exe')
Set-Content -Encoding ascii -LiteralPath (Join-Path $fixture '11-properties-script.cmd') -Value '@echo off'
$context = $null
$placementEvidence = [Collections.Generic.List[object]]::new()

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

function Get-NativePopupHandle {
    $handles = [Collections.Generic.List[IntPtr]]::new()
    $allowedProcessIds = Get-UitestProcessTreeIds
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
            $className = [Text.StringBuilder]::new(64)
            [void][RustExplorerUitest.Native]::GetClassName($hwnd, $className, $className.Capacity)
            [uint32]$processId = 0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($hwnd, [ref]$processId)
            if ($className.ToString() -in @('#32768', 'SuperExplorer.ImmersivePopup.v1') -and $allowedProcessIds.Contains([int]$processId)) {
                $handles.Add($hwnd)
            }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    $handles | Select-Object -First 1
}

function Get-OwnedDialogHandle {
    $handles = [Collections.Generic.List[IntPtr]]::new()
    $allowedProcessIds = Get-UitestProcessTreeIds
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
            $className = [Text.StringBuilder]::new(64)
            [void][RustExplorerUitest.Native]::GetClassName($hwnd, $className, $className.Capacity)
            [uint32]$processId = 0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($hwnd, [ref]$processId)
            if ($className.ToString() -eq '#32770' -and $allowedProcessIds.Contains([int]$processId)) {
                $handles.Add($hwnd)
            }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    $handles | Select-Object -First 1
}

function Invoke-NativeMenuCommand([string]$FileName, [char]$AccessKey, [string]$EnglishName, [int]$Occurrence = 1) {
    $row = Find-UitestFileItem -Root $context.Root -Name $FileName
    $rowBounds = $row.Current.BoundingRectangle
    $physicalPoint = Get-UitestPhysicalPoint -Element $row -HorizontalOffset 100
    Write-Output ("context target {0}: uia=({1},{2},{3},{4}) physical=({5},{6})" -f $FileName, $rowBounds.Left, $rowBounds.Top, $rowBounds.Width, $rowBounds.Height, $physicalPoint.X, $physicalPoint.Y)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    # Exercise the product's genuine pointer hit-testing. UitestHeadful converts the UIA
    # rectangle into native HWND coordinates so a DPI-virtualized PowerShell host cannot
    # redirect the click to the first or an adjacent row.
    Invoke-UitestClick -Element $row -Right
    $popup = $null
    Wait-Until -Description 'native popup handle' -Condition {
        $null -ne (Get-NativePopupHandle)
    }
    $popup = Get-NativePopupHandle
    if ($EnglishName -eq 'Copy') {
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'after-first-right-click.png')
    }
    $menu = [RustExplorerUitest.MenuNative]::SendMessage($popup, 0x01E1, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($menu -eq [IntPtr]::Zero) { throw 'native popup did not return HMENU' }
    $matched = $false
    $matchCount = 0
    for ($position = 0; $position -lt [RustExplorerUitest.MenuNative]::GetMenuItemCount($menu); $position++) {
        $label = [Text.StringBuilder]::new(512)
        [void][RustExplorerUitest.MenuNative]::GetMenuString($menu, [uint32]$position, $label, $label.Capacity, 0x00000400)
        if ($label.ToString() -match "\(&?$AccessKey\)$" -or $label.ToString() -match "^&?$EnglishName(\t|$)") {
            $matchCount++
            if ($matchCount -ne $Occurrence) { continue }
            Write-Output ("select native command {0}: {1}" -f $EnglishName, $label.ToString())
            $className = [Text.StringBuilder]::new(64)
            [void][RustExplorerUitest.Native]::GetClassName($popup, $className, $className.Capacity)
            if ($className.ToString() -eq 'SuperExplorer.ImmersivePopup.v1') {
                $layout = [RustExplorerUitest.MenuNative]::SendMessage($popup, 0x0451, [IntPtr]$position, [IntPtr]::Zero).ToInt64()
                if ($layout -lt 0) { throw "owned popup row query failed for $EnglishName" }
                $top = [int]($layout -band 0xffff)
                $height = [int](($layout -shr 16) -band 0xffff)
                $popupRect = [RustExplorerUitest.Native+RECT]::new()
                if (-not [RustExplorerUitest.Native]::GetWindowRect($popup, [ref]$popupRect)) {
                    throw "owned popup rectangle failed for $EnglishName"
                }
                [void][RustExplorerUitest.Native]::SetCursorPos($popupRect.Left + 100, $popupRect.Top + $top + [int]($height / 2))
            } else {
                $rect = [RustExplorerUitest.MenuNative+RECT]::new()
                if (-not [RustExplorerUitest.MenuNative]::GetMenuItemRect([IntPtr]::Zero, $menu, [uint32]$position, [ref]$rect)) {
                    throw "GetMenuItemRect failed for $EnglishName"
                }
                [void][RustExplorerUitest.Native]::SetCursorPos([int](($rect.Left + $rect.Right) / 2), [int](($rect.Top + $rect.Bottom) / 2))
            }
            [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
            [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
            $matched = $true
            break
        }
    }
    if (-not $matched) { throw "native menu command not found: $EnglishName ($AccessKey)" }
    Wait-Until -Description "$EnglishName popup dismissal" -Condition {
        $null -eq (Get-NativePopupHandle)
    }
    Start-Sleep -Milliseconds 350
}

function Wait-Until([scriptblock]$Condition, [string]$Description, [int]$TimeoutSeconds = 12) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "timed out waiting for $Description"
}

function Assert-RealPropertiesSheet([string]$ExpectedTitlePattern, [string]$ArtifactStem) {
    Wait-Until -Description "$ArtifactStem native Properties dialog" -TimeoutSeconds 12 -Condition {
        $null -ne (Get-OwnedDialogHandle)
    }
    $propertiesHwnd = Get-OwnedDialogHandle
    $propertiesRoot = [Windows.Automation.AutomationElement]::FromHandle($propertiesHwnd)
    Save-UitestScreenshot -Root $propertiesRoot -Path (Join-Path $output "$ArtifactStem-dialog.png")
    $tree = @($propertiesRoot.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
            [pscustomobject]@{
                name = $_.Current.Name
                type = $_.Current.ControlType.ProgrammaticName
                automation_id = $_.Current.AutomationId
            }
        })
    $tree | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output "$ArtifactStem-tree.json")
    $fields = @($tree | ForEach-Object { $_.automation_id })
    $visibleText = ($tree | ForEach-Object { $_.name }) -join "`n"
    if ($propertiesRoot.Current.Name -notlike $ExpectedTitlePattern) {
        throw "wrong Properties target: expected=$ExpectedTitlePattern actual=$($propertiesRoot.Current.Name)"
    }
    if ($visibleText -match 'properties for this item are not available|item.*properties.*unavailable') {
        throw "$ArtifactStem opened the generic unavailable Properties dialog"
    }
    if (($fields -notcontains '13080') -or ($fields -notcontains '13089')) {
        throw "$ArtifactStem is not a real filesystem Properties property sheet"
    }
    $previousDpi = [RustExplorerUitest.Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    try {
        $ownerRect = [RustExplorerUitest.Native+RECT]::new()
        $dialogRect = [RustExplorerUitest.Native+RECT]::new()
        if (-not [RustExplorerUitest.Native]::GetWindowRect($context.Hwnd, [ref]$ownerRect)) {
            throw "$ArtifactStem could not read the SuperExplorer owner rectangle"
        }
        if (-not [RustExplorerUitest.Native]::GetWindowRect($propertiesHwnd, [ref]$dialogRect)) {
            throw "$ArtifactStem could not read the native Properties rectangle"
        }
        $monitor = [RustExplorerUitest.MenuNative]::MonitorFromWindow($context.Hwnd, 2)
        if ($monitor -eq [IntPtr]::Zero) { throw "$ArtifactStem could not resolve the owner monitor" }
        $monitorInfo = [RustExplorerUitest.MenuNative+MONITORINFO]::new()
        $monitorInfo.Size = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][RustExplorerUitest.MenuNative+MONITORINFO])
        if (-not [RustExplorerUitest.MenuNative]::GetMonitorInfo($monitor, [ref]$monitorInfo)) {
            throw "$ArtifactStem could not read the monitor work area"
        }

        $dialogWidth = $dialogRect.Right - $dialogRect.Left
        $dialogHeight = $dialogRect.Bottom - $dialogRect.Top
        $workWidth = $monitorInfo.Work.Right - $monitorInfo.Work.Left
        $workHeight = $monitorInfo.Work.Bottom - $monitorInfo.Work.Top
        $centeredLeft = $ownerRect.Left + [int](($ownerRect.Right - $ownerRect.Left - $dialogWidth) / 2)
        $centeredTop = $ownerRect.Top + [int](($ownerRect.Bottom - $ownerRect.Top - $dialogHeight) / 2)
        $expectedLeft = if ($dialogWidth -ge $workWidth) {
            $monitorInfo.Work.Left
        } else {
            [Math]::Min([Math]::Max($centeredLeft, $monitorInfo.Work.Left), $monitorInfo.Work.Right - $dialogWidth)
        }
        $expectedTop = if ($dialogHeight -ge $workHeight) {
            $monitorInfo.Work.Top
        } else {
            [Math]::Min([Math]::Max($centeredTop, $monitorInfo.Work.Top), $monitorInfo.Work.Bottom - $dialogHeight)
        }
        $deltaX = [Math]::Abs($dialogRect.Left - $expectedLeft)
        $deltaY = [Math]::Abs($dialogRect.Top - $expectedTop)
        $tolerance = 24
        if ($deltaX -gt $tolerance -or $deltaY -gt $tolerance) {
            [uint32]$dialogProcessId = 0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($propertiesHwnd, [ref]$dialogProcessId)
            throw "$ArtifactStem Properties is not owner-centered: actual=($($dialogRect.Left),$($dialogRect.Top)) expected=($expectedLeft,$expectedTop) delta=($deltaX,$deltaY) dialogPid=$dialogProcessId appPid=$($context.Process.Id)"
        }
        if (($dialogWidth -le $workWidth -and ($dialogRect.Left -lt $monitorInfo.Work.Left -or $dialogRect.Right -gt $monitorInfo.Work.Right)) -or
            ($dialogHeight -le $workHeight -and ($dialogRect.Top -lt $monitorInfo.Work.Top -or $dialogRect.Bottom -gt $monitorInfo.Work.Bottom))) {
            throw "$ArtifactStem Properties escaped the monitor work area"
        }
        $placementEvidence.Add([pscustomobject]@{
            target = $ArtifactStem
            owner = [pscustomobject]@{ left=$ownerRect.Left; top=$ownerRect.Top; right=$ownerRect.Right; bottom=$ownerRect.Bottom }
            dialog = [pscustomobject]@{ left=$dialogRect.Left; top=$dialogRect.Top; right=$dialogRect.Right; bottom=$dialogRect.Bottom }
            work_area = [pscustomobject]@{ left=$monitorInfo.Work.Left; top=$monitorInfo.Work.Top; right=$monitorInfo.Work.Right; bottom=$monitorInfo.Work.Bottom }
            expected = [pscustomobject]@{ left=$expectedLeft; top=$expectedTop }
            delta = [pscustomobject]@{ x=$deltaX; y=$deltaY; tolerance=$tolerance }
        })
    } finally {
        if ($previousDpi -ne [IntPtr]::Zero) {
            [void][RustExplorerUitest.Native]::SetThreadDpiAwarenessContext($previousDpi)
        }
    }
    [void][RustExplorerUitest.Native]::SetForegroundWindow($propertiesHwnd)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 350
    Wait-Until -Description "$ArtifactStem Properties dismissal" -Condition {
        $null -eq (Get-OwnedDialogHandle)
    }
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    # A developer may already have another SuperExplorer window open. Keep the isolated
    # fixture window above it so physical pointer input cannot land in the wrong process.
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0003)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 250
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'before-pointer.png')

    $copyPath = Join-Path $fixture '01-copy.txt'
    $beforeCopyFiles = @(Get-ChildItem -LiteralPath $fixture -File | ForEach-Object { $_.FullName })
    $clipboardSequence = [RustExplorerUitest.MenuNative]::GetClipboardSequenceNumber()
    Invoke-NativeMenuCommand -FileName '01-copy.txt' -AccessKey 'C' -EnglishName 'Copy'
    Wait-Until -Description 'Copy clipboard ownership' -Condition {
        [RustExplorerUitest.MenuNative]::GetClipboardSequenceNumber() -ne $clipboardSequence
    }
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 350
    Wait-Until -Description 'Copy paste completion' -Condition {
        $created = @(Get-ChildItem -LiteralPath $fixture -File | Where-Object {
            $beforeCopyFiles -notcontains $_.FullName
        })
        $created.Count -eq 1 -and (Get-Content -Raw -LiteralPath $created[0].FullName) -match '^01-copy\.txt'
    }

    $cutPath = Join-Path $fixture '02-cut.txt'
    $clipboardSequence = [RustExplorerUitest.MenuNative]::GetClipboardSequenceNumber()
    Invoke-NativeMenuCommand -FileName '02-cut.txt' -AccessKey 'T' -EnglishName 'Cut'
    Wait-Until -Description 'Cut clipboard ownership' -Condition {
        [RustExplorerUitest.MenuNative]::GetClipboardSequenceNumber() -ne $clipboardSequence
    }
    if (-not (Test-Path -LiteralPath $cutPath -PathType Leaf)) {
        throw 'Cut removed the item before a paste destination was chosen'
    }

    Invoke-NativeMenuCommand -FileName '03-link.txt' -AccessKey 'S' -EnglishName 'Create shortcut' -Occurrence 2
    Wait-Until -Description 'collision-safe shortcut creation' -Condition {
        @(Get-ChildItem -LiteralPath $fixture -Filter '*.lnk' -File).Count -eq 1
    }

    Invoke-NativeMenuCommand -FileName '04-rename.txt' -AccessKey 'M' -EnglishName 'Rename'
    Set-UitestClipboardText -Text '04-renamed.txt'
    Send-UitestKey -Key 0x41 -Modifiers @(0x11) -DelayMilliseconds 80
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 80
    Send-UitestKey -Key 0x0D -DelayMilliseconds 350
    Wait-Until -Description 'inline rename completion' -Condition {
        Test-Path -LiteralPath (Join-Path $fixture '04-renamed.txt') -PathType Leaf
    }

    # Let the native property sheet rise above the app after earlier pointer input was protected
    # from unrelated developer windows by HWND_TOPMOST.
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-2), 0, 0, 0, 0, 0x0003)
    Invoke-NativeMenuCommand -FileName '05-properties.txt' -AccessKey 'R' -EnglishName 'Properties'
    Assert-RealPropertiesSheet -ExpectedTitlePattern '*05-properties.txt*' -ArtifactStem 'properties-file'

    Invoke-NativeMenuCommand -FileName '07-properties-folder' -AccessKey 'R' -EnglishName 'Properties'
    Assert-RealPropertiesSheet -ExpectedTitlePattern '*07-properties-folder*' -ArtifactStem 'properties-folder'

    $multiA = Find-UitestFileItem -Root $context.Root -Name '08-properties-multi-a.txt'
    $multiB = Find-UitestFileItem -Root $context.Root -Name '09-properties-multi-b.txt'
    Invoke-UitestClick -Element $multiA
    Invoke-UitestClick -Element $multiB -Control
    if ((Get-UitestSelectedCount -Root $context.Root) -ne 2) {
        throw 'failed to establish the two-item Properties selection'
    }
    Invoke-NativeMenuCommand -FileName '09-properties-multi-b.txt' -AccessKey 'R' -EnglishName 'Properties'
    Assert-RealPropertiesSheet -ExpectedTitlePattern '*Properties*' -ArtifactStem 'properties-multi'

    Invoke-NativeMenuCommand -FileName '10-properties-app.exe' -AccessKey 'R' -EnglishName 'Properties'
    Assert-RealPropertiesSheet -ExpectedTitlePattern '*10-properties-app.exe*' -ArtifactStem 'properties-executable'

    Invoke-NativeMenuCommand -FileName '11-properties-script.cmd' -AccessKey 'R' -EnglishName 'Properties'
    Assert-RealPropertiesSheet -ExpectedTitlePattern '*11-properties-script.cmd*' -ArtifactStem 'properties-script'

    # Reproduce the manual sequence that previously failed: close Properties, right-click a
    # different non-first row, and physically click a harmless command. Ten cycles are enough to
    # expose stale COM targets, disposable STA lifetime bugs, and popup ownership leaks.
    $context.Process.Refresh()
    $cycleBaselineHandles = $context.Process.HandleCount
    $cycleBaselineThreads = $context.Process.Threads.Count
    for ($cycle = 1; $cycle -le 10; $cycle++) {
        Invoke-NativeMenuCommand -FileName '05-properties.txt' -AccessKey 'R' -EnglishName 'Properties'
        Assert-RealPropertiesSheet -ExpectedTitlePattern '*05-properties.txt*' -ArtifactStem ("properties-cycle-{0:D2}" -f $cycle)

        $clipboardSequence = [RustExplorerUitest.MenuNative]::GetClipboardSequenceNumber()
        Invoke-NativeMenuCommand -FileName '01-copy.txt' -AccessKey 'C' -EnglishName 'Copy'
        Wait-Until -Description "cycle $cycle post-Properties Copy" -Condition {
            [RustExplorerUitest.MenuNative]::GetClipboardSequenceNumber() -ne $clipboardSequence
        }
    }
    $context.Process.Refresh()
    if ($context.Process.HandleCount -gt ($cycleBaselineHandles + 40)) {
        throw "context-menu handle growth exceeded bound: baseline=$cycleBaselineHandles actual=$($context.Process.HandleCount)"
    }
    if ($context.Process.Threads.Count -gt ($cycleBaselineThreads + 4)) {
        throw "context-menu thread growth exceeded bound: baseline=$cycleBaselineThreads actual=$($context.Process.Threads.Count)"
    }
    $treeProcessIds = Get-UitestProcessTreeIds
    $treeProcesses = @(Get-Process | Where-Object {
        $treeProcessIds.Contains([int]$_.Id)
    })
    if (@($treeProcesses | Where-Object ProcessName -eq 'explorer-extension-broker').Count -gt 1) {
        throw 'more than one extension broker remained in the launched process tree'
    }

    Invoke-NativeMenuCommand -FileName '06-delete.txt' -AccessKey 'D' -EnglishName 'Delete'
    Wait-Until -Description 'recycle delete completion' -Condition {
        -not (Test-Path -LiteralPath (Join-Path $fixture '06-delete.txt'))
    }
    if (-not (Test-Path -LiteralPath (Join-Path $fixture '00-first-sentinel.txt') -PathType Leaf)) {
        throw 'a non-first-row context command was incorrectly applied to the first row'
    }

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'context-menu-builtins.png')
    $placementEvidence | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'properties-placement.json')
    [pscustomobject]@{
        schema = 'superexplorer.context-menu-builtins.v5'
        genuine_pointer_input = $true
        exact_non_first_target = $true
        first_row_sentinel_preserved = $true
        copy_clipboard = $true
        cut_clipboard = $true
        shortcut_created = $true
        rename_completed = $true
        properties_opened = $true
        properties_is_real_item_sheet = $true
        folder_properties_is_real_item_sheet = $true
        multi_properties_is_real_item_sheet = $true
        executable_properties_is_real_item_sheet = $true
        script_properties_is_real_item_sheet = $true
        properties_owner_centered = $true
        properties_work_area_safe = $true
        properties_placement_targets = $placementEvidence.Count
        properties_post_close_cycles = 10
        popup_and_dialog_bound_to_launched_process_tree = $true
        context_menu_resources_bounded = $true
        delete_completed = $true
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Built-in context-command smoke passed: $OutputDirectory"
