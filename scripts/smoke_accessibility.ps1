param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [int]$TimeoutSeconds = 30,
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) }
    else { [IO.Path]::GetFullPath((Join-Path $workspaceRoot $env:CARGO_TARGET_DIR)) }
} else { Join-Path $workspaceRoot 'target' }
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('accessibility-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
} elseif (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) { throw "artifact finalization failed: $LASTEXITCODE" }
}
$executablePath = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) { throw "missing app: $executablePath" }

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ('ExplorerAccessibility.NativeWindow' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerAccessibility {
    public static class NativeWindow {
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool BringWindowToTop(IntPtr window);
        [DllImport("user32.dll")]
        public static extern IntPtr SetFocus(IntPtr window);
        [DllImport("user32.dll")]
        public static extern IntPtr GetForegroundWindow();
        [DllImport("kernel32.dll")]
        public static extern uint GetCurrentThreadId();
        [DllImport("user32.dll")]
        public static extern uint GetWindowThreadProcessId(IntPtr window, IntPtr processId);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool AttachThreadInput(uint source, uint target, bool attach);
        [DllImport("user32.dll")]
        public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extraInfo);
    }
}
'@
}

function Get-AutomationNodes([Windows.Automation.AutomationElement]$Root) {
    $walker = [Windows.Automation.TreeWalker]::RawViewWalker
    $pending = [Collections.Generic.Queue[object]]::new()
    $pending.Enqueue([pscustomobject]@{ element = $Root; depth = 0 })
    $nodes = @()
    while ($pending.Count -gt 0 -and $nodes.Count -lt 512) {
        $item = $pending.Dequeue()
        $element = $item.element
        try {
            $current = $element.Current
            $selectionState = $null
            $selectionPattern = $null
            if ($element.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selectionPattern)) {
                $selectionState = ([Windows.Automation.SelectionItemPattern]$selectionPattern).Current.IsSelected
            }
            $nodes += [pscustomobject][ordered]@{
                depth = $item.depth
                name = $current.Name
                control_type = $current.ControlType.ProgrammaticName
                automation_id = $current.AutomationId
                enabled = $current.IsEnabled
                keyboard_focusable = $current.IsKeyboardFocusable
                has_keyboard_focus = $current.HasKeyboardFocus
                selected = $selectionState
            }
            $child = $walker.GetFirstChild($element)
            while ($null -ne $child) {
                $pending.Enqueue([pscustomobject]@{ element = $child; depth = $item.depth + 1 })
                $child = $walker.GetNextSibling($child)
            }
        } catch [Windows.Automation.ElementNotAvailableException] {
            # A transient AccessKit node may disappear between tree and property queries.
        }
    }
    return @($nodes)
}

$logPath = Join-Path $OutputDirectory 'explorer.log'
$diagnosticsPath = Join-Path $OutputDirectory 'diagnostics.json'
$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $executablePath
$startInfo.WorkingDirectory = $workspaceRoot
$startInfo.UseShellExecute = $false
$startInfo.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$startInfo.Environment['EXPLORER_VISUAL_FIXTURE'] = '1'
$startInfo.Environment['EXPLORER_VISUAL_STATE'] = 'populated'
$startInfo.Environment['EXPLORER_VISUAL_THEME'] = 'light'
$startInfo.Environment['EXPLORER_VISUAL_DPI'] = '175'
$startInfo.Environment['EXPLORER_VISUAL_FONT'] = 'Microsoft JhengHei UI'
$startInfo.Environment['EXPLORER_VISUAL_DIAGNOSTICS'] = $diagnosticsPath
$process = [Diagnostics.Process]::Start($startInfo)

try {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        $windowHandle = $process.MainWindowHandle
        $logText = if (Test-Path -LiteralPath $logPath) {
            Get-Content -Raw -Encoding utf8 -LiteralPath $logPath -ErrorAction SilentlyContinue
        } else { $null }
        $ready = $windowHandle -ne [IntPtr]::Zero -and $null -ne $logText -and
            $logText.Contains('event="visual_fixture_ready"')
        if (-not $ready) { Start-Sleep -Milliseconds 50 }
    } while (-not $ready -and [DateTime]::UtcNow -lt $deadline)
    if (-not $ready) { throw 'timed out waiting for accessibility fixture' }

    $currentThread = [ExplorerAccessibility.NativeWindow]::GetCurrentThreadId()
    $foregroundWindow = [ExplorerAccessibility.NativeWindow]::GetForegroundWindow()
    $foregroundThread = [ExplorerAccessibility.NativeWindow]::GetWindowThreadProcessId($foregroundWindow, [IntPtr]::Zero)
    $attached = $foregroundThread -ne 0 -and $foregroundThread -ne $currentThread -and
        [ExplorerAccessibility.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $true)
    $targetThread = [ExplorerAccessibility.NativeWindow]::GetWindowThreadProcessId($windowHandle, [IntPtr]::Zero)
    $attachedTarget = $targetThread -ne 0 -and $targetThread -ne $currentThread -and
        [ExplorerAccessibility.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $true)
    try {
        [void][ExplorerAccessibility.NativeWindow]::BringWindowToTop($windowHandle)
        [void][ExplorerAccessibility.NativeWindow]::SetForegroundWindow($windowHandle)
        [void][ExplorerAccessibility.NativeWindow]::SetFocus($windowHandle)
        [ExplorerAccessibility.NativeWindow]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
        [ExplorerAccessibility.NativeWindow]::keybd_event(0x46, 0, 0, [UIntPtr]::Zero)
        [ExplorerAccessibility.NativeWindow]::keybd_event(0x46, 0, 2, [UIntPtr]::Zero)
        [ExplorerAccessibility.NativeWindow]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    } finally {
        if ($attachedTarget) {
            [void][ExplorerAccessibility.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $false)
        }
        if ($attached) {
            [void][ExplorerAccessibility.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $false)
        }
    }
    Start-Sleep -Milliseconds 250
    $root = [Windows.Automation.AutomationElement]::FromHandle($windowHandle)
    if ($null -eq $root) { throw 'UI Automation could not resolve the application HWND' }
    $searchEditor = $root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Edit
        )
    ) | Where-Object { $_.Current.Name -like '* VisualFixture;* 0 *' } | Select-Object -First 1
    if ($null -eq $searchEditor) { throw 'search editor was not exposed after Ctrl+F focus' }
    Start-Sleep -Milliseconds 250
    $nodes = @(Get-AutomationNodes $root)
    $namedNodes = @($nodes | Where-Object { -not [string]::IsNullOrWhiteSpace($_.name) })
    $nodes | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'raw-tree.json')
    if ($nodes.Count -lt 15) { throw "accessibility tree is unexpectedly small: $($nodes.Count)" }

    $otherCommands = -join ([char[]](0x5176, 0x5B83))
    $requiredNames = @(
        'C:\VisualFixture', 'Visual Fixture', 'Explorer navigation bar',
        'Explorer command bar', 'Address: C:\VisualFixture', ' VisualFixture;',
        'Navigation pane', 'Visual Fixture folder Folder', 'Create a new item',
        $otherCommands, 'Minimize', 'Close'
    )
    foreach ($required in $requiredNames) {
        if (-not ($namedNodes | Where-Object { $_.name -like "*$required*" })) {
            throw "accessibility tree is missing name: $required"
        }
    }
    $types = @($nodes.control_type | Sort-Object -Unique)
    foreach ($requiredType in @('ControlType.Button','ControlType.Edit','ControlType.TabItem','ControlType.ListItem')) {
        if ($requiredType -notin $types) { throw "accessibility tree is missing role: $requiredType" }
    }
    $focused = @($nodes | Where-Object has_keyboard_focus)
    $systemFocusedElement = [Windows.Automation.AutomationElement]::FocusedElement
    $systemFocused = if ($null -eq $systemFocusedElement) { $null } else {
        [pscustomobject][ordered]@{
            name = $systemFocusedElement.Current.Name
            control_type = $systemFocusedElement.Current.ControlType.ProgrammaticName
            process_id = $systemFocusedElement.Current.ProcessId
        }
    }
    if ($focused.Count -eq 0 -and ($null -eq $systemFocused -or $systemFocused.process_id -ne $process.Id)) {
        throw 'UI Automation global focus is not inside the application'
    }
    $selected = @($nodes | Where-Object { $_.selected -eq $true })
    if (-not ($selected | Where-Object { $_.name -eq 'Visual Fixture' })) {
        throw 'active tab is not exposed as selected'
    }

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        inspector = '.NET UIAutomationClient RawViewWalker (Windows UI Automation)'
        node_count = $nodes.Count
        named_node_count = $namedNodes.Count
        control_types = $types
        focused_nodes = $focused
        global_focused_element = $systemFocused
        selected_nodes = $selected
        nodes = $nodes
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')

    $searchEditor = $null
    $root = $null
    $systemFocusedElement = $null
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
    if (-not [ExplorerAccessibility.NativeWindow]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw 'failed to post WM_CLOSE'
    }
    if (-not $process.WaitForExit($TimeoutSeconds * 1000) -or $process.ExitCode -ne 0) {
        throw 'accessibility fixture did not close cleanly'
    }
    $log = Get-Content -Raw -Encoding utf8 -LiteralPath $logPath
    foreach ($event in @('visual_fixture_ready','application_stopped','clean_shutdown')) {
        if (-not $log.Contains("event=`"$event`"")) { throw "missing lifecycle event: $event" }
    }
    Write-Output "Accessibility smoke passed: $OutputDirectory"
} finally {
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        [void]$process.WaitForExit(5000)
    }
    $process.Dispose()
}
