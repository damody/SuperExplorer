param(
    [Parameter(Mandatory)][string]$RemotePath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Executable
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($Executable)) {
    $Executable = Join-Path $workspace 'target\release\SuperExplorer.exe'
}
$fixture = Join-Path $workspace 'target\transfer-cancel-user-fixture'
New-Item -ItemType Directory -Force -Path $fixture, $OutputDirectory | Out-Null
$payloadName = 'cancel-user-check-{0}.bin' -f ([IO.Path]::GetFileName($OutputDirectory) -replace '[^A-Za-z0-9._-]', '-')
$payload = Join-Path $fixture $payloadName
if (-not (Test-Path -LiteralPath $payload)) {
    $stream = [IO.File]::Open($payload, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    try { $stream.SetLength(1024L * 1024L * 1024L) } finally { $stream.Dispose() }
}

Import-Module (Join-Path $workspace 'scripts\UitestHeadful.psm1') -Force
if (-not ('SuperExplorerTransferProbe.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace SuperExplorerTransferProbe {
    public static class Native {
        [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr hwnd, int index);
        [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hwnd, uint command);
        [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
    }
}
'@
}
function Set-ClipboardTextRetry([string]$Text) {
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        try {
            [Windows.Forms.Clipboard]::SetText($Text)
            return
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    throw "could not acquire clipboard for: $Text"
}
function Send-ControlKey([byte]$Key) {
    [RustExplorerUitest.Native]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
}
function Set-FileClipboard([string]$Path) {
    $files = [Collections.Specialized.StringCollection]::new()
    [void]$files.Add($Path)
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        try {
            [Windows.Forms.Clipboard]::SetFileDropList($files)
            return
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    throw "could not acquire file clipboard for: $Path"
}
$context = $null
$externalProcess = $null
$externalHwnd = [IntPtr]::Zero
try {
    $isolatedLocalAppData = Join-Path $OutputDirectory 'localappdata'
    if ($RemotePath.StartsWith('sftp://', [StringComparison]::OrdinalIgnoreCase)) {
        $savedProfiles = Join-Path $env:LOCALAPPDATA 'RustGpuiExplorer\remote\sftp-profiles.json'
        if (-not (Test-Path -LiteralPath $savedProfiles -PathType Leaf)) {
            throw 'saved SFTP profile was not found'
        }
        $isolatedRemote = Join-Path $isolatedLocalAppData 'RustGpuiExplorer\remote'
        New-Item -ItemType Directory -Force -Path $isolatedRemote | Out-Null
        Copy-Item -LiteralPath $savedProfiles -Destination (Join-Path $isolatedRemote 'sftp-profiles.json') -Force
    }
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $OutputDirectory `
        -Executable $Executable -SkipBuild -UseCurrentProfile
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $address = $null
    for ($navigationAttempt = 0; $navigationAttempt -lt 3 -and $null -eq $address; $navigationAttempt++) {
        Set-UitestAddress -Context $context -Path $RemotePath
        try {
            $address = Find-UitestElement -Root $context.Root -Description 'remote address' -TimeoutSeconds 8 -Predicate {
                param($element)
                $element.Current.Name -eq "Address: $RemotePath"
            }
        } catch {
            if ($navigationAttempt -eq 2) { throw }
            [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
            Start-Sleep -Milliseconds 250
        }
    }
    Start-Sleep -Milliseconds 800
    $remoteItems = @(Get-UitestFileItems -Root $context.Root)
    if ($remoteItems.Count -gt 0) {
        Invoke-UitestClick -Element $remoteItems[0]
    } else {
        $windowBounds = $context.Root.Current.BoundingRectangle
        [void][RustExplorerUitest.Native]::SetCursorPosDpiAware(
            [int]($windowBounds.Left + $windowBounds.Width * 0.75),
            [int]($windowBounds.Top + $windowBounds.Height * 0.60))
        [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    Set-FileClipboard $payload
    Start-Sleep -Milliseconds 900
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Send-ControlKey 0x56
    $cancel = Find-UitestElement -Root $context.Root -Description 'transfer cancel button' -TimeoutSeconds 5 -Predicate {
        param($element)
        $element.Current.Name -eq 'Cancel file operation' -and $element.Current.BoundingRectangle.Width -gt 0
    }
    $speed = Find-UitestElement -Root $context.Root -Description 'non-zero transfer speed' -TimeoutSeconds 15 -Predicate {
        param($element)
        $element.Current.Name -match '[0-9]+(?:\.[0-9]+)?\s+(?:B|KB|MB|GB)/s'
    }
    $firstSpeedAt = [DateTime]::UtcNow
    $firstSpeedText = $speed.Current.Name
    Start-Sleep -Milliseconds 250
    $secondSpeed = Find-UitestElement -Root $context.Root -Description 'second transfer speed snapshot' -TimeoutSeconds 2 -Predicate {
        param($element)
        $element.Current.Name -match '[0-9]+(?:\.[0-9]+)?\s+(?:B|KB|MB|GB)/s'
    }
    $secondSpeedAt = [DateTime]::UtcNow
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'speed-active.png')
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) | Where-Object {
            $_.Current.Name -match 'Transfer' -or $_.Current.AutomationId -match 'transfer'
        } | ForEach-Object {
            [ordered]@{
                name = $_.Current.Name
                automation_id = $_.Current.AutomationId
                control_type = $_.Current.ControlType.ProgrammaticName
                left = $_.Current.BoundingRectangle.Left
                top = $_.Current.BoundingRectangle.Top
                width = $_.Current.BoundingRectangle.Width
                height = $_.Current.BoundingRectangle.Height
            }
        }) | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'transfer-elements.json')
    $windowBounds = $context.Root.Current.BoundingRectangle
    $transferButton = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) | Where-Object {
            $bounds = $_.Current.BoundingRectangle
            $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
                $bounds.Left -gt ($windowBounds.Left + $windowBounds.Width * 0.80) -and
                $bounds.Top -gt ($windowBounds.Top + 120) -and
                $bounds.Top -lt ($windowBounds.Top + 230)
        } | Sort-Object { $_.Current.BoundingRectangle.Left } -Descending | Select-Object -First 1)
    if ($transferButton.Count -ne 1) { throw 'transfer toolbar button was not found' }
    $transferButton[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 250
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'transfer-panel.png')
    $toolDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $candidateHandles = [Collections.Generic.List[IntPtr]]::new()
        $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
            param([IntPtr]$hwnd, [IntPtr]$unused)
            [uint32]$processId = 0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($hwnd, [ref]$processId)
            if ($processId -eq $context.Process.Id -and $hwnd -ne [IntPtr]$context.Hwnd -and
                [RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
                $candidateHandles.Add($hwnd)
            }
            return $true
        }
        [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
        $toolHwnd = @($candidateHandles | Select-Object -First 1)
        if ($toolHwnd.Count -eq 1) { break }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $toolDeadline)
    if ($toolHwnd.Count -ne 1) { throw 'native transfer tool window was not found' }
    $toolHwnd = [IntPtr]$toolHwnd[0]
    $toolRoot = [Windows.Automation.AutomationElement]::FromHandle($toolHwnd)
    $toolStyle = [int64][SuperExplorerTransferProbe.Native]::GetWindowLongPtr($toolHwnd, -20)
    $toolOwner = [SuperExplorerTransferProbe.Native]::GetWindow($toolHwnd, 4)
    $toolBounds = $toolRoot.Current.BoundingRectangle
    Save-UitestScreenshot -Root $toolRoot -Path (Join-Path $OutputDirectory 'transfer-tool-window.png')
    $activeAt = [DateTime]::UtcNow
    $cancelBounds = $cancel.Current.BoundingRectangle
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'active-transfer.png')
    $cancel.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
    $deadline = [DateTime]::UtcNow.AddSeconds(2)
    do {
        $remainingCancel = @($context.Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition) | Where-Object {
                $_.Current.Name -eq 'Cancel file operation' -and $_.Current.BoundingRectangle.Width -gt 0
            })
        if ($remainingCancel.Count -eq 0) { break }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($remainingCancel.Count -ne 0) { throw 'cancel did not reach a terminal UI state within two seconds' }
    $cancelLatency = ([DateTime]::UtcNow - $activeAt).TotalMilliseconds
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'cancelling.png')
    $externalProcess = Start-Process notepad.exe -PassThru
    Start-Sleep -Milliseconds 700
    $externalHwnd = [RustExplorerUitest.Native]::GetForegroundWindow()
    if ($externalHwnd -eq [IntPtr]::Zero -or $externalHwnd -eq [IntPtr]$context.Hwnd -or
        $externalHwnd -eq $toolHwnd) { throw 'external focus test window was not created' }
    $hideDeadline = [DateTime]::UtcNow.AddSeconds(2)
    do {
        $toolVisibleAfterExternalFocus = [RustExplorerUitest.Native]::IsWindowVisible($toolHwnd)
        if (-not $toolVisibleAfterExternalFocus) { break }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $hideDeadline)
    if ($toolVisibleAfterExternalFocus) { throw 'transfer tool window did not hide after application-group focus loss' }
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    [ordered]@{
        status = 'passed'
        remote_path = $RemotePath
        cancel_button_width = $cancelBounds.Width
        cancel_button_compact = $cancelBounds.Width -lt 250
        cancelling_latency_ms = [Math]::Round($cancelLatency, 1)
        cancelling_within_two_seconds = $cancelLatency -lt 2000
        first_speed = $firstSpeedText
        second_speed = $secondSpeed.Current.Name
        speed_snapshot_interval_ms = [Math]::Round(($secondSpeedAt - $firstSpeedAt).TotalMilliseconds, 1)
        terminal = 'cancel request reached terminal UI state'
        tool_window = [ordered]@{
            hwnd = [int64]$toolHwnd
            owner_hwnd = [int64]$toolOwner
            expected_owner_hwnd = [int64]$context.Hwnd
            is_owned = $toolOwner -eq [IntPtr]$context.Hwnd
            has_toolwindow_style = ($toolStyle -band 0x80) -ne 0
            has_appwindow_style = ($toolStyle -band 0x40000) -ne 0
            width = $toolBounds.Width
            height = $toolBounds.Height
            hides_on_external_focus = -not $toolVisibleAfterExternalFocus
        }
    } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
} finally {
    if ($externalHwnd -ne [IntPtr]::Zero) {
        [void][RustExplorerUitest.Native]::PostMessage($externalHwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    }
    if ($null -ne $externalProcess -and -not $externalProcess.HasExited) {
        $externalProcess.CloseMainWindow() | Out-Null
        if (-not $externalProcess.WaitForExit(2000)) { $externalProcess.Kill() }
    }
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}
