param(
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$Executable,
    [ValidateRange(2, 20)][int]$RestartRuns = 10
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$Executable = [IO.Path]::GetFullPath($Executable)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
if (-not ('RoadmapSession.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace RoadmapSession {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr LoadKeyboardLayout(string id, uint flags);
        [DllImport("user32.dll")] public static extern IntPtr ActivateKeyboardLayout(IntPtr layout, uint flags);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, IntPtr processId);
        [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint threadId);
    }
}
'@
}

$runId = 'session-' + [guid]::NewGuid().ToString('N')
$systemFixtureParent = Join-Path ([IO.Path]::GetTempPath()) 'RustGpuiExplorerSessionUITest'
$systemFixture = Join-Path $systemFixtureParent "$runId-system"
$workspaceFixtureParent = Join-Path $workspaceRoot 'target\session-restore-fixtures'
$workspaceFixture = Join-Path $workspaceFixtureParent "$runId-workspace"
$isolatedLocalAppData = Join-Path $OutputDirectory 'isolated-local-app-data'
$statePath = Join-Path $isolatedLocalAppData 'RustGpuiExplorer\state\v1\session.json'

function Assert-OwnedPath([string]$Path, [string]$Parent) {
    $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $fullParent = [IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
    if (-not $fullPath.StartsWith($fullParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing non-owned fixture path: $fullPath"
    }
}

function Send-Key([byte]$Key, [byte[]]$Modifiers = @()) {
    foreach ($modifier in $Modifiers) {
        [RoadmapSession.Native]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero)
    }
    [RoadmapSession.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [RoadmapSession.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    for ($index = $Modifiers.Count - 1; $index -ge 0; $index--) {
        [RoadmapSession.Native]::keybd_event($Modifiers[$index], 0, 2, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 180
}

function Set-EnglishInput([IntPtr]$WindowHandle) {
    $english = [RoadmapSession.Native]::LoadKeyboardLayout('00000409', 1)
    if ($english -eq [IntPtr]::Zero) { throw 'failed to load English (US) keyboard layout' }
    [void][RoadmapSession.Native]::ActivateKeyboardLayout($english, 0)
    if (-not [RoadmapSession.Native]::PostMessage($WindowHandle, 0x0050, [IntPtr]::Zero, $english)) {
        throw 'failed to request English (US) input for explorer window'
    }
    $threadId = [RoadmapSession.Native]::GetWindowThreadProcessId($WindowHandle, [IntPtr]::Zero)
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        $active = [RoadmapSession.Native]::GetKeyboardLayout($threadId).ToInt64() -band 0xFFFF
        if ($active -eq 0x0409) { return }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw ('explorer input language did not switch to English (US); active LANGID=0x{0:X4}' -f $active)
}

function Find-Element(
    [Windows.Automation.AutomationElement]$Root,
    [scriptblock]$Predicate,
    [string]$Description,
    [int]$Seconds = 10
) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        foreach ($element in $Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition
        )) {
            try { if (& $Predicate $element) { return $element } } catch { }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $Description"
}

function Wait-FileRow([Windows.Automation.AutomationElement]$Root, [string]$Name) {
    Find-Element $Root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
            $element.Current.Name -like "*$Name*" -and
            $element.Current.BoundingRectangle.Left -gt 250
    } "file row '$Name'" 12
}

function Assert-NoDisconnectedDirectory([Windows.Automation.AutomationElement]$Root, [string]$Stage) {
    foreach ($element in $Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    )) {
        if ($element.Current.Name -eq 'Directory service is not connected' -and
            $element.Current.BoundingRectangle.Width -gt 0 -and
            $element.Current.BoundingRectangle.Height -gt 0) {
            throw "directory remained disconnected after $Stage"
        }
    }
}

function Click-Element([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
        Start-Sleep -Milliseconds 300
        return
    }
    if ($Element.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.SelectionItemPattern]$pattern).Select()
        Start-Sleep -Milliseconds 300
        return
    }
    $point = $Element.GetClickablePoint()
    [Windows.Forms.Cursor]::Position = [Drawing.Point]::new([int]$point.X, [int]$point.Y)
    [Windows.Forms.SendKeys]::SendWait('{ENTER}')
    Start-Sleep -Milliseconds 300
}

function Set-Address(
    [Diagnostics.Process]$Process,
    [Windows.Automation.AutomationElement]$Root,
    [string]$Location,
    [string]$ExpectedRow
) {
    [void][RoadmapSession.Native]::SetForegroundWindow($Process.MainWindowHandle)
    Set-EnglishInput $Process.MainWindowHandle
    Send-Key 0x1B
    Send-Key 0x4C @(0x11)
    $rootTop = $Root.Current.BoundingRectangle.Top
    $editor = Find-Element $Root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $element.Current.BoundingRectangle.Top -lt ($rootTop + 260)
    } 'address editor' 5
    $editor.SetFocus()
    Start-Sleep -Milliseconds 80
    # Drive the real editor keyboard path. ValuePattern.SetValue replaces the
    # accessibility text node but does not update GPUI's address draft.
    Send-Key 0x41 @(0x11)
    [Windows.Forms.SendKeys]::SendWait($Location)
    Start-Sleep -Milliseconds 120
    Send-Key 0x0D
    Start-Sleep -Milliseconds 650
    if ($ExpectedRow) { [void](Wait-FileRow $Root $ExpectedRow) }
}

function Navigate-Back([Windows.Automation.AutomationElement]$Root) {
    $back = Find-Element $Root {
        param($element)
        $element.Current.Name -eq 'Back' -and
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.IsEnabled
    } 'enabled Back button'
    Click-Element $back
}

function Start-App([string]$Name, [string]$InitialPath = '') {
    $logDirectory = Join-Path $OutputDirectory $Name
    New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.WorkingDirectory = $workspaceRoot
    $start.UseShellExecute = $false
    $start.Environment['LOCALAPPDATA'] = $isolatedLocalAppData
    $start.Environment['EXPLORER_LOG_DIR'] = $logDirectory
    if ($InitialPath) { $start.Environment['EXPLORER_INITIAL_PATH'] = $InitialPath }
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $process.Refresh()
        Start-Sleep -Milliseconds 100
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) {
        $process.Kill()
        throw "$Name window did not appear"
    }
    [void][RoadmapSession.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 700
    return $process
}

function Get-WindowBounds([IntPtr]$Handle) {
    $rect = [RoadmapSession.Native+Rect]::new()
    if (-not [RoadmapSession.Native]::GetWindowRect($Handle, [ref]$rect)) {
        throw 'GetWindowRect failed'
    }
    return [ordered]@{
        left = $rect.Left
        top = $rect.Top
        width = $rect.Right - $rect.Left
        height = $rect.Bottom - $rect.Top
    }
}

function Save-Screenshot([IntPtr]$Handle, [string]$Path) {
    $bounds = Get-WindowBounds $Handle
    $bitmap = [Drawing.Bitmap]::new($bounds.width, $bounds.height)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($bounds.left, $bounds.top, 0, 0, $bitmap.Size)
            $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $graphics.Dispose()
        }
    } finally {
        $bitmap.Dispose()
    }
}

function Capture-UiaState([Diagnostics.Process]$Process, [string]$Name) {
    $root = [Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
    $rootTop = $root.Current.BoundingRectangle.Top
    $tabs = @()
    foreach ($element in $root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    )) {
        if ($element.Current.ControlType -ne [Windows.Automation.ControlType]::TabItem) { continue }
        $bounds = $element.Current.BoundingRectangle
        if ($bounds.Top -gt ($rootTop + 220) -or $bounds.Width -le 0) { continue }
        $selectionPattern = $null
        $selected = $false
        if ($element.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selectionPattern)) {
            $selected = ([Windows.Automation.SelectionItemPattern]$selectionPattern).Current.IsSelected
        }
        $tabs += [ordered]@{
            name = $element.Current.Name
            automation_id = $element.Current.AutomationId
            left = [int]$bounds.Left
            selected = $selected
        }
    }
    $tabs = @($tabs | Sort-Object { $_['left'] })
    $focused = [Windows.Automation.AutomationElement]::FocusedElement
    $state = [ordered]@{
        tabs = $tabs
        active_tab = @($tabs | Where-Object { $_['selected'] } | ForEach-Object { $_['name'] })
        focused = if ($null -eq $focused) { $null } else { [ordered]@{ name=$focused.Current.Name; automation_id=$focused.Current.AutomationId; control_type=$focused.Current.ControlType.ProgrammaticName } }
        bounds = Get-WindowBounds $Process.MainWindowHandle
    }
    $state | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory "$Name-uia.json")
    Save-Screenshot $Process.MainWindowHandle (Join-Path $OutputDirectory "$Name.png")
    return $state
}

function Read-SessionEnvelope {
    if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) {
        throw "session snapshot is missing: $statePath"
    }
    return Get-Content -Raw -Encoding UTF8 -LiteralPath $statePath | ConvertFrom-Json
}

function Get-PayloadOracle($Envelope) {
    $tabs = @()
    foreach ($tab in $Envelope.payload.tabs) {
        $tabs += [ordered]@{
            tab_id = $tab.tab_id
            current = $tab.current.location
            back = @($tab.back | ForEach-Object location)
            forward = @($tab.forward | ForEach-Object location)
            view_settings = $tab.view_settings
        }
    }
    return [ordered]@{
        tabs = $tabs
        active_tab_id = $Envelope.payload.active_tab_id
        window = $Envelope.payload.window
        restore_enabled = $Envelope.payload.restore_enabled
    }
}

function Assert-Equivalent([object]$Expected, [object]$Actual, [string]$Label) {
    $expectedJson = $Expected | ConvertTo-Json -Depth 20 -Compress
    $actualJson = $Actual | ConvertTo-Json -Depth 20 -Compress
    if ($expectedJson -cne $actualJson) {
        $expectedJson | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory "$Label-expected.json")
        $actualJson | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory "$Label-actual.json")
        throw "$Label before/after mismatch"
    }
}

function Assert-UiaEquivalent($Expected, $Actual, [string]$Label) {
    Assert-Equivalent $Expected.tabs $Actual.tabs "$Label-tabs"
    Assert-Equivalent $Expected.active_tab $Actual.active_tab "$Label-active-tab"
    if ($Expected.focused.control_type -eq 'ControlType.Window' -and
        $Actual.focused.control_type -eq 'ControlType.Window') {
        # UIA exposes the same focused top-level surface with either its dynamic title or an empty
        # Name during startup. The control type is the stable focus identity for this surface.
        Assert-Equivalent $Expected.focused.control_type $Actual.focused.control_type "$Label-focus"
    } else {
        Assert-Equivalent $Expected.focused $Actual.focused "$Label-focus"
    }
    if ([Math]::Abs($Expected.bounds.left - $Actual.bounds.left) -gt 8 -or
        [Math]::Abs($Expected.bounds.top - $Actual.bounds.top) -gt 8 -or
        [Math]::Abs($Expected.bounds.width - $Actual.bounds.width) -gt 8 -or
        [Math]::Abs($Expected.bounds.height - $Actual.bounds.height) -gt 8) {
        throw "$Label window bounds mismatch"
    }
}

function Assert-PayloadEquivalent($Expected, $Actual, [string]$Label) {
    Assert-Equivalent $Expected.tabs $Actual.tabs "$Label-tabs"
    Assert-Equivalent $Expected.active_tab_id $Actual.active_tab_id "$Label-active-tab"
    Assert-Equivalent $Expected.restore_enabled $Actual.restore_enabled "$Label-restore-enabled"
    Assert-Equivalent $Expected.window.source_work_area $Actual.window.source_work_area "$Label-work-area"
    Assert-Equivalent $Expected.window.source_dpi $Actual.window.source_dpi "$Label-dpi"
    Assert-Equivalent $Expected.window.maximized $Actual.window.maximized "$Label-maximized"
    if ([Math]::Abs($Expected.window.normal_bounds.left - $Actual.window.normal_bounds.left) -gt 4 -or
        [Math]::Abs($Expected.window.normal_bounds.top - $Actual.window.normal_bounds.top) -gt 4 -or
        [Math]::Abs($Expected.window.normal_bounds.width - $Actual.window.normal_bounds.width) -gt 4 -or
        [Math]::Abs($Expected.window.normal_bounds.height - $Actual.window.normal_bounds.height) -gt 4) {
        $Expected.window | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory "$Label-window-expected.json")
        $Actual.window | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory "$Label-window-actual.json")
        throw "$Label window placement mismatch"
    }
}

function Close-Clean([Diagnostics.Process]$Process) {
    [void][RoadmapSession.Native]::SetForegroundWindow($Process.MainWindowHandle)
    Send-Key 0x73 @(0x12)
    if (-not $Process.WaitForExit(15000)) {
        $Process.Kill()
        throw 'orderly Alt+F4 shutdown timed out'
    }
    if ($Process.ExitCode -ne 0) { throw "clean shutdown exited with $($Process.ExitCode)" }
}

function Assert-RestoredProcess(
    [Diagnostics.Process]$Process,
    [object]$ExpectedUia,
    [object]$ExpectedPayload,
    [string]$Name
) {
    $root = [Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
    [void](Wait-FileRow $root 'workspace-marker.txt')
    Assert-NoDisconnectedDirectory $root 'restored active-tab startup'
    if ($Name -eq 'restart-1') {
        Save-Screenshot $Process.MainWindowHandle (Join-Path $OutputDirectory 'restored-active-loaded.png')
    }
    $uia = Capture-UiaState $Process $Name
    Assert-UiaEquivalent $ExpectedUia $uia $Name

    $systemTabName = $ExpectedUia.tabs[0].name
    $systemTab = Find-Element $root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::TabItem -and
            $element.Current.Name -eq $systemTabName
    } 'restored background system tab'
    Click-Element $systemTab
    [void](Wait-FileRow $root 'system-history-marker.txt')
    Assert-NoDisconnectedDirectory $root 'restored background-tab activation'
    if ($Name -eq 'restart-1') {
        Save-Screenshot $Process.MainWindowHandle (Join-Path $OutputDirectory 'restored-background-loaded.png')
    }

    $workspaceTabName = $ExpectedUia.active_tab[0]
    $workspaceTab = Find-Element $root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::TabItem -and
            $element.Current.Name -eq $workspaceTabName
    } 'restored active workspace tab'
    Click-Element $workspaceTab
    [void](Wait-FileRow $root 'workspace-marker.txt')
    Assert-NoDisconnectedDirectory $root 'return to restored active tab'

    $envelope = Read-SessionEnvelope
    Assert-PayloadEquivalent $ExpectedPayload (Get-PayloadOracle $envelope) "$Name-payload"
    return $uia
}

Assert-OwnedPath $systemFixture $systemFixtureParent
Assert-OwnedPath $workspaceFixture $workspaceFixtureParent
$liveProcesses = [Collections.Generic.List[Diagnostics.Process]]::new()
try {
    New-Item -ItemType Directory -Path $systemFixture, $workspaceFixture -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $systemFixture 'history-system'), (Join-Path $workspaceFixture 'history-workspace') -Force | Out-Null
    Set-Content -Encoding UTF8 -LiteralPath (Join-Path $systemFixture 'system-marker.txt') -Value 'system fixture'
    Set-Content -Encoding UTF8 -LiteralPath (Join-Path $workspaceFixture 'workspace-marker.txt') -Value 'workspace fixture'
    Set-Content -Encoding UTF8 -LiteralPath (Join-Path $systemFixture 'history-system\system-history-marker.txt') -Value 'system history fixture'
    Set-Content -Encoding UTF8 -LiteralPath (Join-Path $workspaceFixture 'history-workspace\workspace-history-marker.txt') -Value 'workspace history fixture'

    $first = Start-App 'seed' $systemFixture
    $liveProcesses.Add($first)
    [void][RoadmapSession.Native]::SetWindowPos($first.MainWindowHandle, [IntPtr](-1), 72, 64, 1260, 780, 0x0040)
    Start-Sleep -Milliseconds 350
    $root = [Windows.Automation.AutomationElement]::FromHandle($first.MainWindowHandle)
    [void](Wait-FileRow $root 'system-marker.txt')

    Set-Address $first $root (Join-Path $systemFixture 'history-system') 'system-history-marker.txt'
    $iconView = Find-Element $root {
        param($element) $element.Current.Name -eq 'Icon view' -and $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button
    } 'status Icon view button'
    Click-Element $iconView

    $newTab = Find-Element $root {
        param($element) $element.Current.Name -eq 'New tab' -and $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button
    } 'New tab button'
    Click-Element $newTab
    Set-Address $first $root $workspaceFixture 'workspace-marker.txt'
    $detailsView = Find-Element $root {
        param($element) $element.Current.Name -eq 'Details view' -and $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button
    } 'status Details view button'
    Click-Element $detailsView

    $newTab = Find-Element $root {
        param($element) $element.Current.Name -eq 'New tab' -and $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button
    } 'New tab button'
    Click-Element $newTab
    Set-Address $first $root 'shell:MyComputerFolder' ''
    $workspaceTabName = Split-Path -Leaf $workspaceFixture
    $workspaceTab = Find-Element $root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::TabItem -and
            $element.Current.Name -eq $workspaceTabName
    } 'workspace tab'
    Click-Element $workspaceTab
    [void](Wait-FileRow $root 'workspace-marker.txt')
    Start-Sleep -Milliseconds 900

    $beforeUia = Capture-UiaState $first 'before'
    if ($beforeUia.tabs.Count -ne 3) { throw "seed UI exposes $($beforeUia.tabs.Count) tabs instead of 3" }
    if (@($beforeUia.active_tab).Count -ne 1 -or $beforeUia.active_tab[0] -ne $beforeUia.tabs[1].name) {
        throw 'seed active tab is not the non-first workspace tab'
    }
    Close-Clean $first
    [void]$liveProcesses.Remove($first)
    $first.Dispose()

    $beforeEnvelope = Read-SessionEnvelope
    if ($beforeEnvelope.payload.tabs.Count -ne 3) { throw 'durable session does not contain 3 tabs' }
    if ($beforeEnvelope.payload.active_tab_id -ne $beforeEnvelope.payload.tabs[1].tab_id) { throw 'durable active tab is not the second tab' }
    if ($beforeEnvelope.payload.tabs[0].back.Count -lt 1 -or $beforeEnvelope.payload.tabs[1].back.Count -lt 1) { throw 'independent tab history was not persisted' }
    if ($beforeEnvelope.payload.tabs[0].view_settings.mode -eq $beforeEnvelope.payload.tabs[1].view_settings.mode -and
        $beforeEnvelope.payload.tabs[0].view_settings.preview_pane -eq $beforeEnvelope.payload.tabs[1].view_settings.preview_pane) {
        throw 'per-tab view settings are not independently distinguishable'
    }
    $beforePayload = Get-PayloadOracle $beforeEnvelope
    $beforePayload | ConvertTo-Json -Depth 20 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'before-payload.json')

    $results = @()
    for ($run = 1; $run -le $RestartRuns; $run++) {
        $name = "restart-$run"
        $process = Start-App $name
        $liveProcesses.Add($process)
        [void](Assert-RestoredProcess $process $beforeUia $beforePayload $name)
        $process.Refresh()
        $resource = [ordered]@{ threads=$process.Threads.Count; handles=$process.HandleCount; working_set_bytes=$process.WorkingSet64 }
        if ($run % 2 -eq 0) {
            $process.Kill()
            $process.WaitForExit()
            [void]$liveProcesses.Remove($process)
            $process.Dispose()
            $recoveryName = "$name-recovery"
            $recovery = Start-App $recoveryName
            $liveProcesses.Add($recovery)
            [void](Assert-RestoredProcess $recovery $beforeUia $beforePayload $recoveryName)
            Close-Clean $recovery
            [void]$liveProcesses.Remove($recovery)
            $recovery.Dispose()
            $results += [ordered]@{ run=$run; exit='forced'; restored=$true; resource_snapshot=$resource }
        } else {
            Close-Clean $process
            [void]$liveProcesses.Remove($process)
            $process.Dispose()
            $results += [ordered]@{ run=$run; exit='clean'; restored=$true; resource_snapshot=$resource }
        }
    }

    $finalEnvelope = Read-SessionEnvelope
    Assert-PayloadEquivalent $beforePayload (Get-PayloadOracle $finalEnvelope) 'final-payload'
    [ordered]@{
        schema = 'roadmap-session-headful-v2'
        result = 'PASS'
        restart_runs = $RestartRuns
        clean_runs = @($results | Where-Object exit -eq 'clean').Count
        crash_runs = @($results | Where-Object exit -eq 'forced').Count
        tab_count = 3
        active_tab_index = 1
        mixed_locations = @('system-filesystem', 'workspace-filesystem', 'shell:MyComputerFolder')
        cross_volume = ([IO.Path]::GetPathRoot($systemFixture) -ne [IO.Path]::GetPathRoot($workspaceFixture))
        full_oracle_per_run = $true
        restored_active_auto_loaded = $true
        restored_background_auto_loaded = $true
        persistent_disconnected_seen = $false
        results = $results
        artifacts = @(
            'before-uia.json',
            'before.png',
            'before-payload.json',
            'restored-active-loaded.png',
            'restored-background-loaded.png'
        )
    } | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'headful-report.json')
} finally {
    foreach ($process in @($liveProcesses)) {
        try {
            if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
            $process.Dispose()
        } catch { }
    }
    foreach ($fixture in @($systemFixture, $workspaceFixture)) {
        $parent = if ($fixture -eq $systemFixture) { $systemFixtureParent } else { $workspaceFixtureParent }
        Assert-OwnedPath $fixture $parent
        if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
    }
}
