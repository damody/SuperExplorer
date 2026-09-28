$ErrorActionPreference = "Stop"
$out = "D:\SuperExplorer\openspec\changes\add-local-column-view\evidence\icons-2026-09-27\log-quiet"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$fixture = Join-Path $out "fixture\a"
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$profileDir = Join-Path $out "localappdata"
New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
$log = Join-Path $out "stdout.log"
if (Test-Path $log) { Remove-Item $log -Force }

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
$psi.WorkingDirectory = "D:\SuperExplorer"
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment["EXPLORER_INITIAL_PATH"] = $fixture
$psi.Environment["LOCALAPPDATA"] = $profileDir
$psi.Environment["EXPLORER_LOG_DIR"] = $out
$process = [Diagnostics.Process]::Start($psi)
$writer = [System.IO.StreamWriter]::new($log, $false)
$writer.AutoFlush = $true
$process.add_OutputDataReceived({ param($sender, $event) if ($event.Data) { $writer.WriteLine($event.Data) } })
$process.add_ErrorDataReceived({ param($sender, $event) if ($event.Data) { $writer.WriteLine($event.Data) } })
$process.BeginOutputReadLine()
$process.BeginErrorReadLine()

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ("LogQuiet.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace LogQuiet {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    }
}
"@
}

function Find-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}
function Click-Element([Windows.Automation.AutomationElement] $Element) {
    try {
        $pattern = $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        return "invoke"
    } catch {
        $bounds = $Element.Current.BoundingRectangle
        [void][LogQuiet.Native]::SetCursorPos([int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
        [LogQuiet.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [LogQuiet.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "pointer"
    }
}

$clicked = $false
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) { throw "exited early" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    [void][LogQuiet.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0040)
    [void][LogQuiet.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { throw "view button missing" }
    Write-Output ("view click: " + (Click-Element $view))
    Start-Sleep -Milliseconds 500
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $columns = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    if ($null -eq $columns) {
        $names = New-Object System.Collections.Generic.List[string]
        $all = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
        foreach ($item in $all) {
            $name = $item.Current.Name
            if ($name -and ($name.Contains([char]0x6B04) -or $name -match "Column|View|menu")) {
                $names.Add($name)
            }
        }
        Set-Content -Path (Join-Path $out "menu-names.txt") -Value ($names -join "`n") -Encoding utf8
        throw "columns item missing"
    }
    Write-Output ("columns click: " + (Click-Element $columns))
    $clicked = $true
    Start-Sleep -Seconds 3
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        $process.WaitForExit(2000) | Out-Null
    }
    $writer.Flush()
    $writer.Dispose()
}

$text = if (Test-Path $log) { Get-Content -Path $log -Raw -ErrorAction SilentlyContinue } else { "" }
$boundary = ([regex]::Matches([string]$text, "UpdatePreviewHostBoundary")).Count
$dispatched = ([regex]::Matches([string]$text, "Explorer action dispatched")).Count
Write-Output "clicked=$clicked boundary=$boundary dispatched=$dispatched"
if (-not $clicked -or $boundary -gt 2) { exit 1 }
exit 0
