param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('inline-rename-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
$fixtureRoot = Join-Path $targetRoot ('inline-rename-fixture-' + [guid]::NewGuid().ToString('N'))
$sessionRoot = Join-Path $targetRoot ('inline-rename-session-' + [guid]::NewGuid().ToString('N'))
$destinationName = 'destination'
$targetName = 'rename-target'
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
New-Item -ItemType Directory -Path $sessionRoot | Out-Null
$destinationPath = Join-Path $fixtureRoot $destinationName
New-Item -ItemType Directory -Path $destinationPath | Out-Null
New-Item -ItemType Directory -Path (Join-Path $destinationPath $targetName) | Out-Null

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ('InlineRenameCapture.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace InlineRenameCapture {
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

function Find-Element(
    [Windows.Automation.AutomationElement]$Root,
    [Windows.Automation.ControlType]$Type,
    [string]$Name,
    [int]$TimeoutSeconds = 10
) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
            if ($element.Current.ControlType -eq $Type -and $element.Current.Name -like "*$Name*") { return $element }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $($Type.ProgrammaticName) $Name"
}

function Save-Capture([Windows.Automation.AutomationElement]$Root, [string]$Path) {
    $bounds = $Root.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::new([int]$bounds.Width, [int]$bounds.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen([int]$bounds.Left, [int]$bounds.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

$process = $null
try {
    $start = [Diagnostics.ProcessStartInfo]::new($executable)
    $start.WorkingDirectory = $workspaceRoot
    $start.UseShellExecute = $false
    # Isolate persisted tabs so EXPLORER_INITIAL_PATH cannot be shadowed by the
    # developer's real restored session.
    $start.Environment['LOCALAPPDATA'] = $sessionRoot
    $start.Environment['EXPLORER_INITIAL_PATH'] = $fixtureRoot
    $start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        if ($process.MainWindowHandle -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][InlineRenameCapture.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 20, 20, 1100, 760, 0x0040)
    [void][InlineRenameCapture.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 800
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $destination = Find-Element $root ([Windows.Automation.ControlType]::ListItem) $destinationName
    $destinationBounds = $destination.Current.BoundingRectangle
    [void][InlineRenameCapture.Native]::SetCursorPos([int]($destinationBounds.Left + 100), [int]($destinationBounds.Top + $destinationBounds.Height / 2))
    1..2 | ForEach-Object {
        [InlineRenameCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [InlineRenameCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
    $row = Find-Element $root ([Windows.Automation.ControlType]::ListItem) $targetName
    $rowBounds = $row.Current.BoundingRectangle
    # Explicitly clear any click-through selection below the only row. This exercises the
    # post-navigation state where no stable item owns focus and F2 must establish the first row.
    [void][InlineRenameCapture.Native]::SetCursorPos([int]($rowBounds.Left + 100), [int]($rowBounds.Bottom + 80))
    [InlineRenameCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [InlineRenameCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
    [void][InlineRenameCapture.Native]::SetForegroundWindow($process.MainWindowHandle)
    [InlineRenameCapture.Native]::keybd_event(0x71, 0, 0, [UIntPtr]::Zero)
    [InlineRenameCapture.Native]::keybd_event(0x71, 0, 2, [UIntPtr]::Zero)
    $editor = Find-Element $root ([Windows.Automation.ControlType]::Edit) 'Rename'
    Start-Sleep -Milliseconds 250
    $editorBounds = $editor.Current.BoundingRectangle
    if ($editorBounds.Height -ge $rowBounds.Height) {
        throw "inline editor is not compact: editor=$editorBounds row=$rowBounds"
    }
    $expectedHeight = $rowBounds.Height * 0.75
    if ([Math]::Abs($editorBounds.Height - $expectedHeight) -gt 2) {
        throw "inline editor height is not 24/32 of the row: editor=$editorBounds row=$rowBounds expected=$expectedHeight"
    }
    $editorCenter = $editorBounds.Top + $editorBounds.Height / 2
    $rowCenter = $rowBounds.Top + $rowBounds.Height / 2
    if ([Math]::Abs($editorCenter - $rowCenter) -gt 2) {
        throw "inline editor is not vertically centered: editorCenter=$editorCenter rowCenter=$rowCenter"
    }
    # Explorer keeps F2 rename active when the pointer is used to position the caret inside the
    # edit box. This physical click must be consumed by the editor rather than the backing row.
    [void][InlineRenameCapture.Native]::SetCursorPos(
        [int]($editorBounds.Left + $editorBounds.Width * 0.65),
        [int]$editorCenter)
    [InlineRenameCapture.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [InlineRenameCapture.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
    $editorAfterPointer = Find-Element $root ([Windows.Automation.ControlType]::Edit) 'Rename' 3
    if (-not $editorAfterPointer.Current.HasKeyboardFocus) {
        throw 'clicking inside the inline rename editor lost editor focus'
    }
    Save-Capture $root (Join-Path $OutputDirectory 'f2-rename.png')
    [ordered]@{
        schema_version=1
        captured_utc=[DateTime]::UtcNow.ToString('o')
        fixture=$fixtureRoot
        navigated_to=$destinationPath
        row=[ordered]@{ left=$rowBounds.Left; top=$rowBounds.Top; width=$rowBounds.Width; height=$rowBounds.Height; center_y=$rowCenter }
        editor=[ordered]@{ left=$editorBounds.Left; top=$editorBounds.Top; width=$editorBounds.Width; height=$editorBounds.Height; center_y=$editorCenter }
        expected_height=$expectedHeight
        center_delta=[Math]::Abs($editorCenter-$rowCenter)
        pointer_caret_click_preserved_editor=$true
        editor_retained_keyboard_focus=$true
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Inline rename capture passed: $OutputDirectory"
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        [void][InlineRenameCapture.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    if (Test-Path -LiteralPath $fixtureRoot) {
        $resolved = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $fixtureRoot).Path)
        $allowed = [IO.Path]::GetFullPath($targetRoot).TrimEnd('\') + '\inline-rename-fixture-'
        if (-not $resolved.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing unsafe fixture cleanup: $resolved"
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
    if (Test-Path -LiteralPath $sessionRoot) {
        $resolvedSession = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $sessionRoot).Path)
        $allowedSession = [IO.Path]::GetFullPath($targetRoot).TrimEnd('\') + '\inline-rename-session-'
        if (-not $resolvedSession.StartsWith($allowedSession, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing unsafe session cleanup: $resolvedSession"
        }
        Remove-Item -LiteralPath $resolvedSession -Recurse -Force
    }
}
