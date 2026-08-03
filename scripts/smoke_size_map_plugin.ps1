param(
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$PluginDll = 'sdk\fixtures\rust-folder-size-map-view\target\x86_64-pc-windows-msvc\debug\rust_folder_size_map_view.dll',
    [string]$InitialPath = '.',
    [string]$OutputDirectory = 'target\size-map-smoke',
    [switch]$UsePointerActivation
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

function Invoke-NamedElement($Root, [string]$Name, [switch]$PointerOnly) {
    $element = Find-NamedElement $Root $Name
    if ($null -eq $element) { throw "UI element '$Name' was not found" }
    if (-not $PointerOnly) {
        try {
            $pattern = $element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
            $pattern.Invoke()
            return
        } catch [InvalidOperationException] {
            # Fall through to a real pointer click for controls without InvokePattern.
        }
    }
    & {
        $bounds = $element.Current.BoundingRectangle
        $rootBounds = $Root.Current.BoundingRectangle
        $windowRect = [SizeMapSmoke.Native+Rect]::new()
        if (-not [SizeMapSmoke.Native]::GetWindowRect($window, [ref]$windowRect)) {
            throw 'GetWindowRect failed while activating menu item'
        }
        # UI Automation reports GPUI's logical coordinates while SetCursorPos
        # consumes physical desktop pixels. Convert relative to the actual
        # HWND bounds so the fallback remains correct at non-100% DPI.
        $scaleX = ($windowRect.Right - $windowRect.Left) / $rootBounds.Width
        $scaleY = ($windowRect.Bottom - $windowRect.Top) / $rootBounds.Height
        $screenX = [int]($windowRect.Left + (($bounds.Left + $bounds.Width / 2) - $rootBounds.Left) * $scaleX)
        $screenY = [int]($windowRect.Top + (($bounds.Top + $bounds.Height / 2) - $rootBounds.Top) * $scaleY)
        [void][SizeMapSmoke.Native]::SetCursorPos($screenX, $screenY)
        Start-Sleep -Milliseconds 50
        [SizeMapSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [SizeMapSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
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

    $rows = $root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::DataItem))
    if ($rows.Count -eq 0) { throw 'Real folder contents did not load in Details view' }
    Capture-Window $window $beforePath

    Invoke-NamedElement $root 'View'
    Start-Sleep -Milliseconds 250
    Capture-Window $window (Join-Path $OutputDirectory 'view-menu.png')
    if ($null -eq (Find-NamedElement $root 'Size Map')) {
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
    $deadline = [DateTime]::UtcNow.AddSeconds(12)
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
    Capture-Window $window $sizeMapPath
    Invoke-NamedElement $root $node.Current.Name -PointerOnly
    Start-Sleep -Milliseconds 200
    Capture-Window $window $selectedPath
    if ((Get-FileHash -LiteralPath $sizeMapPath).Hash -eq (Get-FileHash -LiteralPath $selectedPath).Hash) {
        throw 'Selecting a Size Map node did not update its host-owned GPUI surface'
    }

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

    $report = [pscustomobject]@{
        status = 'passed'
        initial_path = $InitialPath
        details_rows = $rows.Count
        size_map_node = $node.Current.Name
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
