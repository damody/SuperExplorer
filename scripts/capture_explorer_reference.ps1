param(
    [string]$LocationUrl = 'file:///D:/',
    [ValidateSet('light', 'dark')]
    [string]$Theme = 'light',
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $workspaceRoot ('target\explorer-reference-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

Add-Type -AssemblyName System.Drawing
if (-not ('ExplorerReference.NativeWindow' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerReference {
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
        public static extern bool ShowWindow(IntPtr window, int command);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PrintWindow(IntPtr window, IntPtr deviceContext, uint flags);
        [DllImport("dwmapi.dll")]
        public static extern int DwmFlush();
    }
}
'@
}

$shell = New-Object -ComObject Shell.Application
$explorerWindow = $null
try {
    $explorerWindow = @($shell.Windows()) |
        Where-Object { $_.LocationURL -eq $LocationUrl } |
        Select-Object -First 1
    if ($null -eq $explorerWindow) {
        throw "no open Explorer window matches LocationURL '$LocationUrl'"
    }
    $windowHandle = [IntPtr]([int64]$explorerWindow.HWND)
    $locationName = [string]$explorerWindow.LocationName
    [void][ExplorerReference.NativeWindow]::ShowWindow($windowHandle, 9)
    [void][ExplorerReference.NativeWindow]::SetForegroundWindow($windowHandle)
    Start-Sleep -Milliseconds 500
    [void][ExplorerReference.NativeWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $rect = [ExplorerReference.NativeWindow+Rect]::new()
    if (-not [ExplorerReference.NativeWindow]::GetWindowRect($windowHandle, [ref]$rect)) {
        throw "GetWindowRect failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) {
        throw "invalid Explorer capture bounds: $width x $height"
    }
    [void][ExplorerReference.NativeWindow]::DwmFlush()
    $bitmap = [Drawing.Bitmap]::new($width, $height, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $deviceContext = $graphics.GetHdc()
            try {
                if (-not [ExplorerReference.NativeWindow]::PrintWindow($windowHandle, $deviceContext, 2)) {
                    throw "PrintWindow failed with Win32 error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
                }
            } finally {
                $graphics.ReleaseHdc($deviceContext)
            }
        } finally {
            $graphics.Dispose()
        }
        $screenshotPath = Join-Path $OutputDirectory 'screenshot.png'
        $bitmap.Save($screenshotPath, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bitmap.Dispose()
    }

    $windows = Get-CimInstance Win32_OperatingSystem
    $explorerVersion = (Get-Item -LiteralPath (Join-Path $env:WINDIR 'explorer.exe')).VersionInfo.FileVersion
    $dpi = [int][ExplorerReference.NativeWindow]::GetDpiForWindow($windowHandle)
    $metadata = [ordered]@{
        schema_version = 1
        capture_kind = 'windows-explorer-reference'
        captured_utc = [DateTime]::UtcNow.ToString('o')
        windows = [ordered]@{ edition = $windows.Caption; version = $windows.Version; build = $windows.BuildNumber }
        explorer = [ordered]@{ file_version = $explorerVersion; location_url = $LocationUrl; location_name = $locationName }
        window = [ordered]@{ width = $width; height = $height; left = $rect.Left; top = $rect.Top }
        dpi = [ordered]@{ window_dpi = $dpi; percent = [math]::Round($dpi * 100 / 96) }
        theme = $Theme
        font = 'Windows system UI / Microsoft JhengHei UI for Traditional Chinese'
        capture_api = 'PrintWindow(PW_RENDERFULLCONTENT) after DwmFlush'
        screenshot_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $screenshotPath).Hash
    }
    $metadata | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'metadata.json')
    $metadata | ConvertTo-Json -Depth 6
} finally {
    if ($null -ne $explorerWindow) {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($explorerWindow)
    }
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
}
