param(
    [ValidateSet('light', 'dark')]
    [string]$Theme = 'light',
    [ValidateSet(100, 125, 150, 175, 200)]
    [int]$ExpectedDpiPercent = 100,
    [ValidateRange(640, 7680)]
    [int]$Width = 1120,
    [ValidateRange(480, 4320)]
    [int]$Height = 720,
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [ValidateSet('empty', 'populated', 'error', 'multi-tab', 'operation', 'drag-cue', 'search', 'focused')]
    [string]$State = 'populated',
    [ValidateSet('normal', 'hover', 'pressed')]
    [string]$InteractionState = 'normal',
    [ValidateSet('active', 'inactive')]
    [string]$WindowActivation = 'active',
    [int]$TimeoutSeconds = 30,
    [string]$OutputDirectory,
    [switch]$AllowDpiMismatch,
    [string]$RealPath,
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
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('visual-actual\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
} else {
    $OutputDirectory = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
        [System.IO.Path]::GetFullPath($OutputDirectory)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
    }
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

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

if (-not ('ExplorerVisual.NativeWindow' -as [type])) {
    Add-Type -AssemblyName System.Drawing
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace ExplorerVisual {
    public static class NativeWindow {
        [StructLayout(LayoutKind.Sequential)]
        public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(IntPtr window, out Rect rect);

        [DllImport("user32.dll")]
        public static extern uint GetDpiForWindow(IntPtr window);

        [DllImport("user32.dll")]
        public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);

        [DllImport("user32.dll")]
        public static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

        [DllImport("dwmapi.dll")]
        public static extern int DwmFlush();

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PrintWindow(IntPtr window, IntPtr deviceContext, uint flags);
    }
}
'@
}

$diagnosticsPath = Join-Path $OutputDirectory 'diagnostics.json'
$logPath = Join-Path $OutputDirectory 'explorer.log'
$screenshotPath = Join-Path $OutputDirectory 'screenshot.png'
$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $executablePath
$startInfo.WorkingDirectory = $workspaceRoot
$startInfo.UseShellExecute = $false
$startInfo.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$startInfo.Environment['EXPLORER_VISUAL_FIXTURE'] = '1'
$startInfo.Environment['EXPLORER_VISUAL_WIDTH'] = $Width.ToString([Globalization.CultureInfo]::InvariantCulture)
$startInfo.Environment['EXPLORER_VISUAL_HEIGHT'] = $Height.ToString([Globalization.CultureInfo]::InvariantCulture)
$startInfo.Environment['EXPLORER_VISUAL_DPI'] = $ExpectedDpiPercent.ToString([Globalization.CultureInfo]::InvariantCulture)
$startInfo.Environment['EXPLORER_VISUAL_THEME'] = $Theme
$startInfo.Environment['EXPLORER_VISUAL_FONT'] = 'Microsoft JhengHei UI'
$startInfo.Environment['EXPLORER_VISUAL_STATE'] = $State
$startInfo.Environment['EXPLORER_VISUAL_DIAGNOSTICS'] = $diagnosticsPath
if ($RealPath) {
    $resolvedRealPath = (Resolve-Path -LiteralPath $RealPath -ErrorAction Stop).Path
    if (-not (Test-Path -LiteralPath $resolvedRealPath -PathType Container)) {
        throw "RealPath must resolve to an existing directory: $RealPath"
    }
    $startInfo.Environment['EXPLORER_VISUAL_REAL_SHELL'] = '1'
    $startInfo.Environment['EXPLORER_INITIAL_PATH'] = $resolvedRealPath
}
$process = [System.Diagnostics.Process]::Start($startInfo)

try {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $windowHandle = [IntPtr]::Zero
    $ready = $false
    do {
        if ($process.HasExited) {
            throw "explorer-app exited before visual_fixture_ready with code $($process.ExitCode)"
        }
        $process.Refresh()
        $windowHandle = $process.MainWindowHandle
        if (Test-Path -LiteralPath $logPath) {
            $logText = Get-Content -Raw -Encoding utf8 -LiteralPath $logPath
            if ($logText.Contains('event="visual_fixture_failed"')) {
                throw 'application reported visual_fixture_failed; inspect explorer.log'
            }
            $ready = $logText.Contains('event="visual_fixture_ready"') -and (Test-Path -LiteralPath $diagnosticsPath)
        }
        if (-not $ready -or $windowHandle -eq [IntPtr]::Zero) {
            Start-Sleep -Milliseconds 50
        }
    } while ((!$ready -or $windowHandle -eq [IntPtr]::Zero) -and [DateTime]::UtcNow -lt $deadline)
    if (-not $ready -or $windowHandle -eq [IntPtr]::Zero) {
        throw 'timed out waiting for first-frame visual_fixture_ready, diagnostics, and HWND'
    }

    # PowerShell itself is DPI-unaware by default. Use a per-thread PMv2 context so
    # GetWindowRect and System.Drawing.CopyFromScreen agree on physical coordinates.
    [void][ExplorerVisual.NativeWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $actualDpi = [int][ExplorerVisual.NativeWindow]::GetDpiForWindow($windowHandle)
    $expectedDpi = [int](96 * $ExpectedDpiPercent / 100)
    $dpiMatchesExpectation = $actualDpi -eq $expectedDpi
    if (-not $dpiMatchesExpectation -and -not $AllowDpiMismatch) {
        throw "window DPI $actualDpi does not match expected $expectedDpi ($ExpectedDpiPercent%); run in the matching Windows DPI session"
    }
    if (-not $dpiMatchesExpectation) {
        Write-Warning "DPI mismatch allowed for harness validation only: actual $actualDpi, expected $expectedDpi. This capture cannot become a baseline."
    }
    $rect = [ExplorerVisual.NativeWindow+Rect]::new()
    if (-not [ExplorerVisual.NativeWindow]::GetWindowRect($windowHandle, [ref]$rect)) {
        throw "GetWindowRect failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $capturedWidth = $rect.Right - $rect.Left
    $capturedHeight = $rect.Bottom - $rect.Top
    if ($capturedWidth -le 0 -or $capturedHeight -le 0) {
        throw "invalid capture bounds: $capturedWidth x $capturedHeight"
    }
    [void][ExplorerVisual.NativeWindow]::SetForegroundWindow($windowHandle)
    $activationValue = if ($WindowActivation -eq 'active') { [IntPtr]1 } else { [IntPtr]::Zero }
    $activationMessagesPosted =
        [ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x001C, $activationValue, [IntPtr]::Zero) -and
        [ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x0086, $activationValue, [IntPtr]::Zero) -and
        [ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x0006, $activationValue, [IntPtr]::Zero)
    if (-not $activationMessagesPosted) {
        throw "failed to inject deterministic $WindowActivation activation messages"
    }
    Start-Sleep -Milliseconds 150
    $foregroundWindow = [ExplorerVisual.NativeWindow]::GetForegroundWindow()
    $actualScale = [double]$actualDpi / 96.0
    $interactionX = [int][math]::Round(72 * $actualScale)
    $interactionY = [int][math]::Round(73 * $actualScale)
    $interactionPoint = [IntPtr](($interactionY -shl 16) -bor ($interactionX -band 0xffff))
    if ($InteractionState -in @('hover', 'pressed')) {
        [void][ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x0200, [IntPtr]::Zero, $interactionPoint)
    }
    if ($InteractionState -eq 'pressed') {
        [void][ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x0201, [IntPtr]1, $interactionPoint)
    }
    # The application marker proves GPUI submitted the first frame. DwmFlush waits for the
    # compositor to present it so CopyFromScreen cannot capture a partially composed surface.
    Start-Sleep -Milliseconds 100
    $dwmResult = [ExplorerVisual.NativeWindow]::DwmFlush()
    if ($dwmResult -ne 0) {
        throw "DwmFlush failed with HRESULT $dwmResult"
    }
    $bitmap = [System.Drawing.Bitmap]::new($capturedWidth, $capturedHeight, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $deviceContext = $graphics.GetHdc()
            try {
                if (-not [ExplorerVisual.NativeWindow]::PrintWindow($windowHandle, $deviceContext, 2)) {
                    throw "PrintWindow failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
                }
            } finally {
                $graphics.ReleaseHdc($deviceContext)
            }
        } finally {
            $graphics.Dispose()
        }
        $bitmap.Save($screenshotPath, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bitmap.Dispose()
    }

    $diagnostics = Get-Content -Raw -Encoding utf8 -LiteralPath $diagnosticsPath | ConvertFrom-Json
    $windows = Get-CimInstance Win32_OperatingSystem
    $explorerVersion = (Get-Item -LiteralPath (Join-Path $env:WINDIR 'explorer.exe')).VersionInfo.FileVersion
    $appVersion = (Get-Item -LiteralPath $executablePath).VersionInfo.ProductVersion
    $appCommit = (& git -C $workspaceRoot rev-parse HEAD).Trim()
    $gpuiRevision = (& git -C (Join-Path $workspaceRoot 'vendor\gpui-ce') rev-parse HEAD).Trim()
    $dirty = [bool](& git -C $workspaceRoot status --porcelain)
    $metadata = [ordered]@{
        schema_version = 1
        capture_kind = 'application'
        windows = [ordered]@{ edition = $windows.Caption; version = $windows.Version; build = $windows.BuildNumber }
        explorer = [ordered]@{ file_version = $explorerVersion }
        app = [ordered]@{ version = $appVersion; commit = $appCommit; dirty = $dirty }
        gpui = [ordered]@{ repository = 'https://github.com/gpui-ce/gpui-ce.git'; revision = $gpuiRevision }
        dpi = [ordered]@{
            expected_percent = $ExpectedDpiPercent
            actual_window_dpi = $actualDpi
            actual_scale_factor = $diagnostics.fixture.actual_scale_factor
            matches_expectation = $dpiMatchesExpectation
        }
        theme = $Theme
        window = [ordered]@{ logical_width = $Width; logical_height = $Height; captured_width = $capturedWidth; captured_height = $capturedHeight }
        font = 'Microsoft JhengHei UI'
        fixture_state = $State
        real_shell_path = if ($RealPath) { $resolvedRealPath } else { $null }
        interaction_state = $InteractionState
        window_activation = $WindowActivation
        activation_driver = 'WM_ACTIVATEAPP/WM_NCACTIVATE/WM_ACTIVATE fixture messages'
        activation_messages_posted = $activationMessagesPosted
        foreground_window_is_application = ($foregroundWindow -eq $windowHandle)
        captured_utc = [DateTime]::UtcNow.ToString('o')
        screenshot_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $screenshotPath).Hash
        diagnostics_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $diagnosticsPath).Hash
    }
    $metadata | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'metadata.json')

    if ($InteractionState -eq 'pressed') {
        # Release outside the hit target so the fixture records pressed styling without
        # invoking the button's production command after the screenshot is complete.
        [void][ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x001F, [IntPtr]::Zero, [IntPtr]::Zero)
        $releaseX = [int][math]::Round(700 * $actualScale)
        $releaseY = [int][math]::Round(20 * $actualScale)
        $releasePoint = [IntPtr](($releaseY -shl 16) -bor ($releaseX -band 0xffff))
        [void][ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x0200, [IntPtr]::Zero, $releasePoint)
        [void][ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x0202, [IntPtr]::Zero, $releasePoint)
    }

    if (-not [ExplorerVisual.NativeWindow]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw "PostMessage(WM_CLOSE) failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    if (-not $process.WaitForExit($TimeoutSeconds * 1000) -or $process.ExitCode -ne 0) {
        throw 'visual fixture did not exit cleanly after WM_CLOSE'
    }
    $logText = Get-Content -Raw -Encoding utf8 -LiteralPath $logPath
    foreach ($eventName in @('visual_fixture_ready', 'application_stopped', 'clean_shutdown')) {
        if (-not $logText.Contains("event=`"$eventName`"")) {
            throw "visual fixture log is missing event: $eventName"
        }
    }
    Write-Output "Visual fixture captured: $OutputDirectory"
    Write-Output "Theme: $Theme; DPI: $actualDpi; window capture: $capturedWidth x $capturedHeight"
} finally {
    if (-not $process.HasExited) {
        $process.Kill()
        $process.WaitForExit()
    }
    $process.Dispose()
}
