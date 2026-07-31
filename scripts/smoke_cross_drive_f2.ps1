param([switch]$SkipBuild, [string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('cross-drive-f2-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $SkipBuild) {
    cargo build -p explorer-app --locked
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ('CrossDriveF2.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace CrossDriveF2 {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    }
}
'@
}

function Find-NamedElement([Windows.Automation.AutomationElement]$Root, [string]$Name, [scriptblock]$Predicate, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
            if ($element.Current.Name -like "*$Name*" -and (& $Predicate $element)) { return $element }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $Name"
}

function Click-Element([Windows.Automation.AutomationElement]$Element) {
    $bounds = $Element.Current.BoundingRectangle
    [void][CrossDriveF2.Native]::SetCursorPos([int]($bounds.Left + [Math]::Min(100, $bounds.Width / 2)), [int]($bounds.Top + $bounds.Height / 2))
    [CrossDriveF2.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [CrossDriveF2.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
}

function Send-F2([Diagnostics.Process]$Process) {
    [void][CrossDriveF2.Native]::SetForegroundWindow($Process.MainWindowHandle)
    [CrossDriveF2.Native]::keybd_event(0x71, 0, 0, [UIntPtr]::Zero)
    [CrossDriveF2.Native]::keybd_event(0x71, 0, 2, [UIntPtr]::Zero)
}

$process = $null
try {
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $targetRoot 'debug\SuperExplorer.exe'))
    $start.WorkingDirectory = $workspaceRoot
    $start.UseShellExecute = $false
    $start.Environment['EXPLORER_INITIAL_PATH'] = 'C:\'
    $start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do { $process.Refresh(); Start-Sleep -Milliseconds 100 } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][CrossDriveF2.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 20, 20, 1100, 760, 0x0040)
    [void][CrossDriveF2.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)

    $cRow = Find-NamedElement $root 'Windows' { param($e) $e.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and $e.Current.BoundingRectangle.Left -gt 500 }
    $firstTargetName = $cRow.Current.Name
    Click-Element $cRow
    Send-F2 $process
    $firstEditor = Find-NamedElement $root 'Rename' { param($e) $e.Current.ControlType -eq [Windows.Automation.ControlType]::Edit }
    $firstEditorName = $firstEditor.Current.Name

    $dDrive = Find-NamedElement $root 'D:' { param($e) $e.Current.BoundingRectangle.Left -lt 500 }
    Click-Element $dDrive
    $dRow = Find-NamedElement $root 'test' { param($e) $e.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and $e.Current.BoundingRectangle.Left -gt 500 }
    $secondTargetName = $dRow.Current.Name
    Click-Element $dRow
    Send-F2 $process
    $secondEditor = Find-NamedElement $root 'Rename' { param($e) $e.Current.ControlType -eq [Windows.Automation.ControlType]::Edit }
    $secondEditorName = $secondEditor.Current.Name

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        first_target = $firstTargetName
        first_editor = $firstEditorName
        second_target = $secondTargetName
        second_editor = $secondEditorName
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Cross-drive F2 passed: $OutputDirectory"
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        [void][CrossDriveF2.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
}
