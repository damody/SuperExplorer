param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = ""
)

$ErrorActionPreference = "Stop"
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory "fixture"
$parent = Join-Path $fixture "parent"
$empty = Join-Path $parent "empty"
New-Item -ItemType Directory -Force -Path $empty | Out-Null
Set-Content -Path (Join-Path $parent "keep.txt") -Value "keep" -Encoding utf8
$profile = Join-Path $OutputDirectory "localappdata"
New-Item -ItemType Directory -Force -Path $profile | Out-Null

if (-not $Executable) {
    $Executable = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
}
if (-not (Test-Path -LiteralPath $Executable)) {
    $report = [ordered]@{
        status = "SKIP"
        reason = "SuperExplorer.exe was not built, so the empty-folder window interaction was not run."
        executable = $Executable
        inaccessible = "unit-only"
    }
    $report | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 0
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ("ColumnViewEmptySmoke.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ColumnViewEmptySmoke {
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

function Find-Row([Windows.Automation.AutomationElement] $Root, [string] $Pattern) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem)
    foreach ($row in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)) {
        if ($row.Current.Name -match $Pattern) { return $row }
    }
    return $null
}

function Wait-Row([Windows.Automation.AutomationElement] $Root, [string] $Pattern, [int] $TimeoutMs = 4000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $match = Find-Row $Root $Pattern
        if ($null -ne $match) { return $match }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Click-Element([Windows.Automation.AutomationElement] $Element) {
    try {
        $pattern = $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        return
    } catch {
        $bounds = $Element.Current.BoundingRectangle
        $x = [int]($bounds.Left + ($bounds.Width / 2))
        $y = [int]($bounds.Top + ($bounds.Height / 2))
        [void][ColumnViewEmptySmoke.Native]::SetCursorPos($x, $y)
        [ColumnViewEmptySmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnViewEmptySmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
}

$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $Executable
$start.WorkingDirectory = "D:\SuperExplorer"
$start.UseShellExecute = $false
$start.Environment["EXPLORER_INITIAL_PATH"] = $parent
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
    [void][ColumnViewEmptySmoke.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x0040)
    [void][ColumnViewEmptySmoke.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    if ($null -eq (Wait-Row $root '(^| )keep\.txt( |$)' 8000)) {
        throw "initial address did not show the runner-owned parent fixture"
    }
    $observations.Add("parent fixture showed keep.txt")
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-ByName $root "View" }
    if ($null -eq $view) { throw "view menu button was not found" }
    Click-Element $view
    $columns = Wait-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04)))
    if ($null -eq $columns) { $columns = Wait-ByName $root "Columns" 1500 }
    if ($null -eq $columns) { throw "columns menu item was not found" }
    Click-Element $columns
    Start-Sleep -Milliseconds 700
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $surface = Find-ByName $root ([string]::new(@([char]0x5206, [char]0x6B04, [char]0x6AA2, [char]0x8996)))
    if ($null -eq $surface) { $surface = Find-ByName $root "Column view" }
    if ($null -eq $surface) { throw "column view surface was not exposed to UIA" }
    $observations.Add("column surface: $($surface.Current.Name)")
    $emptyRow = Wait-Row $root '(^| )empty( |$)' 4000
    if ($null -eq $emptyRow) { throw "empty folder row was not exposed to UIA" }
    Click-Element $emptyRow
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $emptyText = $null
    foreach ($name in @(([string]::new(@([char]0x6B64, [char]0x8CC7, [char]0x6599, [char]0x593E, [char]0x662F, [char]0x7A7A, [char]0x7684))), "This folder is empty")) {
        $emptyText = Wait-ByName $root $name 5000
        if ($null -ne $emptyText) { break }
    }
    if ($null -eq $emptyText) { throw "empty folder column did not expose a localized empty status" }
    $observations.Add("empty status: $($emptyText.Current.Name)")
    if ($null -eq (Wait-Row $root '(^| )keep\.txt( |$)' 4000)) { throw "empty folder column replaced the usable parent" }
    $observations.Add("parent keep.txt stayed visible")
    $retry = $null
    foreach ($name in @(([string]::new(@([char]0x91CD, [char]0x8A66))), "Retry")) {
        $retry = Find-ByName $root $name
        if ($null -ne $retry) { break }
    }
    if ($null -eq $retry) { throw "empty column did not expose the existing retry control" }
    $observations.Add("retry control: $($retry.Current.Name)")
    $report = [ordered]@{
        status = "PASS"
        reason = "An empty runner-owned folder showed a localized empty status, kept the parent column, and exposed retry. Inaccessible folders stay unit-only; this script does not change ACLs."
        fixture = $parent
        executable = $Executable
        inaccessible = "unit-only"
        observed = @($observations)
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 0
} catch {
    $report = [ordered]@{
        status = "FAIL"
        reason = $_.Exception.Message
        fixture = $parent
        executable = $Executable
        inaccessible = "unit-only"
        observed = @($observations)
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit 1
} finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
