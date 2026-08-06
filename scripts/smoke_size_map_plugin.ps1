param(
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$PluginDll = 'sdk\fixtures\rust-folder-size-map-view\target\x86_64-pc-windows-msvc\debug\rust_folder_size_map_view.dll',
    [string]$InitialPath = '.',
    [string]$OutputDirectory = 'target\size-map-smoke',
    [switch]$UsePointerActivation,
    [switch]$UseExistingPath,
    [switch]$CaptureOnly,
    [int]$ProgressiveCaptureSeconds = 0
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
foreach ($name in 'Executable', 'PluginDll', 'InitialPath', 'OutputDirectory') {
    $value = Get-Variable -Name $name -ValueOnly
    if (-not [IO.Path]::IsPathRooted($value)) {
        Set-Variable -Name $name -Value ([IO.Path]::GetFullPath((Join-Path $workspaceRoot $value)))
    }
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$PluginDll = (Resolve-Path -LiteralPath $PluginDll).Path
$InitialPath = (Resolve-Path -LiteralPath $InitialPath).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $UseExistingPath) {
    $aggregateFixture = Join-Path $OutputDirectory 'aggregate-fixture'
    if (Test-Path -LiteralPath $aggregateFixture) {
        throw "UITEST output must be fresh; aggregate fixture already exists: $aggregateFixture"
    }
    Copy-Item -LiteralPath $InitialPath -Destination $aggregateFixture -Recurse
    foreach ($index in 0..249) {
        [IO.File]::WriteAllText(
            (Join-Path $aggregateFixture ('tiny-{0:D4}.txt' -f $index)),
            'x',
            [Text.UTF8Encoding]::new($false))
    }
    foreach ($index in 0..9) {
        [IO.File]::WriteAllText(
            (Join-Path $aggregateFixture ('aaa-omitted-{0:D4}.txt' -f $index)),
            '',
            [Text.UTF8Encoding]::new($false))
    }
    $InitialPath = (Resolve-Path -LiteralPath $aggregateFixture).Path
}
$profileRoot = Join-Path $OutputDirectory 'profile'
$localAppData = Join-Path $profileRoot 'LocalAppData'
$roamingAppData = Join-Path $profileRoot 'AppData'
$extensionState = Join-Path $profileRoot 'ExtensionState'
New-Item -ItemType Directory -Force -Path $localAppData,$roamingAppData,$extensionState | Out-Null

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ('SizeMapSmoke.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace SizeMapSmoke {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
        [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    }
}
'@
}

function Capture-Window([IntPtr]$Window, [string]$Path) {
    [void][SizeMapSmoke.Native]::DwmFlush()
    $rect = [SizeMapSmoke.Native+Rect]::new()
    if (-not [SizeMapSmoke.Native]::GetWindowRect($Window, [ref]$rect)) {
        throw 'GetWindowRect failed'
    }
    $bitmap = [Drawing.Bitmap]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try {
                if (-not [SizeMapSmoke.Native]::PrintWindow($Window, $dc, 2)) {
                    throw 'PrintWindow failed'
                }
            } finally { $graphics.ReleaseHdc($dc) }
        } finally { $graphics.Dispose() }
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
}

function Find-NamedElement($Root, [string]$Name) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Find-ControlTypeName($Root, $ControlType, [string]$Name) {
    $condition = [Windows.Automation.AndCondition]::new(@(
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            $ControlType),
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty,
            $Name)
    ))
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Find-AutomationId($Root, [string]$Id) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::AutomationIdProperty,
        $Id)
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Invoke-Element($Root, $Element, [string]$Description, [switch]$PointerOnly) {
    if ($null -eq $Element) { throw "UI element '$Description' was not found" }
    if (-not $PointerOnly) {
        try {
            $pattern = $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
            $pattern.Invoke()
            return
        } catch [InvalidOperationException] {
            # Fall through to a real pointer click for controls without InvokePattern.
        }
    }
    & {
        $bounds = $Element.Current.BoundingRectangle
        $windowRect = [SizeMapSmoke.Native+Rect]::new()
        if (-not [SizeMapSmoke.Native]::GetWindowRect($window, [ref]$windowRect)) {
            throw 'GetWindowRect failed while activating menu item'
        }
        # GPUI descendants currently expose logical coordinates even though
        # the UIA root is physical. Use the deterministic fixture dimensions
        # instead of deriving scale from the root bounds.
        $scaleX = ($windowRect.Right - $windowRect.Left) / 1120.0
        $scaleY = ($windowRect.Bottom - $windowRect.Top) / 720.0
        $screenX = [int]($windowRect.Left + ($bounds.Left + $bounds.Width / 2) * $scaleX)
        $screenY = [int]($windowRect.Top + ($bounds.Top + $bounds.Height / 2) * $scaleY)
        [void][SizeMapSmoke.Native]::SetCursorPos($screenX, $screenY)
        Start-Sleep -Milliseconds 50
        [SizeMapSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [SizeMapSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
}

function Invoke-NamedElement($Root, [string]$Name, [switch]$PointerOnly) {
    Invoke-Element $Root (Find-NamedElement $Root $Name) $Name -PointerOnly:$PointerOnly
}

function Invoke-AutomationId($Root, [string]$Id, [switch]$PointerOnly) {
    Invoke-Element $Root (Find-AutomationId $Root $Id) $Id -PointerOnly:$PointerOnly
}

function Find-DetailsViewElement($Root) {
    $extensionEntry = Find-NamedElement $Root 'Size Map'
    if ($null -eq $extensionEntry) { return $null }
    $extensionBounds = $extensionEntry.Current.BoundingRectangle
    $buttons = $Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button))
    $builtInModes = @(0..($buttons.Count - 1) |
        ForEach-Object { $buttons.Item($_) } |
        Where-Object {
            $bounds = $_.Current.BoundingRectangle
            $bounds.Top -lt $extensionBounds.Top -and
            [Math]::Abs($bounds.Left - $extensionBounds.Left) -lt 4 -and
            [Math]::Abs($bounds.Width - $extensionBounds.Width) -lt 4
        } |
        Sort-Object { $_.Current.BoundingRectangle.Top })
    if ($builtInModes.Count -lt 8) { return $null }
    # The host's public view-mode order is ExtraLarge, Large, Medium, Small,
    # List, Details, Tiles, Content. Select the actual sixth UIA element so
    # menu focus and unrelated toolbar buttons cannot affect activation.
    $builtInModes[$builtInModes.Count - 8 + 5]
}

function Find-MoreOptionsElement($Root) {
    $items = $Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::MenuItem))
    $ordered = @(0..($items.Count - 1) |
        ForEach-Object { $items.Item($_) } |
        Where-Object { $_.Current.BoundingRectangle.Height -gt 0 } |
        Sort-Object { $_.Current.BoundingRectangle.Top })
    if ($ordered.Count -lt 2) { return $null }
    # Options and About are the final two commands in the production More menu.
    $ordered[$ordered.Count - 2]
}

function Invoke-NamedElementDoubleClick($Root, [string]$Name) {
    $element = Find-NamedElement $Root $Name
    if ($null -eq $element) { throw "UI element '$Name' was not found for double-click" }
    $bounds = $element.Current.BoundingRectangle
    $rootBounds = $Root.Current.BoundingRectangle
    $windowRect = [SizeMapSmoke.Native+Rect]::new()
    if (-not [SizeMapSmoke.Native]::GetWindowRect($window, [ref]$windowRect)) {
        throw 'GetWindowRect failed while double-clicking Size Map node'
    }
    $scaleX = ($windowRect.Right - $windowRect.Left) / $rootBounds.Width
    $scaleY = ($windowRect.Bottom - $windowRect.Top) / $rootBounds.Height
    $screenX = [int]($windowRect.Left + (($bounds.Left + $bounds.Width / 2) - $rootBounds.Left) * $scaleX)
    $screenY = [int]($windowRect.Top + (($bounds.Top + $bounds.Height / 2) - $rootBounds.Top) * $scaleY)
    [void][SizeMapSmoke.Native]::SetForegroundWindow($window)
    [void][SizeMapSmoke.Native]::SetCursorPos($screenX, $screenY)
    Start-Sleep -Milliseconds 150
    foreach ($click in 1..2) {
        [SizeMapSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [SizeMapSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 120
    }
}

function Send-Key([byte]$VirtualKey) {
    # GPUI consumes real keyboard input; posted WM_KEYDOWN messages bypass its
    # raw-input path and are not equivalent to a user keystroke.
    [SizeMapSmoke.Native]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
    [SizeMapSmoke.Native]::keybd_event($VirtualKey, 0, 2, [UIntPtr]::Zero)
}

$diagnostics = Join-Path $OutputDirectory 'diagnostics.json'
$beforePath = Join-Path $OutputDirectory 'details-before.png'
$sizeMapPath = Join-Path $OutputDirectory 'size-map.png'
$selectedPath = Join-Path $OutputDirectory 'size-map-selected.png'
$afterRefreshPath = Join-Path $OutputDirectory 'size-map-after-f5.png'
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $Executable
$start.Arguments = "--plugin-dll `"$PluginDll`""
$start.WorkingDirectory = $workspaceRoot
$start.UseShellExecute = $false
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.EnvironmentVariables['EXPLORER_VISUAL_FIXTURE'] = '1'
$start.EnvironmentVariables['EXPLORER_VISUAL_REAL_SHELL'] = '1'
$start.EnvironmentVariables['EXPLORER_VISUAL_WIDTH'] = '1120'
$start.EnvironmentVariables['EXPLORER_VISUAL_HEIGHT'] = '720'
$start.EnvironmentVariables['EXPLORER_VISUAL_DPI'] = '175'
$start.EnvironmentVariables['EXPLORER_VISUAL_THEME'] = 'light'
$start.EnvironmentVariables['EXPLORER_VISUAL_FONT'] = 'Microsoft JhengHei UI'
$start.EnvironmentVariables['EXPLORER_VISUAL_STATE'] = 'populated'
$start.EnvironmentVariables['EXPLORER_VISUAL_DIAGNOSTICS'] = $diagnostics
$start.EnvironmentVariables['EXPLORER_INITIAL_PATH'] = $InitialPath
$start.EnvironmentVariables['EXPLORER_LOG_DIR'] = $OutputDirectory
$start.EnvironmentVariables['LOCALAPPDATA'] = $localAppData
$start.EnvironmentVariables['APPDATA'] = $roamingAppData
$start.EnvironmentVariables['EXPLORER_UITEST_EXTENSION_STATE_ROOT'] = $extensionState
$process = [Diagnostics.Process]::Start($start)
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()

try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        $window = $process.MainWindowHandle
    } while (($window -eq [IntPtr]::Zero -or -not (Test-Path -LiteralPath $diagnostics)) -and [DateTime]::UtcNow -lt $deadline)
    if ($window -eq [IntPtr]::Zero) { throw 'Timed out waiting for the SuperExplorer window' }
    [void][SizeMapSmoke.Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    [void][SizeMapSmoke.Native]::SetForegroundWindow($window)
    $root = [Windows.Automation.AutomationElement]::FromHandle($window)

    $rows = $null
    $rowsDeadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 100
        $rows = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::ListItem))
    } while ($rows.Count -eq 0 -and [DateTime]::UtcNow -lt $rowsDeadline)
    if ($rows.Count -eq 0) { throw 'Real folder contents did not load in Details view' }
    Capture-Window $window $beforePath

    # GPUI exposes InvokePattern for the toolbar button, but invoking that
    # pattern may only focus the control without opening its flyout.  Exercise
    # the same pointer behavior a user relies on before asserting menu items.
    Invoke-NamedElement $root 'View' -PointerOnly
    $viewEntry = $null
    $viewDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $viewEntry = Find-NamedElement $root 'Size Map'
    } while ($null -eq $viewEntry -and [DateTime]::UtcNow -lt $viewDeadline)
    Capture-Window $window (Join-Path $OutputDirectory 'view-menu.png')
    if ($null -eq $viewEntry) {
        throw "View menu did not expose the loaded plugin's Size Map entry"
    }
    if ($UsePointerActivation) {
        Invoke-NamedElement $root 'Size Map' -PointerOnly
    } else {
        Send-Key 0x23 # End: dynamically appended Size Map item.
        Start-Sleep -Milliseconds 100
        Send-Key 0x0D # Enter
    }

    $node = $null
    $renderTimeoutSeconds = if ($CaptureOnly) { 180 } else { 12 }
    $deadline = [DateTime]::UtcNow.AddSeconds($renderTimeoutSeconds)
    do {
        Start-Sleep -Milliseconds 100
        $buttons = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Button))
        $node = 0..($buttons.Count - 1) |
            ForEach-Object { $buttons.Item($_) } |
            Where-Object { $_.Current.Name -match '\d+(\.\d+)?%.*Complete' } |
            Select-Object -First 1
    } while ($null -eq $node -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $node) {
        Capture-Window $window (Join-Path $OutputDirectory 'activation-failure.png')
        $visibleNames = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition) |
            ForEach-Object { $_.Current.Name } |
            Where-Object { $_ -match 'Size Map|Exact|%' } |
            Select-Object -Unique
        throw "Size Map did not expose a rendered percentage node; visible markers: $($visibleNames -join ', ')"
    }
    if ($CaptureOnly) {
        if ($ProgressiveCaptureSeconds -gt 0) {
            Start-Sleep -Seconds $ProgressiveCaptureSeconds
            Capture-Window $window $sizeMapPath
            [pscustomobject]@{
                status = 'passed'
                case_id = 'size-map-progressive-capture'
                initial_path = $InitialPath
                screenshots = @($sizeMapPath)
            } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDirectory 'report.json') -Encoding utf8
            return
        }
        $exactDeadline = [DateTime]::UtcNow.AddSeconds(300)
        $methodSeen = $false
        $completed = $false
        do {
            Start-Sleep -Milliseconds 250
            $methodSeen = $methodSeen -or
                ($null -ne (Find-NamedElement $root 'Calculating sizes · NTFS MFT')) -or
                ($null -ne (Find-NamedElement $root 'Calculating sizes · Breadth-first fallback')) -or
                ($null -ne (Find-NamedElement $root 'Calculating sizes · Detecting scan method'))
            if ($methodSeen) {
                $stillCalculating = $null -ne (Find-NamedElement $root 'Calculating sizes · NTFS MFT') -or
                    $null -ne (Find-NamedElement $root 'Calculating sizes · Breadth-first fallback') -or
                    $null -ne (Find-NamedElement $root 'Calculating sizes · Detecting scan method')
                $completed = -not $stillCalculating
            }
        } while (-not $completed -and [DateTime]::UtcNow -lt $exactDeadline)
        if (-not $completed) {
            Capture-Window $window (Join-Path $OutputDirectory 'size-map-incomplete.png')
            throw 'Size Map did not clear its method status after completing'
        }
        Capture-Window $window $sizeMapPath
        [pscustomobject]@{
            status = 'passed'
            case_id = 'size-map-capture-only'
            initial_path = $InitialPath
            screenshots = @($sizeMapPath)
        } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDirectory 'report.json') -Encoding utf8
        return
    }
    $nodeNames = 0..($buttons.Count - 1) |
        ForEach-Object { $buttons.Item($_).Current.Name }
    $expectedNodes = Get-ChildItem -LiteralPath $InitialPath -Force | ForEach-Object {
        $bytes = if ($_.PSIsContainer) {
            $sum = Get-ChildItem -LiteralPath $_.FullName -Force -Recurse -File |
                Measure-Object -Property Length -Sum
            if ($null -eq $sum.Sum) { 0L } else { [long]$sum.Sum }
        } else {
            [long]$_.Length
        }
        [pscustomobject]@{ Name = $_.Name; Bytes = $bytes }
    }
    foreach ($expected in $expectedNodes) {
        $prefix = '^' + [regex]::Escape("$($expected.Name): $($expected.Bytes) bytes")
        if (-not ($nodeNames | Where-Object { $_ -match $prefix -and $_ -match 'Complete$' })) {
            throw "Size Map did not expose the exact recursive total for $($expected.Name): $($expected.Bytes) bytes"
        }
    }
    $nodeName = $node.Current.Name
    $other = $root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) |
        ForEach-Object { $_ } |
        Where-Object { $_.Current.Name -match '^Other \(\d+ items\): .*Aggregated$' } |
        Select-Object -First 1
    if ($null -eq $other) { throw 'Size Map did not expose the aggregated Other accessibility group' }
    Capture-Window $window $sizeMapPath
    Invoke-NamedElement $root $nodeName -PointerOnly
    $selectionChanged = $false
    $selectionDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        Capture-Window $window $selectedPath
        $selectionChanged = (Get-FileHash -LiteralPath $sizeMapPath).Hash -ne (Get-FileHash -LiteralPath $selectedPath).Hash
    } while (-not $selectionChanged -and [DateTime]::UtcNow -lt $selectionDeadline)
    if (-not $selectionChanged) {
        throw 'Selecting a Size Map node did not update its host-owned GPUI surface'
    }

    $aggregateItemName = 'aaa-omitted-0009.txt: 0 bytes. Complete'
    $aggregateItem = Find-NamedElement $root $aggregateItemName
    if ($null -eq $aggregateItem) { throw 'Aggregated item was not retained in the UIA/search tree' }
    if (-not $aggregateItem.Current.IsKeyboardFocusable) { throw 'Aggregated item is not keyboard focusable' }
    Invoke-NamedElement $root $aggregateItemName
    Start-Sleep -Milliseconds 150
    $selectedLabel = 'aaa-omitted-0009.txt'
    Invoke-NamedElement $root 'View'
    $detailsEntry = $null
    $detailsEntryDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $detailsEntry = Find-DetailsViewElement $root
    } while ($null -eq $detailsEntry -and [DateTime]::UtcNow -lt $detailsEntryDeadline)
    Invoke-Element $root $detailsEntry 'Details view choice' -PointerOnly
    $detailsRows = $null
    $detailsDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $detailsRows = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::ListItem))
    } while ($detailsRows.Count -eq 0 -and [DateTime]::UtcNow -lt $detailsDeadline)
    @($detailsRows | ForEach-Object {
        [pscustomobject]@{
            name = $_.Current.Name
            selected = $_.GetCurrentPropertyValue([Windows.Automation.SelectionItemPattern]::IsSelectedProperty, $true)
            automation_id = $_.Current.AutomationId
        }
    }) | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'details-selection.json') -Encoding utf8
    $sharedSelection = 0..($detailsRows.Count - 1) |
        ForEach-Object { $detailsRows.Item($_) } |
        Where-Object {
            $_.Current.Name -match [regex]::Escape($selectedLabel) -and
            $_.GetCurrentPropertyValue([Windows.Automation.SelectionItemPattern]::IsSelectedProperty, $true) -eq $true
        } |
        Select-Object -First 1
    if ($null -eq $sharedSelection) {
        throw "Size Map selection for '$selectedLabel' was not shared with Details"
    }
    Invoke-NamedElement $root 'View'
    $sizeMapEntry = $null
    $sizeMapEntryDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $sizeMapEntry = Find-NamedElement $root 'Size Map'
    } while ($null -eq $sizeMapEntry -and [DateTime]::UtcNow -lt $sizeMapEntryDeadline)
    Invoke-Element $root $sizeMapEntry 'Size Map' -PointerOnly

    $largeNode = $null
    $largeDeadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        Start-Sleep -Milliseconds 100
        $largeNode = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Button)) |
            ForEach-Object { $_ } |
            Where-Object { $_.Current.Name -match '^large: .*Complete$' } |
            Select-Object -First 1
    } while ($null -eq $largeNode -and [DateTime]::UtcNow -lt $largeDeadline)
    if ($null -eq $largeNode) { throw 'Deterministic large folder node was not available for navigation' }
    Invoke-NamedElementDoubleClick $root $largeNode.Current.Name
    $navigationDeadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        Start-Sleep -Milliseconds 100
        $nestedNode = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Button)) |
            ForEach-Object { $_ } |
            Where-Object { $_.Current.Name -match '^nested: .*Complete$' } |
            Select-Object -First 1
    } while ($null -eq $nestedNode -and [DateTime]::UtcNow -lt $navigationDeadline)
    if ($null -eq $nestedNode) { throw 'Double-clicking a Size Map folder did not navigate through the formal host path' }
    $back = Find-AutomationId $root 'navigation-back'
    if ($null -eq $back) { $back = Find-NamedElement $root 'Back' }
    if ($null -eq $back) { throw 'Formal Back navigation was unavailable after entering the Size Map folder' }
    Invoke-NamedElement $root $back.Current.Name
    Start-Sleep -Milliseconds 400

    Send-Key 0x74
    Start-Sleep -Seconds 2
    $refreshed = $root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button))
    $hasRefreshedNode = 0..($refreshed.Count - 1) |
        ForEach-Object { $refreshed.Item($_) } |
        Where-Object { $_.Current.Name -match '\d+(\.\d+)?%.*Complete' } |
        Select-Object -First 1
    if ($null -eq $hasRefreshedNode) { throw 'Size Map did not recover after F5' }
    Capture-Window $window $afterRefreshPath

    # Disable the active extension through the production Folder Options UI.
    # The active custom view must immediately fall back to Details and the
    # disabled contribution must disappear from View.
    $moreButton = Find-AutomationId $root 'command-more-menu'
    if ($null -eq $moreButton) {
        $moreName = [string]([char]0x5176) + [char]0x5B83
        $moreButton = Find-NamedElement $root $moreName
    }
    Invoke-Element $root $moreButton 'command-more-menu'
    $optionsEntry = $null
    $optionsDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $optionsEntry = Find-AutomationId $root 'more-options'
        if ($null -eq $optionsEntry) { $optionsEntry = Find-MoreOptionsElement $root }
    } while ($null -eq $optionsEntry -and [DateTime]::UtcNow -lt $optionsDeadline)
    Invoke-Element $root $optionsEntry 'more-options'
    $extensionsTab = $null
    $extensionsDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $extensionsTab = Find-AutomationId $root 'folder-options-extensions-tab'
        if ($null -eq $extensionsTab) { $extensionsTab = Find-NamedElement $root 'Extensions' }
    } while ($null -eq $extensionsTab -and [DateTime]::UtcNow -lt $extensionsDeadline)
    Invoke-Element $root $extensionsTab 'folder-options-extensions-tab'
    $sizeMapToggle = $null
    $toggleDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $sizeMapToggle = Find-ControlTypeName $root ([Windows.Automation.ControlType]::CheckBox) 'Size Map'
    } while ($null -eq $sizeMapToggle -and [DateTime]::UtcNow -lt $toggleDeadline)
    Invoke-Element $root $sizeMapToggle 'Size Map extension toggle'
    $applyButton = Find-AutomationId $root 'folder-options-apply'
    if ($null -eq $applyButton) {
        $applyName = [string]([char]0x5957) + [char]0x7528
        $applyButton = Find-NamedElement $root $applyName
    }
    Invoke-Element $root $applyButton 'folder-options-apply'
    Start-Sleep -Milliseconds 200
    $okName = [string]([char]0x78BA) + [char]0x5B9A
    $okButton = Find-NamedElement $root $okName
    Invoke-Element $root $okButton 'folder-options-ok'
    $fallbackRows = $null
    $fallbackDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $fallbackRows = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::ListItem))
    } while (($fallbackRows.Count -eq 0 -or $null -ne (Find-NamedElement $root 'Extensions')) -and [DateTime]::UtcNow -lt $fallbackDeadline)
    if ($fallbackRows.Count -eq 0 -or $null -ne (Find-NamedElement $root 'Extensions')) {
        throw 'Disabling the active Size Map did not close Folder Options and fall back to Details'
    }
    Invoke-NamedElement $root 'View'
    Start-Sleep -Milliseconds 200
    if ($null -ne (Find-NamedElement $root 'Size Map')) {
        throw 'Disabled Size Map contribution remained visible in View'
    }
    Send-Key 0x1B

    $report = [pscustomobject]@{
        status = 'passed'
        initial_path = $InitialPath
        details_rows = $rows.Count
        size_map_node = $nodeName
        exact_nodes = @($expectedNodes)
        selection_shared_with_details = $true
        aggregated_other_accessible = $true
        aggregated_item_keyboard_focusable = $true
        folder_navigation_and_back = $true
        disable_active_falls_back_to_details = $true
        disabled_contribution_hidden = $true
        screenshots = @($beforePath, $sizeMapPath, $selectedPath, $afterRefreshPath)
    }
    $json = $report | ConvertTo-Json -Depth 3
    Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Value $json -Encoding utf8
    $json
} finally {
    if (-not $process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
    $stdoutTask.Result | Set-Content -LiteralPath (Join-Path $OutputDirectory 'stdout.log') -Encoding utf8
    $stderrTask.Result | Set-Content -LiteralPath (Join-Path $OutputDirectory 'stderr.log') -Encoding utf8
}
