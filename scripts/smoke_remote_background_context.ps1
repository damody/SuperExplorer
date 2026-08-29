param(
    [string]$RemotePath = 'adb://emulator-5554/sdcard/Download',
    [string]$OutputDirectory,
    [switch]$UseCurrentProfile,
    [switch]$SeedCurrentRemoteProfile,
    [switch]$ActivateCreateFolder,
    [switch]$CleanupCreatedFolder,
    [switch]$InteractionMatrix,
    [switch]$CaptureFirstItem,
    [switch]$ItemOnly,
    [ValidateRange(0, 60000)][int]$NavigationSettleMilliseconds = 3000,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$target = Join-Path $workspace 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $target ('remote-background-context-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$session = Join-Path $target ('remote-background-context-session-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $OutputDirectory, $session | Out-Null
if ($SeedCurrentRemoteProfile) {
    $sourceProfile = Join-Path $env:LOCALAPPDATA 'RustGpuiExplorer\remote\sftp-profiles.json'
    if (-not (Test-Path -LiteralPath $sourceProfile)) { throw 'saved SFTP profile was not found' }
    $testRemoteDirectory = Join-Path $session 'RustGpuiExplorer\remote'
    New-Item -ItemType Directory -Force -Path $testRemoteDirectory | Out-Null
    Copy-Item -LiteralPath $sourceProfile -Destination (Join-Path $testRemoteDirectory 'sftp-profiles.json')
}

if (-not $SkipBuild) {
    cargo build -p explorer-app --locked
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('RemoteBackgroundContext.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace RemoteBackgroundContext {
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

function Find-NamedElement([Windows.Automation.AutomationElement]$Root, [string]$Name, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
            if ($element.Current.Name -eq $Name) { return $element }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $Name"
}

function Save-Window([Windows.Automation.AutomationElement]$Root, [string]$Path) {
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

function Get-NamedElementCount([Windows.Automation.AutomationElement]$Root, [string]$Name) {
    @($Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) |
        Where-Object { $_.Current.Name -eq $Name }).Count
}

function Wait-NamedElementGone(
    [Windows.Automation.AutomationElement]$Root,
    [string]$Name,
    [int]$TimeoutSeconds = 5
) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ((Get-NamedElementCount $Root $Name) -eq 0) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element remained visible: $Name"
}

function Send-PhysicalEscape([IntPtr]$Window) {
    [void][RemoteBackgroundContext.Native]::SetForegroundWindow($Window)
    [RemoteBackgroundContext.Native]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    [RemoteBackgroundContext.Native]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
}

$process = $null
try {
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $target 'debug\SuperExplorer.exe'))
    $start.WorkingDirectory = $workspace
    $start.UseShellExecute = $false
    if (-not $UseCurrentProfile) { $start.Environment['LOCALAPPDATA'] = $session }
    $start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        Start-Sleep -Milliseconds 100
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'application window did not appear' }

    [void][RemoteBackgroundContext.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 30, 30, 1200, 820, 0x0040)
    [void][RemoteBackgroundContext.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 500
    [void][RemoteBackgroundContext.Native]::SetCursorPos(600, 400)
    [RemoteBackgroundContext.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [RemoteBackgroundContext.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    [RemoteBackgroundContext.Native]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    [RemoteBackgroundContext.Native]::keybd_event(0x4C, 0, 0, [UIntPtr]::Zero)
    [RemoteBackgroundContext.Native]::keybd_event(0x4C, 0, 2, [UIntPtr]::Zero)
    [RemoteBackgroundContext.Native]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 200
    $clipboardSet = $false
    for ($attempt = 0; $attempt -lt 20 -and -not $clipboardSet; $attempt++) {
        try {
            [Windows.Forms.Clipboard]::SetText($RemotePath)
            $clipboardSet = $true
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    if (-not $clipboardSet) { throw 'could not stage the remote path on the clipboard' }
    [void][RemoteBackgroundContext.Native]::SetForegroundWindow($process.MainWindowHandle)
    [Windows.Forms.SendKeys]::SendWait('^a')
    [Windows.Forms.SendKeys]::SendWait('^v')
    [Windows.Forms.SendKeys]::SendWait('{ENTER}')

    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $address = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
            Where-Object { $_.Current.Name -eq "Address: $RemotePath" })
        if ($address.Count -gt 0) { break }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($address.Count -eq 0) {
        Save-Window $root (Join-Path $OutputDirectory 'remote-navigation-failed.png')
        throw "remote navigation did not complete: $RemotePath"
    }

    # Remote navigation updates the address before the provider finishes its first
    # viewport. Wait for that render so a completion update cannot dismiss the menu.
    Start-Sleep -Milliseconds $NavigationSettleMilliseconds
    $windowBounds = $root.Current.BoundingRectangle
    if ($ItemOnly) {
        $item = @($root.FindAll(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition) |
            Where-Object {
                $_.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
                $_.Current.Name -match ' (Folder|File)$' -and
                $_.Current.BoundingRectangle.Width -gt 0
            } |
            Select-Object -First 1)
        if ($item.Count -ne 1) { throw 'no visible remote item row was available for context-menu capture' }
        $itemBounds = $item[0].Current.BoundingRectangle
        [void][RemoteBackgroundContext.Native]::SetCursorPos(
            [int]($itemBounds.Left + [Math]::Min(80, $itemBounds.Width / 2)),
            [int]($itemBounds.Top + $itemBounds.Height / 2))
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $itemMenu = Find-NamedElement $root 'Remote file context menu' 5
        $itemScreenshot = Join-Path $OutputDirectory 'remote-item-context-menu.png'
        Save-Window $itemMenu $itemScreenshot
        $itemCommands = @($itemMenu.FindAll(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition) |
            Where-Object { $_.Current.ControlType -eq [Windows.Automation.ControlType]::MenuItem } |
            ForEach-Object { $_.Current.Name })
        [ordered]@{
            status = 'passed'
            path = $RemotePath
            item = $item[0].Current.Name
            item_commands = $itemCommands
            item_screenshot = $itemScreenshot
        } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
        Write-Output "Remote item context smoke passed: $OutputDirectory"
        return
    }
    $x = [int]($windowBounds.Left + $windowBounds.Width * 0.72)
    $y = [int]($windowBounds.Bottom - 90)
    [void][RemoteBackgroundContext.Native]::SetCursorPos($x, $y)
    [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)

    $menu = Find-NamedElement $root 'Remote file context menu' 5
    # Keep the source Windows PowerShell 5.1 compatible without depending on its
    # legacy no-BOM source encoding detection.
    $createFolderLabel = -join @([char]0x65B0, [char]0x589E, [char]0x8CC7, [char]0x6599, [char]0x593E)
    $pasteLabel = -join @([char]0x8CBC, [char]0x4E0A)
    $createFolder = Find-NamedElement $menu $createFolderLabel 2
    $menuName = $menu.Current.Name
    $menuBounds = $menu.Current.BoundingRectangle
    if ($menuBounds.Left -lt $windowBounds.Left -or $menuBounds.Top -lt $windowBounds.Top -or
        $menuBounds.Right -gt $windowBounds.Right -or $menuBounds.Bottom -gt $windowBounds.Bottom) {
        throw 'remote context menu was not clamped inside the application work area'
    }
    if ($createFolder.Current.ControlType -ne [Windows.Automation.ControlType]::MenuItem) {
        throw "remote command did not expose the MenuItem accessibility role: $($createFolder.Current.ControlType.ProgrammaticName)"
    }
    $screenshot = Join-Path $OutputDirectory 'remote-background-context-menu.png'
    Save-Window $menu $screenshot
    $paste = @($menu.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
        Where-Object { $_.Current.Name -eq $pasteLabel } | Select-Object -First 1)
    $pasteNames = @($paste | ForEach-Object { $_.Current.Name })
    $itemScreenshot = $null
    $itemCommands = @()
    if ($CaptureFirstItem) {
        Send-PhysicalEscape $process.MainWindowHandle
        Wait-NamedElementGone $root 'Remote file context menu'
        $item = @($root.FindAll(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition) |
            Where-Object {
                $_.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
                $_.Current.Name -match ' (Folder|File)$' -and
                $_.Current.BoundingRectangle.Width -gt 0
            } |
            Select-Object -First 1)
        if ($item.Count -ne 1) { throw 'no visible remote item row was available for context-menu capture' }
        $itemBounds = $item[0].Current.BoundingRectangle
        [void][RemoteBackgroundContext.Native]::SetCursorPos(
            [int]($itemBounds.Left + [Math]::Min(80, $itemBounds.Width / 2)),
            [int]($itemBounds.Top + $itemBounds.Height / 2))
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $itemMenu = Find-NamedElement $root 'Remote file context menu' 5
        $itemScreenshot = Join-Path $OutputDirectory 'remote-item-context-menu.png'
        Save-Window $itemMenu $itemScreenshot
        $itemCommands = @($itemMenu.FindAll(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition) |
            Where-Object { $_.Current.ControlType -eq [Windows.Automation.ControlType]::MenuItem } |
            ForEach-Object { $_.Current.Name })
        Send-PhysicalEscape $process.MainWindowHandle
        Wait-NamedElementGone $root 'Remote file context menu'

        [void][RemoteBackgroundContext.Native]::SetCursorPos($x, $y)
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $menu = Find-NamedElement $root 'Remote file context menu' 5
        $createFolder = Find-NamedElement $menu $createFolderLabel 2
    }
    $interactionEvidence = $null
    if ($InteractionMatrix) {
        $createBounds = $createFolder.Current.BoundingRectangle
        $rowX = [int]($createBounds.Left + $createBounds.Width / 2)
        $rowY = [int]($createBounds.Top + $createBounds.Height / 2)
        [void][RemoteBackgroundContext.Native]::SetCursorPos($rowX, $rowY)
        Start-Sleep -Milliseconds 200
        $hoverScreenshot = Join-Path $OutputDirectory 'remote-context-hover.png'
        Save-Window $menu $hoverScreenshot
        [RemoteBackgroundContext.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 150
        $pressedScreenshot = Join-Path $OutputDirectory 'remote-context-pressed.png'
        Save-Window $menu $pressedScreenshot
        [void][RemoteBackgroundContext.Native]::SetCursorPos($x - 120, $y - 120)
        [RemoteBackgroundContext.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Send-PhysicalEscape $process.MainWindowHandle
        Wait-NamedElementGone $root 'Remote file context menu'

        [void][RemoteBackgroundContext.Native]::SetCursorPos($x, $y)
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $menu = Find-NamedElement $root 'Remote file context menu' 5
        [void][RemoteBackgroundContext.Native]::SetCursorPos(
            [int]($windowBounds.Left + 12),
            [int]($windowBounds.Top + 90))
        [RemoteBackgroundContext.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Wait-NamedElementGone $root 'Remote file context menu'

        [void][RemoteBackgroundContext.Native]::SetCursorPos($x, $y)
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $menu = Find-NamedElement $root 'Remote file context menu' 5
        [void][RemoteBackgroundContext.Native]::SetCursorPos($x - 80, $y - 40)
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 250
        if ((Get-NamedElementCount $root 'Remote file context menu') -ne 1) {
            throw 'right-click replacement did not leave exactly one remote context menu'
        }
        Send-PhysicalEscape $process.MainWindowHandle
        Wait-NamedElementGone $root 'Remote file context menu'

        [void][RemoteBackgroundContext.Native]::SetCursorPos($x, $y)
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $menu = Find-NamedElement $root 'Remote file context menu' 5
        $createFolder = Find-NamedElement $menu $createFolderLabel 2
        $createBounds = $createFolder.Current.BoundingRectangle
        [void][RemoteBackgroundContext.Native]::SetCursorPos(
            [int]($createBounds.Left + $createBounds.Width / 2),
            [int]($createBounds.Top + $createBounds.Height / 2))
        [RemoteBackgroundContext.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            $rename = @($root.FindAll(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition) |
                Where-Object {
                    $_.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
                    $_.Current.Name -like 'Rename New folder*'
                })
            if ($rename.Count -eq 1) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if ($rename.Count -ne 1) { throw "single dispatch produced $($rename.Count) rename editors" }
        Send-PhysicalEscape $process.MainWindowHandle

        [void][RemoteBackgroundContext.Native]::SetCursorPos($x, $y)
        [RemoteBackgroundContext.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
        $menu = Find-NamedElement $root 'Remote file context menu' 5
        $createFolder = Find-NamedElement $menu $createFolderLabel 2
        $createFolder.SetFocus()
        Start-Sleep -Milliseconds 100
        if ([Windows.Automation.AutomationElement]::FocusedElement.Current.Name -ne $createFolderLabel) {
            throw 'remote command did not accept keyboard focus in UIA order'
        }
        [RemoteBackgroundContext.Native]::keybd_event(0x0D, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::keybd_event(0x0D, 0, 2, [UIntPtr]::Zero)
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            $keyboardRename = @($root.FindAll(
                [Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition) |
                Where-Object {
                    $_.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
                    $_.Current.Name -like 'Rename New folder*'
                })
            if ($keyboardRename.Count -eq 1) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if ($keyboardRename.Count -ne 1) { throw 'Enter did not activate the focused remote command once' }
        Send-PhysicalEscape $process.MainWindowHandle
        $interactionEvidence = [ordered]@{
            hover = $true
            pressed = $true
            escape_dismissal = $true
            outside_click_dismissal = $true
            right_click_replacement = $true
            single_dispatch = $true
            keyboard_focus_and_enter = $true
            accessible_menu_name = $menuName
            accessible_command_role = 'MenuItem'
            edge_clamped = $true
            hover_screenshot = $hoverScreenshot
            pressed_screenshot = $pressedScreenshot
        }
    }
    $renameName = $null
    if ($ActivateCreateFolder) {
        $createBounds = $createFolder.Current.BoundingRectangle
        [void][RemoteBackgroundContext.Native]::SetCursorPos(
            [int]($createBounds.Left + $createBounds.Width / 2),
            [int]($createBounds.Top + $createBounds.Height / 2))
        [RemoteBackgroundContext.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RemoteBackgroundContext.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        do {
            $rename = @($root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
                Where-Object { $_.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and $_.Current.Name -like 'Rename New folder*' } |
                Select-Object -First 1)
            if ($rename.Count -gt 0) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $deadline)
        if ($rename.Count -eq 0) { throw 'new folder did not enter inline rename' }
        $renameName = $rename[0].Current.Name.Substring('Rename '.Length)
        Save-Window $rename[0] $screenshot
        if ($CleanupCreatedFolder) {
            [Windows.Forms.SendKeys]::SendWait('{ENTER}')
            Start-Sleep -Milliseconds 1500
            $createdRow = Find-NamedElement $root "$renameName Folder" 20
            $createdRow.SetFocus()
            Start-Sleep -Milliseconds 300
            [Windows.Forms.SendKeys]::SendWait('+{DELETE}')
            $dialog = Find-NamedElement $root 'Permanently delete 1 item' 5
            [Windows.Forms.SendKeys]::SendWait('{ENTER}')
            Start-Sleep -Milliseconds 1000
        }
    } else {
        # The menu was captured before optional clipboard commands were inspected. Paste is
        # legitimately absent when the isolated test profile has no transferable clipboard.
    }
    [ordered]@{
        status = 'passed'
        path = $RemotePath
        click = [ordered]@{ x = $x; y = $y }
        menu = $menuName
        commands = @($createFolderLabel) + $pasteNames
        inline_rename = $renameName
        interactions = $interactionEvidence
        screenshot = $screenshot
        item_commands = $itemCommands
        item_screenshot = $itemScreenshot
    } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Remote background context smoke passed: $OutputDirectory"
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        [void][RemoteBackgroundContext.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    if (Test-Path -LiteralPath $session) {
        $resolved = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $session).Path)
        $allowed = [IO.Path]::GetFullPath($target).TrimEnd('\') + '\remote-background-context-session-'
        if (-not $resolved.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing unsafe session cleanup: $resolved"
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
