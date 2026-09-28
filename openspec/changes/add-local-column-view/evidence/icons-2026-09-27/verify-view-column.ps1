$ErrorActionPreference = "Continue"
$out = "D:\SuperExplorer\openspec\changes\add-local-column-view\evidence\icons-2026-09-27\view-column"
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
if (-not ("ViewCol.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ViewCol {
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
function Click-At([Windows.Automation.AutomationElement] $Element) {
    $bounds = $Element.Current.BoundingRectangle
    $x = [int]($bounds.Left + ($bounds.Width / 2))
    $y = [int]($bounds.Top + ($bounds.Height / 2))
    [void][ViewCol.Native]::SetCursorPos($x, $y)
    Start-Sleep -Milliseconds 60
    [ViewCol.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 50
    [ViewCol.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    return "$x,$y"
}
function Save-Shot([IntPtr] $Hwnd, [string] $Path) {
    $rect = New-Object ViewCol.Native+RECT
    [void][ViewCol.Native]::GetWindowRect($Hwnd, [ref] $rect)
    $width = [Math]::Max(1, $rect.Right - $rect.Left)
    $height = [Math]::Max(1, $rect.Bottom - $rect.Top)
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
    $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
}

$lines = New-Object System.Collections.Generic.List[string]
$viewName = [string]::new(@([char]0x6AA2, [char]0x8996))
$columnsName = [string]::new(@([char]0x5206, [char]0x6B04))
$detailsName = [string]::new(@([char]0x8A73, [char]0x7D30, [char]0x8CC7, [char]0x6599))
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw "exited early" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    [void][ViewCol.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0040)
    [void][ViewCol.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 1200
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $view = Find-ByName $root $viewName
    $lines.Add("open-view " + (Click-At $view))
    Start-Sleep -Milliseconds 400
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $columns = Find-ByName $root $columnsName
    if ($null -eq $columns) { throw "columns missing before switch" }
    $lines.Add("click-columns " + (Click-At $columns))
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $view = Find-ByName $root $viewName
    $lines.Add("reopen-view " + (Click-At $view))
    Start-Sleep -Milliseconds 500
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $columns = Find-ByName $root $columnsName
    $details = Find-ByName $root $detailsName
    $lines.Add("after-reopen columns=$($null -ne $columns) details=$($null -ne $details)")
    Save-Shot $process.MainWindowHandle (Join-Path $out "column-menu.png")
    if ($null -ne $details) {
        $lines.Add("click-details " + (Click-At $details))
        Start-Sleep -Milliseconds 600
        Save-Shot $process.MainWindowHandle (Join-Path $out "after-details.png")
        $lines.Add("clicked-details")
    }
} catch {
    $lines.Add("ERROR $($_.Exception.Message)")
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
$lines | Set-Content -Path $result -Encoding utf8
Get-Content -Path $result
