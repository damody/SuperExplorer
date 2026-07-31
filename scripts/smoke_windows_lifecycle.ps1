param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [int]$TimeoutSeconds = 20,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = if ($env:CARGO_TARGET_DIR) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $env:CARGO_TARGET_DIR))
    }
} else {
    Join-Path $workspaceRoot 'target'
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) {
        throw "artifact finalization failed with exit code $LASTEXITCODE"
    }
}

$executablePath = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "explorer-app executable not found: $executablePath"
}

if (-not ('ExplorerSmoke.NativeWindow' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace ExplorerSmoke {
    public static class NativeWindow {
        [StructLayout(LayoutKind.Sequential)]
        public struct Rect {
            public int Left;
            public int Top;
            public int Right;
            public int Bottom;
        }

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(IntPtr window, out Rect rect);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool MoveWindow(
            IntPtr window,
            int x,
            int y,
            int width,
            int height,
            [MarshalAs(UnmanagedType.Bool)] bool repaint);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

        [DllImport("user32.dll")]
        public static extern uint GetGuiResources(IntPtr process, uint flags);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool ShowWindow(IntPtr window, int command);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool IsIconic(IntPtr window);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool IsZoomed(IntPtr window);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);

        [DllImport("user32.dll")]
        public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);

        [DllImport("dwmapi.dll")]
        public static extern int DwmFlush();

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PrintWindow(IntPtr window, IntPtr deviceContext, uint flags);
    }
}
'@
}

Add-Type -AssemblyName System.Drawing

function Wait-WindowState([scriptblock]$Predicate, [string]$Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (-not (& $Predicate)) {
        if ([DateTime]::UtcNow -ge $deadline) {
            throw "timed out waiting for window state: $Description"
        }
        Start-Sleep -Milliseconds 25
    }
}

function Save-WindowScreenshot([IntPtr]$WindowHandle, [string]$Path) {
    Start-Sleep -Milliseconds 150
    [void][ExplorerSmoke.NativeWindow]::DwmFlush()
    $captureRect = [ExplorerSmoke.NativeWindow+Rect]::new()
    if (-not [ExplorerSmoke.NativeWindow]::GetWindowRect($WindowHandle, [ref]$captureRect)) {
        throw "GetWindowRect failed while capturing $Path"
    }
    $width = $captureRect.Right - $captureRect.Left
    $height = $captureRect.Bottom - $captureRect.Top
    $bitmap = [System.Drawing.Bitmap]::new($width, $height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $deviceContext = $graphics.GetHdc()
            try {
                if (-not [ExplorerSmoke.NativeWindow]::PrintWindow($WindowHandle, $deviceContext, 2)) {
                    throw "PrintWindow failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
                }
            } finally {
                $graphics.ReleaseHdc($deviceContext)
            }
        } finally {
            $graphics.Dispose()
        }
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bitmap.Dispose()
    }
}

function Get-ProcessResourceSample([System.Diagnostics.Process]$Process) {
    $Process.Refresh()
    return [ordered]@{
        thread_count = $Process.Threads.Count
        process_handle_count = $Process.HandleCount
        gdi_handle_count = [ExplorerSmoke.NativeWindow]::GetGuiResources($Process.Handle, 0)
        user_handle_count = [ExplorerSmoke.NativeWindow]::GetGuiResources($Process.Handle, 1)
        working_set_bytes = $Process.WorkingSet64
        peak_working_set_bytes = $Process.PeakWorkingSet64
    }
}

$evidenceDirectory = Join-Path $targetRoot ('smoke-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null
$logPath = Join-Path $evidenceDirectory 'explorer.log'

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $executablePath
$startInfo.WorkingDirectory = $workspaceRoot
$startInfo.UseShellExecute = $false
$startInfo.Environment['EXPLORER_LOG_DIR'] = $evidenceDirectory
$launchUtc = [DateTime]::UtcNow
$readyStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$process = [System.Diagnostics.Process]::Start($startInfo)

try {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $windowHandle = [IntPtr]::Zero
    do {
        if ($process.HasExited) {
            throw "explorer-app exited before window_ready with code $($process.ExitCode)"
        }
        $process.Refresh()
        $windowHandle = $process.MainWindowHandle
        $logText = if (Test-Path -LiteralPath $logPath) {
            Get-Content -Raw -Encoding utf8 -LiteralPath $logPath -ErrorAction SilentlyContinue
        } else { $null }
        $ready = $null -ne $logText -and $logText.Contains('event="window_ready"')
        if (-not $ready -or $windowHandle -eq [IntPtr]::Zero) {
            Start-Sleep -Milliseconds 50
        }
    } while ((!$ready -or $windowHandle -eq [IntPtr]::Zero) -and [DateTime]::UtcNow -lt $deadline)

    if (-not $ready -or $windowHandle -eq [IntPtr]::Zero) {
        throw 'timed out waiting for window_ready and the native window handle.'
    }
    $readyStopwatch.Stop()
    $readyDurationMs = [math]::Round($readyStopwatch.Elapsed.TotalMilliseconds, 3)
    [void][ExplorerSmoke.NativeWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $readyResourceSample = Get-ProcessResourceSample $process

    $rect = [ExplorerSmoke.NativeWindow+Rect]::new()
    if (-not [ExplorerSmoke.NativeWindow]::GetWindowRect($windowHandle, [ref]$rect)) {
        throw "GetWindowRect failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $originalWidth = $rect.Right - $rect.Left
    $originalHeight = $rect.Bottom - $rect.Top
    $resizedWidth = $originalWidth + 120
    $resizedHeight = $originalHeight + 80
    if (-not [ExplorerSmoke.NativeWindow]::MoveWindow(
        $windowHandle,
        $rect.Left,
        $rect.Top,
        $resizedWidth,
        $resizedHeight,
        $true)) {
        throw "MoveWindow failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    Save-WindowScreenshot $windowHandle (Join-Path $evidenceDirectory '01-resized.png')

    [void][ExplorerSmoke.NativeWindow]::ShowWindow($windowHandle, 6)
    Wait-WindowState { [ExplorerSmoke.NativeWindow]::IsIconic($windowHandle) } 'minimized'
    [void][ExplorerSmoke.NativeWindow]::ShowWindow($windowHandle, 9)
    Wait-WindowState { -not [ExplorerSmoke.NativeWindow]::IsIconic($windowHandle) } 'restored after minimize'
    Save-WindowScreenshot $windowHandle (Join-Path $evidenceDirectory '02-restored-after-minimize.png')

    [void][ExplorerSmoke.NativeWindow]::ShowWindow($windowHandle, 3)
    Wait-WindowState { [ExplorerSmoke.NativeWindow]::IsZoomed($windowHandle) } 'maximized'
    Save-WindowScreenshot $windowHandle (Join-Path $evidenceDirectory '03-maximized.png')
    [void][ExplorerSmoke.NativeWindow]::ShowWindow($windowHandle, 9)
    Wait-WindowState { -not [ExplorerSmoke.NativeWindow]::IsZoomed($windowHandle) } 'restored after maximize'
    Save-WindowScreenshot $windowHandle (Join-Path $evidenceDirectory '04-restored-after-maximize.png')

    $interactionResourceSample = Get-ProcessResourceSample $process

    if (-not [ExplorerSmoke.NativeWindow]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw "PostMessage(WM_CLOSE) failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        throw 'timed out waiting for explorer-app to exit after WM_CLOSE.'
    }
    if ($process.ExitCode -ne 0) {
        throw "explorer-app exited with code $($process.ExitCode)"
    }

    $logText = Get-Content -Raw -Encoding utf8 -LiteralPath $logPath
    $eventNames = @('window_ready', 'application_stopped', 'clean_shutdown')
    $previousIndex = -1
    foreach ($eventName in $eventNames) {
        $eventIndex = $logText.IndexOf("event=`"$eventName`"", [StringComparison]::Ordinal)
        if ($eventIndex -lt 0) {
            throw "smoke log is missing event: $eventName"
        }
        if ($eventIndex -le $previousIndex) {
            throw "smoke lifecycle events are out of order at: $eventName"
        }
        $previousIndex = $eventIndex
    }

    $summary = [ordered]@{
        executable = $executablePath
        profile = $Profile
        process_id = $process.Id
        exit_code = $process.ExitCode
        original_size = "$originalWidth x $originalHeight"
        resized_size = "$resizedWidth x $resizedHeight"
        window_state_steps = @('ready', 'resized', 'minimized', 'restored', 'maximized', 'restored', 'closed')
        screenshots = @('01-resized.png', '02-restored-after-minimize.png', '03-maximized.png', '04-restored-after-maximize.png')
        close_method = 'PostMessage(WM_CLOSE)'
        lifecycle_events = $eventNames
        launch_utc = $launchUtc.ToString('o')
        ready_duration_ms = $readyDurationMs
        ready_resource_sample = $readyResourceSample
        post_interaction_resource_sample = $interactionResourceSample
        completed_utc = [DateTime]::UtcNow.ToString('o')
    }
    $summary | ConvertTo-Json -Depth 3 | Set-Content -Encoding utf8 (Join-Path $evidenceDirectory 'summary.json')
    Write-Output "Headful lifecycle smoke passed: $evidenceDirectory"
    Write-Output "Resize: $originalWidth x $originalHeight -> $resizedWidth x $resizedHeight"
    Write-Output 'Close: WM_CLOSE; exit code: 0; cleanup events: ordered'
} finally {
    if (-not $process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
    $process.Dispose()
}
