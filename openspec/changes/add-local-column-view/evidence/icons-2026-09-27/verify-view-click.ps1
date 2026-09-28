$ErrorActionPreference = "Continue"
$out = "D:\SuperExplorer\openspec\changes\add-local-column-view\evidence\icons-2026-09-27\view-click"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$fixture = Join-Path $out "fixture\a"
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$profileDir = Join-Path $out "localappdata"
New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
$result = Join-Path $out "result.txt"

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
$psi.WorkingDirectory = "D:\SuperExplorer"
$psi.UseShellExecute = $false
$psi.Environment["EXPLORER_INITIAL_PATH"] = $fixture
$psi.Environment["LOCALAPPDATA"] = $profileDir
$psi.Environment["EXPLORER_LOG_DIR"] = $out
$process = [Diagnostics.Process]::Start($psi)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ("ViewClick2.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ViewClick2 {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    }
}
"@
}

function Find-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

$lines = New-Object System.Collections.Generic.List[string]
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw "exited early $($process.ExitCode)" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    [void][ViewClick2.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0040)
    [void][ViewClick2.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 1500
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { throw "view button missing" }
    $bounds = $view.Current.BoundingRectangle
    $x = [int]($bounds.Left + ($bounds.Width / 2))
    $y = [int]($bounds.Top + ($bounds.Height / 2))
    $lines.Add("click $x $y size $($bounds.Width)x$($bounds.Height)")
    [void][ViewClick2.Native]::SetCursorPos($x, $y)
    Start-Sleep -Milliseconds 100
    [ViewClick2.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [ViewClick2.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $menu = Find-ByName $root "view-menu"
    $columns = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    $details = Find-ByName $root ([string]::new(@([char]0x8A73, [char]0x7D30, [char]0x8CC7, [char]0x6599)))
    $lines.Add("menu-id=$($null -ne $menu) columns=$($null -ne $columns) details=$($null -ne $details)")
    $rect = New-Object ViewClick2.Native+RECT
    [void][ViewClick2.Native]::GetWindowRect($process.MainWindowHandle, [ref] $rect)
    $width = [Math]::Max(1, $rect.Right - $rect.Left)
    $height = [Math]::Max(1, $rect.Bottom - $rect.Top)
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
    $shot = Join-Path $out "after-view-click.png"
    $bitmap.Save($shot, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
    $lines.Add("shot=$shot")
} catch {
    $lines.Add("ERROR $($_.Exception.Message)")
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
$lines | Set-Content -Path $result -Encoding utf8
Get-Content -Path $result
