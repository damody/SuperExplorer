param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [int]$TimeoutSeconds = 30,
    [string]$OutputDirectory,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) }
    else { [IO.Path]::GetFullPath((Join-Path $workspaceRoot $env:CARGO_TARGET_DIR)) }
} else { Join-Path $workspaceRoot 'target' }
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('ime-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
} elseif (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) { throw "artifact finalization failed: $LASTEXITCODE" }
}
$executablePath = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) { throw "missing app: $executablePath" }

if (-not ('ExplorerIme.NativeWindow' -as [type])) {
    Add-Type -AssemblyName System.Drawing
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerIme {
    public static class NativeWindow {
        [StructLayout(LayoutKind.Sequential)]
        public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern IntPtr LoadKeyboardLayout(string id, uint flags);
        [DllImport("user32.dll")]
        public static extern IntPtr ActivateKeyboardLayout(IntPtr layout, uint flags);
        [DllImport("user32.dll")]
        public static extern IntPtr GetKeyboardLayout(uint threadId);
        [DllImport("kernel32.dll")]
        public static extern uint GetCurrentThreadId();
        [DllImport("user32.dll")]
        public static extern uint GetWindowThreadProcessId(IntPtr window, IntPtr processId);
        [DllImport("user32.dll")]
        public static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool BringWindowToTop(IntPtr window);
        [DllImport("user32.dll")]
        public static extern IntPtr SetFocus(IntPtr window);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool AttachThreadInput(uint source, uint target, bool attach);
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")]
        public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extraInfo);
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
        [DllImport("dwmapi.dll")]
        public static extern int DwmFlush();
    }
}
'@
}

function Send-Key([byte]$Key, [byte[]]$Modifiers = @()) {
    foreach ($modifier in $Modifiers) { [ExplorerIme.NativeWindow]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero) }
    [ExplorerIme.NativeWindow]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [ExplorerIme.NativeWindow]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    for ($index = $Modifiers.Count - 1; $index -ge 0; $index--) {
        [ExplorerIme.NativeWindow]::keybd_event($Modifiers[$index], 0, 2, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 120
}

function Save-Window([IntPtr]$WindowHandle, [string]$Path) {
    [void][ExplorerIme.NativeWindow]::DwmFlush()
    $rect = [ExplorerIme.NativeWindow+Rect]::new()
    if (-not [ExplorerIme.NativeWindow]::GetWindowRect($WindowHandle, [ref]$rect)) { throw 'GetWindowRect failed' }
    $bitmap = [Drawing.Bitmap]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try {
                if (-not [ExplorerIme.NativeWindow]::PrintWindow($WindowHandle, $dc, 2)) { throw 'PrintWindow failed' }
            } finally { $graphics.ReleaseHdc($dc) }
        } finally { $graphics.Dispose() }
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
}

$logPath = Join-Path $OutputDirectory 'explorer.log'
$diagnosticsPath = Join-Path $OutputDirectory 'diagnostics.json'
$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $executablePath
$startInfo.WorkingDirectory = $workspaceRoot
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$startInfo.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$startInfo.Environment['EXPLORER_VISUAL_FIXTURE'] = '1'
$startInfo.Environment['EXPLORER_VISUAL_STATE'] = 'populated'
$startInfo.Environment['EXPLORER_VISUAL_THEME'] = 'light'
$startInfo.Environment['EXPLORER_VISUAL_DPI'] = '175'
$startInfo.Environment['EXPLORER_VISUAL_FONT'] = 'Microsoft JhengHei UI'
$startInfo.Environment['EXPLORER_VISUAL_DIAGNOSTICS'] = $diagnosticsPath
$process = [Diagnostics.Process]::Start($startInfo)
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()
$originalLayout = [ExplorerIme.NativeWindow]::GetKeyboardLayout([ExplorerIme.NativeWindow]::GetCurrentThreadId())

try {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        $windowHandle = $process.MainWindowHandle
        $logText = if (Test-Path -LiteralPath $logPath) {
            Get-Content -Raw -Encoding utf8 -LiteralPath $logPath -ErrorAction SilentlyContinue
        } else { $null }
        $ready = $windowHandle -ne [IntPtr]::Zero -and $null -ne $logText -and
            $logText.Contains('event="visual_fixture_ready"')
        if (-not $ready) { Start-Sleep -Milliseconds 50 }
    } while (-not $ready -and [DateTime]::UtcNow -lt $deadline)
    if (-not $ready) { throw 'timed out waiting for IME fixture' }

    $currentThread = [ExplorerIme.NativeWindow]::GetCurrentThreadId()
    $foregroundThread = [ExplorerIme.NativeWindow]::GetWindowThreadProcessId([ExplorerIme.NativeWindow]::GetForegroundWindow(), [IntPtr]::Zero)
    $targetThread = [ExplorerIme.NativeWindow]::GetWindowThreadProcessId($windowHandle, [IntPtr]::Zero)
    $attachedForeground = $foregroundThread -ne 0 -and $foregroundThread -ne $currentThread -and [ExplorerIme.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $true)
    $attachedTarget = $targetThread -ne 0 -and $targetThread -ne $currentThread -and [ExplorerIme.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $true)
    try {
        [void][ExplorerIme.NativeWindow]::BringWindowToTop($windowHandle)
        [void][ExplorerIme.NativeWindow]::SetForegroundWindow($windowHandle)
        [void][ExplorerIme.NativeWindow]::SetFocus($windowHandle)
        Send-Key 0x46 @(0x11)
        $targetLayout = [ExplorerIme.NativeWindow]::GetKeyboardLayout($targetThread)
        for ($layoutAttempt = 1; $layoutAttempt -le 5 -and (($targetLayout.ToInt64() -band 0xffff) -ne 0x0804); $layoutAttempt++) {
            Send-Key 0x20 @(0x5B)
            Start-Sleep -Milliseconds 450
            $targetLayout = [ExplorerIme.NativeWindow]::GetKeyboardLayout($targetThread)
        }
        if (($targetLayout.ToInt64() -band 0xffff) -ne 0x0804) {
            throw ('Win+Space could not activate Microsoft Pinyin; target HKL=0x{0:X}' -f $targetLayout.ToInt64())
        }
        $pinyinLayout = $targetLayout
        Start-Sleep -Milliseconds 500
        foreach ($key in @(0x43,0x45,0x53,0x48,0x49)) { Send-Key ([byte]$key) }
        Save-Window $windowHandle (Join-Path $OutputDirectory '01-composition.png')
        Send-Key 0x20
        Start-Sleep -Milliseconds 400
        Save-Window $windowHandle (Join-Path $OutputDirectory '02-committed.png')
        Send-Key 0x44 @(0x11,0x10)
        Save-Window $windowHandle (Join-Path $OutputDirectory '03-shortcut-after-ime.png')
        Send-Key 0x73 @(0x12)
    } finally {
        if ($attachedTarget) { [void][ExplorerIme.NativeWindow]::AttachThreadInput($currentThread, $targetThread, $false) }
        if ($attachedForeground) { [void][ExplorerIme.NativeWindow]::AttachThreadInput($currentThread, $foregroundThread, $false) }
    }

    if (-not $process.WaitForExit(3000)) {
        [void][ExplorerIme.NativeWindow]::PostMessage($windowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    }
    if (-not $process.WaitForExit($TimeoutSeconds * 1000) -or $process.ExitCode -ne 0) { throw 'IME fixture did not close cleanly' }
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    $stdout | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'app-stdout.log')
    $stderr | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'app-stderr.log')
    $combinedOutput = $stdout + $stderr
    if ($combinedOutput -notmatch 'contains_cjk.*true') { throw 'IME did not commit a CJK character into the GPUI input state' }
    if ($combinedOutput -notmatch 'action.*ToggleTheme') { throw 'theme shortcut did not dispatch after IME composition' }

    $screenshots = @(Get-ChildItem -LiteralPath $OutputDirectory -Filter '*.png' | Sort-Object Name | ForEach-Object {
        [pscustomobject]@{ name = $_.Name; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash }
    })
    if (@($screenshots.sha256 | Sort-Object -Unique).Count -lt 2) { throw 'IME and post-shortcut frames were not distinct' }
    $pinyinTip = @(Get-WinUserLanguageList | Where-Object LanguageTag -eq 'zh-Hans-CN' | ForEach-Object InputMethodTips)
    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        ime = 'Microsoft Pinyin'
        installed_input_method_tips = $pinyinTip
        requested_hkl = ('0x{0:X}' -f $pinyinLayout.ToInt64())
        target_hkl = ('0x{0:X}' -f $targetLayout.ToInt64())
        target_language_id = ('0x{0:X4}' -f ($targetLayout.ToInt64() -band 0xffff))
        composition_keys = 'ceshi'
        commit_key = 'Space'
        cjk_commit_observed_privacy_safe = $true
        shortcut_after_composition = 'Ctrl+Shift+D handled'
        screenshots = $screenshots
        exit_code = $process.ExitCode
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "IME smoke passed: $OutputDirectory"
} finally {
    [void][ExplorerIme.NativeWindow]::ActivateKeyboardLayout($originalLayout, 0)
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        [void]$process.WaitForExit(5000)
    }
    $process.Dispose()
}
