param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [ValidateSet('both','app-to-explorer','explorer-to-app','app-internal')]
    [string]$Direction = 'both',
    [ValidateSet('all','move','copy','cancel')]
    [string]$ExplorerScenario = 'all',
    [string]$OutputDirectory = 'target\explorer-interop-evidence\actual-drag',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = Join-Path $workspaceRoot $OutputDirectory
}
# Shell.Application.Explore silently ignores otherwise valid absolute paths that
# still contain a `.` segment. Canonicalize both relative and absolute input so
# Explorer and SuperExplorer are always launched against the same directory.
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) { throw "artifact finalization failed: $LASTEXITCODE" }
}

if (-not ('ExplorerDragInterop.Native' -as [type])) {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    Add-Type -AssemblyName System.Drawing
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerDragInterop {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
        [StructLayout(LayoutKind.Sequential)] public struct Point { public int x, y; }
        [StructLayout(LayoutKind.Sequential)] public struct MouseInput { public int dx, dy; public uint data, flags, time; public UIntPtr extra; }
        [StructLayout(LayoutKind.Sequential)] public struct Input { public uint type; public MouseInput mouse; }
        [DllImport("user32.dll", SetLastError=true)] public static extern uint SendInput(uint count, Input[] inputs, int size);
        [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern short VkKeyScan(char ch);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int key);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool PostThreadMessage(uint thread, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
        [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, IntPtr process);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool AttachThreadInput(uint source, uint target, bool attach);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool BringWindowToTop(IntPtr window);
        [DllImport("user32.dll")] public static extern IntPtr SetFocus(IntPtr window);
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool ShowWindow(IntPtr window, int command);
        [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr LoadKeyboardLayout(string id, uint flags);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll", SetLastError=true)] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
        [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
        [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
        [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window, uint flags);
        [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
        public static void Key(ushort vk, bool down) {
            keybd_event((byte)vk, 0, down ? 0u : 2u, UIntPtr.Zero);
        }
        public static void Text(string value) {
            foreach (char ch in value) {
                short mapped = VkKeyScan(ch);
                if (mapped == -1) throw new InvalidOperationException("character cannot be typed by current layout: " + ch);
                byte vk = (byte)(mapped & 0xff); byte modifiers = (byte)((mapped >> 8) & 0xff);
                if ((modifiers & 1) != 0) Key(0x10, true);
                if ((modifiers & 2) != 0) Key(0x11, true);
                if ((modifiers & 4) != 0) Key(0x12, true);
                Key(vk, true); Key(vk, false);
                if ((modifiers & 4) != 0) Key(0x12, false);
                if ((modifiers & 2) != 0) Key(0x11, false);
                if ((modifiers & 1) != 0) Key(0x10, false);
            }
        }
        public static void Mouse(uint flags) {
            mouse_event(flags, 0, 0, 0, UIntPtr.Zero);
        }
        public static void Move(int dx, int dy) {
            var input = new Input { type=0, mouse=new MouseInput { dx=dx, dy=dy, flags=1u } };
            if (SendInput(1, new[]{input}, Marshal.SizeOf<Input>()) != 1)
                throw new InvalidOperationException("SendInput move failed: " + Marshal.GetLastWin32Error());
        }
        public static void MoveAbsolute(int x, int y) {
            int left=GetSystemMetrics(76), top=GetSystemMetrics(77);
            int width=GetSystemMetrics(78), height=GetSystemMetrics(79);
            int nx=(int)Math.Round((x-left)*65535.0/Math.Max(1,width-1));
            int ny=(int)Math.Round((y-top)*65535.0/Math.Max(1,height-1));
            var input = new Input { type=0, mouse=new MouseInput { dx=nx, dy=ny, flags=0xC001u } };
            if (SendInput(1, new[]{input}, Marshal.SizeOf<Input>()) != 1)
                throw new InvalidOperationException("SendInput absolute move failed: " + Marshal.GetLastWin32Error());
        }
    }
}
'@
}

function Send-Chord([uint16]$Modifier, [uint16]$Key) {
    [ExplorerDragInterop.Native]::Key($Modifier, $true); [ExplorerDragInterop.Native]::Key($Key, $true)
    [ExplorerDragInterop.Native]::Key($Key, $false); [ExplorerDragInterop.Native]::Key($Modifier, $false)
}
function Focus-Window([IntPtr]$Window) {
    $current = [ExplorerDragInterop.Native]::GetCurrentThreadId()
    $target = [ExplorerDragInterop.Native]::GetWindowThreadProcessId($Window,[IntPtr]::Zero)
    $foreground = [ExplorerDragInterop.Native]::GetForegroundWindow()
    $foregroundThread = if ($foreground -eq [IntPtr]::Zero) { 0 } else { [ExplorerDragInterop.Native]::GetWindowThreadProcessId($foreground,[IntPtr]::Zero) }
    $attachedTarget = $target -ne $current -and [ExplorerDragInterop.Native]::AttachThreadInput($current,$target,$true)
    $attachedForeground = $foregroundThread -ne 0 -and $foregroundThread -ne $current -and $foregroundThread -ne $target -and [ExplorerDragInterop.Native]::AttachThreadInput($current,$foregroundThread,$true)
    try {
        # A synthetic Alt transition grants the calling input queue the same foreground
        # activation opportunity as a real keyboard gesture before SetForegroundWindow.
        [ExplorerDragInterop.Native]::Key(0x12, $true)
        [ExplorerDragInterop.Native]::Key(0x12, $false)
        [void][ExplorerDragInterop.Native]::ShowWindow($Window, 9)
        [void][ExplorerDragInterop.Native]::BringWindowToTop($Window)
        [void][ExplorerDragInterop.Native]::SetForegroundWindow($Window)
        [void][ExplorerDragInterop.Native]::SetFocus($Window)
    } finally {
        if ($attachedForeground) { [void][ExplorerDragInterop.Native]::AttachThreadInput($current,$foregroundThread,$false) }
        if ($attachedTarget) { [void][ExplorerDragInterop.Native]::AttachThreadInput($current,$target,$false) }
    }
    Start-Sleep -Milliseconds 200
    if ([ExplorerDragInterop.Native]::GetForegroundWindow() -ne $Window) { throw "foreground acquisition failed: $Window" }
}
function Find-ElementByNames([IntPtr]$Window, [string[]]$Names, [int]$TimeoutSeconds = 15) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $root = [Windows.Automation.AutomationElement]::FromHandle($Window)
            # A property-scoped FindFirst activates AccessKit without synchronously materializing
            # the entire RawView tree on the GPUI main thread.
            foreach ($name in $Names) {
                $condition = [Windows.Automation.PropertyCondition]::new(
                    [Windows.Automation.AutomationElement]::NameProperty, $name)
                $element = $root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
                if ($null -ne $element) { return $element }
            }
        } catch {}
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UI Automation element not found: $($Names -join ' or ')"
}
function Find-FileElement([IntPtr]$Window, [string]$FileName, [int]$TimeoutSeconds = 15) {
    # Explorer's current "hide known extensions" setting also affects Shell display names used by
    # the app, so UIA may expose either "name.ext" or "name" for the same real item.
    $stem = [IO.Path]::GetFileNameWithoutExtension($FileName)
    return Find-ElementByNames $Window @($FileName, $stem) $TimeoutSeconds
}
function Get-AppFileBounds([IntPtr]$Window, [int]$RowIndex) {
    $rect = [ExplorerDragInterop.Native+Rect]::new()
    if (-not [ExplorerDragInterop.Native]::GetWindowRect($Window, [ref]$rect)) {
        throw 'GetWindowRect failed for app drag source'
    }
    $scale = [ExplorerDragInterop.Native]::GetDpiForWindow($Window) / 96.0
    # These are production UiTokens logical bounds, also asserted by render_structure and
    # diagnostics: the file view begins at x=301.142857, y=164 and Details rows are 32 high.
    [pscustomobject]@{
        Left = $rect.left + 301.142857 * $scale
        Top = $rect.top + (164 + 32 * $RowIndex) * $scale
        Width = [Math]::Max(160, ($rect.right - $rect.left) / $scale - 301.142857) * $scale
        Height = 32 * $scale
    }
}
function Navigate-App([IntPtr]$Window, [string]$Path, [string]$ExpectedFile) {
    Focus-Window $Window
    $english = [ExplorerDragInterop.Native]::LoadKeyboardLayout('00000409',1)
    [void][ExplorerDragInterop.Native]::PostMessage($Window,0x0050,[IntPtr]::Zero,$english)
    Start-Sleep -Milliseconds 150
    Send-Chord 0x11 0x4c; Start-Sleep -Milliseconds 100
    Send-Chord 0x11 0x41
    [ExplorerDragInterop.Native]::Text($Path)
    [ExplorerDragInterop.Native]::Key(0x0d, $true); [ExplorerDragInterop.Native]::Key(0x0d, $false)
    if ($ExpectedFile) { [void](Find-FileElement $Window $ExpectedFile 20) }
    else { Start-Sleep -Seconds 1 }
}
function Drag-Bounds($Bounds, [int]$TargetX, [int]$TargetY, [ValidateSet('left','right')][string]$Button, [switch]$Copy, [switch]$Move, [switch]$Cancel) {
    $startX = [int][Math]::Round($Bounds.Left + [Math]::Min($Bounds.Width * 0.35, 80))
    $startY = [int][Math]::Round($Bounds.Top + $Bounds.Height / 2)
    [void][ExplorerDragInterop.Native]::SetWindowPos(
        $script:sourceHwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0003)
    [void][ExplorerDragInterop.Native]::BringWindowToTop($script:sourceHwnd)
    [void][ExplorerDragInterop.Native]::SetCursorPos($startX, $startY)
    $sourcePoint = [ExplorerDragInterop.Native+Point]::new()
    $sourcePoint.x = $startX; $sourcePoint.y = $startY
    $sourceHit = [ExplorerDragInterop.Native]::WindowFromPoint($sourcePoint)
    $sourceRoot = [ExplorerDragInterop.Native]::GetAncestor($sourceHit, 2)
    if ($sourceRoot -ne $script:sourceHwnd) {
        throw "drag source is covered: expected $([int64]$script:sourceHwnd), got $([int64]$sourceRoot) at $startX,$startY"
    }
    Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "ready button=$Button source=$startX,$startY source_root=$([int64]$sourceRoot)"
    $down = if ($Button -eq 'left') { 0x0002 } else { 0x0008 }
    $up = if ($Button -eq 'left') { 0x0004 } else { 0x0010 }
    [ExplorerDragInterop.Native]::Mouse($down)
    Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "mouse-down button=$Button"
    # Apply the transfer modifier after the source row has received mouse-down, but before
    # crossing the drag threshold. This preserves Explorer selection semantics while ensuring
    # the destination observes Ctrl/Shift on its very first DragEnter negotiation.
    if ($Copy) { [ExplorerDragInterop.Native]::Key(0x11, $true) }
    if ($Move) { [ExplorerDragInterop.Native]::Key(0x10, $true) }
    if ($script:overlayHwnd -and $script:overlayHwnd -ne [IntPtr]::Zero) {
        # Re-raise the destination after mouse-down but before crossing the drag threshold.
        # Once the threshold is crossed the source is inside modal DoDragDrop, and a z-order
        # request involving its input queue can synchronously wait on that modal call.
        [void][ExplorerDragInterop.Native]::SetWindowPos(
            $script:overlayHwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0013)
    }
    Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "destination-raised hwnd=$([int64]$script:overlayHwnd)"
    # SendInput waits for the receiver to finish processing. Crossing GPUI's threshold enters
    # modal DoDragDrop on that receiver, so a synchronous SendInput move deadlocks the driver.
    # SetCursorPos queues the same real pointer transition without waiting for DoDragDrop to end.
    [void][ExplorerDragInterop.Native]::SetCursorPos($startX + 24, $startY)
    Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "threshold-crossed cursor=$($startX+24),$startY"
    Start-Sleep -Milliseconds 750
    if ($script:overlayHwnd -and $script:overlayHwnd -ne [IntPtr]::Zero) {
        [void][ExplorerDragInterop.Native]::SetWindowPos(
            $script:overlayHwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0013)
    }
    Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "start button=$Button source=$startX,$startY source_hit=$([int64]$sourceHit) source_root=$([int64]$sourceRoot) expected_root=$([int64]$script:sourceHwnd) target=$TargetX,$TargetY"
    for ($step=1; $step -le 24; $step++) {
        $x = [int][Math]::Round($startX + ($TargetX-$startX)*$step/24)
        $y = [int][Math]::Round($startY + ($TargetY-$startY)*$step/24)
        # UIA returns physical screen coordinates. SetCursorPos consumes that same POINT
        # contract, while SendInput absolute normalization can be DPI-virtualized by the
        # PowerShell host and wrap a 3195px target back to 657px on a 175% desktop.
        if (-not [ExplorerDragInterop.Native]::SetCursorPos($x,$y)) {
            throw "SetCursorPos failed during drag at $x,$y"
        }
        Start-Sleep -Milliseconds 25
    }
    if ($script:dragIndex -eq 0) {
        $screen = [Windows.Forms.SystemInformation]::VirtualScreen
        $bitmap = [Drawing.Bitmap]::new($screen.Width,$screen.Height)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try { $graphics.CopyFromScreen($screen.Left,$screen.Top,0,0,$bitmap.Size) }
            finally { $graphics.Dispose() }
            $bitmap.Save((Join-Path $OutputDirectory 'desktop-before-first-drop.png'),[Drawing.Imaging.ImageFormat]::Png)
        } finally { $bitmap.Dispose() }
    }
    if ($Cancel) {
        [ExplorerDragInterop.Native]::Key(0x1b,$true); [ExplorerDragInterop.Native]::Key(0x1b,$false)
        Start-Sleep -Milliseconds 150
    }
    if ($Cancel) {
        # A synthetic driver cannot reliably interact with Explorer's native right-drop menu
        # (and Escape delivery can race a modal DoDragDrop loop) across DPI/input queues.
        # Leaving the target before release exercises the same OLE cancellation contract for
        # either button without accidentally committing a drop or selecting a menu default.
        [void][ExplorerDragInterop.Native]::SetCursorPos($startX, $startY)
        Start-Sleep -Milliseconds 100
    }
    $targetPoint = [ExplorerDragInterop.Native+Point]::new()
    $targetPoint.x = $TargetX; $targetPoint.y = $TargetY
    $targetHit = [ExplorerDragInterop.Native]::WindowFromPoint($targetPoint)
    $targetRoot = [ExplorerDragInterop.Native]::GetAncestor($targetHit, 2)
    Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "before-up target_hit=$([int64]$targetHit) target_root=$([int64]$targetRoot) expected_target=$([int64]$script:overlayHwnd)"
    if (-not $Cancel -and $targetRoot -ne $script:overlayHwnd) {
        throw "drop destination is covered: expected $([int64]$script:overlayHwnd), got $([int64]$targetRoot)"
    }
    [ExplorerDragInterop.Native]::Mouse($up)
    if ($Cancel -and $Button -eq 'right') {
        # Explorer opens the right-drag choice menu only after button-up.
        foreach ($attempt in 1..6) {
            Start-Sleep -Milliseconds 25
            [ExplorerDragInterop.Native]::Key(0x1b,$true); [ExplorerDragInterop.Native]::Key(0x1b,$false)
        }
    }
    $buttonKey = if ($Button -eq 'left') { 1 } else { 2 }
    $stateAfterUp = [ExplorerDragInterop.Native]::GetAsyncKeyState($buttonKey)
    $cursorAfterUp = [Windows.Forms.Cursor]::Position
    Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "after-up button=$Button state=$stateAfterUp cursor=$($cursorAfterUp.X),$($cursorAfterUp.Y) target=$TargetX,$TargetY"
    [ExplorerDragInterop.Native]::Move(1,0)
    $wakeProcess = Get-Process -Id $script:wakeProcessId
    $wakeProcess.Refresh()
    $threadUp = if ($Button -eq 'left') { 0x0202 } else { 0x0205 }
    Start-Sleep -Milliseconds 500
    if (([ExplorerDragInterop.Native]::GetAsyncKeyState($buttonKey) -band 0x8000) -ne 0) {
        [ExplorerDragInterop.Native]::Mouse($up)
        $stateAfterRetry = [ExplorerDragInterop.Native]::GetAsyncKeyState($buttonKey)
        Add-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'input-state.log') -Value "after-retry button=$Button state=$stateAfterRetry"
        [void][ExplorerDragInterop.Native]::SetCursorPos($TargetX+2,$TargetY)
        foreach ($thread in $wakeProcess.Threads) {
            $upPosted = [ExplorerDragInterop.Native]::PostThreadMessage([uint32]$thread.Id,$threadUp,[IntPtr]::Zero,[IntPtr]::Zero)
            $movePosted = [ExplorerDragInterop.Native]::PostThreadMessage([uint32]$thread.Id,0x0200,[IntPtr]::Zero,[IntPtr]::Zero)
            $script:wakeResults += [ordered]@{ thread=[uint32]$thread.Id; up=$upPosted; move=$movePosted }
        }
        Start-Sleep -Milliseconds 250
    }
    if ($Copy) { [ExplorerDragInterop.Native]::Key(0x11, $false) }
    if ($Move) { [ExplorerDragInterop.Native]::Key(0x10, $false) }
    $script:dragIndex++
}
function Wait-Path([string]$Path, [bool]$ShouldExist, [int]$Seconds = 15) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if ((Test-Path -LiteralPath $Path) -eq $ShouldExist) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "path oracle mismatch: $Path expected exists=$ShouldExist"
}

$fixture = Join-Path $OutputDirectory 'fixture'
$appSource = Join-Path $fixture 'app-source'
$explorerTarget = Join-Path $fixture 'explorer-target'
$explorerSource = Join-Path $fixture 'explorer-source'
$appTarget = Join-Path $fixture 'app-target'
$appInternal = Join-Path $fixture 'app-internal'
$appInternalTarget = Join-Path $appInternal 'destination'
foreach ($directory in @($appSource,$explorerTarget,$explorerSource,$appTarget,$appInternal,$appInternalTarget)) { New-Item -ItemType Directory -Force -Path $directory | Out-Null }
$files = [ordered]@{
    app_copy='app-left-copy.txt'; app_move='app-left-move.txt'; app_cancel='app-left-none.txt'; app_right='app-right-none.txt'
    explorer_copy='explorer-left-copy.txt'; explorer_move='explorer-left-move.txt'; explorer_cancel='explorer-left-none.txt'; explorer_right='explorer-right-none.txt'
}
foreach ($name in @($files.app_copy,$files.app_move,$files.app_cancel,$files.app_right)) { Set-Content -Encoding utf8 -LiteralPath (Join-Path $appSource $name) -Value $name }
foreach ($name in @($files.explorer_copy,$files.explorer_move,$files.explorer_cancel,$files.explorer_right)) { Set-Content -Encoding utf8 -LiteralPath (Join-Path $explorerSource $name) -Value $name }
$internalFiles = [ordered]@{
    default_move='internal-default-move.txt'
    shift_move='internal-shift-move.txt'
    ctrl_copy='internal-ctrl-copy.txt'
    cancel='internal-cancel.txt'
}
foreach ($name in $internalFiles.Values) { Set-Content -Encoding utf8 -LiteralPath (Join-Path $appInternal $name) -Value $name }

$startInfo = [Diagnostics.ProcessStartInfo]::new((Join-Path $targetRoot "$Profile\SuperExplorer.exe"))
$startInfo.WorkingDirectory = $workspaceRoot; $startInfo.UseShellExecute = $false
# Do not redirect without continuously draining the pipes. The headful drag matrix emits
# enough diagnostics to fill a pipe and freeze the application inside modal DoDragDrop.
$startInfo.RedirectStandardOutput = $false; $startInfo.RedirectStandardError = $false
$startInfo.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$startInfo.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata-source')
$startInfo.Environment['EXPLORER_INITIAL_PATH'] = if ($Direction -eq 'app-internal') { $appInternal } else { $appSource }
$app = [Diagnostics.Process]::Start($startInfo)
$script:wakeResults = @()
$script:dragIndex = 0
$matrixPassed = [Collections.Generic.List[string]]::new()
$script:wakeProcessId = $app.Id
$shell = New-Object -ComObject Shell.Application
$explorerWindows = @()
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do { $app.Refresh(); $appHwnd=$app.MainWindowHandle; if ($appHwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 50 } } while ($appHwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($appHwnd -eq [IntPtr]::Zero) { throw 'application HWND timeout' }
    [void][ExplorerDragInterop.Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    # Keep both drag endpoints inside the current work area without overlap. A 52/48 split
    # preserves the app's Explorer-sized minimum width at 175% DPI while preventing either
    # topmost source window from covering the destination during OLE's nested input loop.
    $workArea = [Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    $paneWidth = [Math]::Max(800, [int][Math]::Floor($workArea.Width * 0.48))
    $paneHeight = [Math]::Max(480, $workArea.Height)
    $leftX = $workArea.Left
    $rightX = $workArea.Right - $paneWidth
    $appWidth = $rightX - $leftX
    [void][ExplorerDragInterop.Native]::SetWindowPos($appHwnd,[IntPtr](-1),$leftX,$workArea.Top,$appWidth,$paneHeight,0x0040)

    if ($Direction -eq 'app-internal') {
        $script:overlayHwnd = $appHwnd
        $script:sourceHwnd = $appHwnd
        $script:wakeProcessId = $app.Id
        Focus-Window $appHwnd
        # The destination folder is the first Details row because Explorer-style sorting keeps
        # containers before files. Use the production row geometry instead of an exact UIA name;
        # AccessKit can expose a localized "destination Folder" label for container rows.
        $destinationBounds = Get-AppFileBounds $appHwnd 0
        $destinationX = [int][Math]::Round($destinationBounds.Left + [Math]::Min($destinationBounds.Width * 0.35, 80))
        $destinationY = [int][Math]::Round($destinationBounds.Top + $destinationBounds.Height / 2)
        foreach ($scenario in @(
            @{ name='app-internal-default-move'; file=$internalFiles.default_move; copy=$false; move=$false; cancel=$false; target=$true; source=$false },
            @{ name='app-internal-shift-move'; file=$internalFiles.shift_move; copy=$false; move=$true; cancel=$false; target=$true; source=$false },
            @{ name='app-internal-ctrl-copy'; file=$internalFiles.ctrl_copy; copy=$true; move=$false; cancel=$false; target=$true; source=$true },
            @{ name='app-internal-cancel'; file=$internalFiles.cancel; copy=$false; move=$false; cancel=$true; target=$false; source=$true }
        )) {
            $currentOrder = @(Get-ChildItem -LiteralPath $appInternal | Sort-Object @{ Expression = { -not $_.PSIsContainer } }, Name | ForEach-Object Name)
            $rowIndex = [Array]::IndexOf($currentOrder, $scenario.file)
            if ($rowIndex -lt 0) { throw "internal app drag source row not found: $($scenario.file)" }
            $sourceBounds = Get-AppFileBounds $appHwnd $rowIndex
            Drag-Bounds $sourceBounds $destinationX $destinationY 'left' -Copy:$scenario.copy -Move:$scenario.move -Cancel:$scenario.cancel
            Wait-Path (Join-Path $appInternalTarget $scenario.file) $scenario.target
            Wait-Path (Join-Path $appInternal $scenario.file) $scenario.source
            Start-Sleep -Milliseconds 1000
            $matrixPassed.Add($scenario.name)
        }
        [ordered]@{
            schema_version=1; captured_utc=[DateTime]::UtcNow.ToString('o'); fixture=$fixture
            driver='real foreground left mouse through production OLE source and same-window GPUI OLE folder target'
            matrix=$matrixPassed.ToArray(); passed=$matrixPassed.Count; disk_oracle='source/target existence asserted after every internal drag'
        } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
        Write-Output "Internal app drag passed: $OutputDirectory"
        return
    }

    foreach ($path in @($explorerTarget)) {
        # Shell.Application.Explore silently ignores valid paths containing a `.` segment, such
        # as the runner's `D:\test\.\target\...` evidence directory. Canonicalize before both
        # navigation and LocationURL comparison so tabbed Explorer and separate windows agree.
        $canonicalPath = [IO.Path]::GetFullPath($path)
        $url = ([Uri]$canonicalPath).AbsoluteUri
        $shell.Explore($canonicalPath)
        $wait = [DateTime]::UtcNow.AddSeconds(15); $window=$null
        do {
            Start-Sleep -Milliseconds 200
            $window = @($shell.Windows()) | Where-Object LocationURL -eq $url | Select-Object -First 1
        } while ($null -eq $window -and [DateTime]::UtcNow -lt $wait)
        if ($null -eq $window) { throw "Explorer window timeout: $canonicalPath ($url)" }
        $explorerWindows += $window
    }
    $targetWindow = $explorerWindows[0]
    $targetHwnd = [IntPtr]([int64]$targetWindow.HWND)
    [void][ExplorerDragInterop.Native]::SetWindowPos($targetHwnd,[IntPtr](-1),$rightX,$workArea.Top,$paneWidth,$paneHeight,0x0040)
    $script:overlayHwnd = $targetHwnd
    $script:sourceHwnd = $appHwnd
    Start-Sleep -Seconds 1

    $targetRootElement = [Windows.Automation.AutomationElement]::FromHandle($targetHwnd)
    $targetBounds = $targetRootElement.Current.BoundingRectangle
    $explorerDropX = [int]($targetBounds.Left + $targetBounds.Width*0.65)
    $explorerDropY = [int]($targetBounds.Top + $targetBounds.Height*0.65)

    $probeItem = Get-AppFileBounds $appHwnd 0
    [ordered]@{
        app_hwnd=[int64]$appHwnd; explorer_hwnd=[int64]$targetHwnd
        app_item_bounds=$probeItem.ToString()
        explorer_bounds=$targetBounds.ToString(); drop_x=$explorerDropX; drop_y=$explorerDropY
        explorer_location=$targetWindow.LocationURL
    } | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'window-layout.json')
    if ($Direction -ne 'explorer-to-app') { foreach ($scenario in @(
        # Same-volume Explorer drops default to move. Avoid synthesizing Shift after GPUI has
        # entered its modal OLE loop; that modifier can race target entry on high-DPI desktops.
        @{ name='app-to-explorer-left-move'; file=$files.app_move; copy=$false; cancel=$false; default_move=$true; button='left'; exists=$true; source=$false },
        @{ name='app-to-explorer-left-copy'; file=$files.app_copy; copy=$true; cancel=$false; button='left'; exists=$true; source=$true },
        @{ name='app-to-explorer-left-none'; file=$files.app_cancel; copy=$false; cancel=$true; button='left'; exists=$false; source=$true },
        @{ name='app-to-explorer-right-none'; file=$files.app_right; copy=$false; cancel=$true; button='right'; exists=$false; source=$true }
    )) {
        # Mutating scenarios remove rows and change every later visual index. Rebuild the
        # same name-sorted presentation order from the real fixture before every gesture.
        $currentOrder = @(Get-ChildItem -LiteralPath $appSource -File | Sort-Object Name | ForEach-Object Name)
        $rowIndex = [Array]::IndexOf($currentOrder, $scenario.file)
        if ($rowIndex -lt 0) { throw "app drag source row not found: $($scenario.file)" }
        $itemBounds = Get-AppFileBounds $appHwnd $rowIndex
        Drag-Bounds $itemBounds $explorerDropX $explorerDropY $scenario.button -Copy:$scenario.copy -Move:(-not $scenario.copy -and -not $scenario.cancel -and -not $scenario.default_move) -Cancel:$scenario.cancel
        Wait-Path (Join-Path $explorerTarget $scenario.file) $scenario.exists
        Wait-Path (Join-Path $appSource $scenario.file) $scenario.source
        # The file-system path can become visible before Explorer has left its nested drop
        # completion loop. Do not start the next real OLE session against that busy target.
        Start-Sleep -Milliseconds 1000
        $matrixPassed.Add("app->Explorer $($scenario.button) $(if ($scenario.cancel) {'none'} elseif ($scenario.copy) {'copy'} else {'move'})")
    } }

    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force; [void]$app.WaitForExit(5000) }
    $app.Dispose()
    $startInfo.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata-target')
    $startInfo.Environment['EXPLORER_INITIAL_PATH'] = $appTarget
    $app = [Diagnostics.Process]::Start($startInfo)
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do { $app.Refresh(); $appHwnd=$app.MainWindowHandle; if ($appHwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 50 } } while ($appHwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($appHwnd -eq [IntPtr]::Zero) { throw 'target application HWND timeout' }
    [void][ExplorerDragInterop.Native]::SetWindowPos($appHwnd,[IntPtr](-1),$leftX,$workArea.Top,$appWidth,$paneHeight,0x0040)
    $targetWindow.Navigate2($explorerSource)
    $sourceDeadline = [DateTime]::UtcNow.AddSeconds(15)
    do { Start-Sleep -Milliseconds 200 } while ($targetWindow.LocationURL -ne ([Uri]$explorerSource).AbsoluteUri -and [DateTime]::UtcNow -lt $sourceDeadline)
    if ($targetWindow.LocationURL -ne ([Uri]$explorerSource).AbsoluteUri) { throw 'Explorer source navigation timeout' }
    $sourceHwnd = $targetHwnd
    $sourcePid = [uint32]0
    [void][ExplorerDragInterop.Native]::GetWindowThreadProcessId($sourceHwnd,[ref]$sourcePid)
    $script:wakeProcessId = [int]$sourcePid
    [void][ExplorerDragInterop.Native]::SetWindowPos($sourceHwnd,[IntPtr](-1),$rightX,$workArea.Top,$paneWidth,$paneHeight,0x0040)
    $script:overlayHwnd = $appHwnd
    $script:sourceHwnd = $sourceHwnd
    Start-Sleep -Seconds 1
    $appRoot = [Windows.Automation.AutomationElement]::FromHandle($appHwnd)
    $appBounds = $appRoot.Current.BoundingRectangle
    # Use the center of the non-overlapping app target surface.
    $appDropX = [int]($appBounds.Left + $appBounds.Width*0.28)
    $appDropY = [int]($appBounds.Top + $appBounds.Height*0.68)
    $explorerScenarios = @(
        @{ name='explorer-to-app-left-move'; file=$files.explorer_move; copy=$false; cancel=$false; button='left'; exists=$true; source=$false },
        @{ name='explorer-to-app-left-copy'; file=$files.explorer_copy; copy=$true; cancel=$false; button='left'; exists=$true; source=$true },
        @{ name='explorer-to-app-left-none'; file=$files.explorer_cancel; copy=$false; cancel=$true; button='left'; exists=$false; source=$true },
        @{ name='explorer-to-app-right-none'; file=$files.explorer_right; copy=$false; cancel=$true; button='right'; exists=$false; source=$true }
    )
    if ($ExplorerScenario -ne 'all') {
        $explorerScenarios = @($explorerScenarios | Where-Object {
            ($ExplorerScenario -eq 'move' -and -not $_.copy -and -not $_.cancel) -or
            ($ExplorerScenario -eq 'copy' -and $_.copy) -or
            ($ExplorerScenario -eq 'cancel' -and $_.cancel)
        })
    }
    if ($Direction -ne 'app-to-explorer') { foreach ($scenario in $explorerScenarios) {
        Focus-Window $sourceHwnd
        $item = Find-FileElement $sourceHwnd $scenario.file 15
        # Explorer may carry a multi-selection across the folder navigation. OLE exports the
        # entire selected set when the pointer starts on any selected row, so explicitly make
        # the intended fixture the sole selection before each matrix gesture.
        $selection = $item.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern)
        $selection.Select()
        Start-Sleep -Milliseconds 150
        Drag-Bounds $item.Current.BoundingRectangle $appDropX $appDropY $scenario.button -Copy:$scenario.copy -Move:(-not $scenario.copy -and -not $scenario.cancel) -Cancel:$scenario.cancel
        Wait-Path (Join-Path $appTarget $scenario.file) $scenario.exists
        Wait-Path (Join-Path $explorerSource $scenario.file) $scenario.source
        # Explorer can publish the copied path before its COM/OLE source has completed the
        # nested drag loop. Starting another gesture immediately then loses the next button-up
        # at the GPUI target. Wait for the native source and target to settle between sessions.
        Start-Sleep -Milliseconds 1000
        $matrixPassed.Add("Explorer->app $($scenario.button) $(if ($scenario.cancel) {'none'} elseif ($scenario.copy) {'copy'} else {'move'})")
    } }

    [ordered]@{
        schema_version=1; captured_utc=[DateTime]::UtcNow.ToString('o'); fixture=$fixture
        driver='real foreground SetCursorPos + mouse_event through production OLE and real Explorer HWNDs'
        matrix=$matrixPassed.ToArray(); passed=$matrixPassed.Count; disk_oracle='source/target existence asserted after every drag'
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
    Write-Output "Explorer drag interop passed: $OutputDirectory"
} catch {
    ($_ | Format-List * -Force | Out-String) | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'failure.txt')
    throw
} finally {
    try { $script:wakeResults | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'wake-results.json') } catch {}
    foreach ($window in $explorerWindows) { try { $window.Quit() } catch {}; [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($window) }
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue; [void]$app.WaitForExit(5000) }
    $app.Dispose()
}
