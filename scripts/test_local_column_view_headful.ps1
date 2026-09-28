param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = ""
)

$ErrorActionPreference = "Stop"
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory "fixture"
$nested = Join-Path $fixture "a\b"
New-Item -ItemType Directory -Force -Path $nested | Out-Null
Set-Content -Path (Join-Path $fixture "a\notes.txt") -Value "column-preview" -Encoding utf8
Set-Content -Path (Join-Path $fixture "a\b\child.txt") -Value "nested" -Encoding utf8
$profile = Join-Path $OutputDirectory "localappdata"
New-Item -ItemType Directory -Force -Path $profile | Out-Null

if (-not $Executable) {
    $Executable = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
}
if (-not (Test-Path -LiteralPath $Executable)) {
    $report = [ordered]@{
        status = "SKIP"
        reason = "SuperExplorer.exe was not built, so the window interaction was not run."
        executable = $Executable
    }
    $report | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 0
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ("ColumnViewSmoke.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ColumnViewSmoke {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
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
        [void][ColumnViewSmoke.Native]::SetCursorPos($x, $y)
        [ColumnViewSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnViewSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "pointer"
    }
}

function Find-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Wait-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name, [int] $TimeoutMs = 4000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $match = Find-ByName $Root $Name
        if ($null -ne $match) { return $match }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $Executable
$start.WorkingDirectory = "D:\SuperExplorer"
$start.UseShellExecute = $false
$start.Environment["EXPLORER_INITIAL_PATH"] = (Join-Path $fixture "a")
$start.Environment["EXPLORER_LOG_DIR"] = $OutputDirectory
$start.Environment["LOCALAPPDATA"] = $profile
$start.Environment["SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION"] = "1"
$process = [Diagnostics.Process]::Start($start)
$observations = New-Object System.Collections.Generic.List[string]
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw "application window did not appear" }
    [void][ColumnViewSmoke.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0040)
    [void][ColumnViewSmoke.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    if ($null -eq (Wait-ByName $root "notes.txt" 2500)) {
        $rows = $root.FindAll([Windows.Automation.TreeScope]::Descendants, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)))
        if (-not (@($rows | Where-Object { $_.Current.Name -match 'notes\.txt' }).Count)) { throw "initial address did not show the runner fixture" }
    }
    $observations.Add("initial fixture entry notes.txt was visible")
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-ByName $root "View" }
    if ($null -eq $view) { throw "view menu button was not found" }
    $observations.Add("found view button: $($view.Current.Name)")
    $observations.Add(("view click: " + (Click-Element $view)))
    $columns = Wait-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    if ($null -eq $columns) { $columns = Wait-ByName $root "Columns" 1000 }
    if ($null -eq $columns) { throw "columns menu item was not found" }
    $observations.Add("found columns item: $($columns.Current.Name)")
    $observations.Add(("columns click: " + (Click-Element $columns)))
    Start-Sleep -Milliseconds 700
    $surface = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04, [char]0x6AA2, [char]0x8996)))
    if ($null -eq $surface) { $surface = Find-ByName $root "Column view" }
    if ($null -eq $surface) { throw "column view surface was not exposed to UIA" }
    $observations.Add("column surface: $($surface.Current.Name)")
    $listItems = $root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        (New-Object Windows.Automation.PropertyCondition(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem)))
    $folder = $null
    foreach ($item in $listItems) {
        if ($item.Current.Name -match '(^| )b( |$)') {
            $folder = $item
            break
        }
    }
    if ($null -eq $folder) { throw "column folder row b was not exposed to UIA" }
    $observations.Add("found folder row: $($folder.Current.Name)")
    $observations.Add(("folder click: " + (Click-Element $folder)))
    Start-Sleep -Milliseconds 500
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $childRows = $root.FindAll([Windows.Automation.TreeScope]::Descendants, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)))
    if (-not (@($childRows | Where-Object { $_.Current.Name -match 'child\.txt' }).Count)) { throw "folder click did not reveal child.txt in the next column" }
    $observations.Add("nested child.txt was exposed after clicking b")
    $previewName = [string]::new(@([char]0x6574, [char]0x5408, [char]0x9810, [char]0x89BD))
    $preview = Find-ByName $root $previewName
    if ($null -eq $preview) { $preview = Find-ByName $root "Integrated preview" }
    if ($null -eq $preview) { throw "integrated preview was missing before Alt+P" }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait("%p")
    Start-Sleep -Milliseconds 450
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $previewAfter = Find-ByName $root $previewName
    if ($null -eq $previewAfter) { $previewAfter = Find-ByName $root "Integrated preview" }
    if ($null -ne $previewAfter) { throw "Alt+P did not hide integrated preview" }
    $observations.Add("Alt+P hid the integrated preview")
    $report = [ordered]@{
        status = "PASS"
        reason = "Fixture navigation exposed a child column and Alt+P hid the integrated preview."
        fixture = (Join-Path $fixture "a")
        executable = $Executable
        observed = @($observations)
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 0
} catch {
    $report = [ordered]@{
        status = "FAIL"
        reason = $_.Exception.Message
        fixture = (Join-Path $fixture "a")
        executable = $Executable
        observed = @($observations)
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 1
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
