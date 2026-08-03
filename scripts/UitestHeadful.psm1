Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Initialize-UitestHeadful {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    if (-not ('RustExplorerUitest.Native' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace RustExplorerUitest {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
        public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern bool SetPhysicalCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
        [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder text, int count);
        [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
        public static bool SetCursorPosDpiAware(int x, int y) {
            IntPtr previous = SetThreadDpiAwarenessContext(new IntPtr(-4));
            try { return SetCursorPos(x, y); }
            finally { if (previous != IntPtr.Zero) SetThreadDpiAwarenessContext(previous); }
        }
    }
}
'@
    }
}

function Start-UitestExplorer {
    param(
        [Parameter(Mandatory)][string]$InitialPath,
        [Parameter(Mandatory)][string]$OutputDirectory,
        [ValidateSet('debug','release')][string]$Profile = 'debug',
        [switch]$SkipBuild,
        [int]$TimeoutSeconds = 25,
        [string[]]$CargoFeatures = @(),
        [hashtable]$AdditionalEnvironment = @{}
    )
    Initialize-UitestHeadful
    $workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    if (-not $SkipBuild) {
        $featureArguments = @()
        if ($CargoFeatures.Count -gt 0) { $featureArguments = @('--features', ($CargoFeatures -join ',')) }
        if ($Profile -eq 'release') { & cargo.exe build -p explorer-app --release --locked @featureArguments }
        else { & cargo.exe build -p explorer-app --locked @featureArguments }
        if ($LASTEXITCODE -ne 0) { throw "explorer-app build failed: $LASTEXITCODE" }
    }
    $executable = Join-Path $workspace "target\$Profile\SuperExplorer.exe"
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "missing app executable: $executable" }
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $start = [Diagnostics.ProcessStartInfo]::new($executable)
    $start.WorkingDirectory = $workspace
    $start.UseShellExecute = $false
    $start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
    $start.Environment['EXPLORER_INITIAL_PATH'] = $InitialPath
    $start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    foreach ($name in $AdditionalEnvironment.Keys) {
        $start.Environment[[string]$name] = [string]$AdditionalEnvironment[$name]
    }
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ($process.HasExited) { throw "application exited before showing a window: $($process.ExitCode)" }
        $process.Refresh()
        Start-Sleep -Milliseconds 100
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][RustExplorerUitest.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 20, 20, 1440, 880, 0x0040)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 900
    [pscustomobject]@{
        Process = $process
        Hwnd = $process.MainWindowHandle
        Root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        Workspace = $workspace
        InitialPath = $InitialPath
    }
}

function Stop-UitestExplorer {
    param([Parameter(Mandatory)]$Context)
    $process = $Context.Process
    if ($null -ne $process -and -not $process.HasExited) {
        [void][RustExplorerUitest.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    if ($null -ne $process) { $process.Dispose() }
}

function Find-UitestElement {
    param(
        [Parameter(Mandatory)][Windows.Automation.AutomationElement]$Root,
        [Parameter(Mandatory)][scriptblock]$Predicate,
        [Parameter(Mandatory)][string]$Description,
        [int]$TimeoutSeconds = 10
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
            try { if (& $Predicate $element) { return $element } } catch { }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $Description"
}

function Get-UitestFileItems {
    param([Parameter(Mandatory)][Windows.Automation.AutomationElement]$Root)
    $window = $Root.Current.BoundingRectangle
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem)
    @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) | Where-Object {
        $bounds = $_.Current.BoundingRectangle
        $bounds.Width -gt 0 -and $bounds.Height -gt 0 -and
            $bounds.Left -gt ($window.Left + 330) -and $bounds.Top -gt ($window.Top + 175)
    })
}

function Find-UitestFileItem {
    param(
        [Parameter(Mandatory)][Windows.Automation.AutomationElement]$Root,
        [Parameter(Mandatory)][string]$Name,
        [int]$TimeoutSeconds = 12
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        foreach ($item in @(Get-UitestFileItems -Root $Root)) {
            if ($item.Current.Name -eq $Name -or $item.Current.Name -like "$Name *" -or $item.Current.Name -like "*$Name*") { return $item }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $names = @(Get-UitestFileItems -Root $Root | ForEach-Object { $_.Current.Name }) -join ', '
    throw "file item not found: $Name; visible=[$names]"
}

function Invoke-UitestClick {
    param([Parameter(Mandatory)][Windows.Automation.AutomationElement]$Element, [switch]$Double, [switch]$Right, [switch]$Middle, [switch]$Shift, [switch]$Control)
    if ($Right -and $Middle) { throw 'Right and Middle are mutually exclusive pointer buttons' }
    $windowElement = $Element
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
    while ($null -ne $windowElement -and $windowElement.Current.NativeWindowHandle -eq 0) {
        $windowElement = $walker.GetParent($windowElement)
    }
    if ($null -ne $windowElement -and $windowElement.Current.NativeWindowHandle -ne 0) {
        [void][RustExplorerUitest.Native]::SetForegroundWindow([IntPtr]$windowElement.Current.NativeWindowHandle)
        Start-Sleep -Milliseconds 100
    }
    $point = Get-UitestPhysicalPoint -Element $Element -HorizontalOffset 100
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)) {
        throw "DPI-aware cursor positioning failed at ($($point.X),$($point.Y))"
    }
    if ($Shift) { [RustExplorerUitest.Native]::keybd_event(0x10, 0, 0, [UIntPtr]::Zero) }
    if ($Control) { [RustExplorerUitest.Native]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero) }
    $count = if ($Double) { 2 } else { 1 }
    $down = if ($Middle) { 0x0020 } elseif ($Right) { 0x0008 } else { 0x0002 }
    $up = if ($Middle) { 0x0040 } elseif ($Right) { 0x0010 } else { 0x0004 }
    foreach ($index in 1..$count) {
        [RustExplorerUitest.Native]::mouse_event($down, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event($up, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 70
    }
    if ($Control) { [RustExplorerUitest.Native]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero) }
    if ($Shift) { [RustExplorerUitest.Native]::keybd_event(0x10, 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 220
}

function Get-UitestPhysicalPoint {
    param(
        [Parameter(Mandatory)][Windows.Automation.AutomationElement]$Element,
        [double]$HorizontalOffset = 0
    )
    $windowElement = $Element
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
    while ($null -ne $windowElement -and $windowElement.Current.NativeWindowHandle -eq 0) {
        $windowElement = $walker.GetParent($windowElement)
    }
    if ($null -eq $windowElement -or $windowElement.Current.NativeWindowHandle -eq 0) {
        throw 'cannot resolve native owner for UIA coordinate conversion'
    }
    $elementBounds = $Element.Current.BoundingRectangle
    # UI Automation bounding rectangles use physical desktop coordinates. Use the physical
    # cursor API directly; SetCursorPos virtualizes these values when PowerShell is DPI-unaware.
    $physicalX = if ($HorizontalOffset -gt 0) {
        $elementBounds.Left + [Math]::Min($HorizontalOffset, $elementBounds.Width / 2)
    } else {
        $elementBounds.Left + ($elementBounds.Width / 2)
    }
    [pscustomobject]@{
        X = [int]$physicalX
        Y = [int]($elementBounds.Top + ($elementBounds.Height / 2))
    }
}

function Send-UitestKey {
    param([Parameter(Mandatory)][byte]$Key, [byte[]]$Modifiers = @(), [int]$DelayMilliseconds = 180)
    foreach ($modifier in $Modifiers) { [RustExplorerUitest.Native]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero) }
    [RustExplorerUitest.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    for ($index = $Modifiers.Count - 1; $index -ge 0; $index--) {
        [RustExplorerUitest.Native]::keybd_event($Modifiers[$index], 0, 2, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds $DelayMilliseconds
}

function Set-UitestClipboardText {
    param([Parameter(Mandatory)][string]$Text)
    $lastError = $null
    foreach ($attempt in 1..20) {
        try { [Windows.Forms.Clipboard]::SetText($Text); return } catch { $lastError = $_; Start-Sleep -Milliseconds 50 }
    }
    throw $lastError
}

function Set-UitestAddress {
    param([Parameter(Mandatory)]$Context, [Parameter(Mandatory)][string]$Path, [string]$ExpectedItem)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($Context.Hwnd)
    Start-Sleep -Milliseconds 150
    Send-UitestKey -Key 0x1B
    Send-UitestKey -Key 0x4C -Modifiers @(0x11)
    $window = $Context.Root.Current.BoundingRectangle
    try {
        $editor = Find-UitestElement -Root $Context.Root -Description 'address editor after Ctrl+L' -TimeoutSeconds 2 -Predicate {
            param($element)
            $bounds = $element.Current.BoundingRectangle
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
                $bounds.Top -lt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + $window.Width * 0.58)
        }
    } catch {
        $address = Find-UitestElement -Root $Context.Root -Description 'browsing address field fallback' -TimeoutSeconds 4 -Predicate {
            param($element)
            $bounds = $element.Current.BoundingRectangle
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Document -and
                $element.Current.Name -like 'Address: *' -and
                $bounds.Top -lt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + $window.Width * 0.58)
        }
        $bounds = $address.Current.BoundingRectangle
        [void][RustExplorerUitest.Native]::SetCursorPos([int]($bounds.Right - 14), [int]($bounds.Top + $bounds.Height / 2))
        [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 250
        $editor = Find-UitestElement -Root $Context.Root -Description 'clicked address editor' -TimeoutSeconds 4 -Predicate {
            param($element)
            $bounds = $element.Current.BoundingRectangle
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
                $bounds.Top -lt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + $window.Width * 0.58)
        }
    }
    $editor.SetFocus()
    Send-UitestKey -Key 0x41 -Modifiers @(0x11)
    Set-UitestClipboardText -Text $Path
    Send-UitestKey -Key 0x56 -Modifiers @(0x11)
    $valuePattern = $null
    if ($editor.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$valuePattern)) {
        $actual = ([Windows.Automation.ValuePattern]$valuePattern).Current.Value
        if (-not $actual.Equals($Path, [StringComparison]::OrdinalIgnoreCase)) {
            throw "address paste mismatch: expected=$Path actual=$actual"
        }
    }
    Send-UitestKey -Key 0x0D -DelayMilliseconds 700
    if ($ExpectedItem) { Find-UitestFileItem -Root $Context.Root -Name $ExpectedItem | Out-Null }
}

function Wait-UitestPath {
    param([Parameter(Mandatory)][string]$Path, [bool]$Exists = $true, [int]$TimeoutSeconds = 15)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ((Test-Path -LiteralPath $Path) -eq $Exists) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "filesystem oracle timed out: exists=$Exists path=$Path"
}

function Get-UitestSelectedCount {
    param([Parameter(Mandatory)][Windows.Automation.AutomationElement]$Root)
    $count = 0
    foreach ($item in @(Get-UitestFileItems -Root $Root)) {
        $pattern = $null
        if ($item.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern) -and
            ([Windows.Automation.SelectionItemPattern]$pattern).Current.IsSelected) { $count++ }
    }
    $count
}

function Save-UitestScreenshot {
    param([Parameter(Mandatory)][Windows.Automation.AutomationElement]$Root, [Parameter(Mandatory)][string]$Path)
    $bounds = $Root.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::new([int]$bounds.Width, [int]$bounds.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen([int]$bounds.Left, [int]$bounds.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

Export-ModuleMember -Function Initialize-UitestHeadful,Start-UitestExplorer,Stop-UitestExplorer,Find-UitestElement,Get-UitestFileItems,Find-UitestFileItem,Get-UitestPhysicalPoint,Invoke-UitestClick,Send-UitestKey,Set-UitestClipboardText,Set-UitestAddress,Wait-UitestPath,Get-UitestSelectedCount,Save-UitestScreenshot
