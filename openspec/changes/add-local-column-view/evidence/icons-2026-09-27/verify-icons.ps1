param(
    [string] $OutputDirectory = $PSScriptRoot,
    [string] $Executable = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory "fixture"
$nested = Join-Path $fixture "a\b"
New-Item -ItemType Directory -Force -Path $nested | Out-Null
Set-Content -Path (Join-Path $fixture "a\notes.txt") -Value "column-icon" -Encoding utf8
Set-Content -Path (Join-Path $fixture "a\b\child.txt") -Value "nested" -Encoding utf8
$profile = Join-Path $OutputDirectory "localappdata"
New-Item -ItemType Directory -Force -Path $profile | Out-Null

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ("ColumnIconSmoke.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ColumnIconSmoke {
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

function Click-Element([Windows.Automation.AutomationElement] $Element) {
    try {
        $pattern = $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        return "invoke"
    } catch {
        $bounds = $Element.Current.BoundingRectangle
        $x = [int]($bounds.Left + ($bounds.Width / 2))
        $y = [int]($bounds.Top + ($bounds.Height / 2))
        [void][ColumnIconSmoke.Native]::SetCursorPos($x, $y)
        [ColumnIconSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnIconSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "pointer"
    }
}

function Find-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $Executable
$start.WorkingDirectory = "D:\SuperExplorer"
$start.UseShellExecute = $false
$start.Environment["EXPLORER_INITIAL_PATH"] = (Join-Path $fixture "a")
$start.Environment["EXPLORER_LOG_DIR"] = $OutputDirectory
$start.Environment["LOCALAPPDATA"] = $profile
$process = [Diagnostics.Process]::Start($start)
$observations = New-Object System.Collections.Generic.List[string]
$shot = Join-Path $OutputDirectory "column-icons.png"
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw "application window did not appear" }
    [void][ColumnIconSmoke.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0040)
    [void][ColumnIconSmoke.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 900
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-ByName $root "View" }
    if ($null -eq $view) { throw "view menu button was not found" }
    $observations.Add(("view click: " + (Click-Element $view)))
    Start-Sleep -Milliseconds 400
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $columns = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    if ($null -eq $columns) { $columns = Find-ByName $root "Columns" }
    if ($null -eq $columns) { throw "columns menu item was not found" }
    $observations.Add(("columns click: " + (Click-Element $columns)))
    $iconDeadline = [DateTime]::UtcNow.AddSeconds(8)
    $iconCount = 0
    $rowCount = 0
    do {
        Start-Sleep -Milliseconds 400
        $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $images = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Image)))
        $iconCount = 0
        foreach ($image in $images) {
            if ($image.Current.Name -like "column-icon *") { $iconCount++ }
        }
        $listItems = $root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::ListItem)))
        $rowCount = 0
        foreach ($item in $listItems) {
            if ($item.Current.Name -like "*b*" -or $item.Current.Name -like "*notes*") { $rowCount++ }
        }
        if ($iconCount -ge 1 -and $rowCount -ge 1) { break }
    } while ([DateTime]::UtcNow -lt $iconDeadline)
    $observations.Add("column-icon images: $iconCount")
    $observations.Add("matching rows: $rowCount")
    Start-Sleep -Milliseconds 1200
    $rect = New-Object ColumnIconSmoke.Native+RECT
    [void][ColumnIconSmoke.Native]::GetWindowRect($process.MainWindowHandle, [ref] $rect)
    $width = [Math]::Max(1, $rect.Right - $rect.Left)
    $height = [Math]::Max(1, $rect.Bottom - $rect.Top)
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
    $bitmap.Save($shot, [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
    if ($iconCount -lt 1) { throw "column rows did not expose column-icon images" }
    if ($rowCount -lt 1) { throw "column rows were not exposed" }
    $report = [ordered]@{
        status = "PASS"
        reason = "Column rows exposed column-icon images."
        iconCount = $iconCount
        rowCount = $rowCount
        screenshot = $shot
        observed = @($observations)
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 0
} catch {
    if (-not (Test-Path -LiteralPath $shot) -and $process -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
        try {
            $rect = New-Object ColumnIconSmoke.Native+RECT
            [void][ColumnIconSmoke.Native]::GetWindowRect($process.MainWindowHandle, [ref] $rect)
            $width = [Math]::Max(1, $rect.Right - $rect.Left)
            $height = [Math]::Max(1, $rect.Bottom - $rect.Top)
            $bitmap = New-Object System.Drawing.Bitmap $width, $height
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
            $bitmap.Save($shot, [System.Drawing.Imaging.ImageFormat]::Png)
            $graphics.Dispose()
            $bitmap.Dispose()
        } catch {}
    }
    $report = [ordered]@{
        status = "FAIL"
        reason = $_.Exception.Message
        screenshot = $shot
        observed = @($observations)
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 1
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
