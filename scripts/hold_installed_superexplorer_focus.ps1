param(
    [int]$DurationSeconds = 1250,
    [string]$InitialPath = ''
)
$ErrorActionPreference = 'Stop'

if (-not ('SuperExplorerFocus.NativeWindow' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace SuperExplorerFocus {
    public static class NativeWindow {
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
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool ShowWindow(IntPtr window, int command);
    }
}
'@
}

function Set-SuperExplorerForeground([Diagnostics.Process]$Process) {
    $Process.Refresh()
    $window = $Process.MainWindowHandle
    if ($window -eq [IntPtr]::Zero) { return $false }

    $currentThread = [SuperExplorerFocus.NativeWindow]::GetCurrentThreadId()
    $foregroundWindow = [SuperExplorerFocus.NativeWindow]::GetForegroundWindow()
    $foregroundThread = [SuperExplorerFocus.NativeWindow]::GetWindowThreadProcessId(
        $foregroundWindow, [IntPtr]::Zero)
    $targetThread = [SuperExplorerFocus.NativeWindow]::GetWindowThreadProcessId(
        $window, [IntPtr]::Zero)
    $attachedForeground = $foregroundThread -ne 0 -and $foregroundThread -ne $currentThread -and
        [SuperExplorerFocus.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $true)
    $attachedTarget = $targetThread -ne 0 -and $targetThread -ne $currentThread -and
        [SuperExplorerFocus.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $true)
    try {
        [void][SuperExplorerFocus.NativeWindow]::ShowWindow($window, 9)
        [void][SuperExplorerFocus.NativeWindow]::BringWindowToTop($window)
        [void][SuperExplorerFocus.NativeWindow]::SetForegroundWindow($window)
        [void][SuperExplorerFocus.NativeWindow]::SetFocus($window)
        return [SuperExplorerFocus.NativeWindow]::GetForegroundWindow() -eq $window
    } finally {
        if ($attachedTarget) {
            [void][SuperExplorerFocus.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $false)
        }
        if ($attachedForeground) {
            [void][SuperExplorerFocus.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $false)
        }
    }
}

$install = 'C:\Program Files\SuperExplorer'
$exe = Join-Path $install 'SuperExplorer.exe'
$plugins = Get-ChildItem -LiteralPath (Join-Path $install 'plugins') -Filter '*.dll' -File | Sort-Object Name
$arguments = [Collections.Generic.List[string]]::new()
foreach ($plugin in $plugins) {
    $arguments.Add('--plugin-dll')
    $arguments.Add(('"{0}"' -f $plugin.FullName))
}
if ([string]::IsNullOrWhiteSpace($InitialPath)) {
    Remove-Item Env:EXPLORER_INITIAL_PATH -ErrorAction SilentlyContinue
} else {
    $env:EXPLORER_INITIAL_PATH = $InitialPath
}
$process = Start-Process -FilePath $exe -ArgumentList ($arguments -join ' ') -PassThru
$shell = New-Object -ComObject WScript.Shell
$timer = [Diagnostics.Stopwatch]::StartNew()
try {
    while ($timer.Elapsed.TotalSeconds -lt $DurationSeconds -and -not $process.HasExited) {
        if (-not (Set-SuperExplorerForeground $process)) {
            [void]$shell.AppActivate($process.Id)
        }
        Start-Sleep -Milliseconds 500
        $process.Refresh()
    }
} finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
}
