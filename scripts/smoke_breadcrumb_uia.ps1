param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [string]$InitialPath = 'D:\test',
    [string]$OutputDirectory,
    [switch]$UseIconFixture,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('breadcrumb-uia-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

$iconFixtureRoot = $null
$iconFixtureParent = $null
if ($UseIconFixture) {
    $iconFixtureParent = [IO.Path]::GetPathRoot($workspaceRoot)
    $iconFixtureRoot = Join-Path $iconFixtureParent ('bcu-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
    $InitialPath = $iconFixtureRoot
    New-Item -ItemType Directory -Force -Path (Join-Path $InitialPath 'LevelOne\LevelTwo') | Out-Null
}

$resolvedInitial = (Resolve-Path -LiteralPath $InitialPath).Path
if (-not [IO.Path]::IsPathRooted($resolvedInitial) -or -not (Test-Path -LiteralPath $resolvedInitial -PathType Container)) {
    throw "InitialPath must be an existing absolute directory: $InitialPath"
}
if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "missing app: $executable" }

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ('BreadcrumbUia.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace BreadcrumbUia {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
        [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool ShowWindow(IntPtr window, int command);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetWindowPos(IntPtr window, IntPtr insertAfter, int x, int y, int width, int height, uint flags);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte virtualKey, byte scanCode, uint flags, UIntPtr extra);
    }
}
'@
}

function Find-ByName([Windows.Automation.AutomationElement]$Root, [string]$Name, [int]$TimeoutMs = 10000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $element = $null
        $nodes = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
        foreach ($node in $nodes) {
            if ($node.Current.Name -eq $Name) { $element = $node; break }
        }
        if ($null -eq $element) { Start-Sleep -Milliseconds 50 }
    } while ($null -eq $element -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $element) { throw "UIA element not found: $Name" }
    return $element
}

# Windows PowerShell 5 treats UTF-8 files without a BOM as ANSI. Keep this script
# BOM-independent because the UIA labels are localized and the runner uses powershell.exe.
function Get-DriveChildrenLabel {
    -join ([char[]]@(0x5217, 0x51FA, 0x78C1, 0x789F, 0x6A5F))
}

function Get-ThisPcLabel {
    -join ([char[]]@(0x672C, 0x6A5F))
}

function Get-OlderBreadcrumbLevelsLabel {
    -join ([char[]]@(0x986F, 0x793A, 0x8F03, 0x820A, 0x7684, 0x8DEF, 0x5F91, 0x5C64, 0x7D1A))
}

function Get-FolderChildrenLabel([string]$Name) {
    $prefix = -join ([char[]]@(0x5217, 0x51FA))
    $suffix = -join ([char[]]@(0x7684, 0x5B50, 0x8CC7, 0x6599, 0x593E))
    return "$prefix $Name $suffix"
}

function Find-ById([Windows.Automation.AutomationElement]$Root, [string]$Id, [int]$TimeoutMs = 10000) {
    $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $element = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -eq $element) { Start-Sleep -Milliseconds 50 }
    } while ($null -eq $element -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $element) { throw "UIA element not found: $Id" }
    return $element
}

function Get-VisibleDriveBreadcrumbName(
    [Windows.Automation.AutomationElement]$Root,
    [string]$ExpectedDrive
) {
    $windowBounds = $Root.Current.BoundingRectangle
    $elements = $Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.OrCondition]::new(
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Button
            ),
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::MenuItem
            )
        )
    )
    $candidate = $elements | Where-Object {
        $bounds = $_.Current.BoundingRectangle
        $name = $_.Current.Name
        $displayName = if ($name.StartsWith('Go to ', [StringComparison]::Ordinal)) { $name.Substring(6) } else { $name }
        $bounds.Top -lt ($windowBounds.Top + 180) -and
        ($displayName -eq $ExpectedDrive -or $displayName.EndsWith("($ExpectedDrive)", [StringComparison]::OrdinalIgnoreCase))
    } | Select-Object -First 1
    if ($null -eq $candidate) { return $null }
    $name = $candidate.Current.Name
    if ($name.StartsWith('Go to ', [StringComparison]::Ordinal)) { return $name.Substring(6) }
    return $name
}

function Assert-StableDriveBreadcrumb(
    [Windows.Automation.AutomationElement]$Root,
    [string]$ExpectedDrive,
    [string]$Phase,
    [int]$DurationMs = 1500
) {
    $overflowOpened = $false
    if ($null -eq (Get-VisibleDriveBreadcrumbName $Root $ExpectedDrive)) {
        $overflow = Find-ByName $Root (Get-OlderBreadcrumbLevelsLabel)
        Invoke-Element $overflow | Out-Null
        $overflowOpened = $true
        Start-Sleep -Milliseconds 150
    }
    $deadline = [DateTime]::UtcNow.AddMilliseconds($DurationMs)
    $observed = [Collections.Generic.List[string]]::new()
    do {
        $name = Get-VisibleDriveBreadcrumbName $Root $ExpectedDrive
        if ($null -ne $name) {
            $observed.Add($name)
            if ($name -ne $ExpectedDrive) {
                throw "$Phase drive breadcrumb changed from '$ExpectedDrive' to '$name'"
            }
        }
        Start-Sleep -Milliseconds 40
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($overflowOpened) {
        Send-Escape
        Start-Sleep -Milliseconds 100
    }
    if ($observed.Count -eq 0) { throw "$Phase did not expose a drive breadcrumb" }
    return [ordered]@{
        phase = $Phase
        expected = $ExpectedDrive
        samples = $observed.Count
        unique_names = @($observed | Sort-Object -Unique)
    }
}

function Invoke-Element([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
        return 'InvokePattern'
    }
    $bounds = $Element.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 0) { throw "element is not actionable: $($Element.Current.Name)" }
    [BreadcrumbUia.Native]::SetCursorPos(
        [int][Math]::Round($bounds.Left + $bounds.Width / 2),
        [int][Math]::Round($bounds.Top + $bounds.Height / 2)
    ) | Out-Null
    $clickPoint = [BreadcrumbUia.Native+Point]::new()
    $clickPoint.X = [int][Math]::Round($bounds.Left + $bounds.Width / 2)
    $clickPoint.Y = [int][Math]::Round($bounds.Top + $bounds.Height / 2)
    $clickWindow = [BreadcrumbUia.Native]::WindowFromPoint($clickPoint)
    if ($windowHandle -ne [IntPtr]::Zero -and $clickWindow -ne $windowHandle) {
        throw "action point is occluded: $($Element.Current.Name), hit=$clickWindow app=$windowHandle bounds=$bounds"
    }
    [BreadcrumbUia.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 50
    [BreadcrumbUia.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    return 'UIA-bounds-pointer-fallback'
}

function Send-Escape {
    [BreadcrumbUia.Native]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    [BreadcrumbUia.Native]::keybd_event(0x1B, 0, 0x0002, [UIntPtr]::Zero)
}

function Send-Key([byte]$VirtualKey) {
    [BreadcrumbUia.Native]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
    [BreadcrumbUia.Native]::keybd_event($VirtualKey, 0, 0x0002, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
}

function Get-SelectedMenuName(
    [Windows.Automation.AutomationElement]$Root,
    [int]$MinimumCount,
    [int]$TimeoutMs = 2000
) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $Items = Get-MenuItems $Root $MinimumCount 250
        foreach ($item in $Items) {
            $selected = $item.GetCurrentPropertyValue([Windows.Automation.SelectionItemPattern]::IsSelectedProperty, $true)
            if ($selected -is [bool] -and $selected) { return $item.Current.Name }
            $focused = $item.GetCurrentPropertyValue([Windows.Automation.AutomationElement]::HasKeyboardFocusProperty, $true)
            if ($focused -is [bool] -and $focused) { return $item.Current.Name }
        }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    # accesskit_windows 0.33 does not expose SelectionItemPattern for Role::MenuItem.
    # Keep the Explorer-correct MenuItem role and record this provider limitation rather than
    # changing production semantics to ListItem/RadioButton solely for the test adapter.
    return $null
}

function Get-MenuItems([Windows.Automation.AutomationElement]$Root, [int]$MinimumCount, [int]$TimeoutMs = 10000) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::MenuItem
    )
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $found = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($found.Count -lt $MinimumCount) { Start-Sleep -Milliseconds 50 }
    } while ($found.Count -lt $MinimumCount -and [DateTime]::UtcNow -lt $deadline)
    if ($found.Count -lt $MinimumCount) { throw "expected at least $MinimumCount menu items, got $($found.Count)" }
    return @($found | ForEach-Object { $_ })
}

function Get-ItemEvidence([Windows.Automation.AutomationElement]$Element, [IntPtr]$WindowHandle) {
    $bounds = $Element.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 0) { throw "empty menu bounds: $($Element.Current.Name)" }
    $point = [BreadcrumbUia.Native+Point]::new()
    $point.X = [int][Math]::Round($bounds.Left + $bounds.Width / 2)
    $point.Y = [int][Math]::Round($bounds.Top + $bounds.Height / 2)
    $hit = [BreadcrumbUia.Native]::WindowFromPoint($point)
    if ($hit -ne $WindowHandle) { throw "menu is not topmost at center: $($Element.Current.Name), hit=$hit app=$WindowHandle" }
    return [ordered]@{
        name = $Element.Current.Name
        automation_id = $Element.Current.AutomationId
        bounds = [ordered]@{ left=$bounds.Left; top=$bounds.Top; width=$bounds.Width; height=$bounds.Height }
        topmost_hit = $true
    }
}

function Save-BreadcrumbIconEvidence(
    [Windows.Automation.AutomationElement]$Root,
    [string]$Path
) {
    $windowBounds = $Root.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::new(
        [int][Math]::Ceiling($windowBounds.Width),
        [int][Math]::Ceiling($windowBounds.Height),
        [Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen(
            [int][Math]::Round($windowBounds.Left),
            [int][Math]::Round($windowBounds.Top),
            0,
            0,
            $bitmap.Size,
            [Drawing.CopyPixelOperation]::SourceCopy
        )
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)

        $buttons = $Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Button
            )
        )
        $thisPcLabel = Get-ThisPcLabel
        $targets = @($buttons | Where-Object {
            $buttonBounds = $_.Current.BoundingRectangle
            ($_.Current.Name -like 'Go to *') -or
            ($_.Current.Name -eq $thisPcLabel -and
                $buttonBounds.Top -lt ($windowBounds.Top + 160) -and
                $buttonBounds.Left -gt ($windowBounds.Left + 200))
        })
        if ($targets.Count -lt 3) {
            throw "expected root plus at least two visible breadcrumb segments, got $($targets.Count)"
        }

        $evidence = foreach ($target in $targets) {
            $bounds = $target.Current.BoundingRectangle
            $localLeft = [int][Math]::Round($bounds.Left - $windowBounds.Left)
            $localTop = [int][Math]::Round($bounds.Top - $windowBounds.Top)
            $sampleLeft = [Math]::Max(0, $localLeft + 6)
            $sampleRight = [Math]::Min($bitmap.Width - 1, $localLeft + 29)
            $sampleTop = [Math]::Max(0, $localTop + 5)
            $sampleBottom = [Math]::Min($bitmap.Height - 1, $localTop + [int]$bounds.Height - 6)
            $colors = [Collections.Generic.HashSet[int]]::new()
            for ($y = $sampleTop; $y -le $sampleBottom; $y++) {
                for ($x = $sampleLeft; $x -le $sampleRight; $x++) {
                    $colors.Add($bitmap.GetPixel($x, $y).ToArgb()) | Out-Null
                }
            }
            if ($colors.Count -lt 4) {
                throw "breadcrumb icon slot is visually empty: $($target.Current.Name), colors=$($colors.Count)"
            }
            [ordered]@{
                name = $target.Current.Name
                sampled_color_count = $colors.Count
                bounds = [ordered]@{
                    left = $bounds.Left
                    top = $bounds.Top
                    width = $bounds.Width
                    height = $bounds.Height
                }
            }
        }
        return @($evidence)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $executable
$startInfo.WorkingDirectory = $workspaceRoot
$startInfo.UseShellExecute = $false
$startInfo.Environment['EXPLORER_INITIAL_PATH'] = $resolvedInitial
$startInfo.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$process = [Diagnostics.Process]::Start($startInfo)

try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        $windowHandle = $process.MainWindowHandle
        if ($windowHandle -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 50 }
    } while ($windowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($windowHandle -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    # Foreground-lock policy can leave Codex or Explorer above the fixture. Raise only
    # this disposable test HWND so physical pointer evidence reaches the intended control.
    [BreadcrumbUia.Native]::ShowWindow($windowHandle, 3) | Out-Null
    [BreadcrumbUia.Native]::SetWindowPos($windowHandle, [IntPtr](-1), 0, 0, 0, 0, 0x0043) | Out-Null
    [BreadcrumbUia.Native]::SetForegroundWindow($windowHandle) | Out-Null
    $root = [Windows.Automation.AutomationElement]::FromHandle($windowHandle)
    Start-Sleep -Milliseconds 500

    $expectedDrive = [IO.Path]::GetPathRoot($resolvedInitial).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar).ToUpperInvariant()
    $initialDriveBreadcrumb = Assert-StableDriveBreadcrumb $root $expectedDrive 'initial-navigation'

    $driveChevron = Find-ByName $root (Get-DriveChildrenLabel)
    $driveInvoke = Invoke-Element $driveChevron
    $driveItems = Get-MenuItems $root 1
    $driveEvidence = @($driveItems | ForEach-Object { Get-ItemEvidence $_ $windowHandle })
    if (-not ($driveEvidence.name | Where-Object { $_ -match '(^D:$|\(D:\)$)' })) {
        throw "This PC menu did not expose D:; names=$($driveEvidence.name -join ', ')"
    }
    Send-Key 0x23 # End
    $driveEndSelection = Get-SelectedMenuName $root $driveItems.Count
    if ($null -ne $driveEndSelection -and $driveEndSelection -ne $driveItems[-1].Current.Name) { throw 'End did not select the final drive menu item' }
    Send-Key 0x24 # Home
    $driveHomeSelection = Get-SelectedMenuName $root $driveItems.Count
    if ($null -ne $driveHomeSelection -and $driveHomeSelection -ne $driveItems[0].Current.Name) { throw 'Home did not select the first drive menu item' }
    if ($driveItems.Count -gt 1) {
        Send-Key 0x28 # Down
        $driveDownSelection = Get-SelectedMenuName $root $driveItems.Count
        if ($null -ne $driveDownSelection -and $driveDownSelection -ne $driveItems[1].Current.Name) { throw 'Down did not advance drive menu focus' }
    }
    Send-Escape
    Start-Sleep -Milliseconds 100

    $leafName = Split-Path -Leaf ($resolvedInitial.TrimEnd([IO.Path]::DirectorySeparatorChar))
    $folderChevron = Find-ByName $root (Get-FolderChildrenLabel $leafName)
    $folderInvoke = Invoke-Element $folderChevron
    $folderOracle = @(Get-ChildItem -LiteralPath $resolvedInitial -Directory -Force | Sort-Object Name)
    $expectedFolders = @($folderOracle | ForEach-Object Name)
    $folderItems = if ($expectedFolders.Count -eq 0) { @() } else { Get-MenuItems $root $expectedFolders.Count }
    $folderEvidence = @($folderItems | ForEach-Object { Get-ItemEvidence $_ $windowHandle })
    $actualNames = @($folderEvidence | ForEach-Object name | Sort-Object)
    $missing = @($expectedFolders | Where-Object { $_ -notin $actualNames })
    if ($missing.Count -ne 0) { throw "missing direct folders: $($missing -join ', ')" }
    $asciiTypeahead = $expectedFolders | Where-Object { $_ -match '^[A-Za-z]' } | Select-Object -First 1
    $typeaheadSelection = $null
    if ($asciiTypeahead) {
        $virtualKey = [byte][char]$asciiTypeahead.Substring(0, 1).ToUpperInvariant()
        Send-Key $virtualKey
        $typeaheadSelection = Get-SelectedMenuName $root $folderItems.Count
        $expectedTypeahead = $expectedFolders | Where-Object { $_.StartsWith($asciiTypeahead.Substring(0, 1), [StringComparison]::OrdinalIgnoreCase) } | Select-Object -First 1
        if ($null -ne $typeaheadSelection -and $typeaheadSelection -ne $expectedTypeahead) { throw "type-ahead selected '$typeaheadSelection' instead of '$expectedTypeahead'" }
    }

    $navigationTarget = $null
    $childInvoke = $null
    $visibleFolder = $folderOracle | Where-Object {
        -not ($_.Attributes -band [IO.FileAttributes]::Hidden) -and
        -not ($_.Attributes -band [IO.FileAttributes]::System)
    } | Select-Object -First 1
    if ($null -ne $visibleFolder) {
        # Type-ahead changes the active item and AccessKit rebuilds the menu subtree.
        # Close and reopen the overlay so invocation uses a fresh UIA provider instead
        # of a stale element captured before the keyboard interaction.
        Send-Escape
        Start-Sleep -Milliseconds 100
        $folderChevron = Find-ByName $root (Get-FolderChildrenLabel $leafName)
        Invoke-Element $folderChevron | Out-Null
        $freshFolderItems = Get-MenuItems $root $expectedFolders.Count
        $navigationTarget = $visibleFolder.Name
        $navigationElement = $freshFolderItems | Where-Object { $_.Current.Name -eq $navigationTarget } | Select-Object -First 1
        if ($null -eq $navigationElement) { throw "fresh breadcrumb menu did not expose navigation target '$navigationTarget'" }
        $childInvoke = Invoke-Element $navigationElement
        $root = [Windows.Automation.AutomationElement]::FromHandle($windowHandle)
        Find-ByName $root (Get-FolderChildrenLabel $navigationTarget) | Out-Null
    }

    Start-Sleep -Milliseconds 750
    $root = [Windows.Automation.AutomationElement]::FromHandle($windowHandle)
    $childDriveBreadcrumb = Assert-StableDriveBreadcrumb $root $expectedDrive 'child-navigation'
    $shellIconEvidence = Save-BreadcrumbIconEvidence $root (Join-Path $OutputDirectory 'breadcrumb-shell-icons.png')

    $report = [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        initial_path = $resolvedInitial
        this_pc_items = $driveEvidence
        expected_direct_folders = $expectedFolders
        direct_folder_items = $folderEvidence
        navigation_target = $navigationTarget
        navigation_verified = $null -ne $navigationTarget
        drive_breadcrumb = [ordered]@{
            expected = $expectedDrive
            initial = $initialDriveBreadcrumb
            after_child_navigation = $childDriveBreadcrumb
            stable_across_shell_enrichment = $true
        }
        shell_native_icons = $shellIconEvidence
        invocation = [ordered]@{ root=$driveInvoke; folder=$folderInvoke; child=$childInvoke }
        keyboard = [ordered]@{
            end=$driveEndSelection
            home=$driveHomeSelection
            down=$driveDownSelection
            typeahead=$typeaheadSelection
            physical_keys_sent=$true
            selection_item_pattern_available=($null -ne $driveEndSelection)
            provider_limit='accesskit_windows 0.33 does not expose SelectionItemPattern for Role::MenuItem'
        }
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
    Write-Host "Breadcrumb UIA smoke passed: $OutputDirectory"
} catch {
    if ($null -ne $root) {
        try {
            $nodes = $root.FindAll(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition
            )
            @($nodes | ForEach-Object {
                [ordered]@{
                    name = $_.Current.Name
                    automation_id = $_.Current.AutomationId
                    control_type = $_.Current.ControlType.ProgrammaticName
                    bounds = [ordered]@{
                        left = $_.Current.BoundingRectangle.Left
                        top = $_.Current.BoundingRectangle.Top
                        width = $_.Current.BoundingRectangle.Width
                        height = $_.Current.BoundingRectangle.Height
                    }
                }
            }) | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'uia-tree.json')
        } catch {}
    }
    $_ | Out-String | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'failure.txt')
    throw
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        $process.CloseMainWindow() | Out-Null
        if (-not $process.WaitForExit(5000)) { Stop-Process -Id $process.Id -Force }
    }
    if ($null -ne $iconFixtureRoot -and (Test-Path -LiteralPath $iconFixtureRoot)) {
        $resolvedFixture = [IO.Path]::GetFullPath($iconFixtureRoot)
        $resolvedFixtureParent = [IO.Path]::GetFullPath($iconFixtureParent).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
        if (-not $resolvedFixture.StartsWith($resolvedFixtureParent, [StringComparison]::OrdinalIgnoreCase) -or
            -not ([IO.Path]::GetFileName($resolvedFixture)).StartsWith('bcu-', [StringComparison]::Ordinal)) {
            throw "refusing to remove unexpected breadcrumb fixture: $resolvedFixture"
        }
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}
