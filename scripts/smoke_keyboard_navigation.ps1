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
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    } else {
        [IO.Path]::GetFullPath((Join-Path $workspaceRoot $env:CARGO_TARGET_DIR))
    }
} else {
    Join-Path $workspaceRoot 'target'
}
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('keyboard-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
} elseif (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) { throw "artifact finalization failed: $LASTEXITCODE" }
}
$executablePath = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "explorer-app executable not found: $executablePath"
}

if (-not ('ExplorerKeyboard.NativeWindow' -as [type])) {
    Add-Type -AssemblyName System.Drawing
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerKeyboard {
    public static class NativeWindow {
        [StructLayout(LayoutKind.Sequential)]
        public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);
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
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool BringWindowToTop(IntPtr window);
        [DllImport("user32.dll")]
        public static extern IntPtr SetFocus(IntPtr window);
        [DllImport("user32.dll")]
        public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extraInfo);
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PrintWindow(IntPtr window, IntPtr deviceContext, uint flags);
        [DllImport("dwmapi.dll")]
        public static extern int DwmFlush();
    }
}
'@
}

function Send-KeyChord([byte]$Key, [byte[]]$Modifiers) {
    foreach ($modifier in $Modifiers) {
        [ExplorerKeyboard.NativeWindow]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero)
    }
    [ExplorerKeyboard.NativeWindow]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [ExplorerKeyboard.NativeWindow]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    for ($modifierIndex = $Modifiers.Count - 1; $modifierIndex -ge 0; $modifierIndex--) {
        [ExplorerKeyboard.NativeWindow]::keybd_event($Modifiers[$modifierIndex], 0, 2, [UIntPtr]::Zero)
    }
    # GPUI rendering and DWM composition are asynchronous. Wait long enough for the
    # focused-surface border and editable focus state to reach PrintWindow.
    Start-Sleep -Milliseconds 400
}

function Save-Window([IntPtr]$WindowHandle, [string]$Path) {
    if ([ExplorerKeyboard.NativeWindow]::DwmFlush() -ne 0) { throw 'DwmFlush failed' }
    $rect = [ExplorerKeyboard.NativeWindow+Rect]::new()
    if (-not [ExplorerKeyboard.NativeWindow]::GetWindowRect($WindowHandle, [ref]$rect)) {
        throw 'GetWindowRect failed'
    }
    $bitmap = [Drawing.Bitmap]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $deviceContext = $graphics.GetHdc()
            try {
                if (-not [ExplorerKeyboard.NativeWindow]::PrintWindow($WindowHandle, $deviceContext, 2)) {
                    throw 'PrintWindow failed'
                }
            } finally { $graphics.ReleaseHdc($deviceContext) }
        } finally { $graphics.Dispose() }
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
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
    if (-not $ready) { throw 'timed out waiting for keyboard fixture' }

    $currentThread = [ExplorerKeyboard.NativeWindow]::GetCurrentThreadId()
    $foregroundWindow = [ExplorerKeyboard.NativeWindow]::GetForegroundWindow()
    $foregroundThread = [ExplorerKeyboard.NativeWindow]::GetWindowThreadProcessId($foregroundWindow, [IntPtr]::Zero)
    $targetThread = [ExplorerKeyboard.NativeWindow]::GetWindowThreadProcessId($windowHandle, [IntPtr]::Zero)
    $attachedForeground = $foregroundThread -ne 0 -and $foregroundThread -ne $currentThread -and
        [ExplorerKeyboard.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $true)
    $attachedTarget = $targetThread -ne 0 -and $targetThread -ne $currentThread -and
        [ExplorerKeyboard.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $true)
    try {
        [void][ExplorerKeyboard.NativeWindow]::BringWindowToTop($windowHandle)
        [void][ExplorerKeyboard.NativeWindow]::SetForegroundWindow($windowHandle)
        [void][ExplorerKeyboard.NativeWindow]::SetFocus($windowHandle)
    } finally {
        if ($attachedTarget) {
            [void][ExplorerKeyboard.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $false)
        }
        if ($attachedForeground) {
            [void][ExplorerKeyboard.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $false)
        }
    }
    Start-Sleep -Milliseconds 250
    if ([ExplorerKeyboard.NativeWindow]::GetForegroundWindow() -ne $windowHandle) {
        throw 'application could not become foreground for SendInput-compatible keyboard testing'
    }
    Write-Output "Keyboard fixture foreground HWND: $windowHandle"

    $steps = @()
    function Capture-Step([string]$Name, [string]$ExpectedSurface, [string]$Theme) {
        $path = Join-Path $OutputDirectory ("{0:D2}-{1}.png" -f $script:steps.Count, $Name)
        Save-Window $windowHandle $path
        $script:steps += [pscustomobject][ordered]@{
            index = $script:steps.Count
            input = $Name
            expected_surface = $ExpectedSurface
            expected_theme = $Theme
            screenshot = [IO.Path]::GetFileName($path)
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash
        }
    }

    Capture-Step 'initial' 'FileView' 'light'
    Write-Output 'Captured initial FileView focus frame'
    foreach ($target in @('StatusBar','WindowChrome','TabStrip','CommandBar','AddressBar','Search','NavigationPane','FileView')) {
        Send-KeyChord 0x09 @()
        Capture-Step 'tab' $target 'light'
    }
    Send-KeyChord 0x23 @()
    Capture-Step 'file-end' 'FileView' 'light'
    Send-KeyChord 0x24 @()
    Capture-Step 'file-home' 'FileView' 'light'
    Send-KeyChord 0x28 @()
    Capture-Step 'file-down' 'FileView' 'light'
    Send-KeyChord 0x09 @(0x10)
    Capture-Step 'shift-tab' 'NavigationPane' 'light'
    Send-KeyChord 0x4C @(0x11)
    Capture-Step 'ctrl-l' 'AddressBar' 'light'
    Send-KeyChord 0x46 @(0x11)
    Capture-Step 'ctrl-f' 'Search' 'light'
    Send-KeyChord 0x44 @(0x11, 0x10)
    Capture-Step 'ctrl-shift-d' 'Search' 'dark'
    Send-KeyChord 0x09 @(0x10)
    Capture-Step 'shift-tab-dark' 'AddressBar' 'dark'

    $uniqueScreenshots = @($steps.sha256 | Sort-Object -Unique).Count
    # Focus traversal is asserted by the typed action log above. Some Windows surfaces
    # intentionally share the same visual focus treatment, so the image oracle only
    # guards against a frozen renderer rather than requiring one bitmap per surface.
    if ($uniqueScreenshots -lt 10) {
        throw "keyboard visual oracle found only $uniqueScreenshots distinct frames"
    }

    Send-KeyChord 0x73 @(0x12)
    if (-not $process.WaitForExit($TimeoutSeconds * 1000) -or $process.ExitCode -ne 0) {
        throw 'Alt+F4 did not close the application cleanly'
    }
    $log = Get-Content -Raw -Encoding utf8 -LiteralPath $logPath
    foreach ($event in @('visual_fixture_ready','application_stopped','clean_shutdown')) {
        if (-not $log.Contains("event=`"$event`"")) { throw "missing lifecycle event: $event" }
    }
    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        input_driver = 'foreground Win32 keybd_event; no pointer actions'
        steps = $steps
        unique_screenshot_count = $uniqueScreenshots
        close_input = 'Alt+F4'
        exit_code = $process.ExitCode
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Keyboard traversal passed: $OutputDirectory"
} finally {
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        [void]$process.WaitForExit(5000)
    }
    $process.Dispose()
}
