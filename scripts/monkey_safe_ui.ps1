param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [int]$DurationSeconds = 90,
    [int]$Seed = 0,
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
    $OutputDirectory = Join-Path $targetRoot ('monkey-safe-ui\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if ($Seed -eq 0) { $Seed = [DateTime]::UtcNow.Ticks % 2147483647 }
$rng = [Random]::new([int]$Seed)

if (-not $SkipBuild) {
    if ($Profile -eq 'release') { cargo build -p explorer-app --release --locked }
    else { cargo build -p explorer-app --locked }
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot "$Profile\SuperExplorer.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "missing app: $executable" }

$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('explorer-uitest-monkey-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path (Join-Path $fixtureRoot 'alpha') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $fixtureRoot 'beta\nested') | Out-Null
Set-Content -LiteralPath (Join-Path $fixtureRoot 'readme.txt') -Value 'monkey fixture' -Encoding utf8
Set-Content -LiteralPath (Join-Path $fixtureRoot 'alpha\one.txt') -Value 'one' -Encoding utf8
Set-Content -LiteralPath (Join-Path $fixtureRoot 'beta\nested\two.txt') -Value 'two' -Encoding utf8

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ('MonkeySafeUi.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace MonkeySafeUi {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool IsHungAppWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    }
}
'@
}

function Send-Key([byte]$Key, [byte[]]$Modifiers = @()) {
    foreach ($modifier in $Modifiers) {
        [MonkeySafeUi.Native]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero)
    }
    [MonkeySafeUi.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [MonkeySafeUi.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    for ($index = $Modifiers.Count - 1; $index -ge 0; $index--) {
        [MonkeySafeUi.Native]::keybd_event($Modifiers[$index], 0, 2, [UIntPtr]::Zero)
    }
}

function Test-BlockedName([string]$Name) {
    if ([string]::IsNullOrEmpty($Name)) { return $false }
    return $Name -match '(?i)delete|\u522a\u9664|recycle|\u56de\u6536|empty recycle|\u6e05\u7a7a|\u6c38\u4e45\u522a|shift\+del|format|\u683c\u5f0f\u5316|rename|\u91cd\u65b0\u547d\u540d|\u6539\u540d|^close$|close tab|\u95dc\u9589|\u5173\u95ed|minimize|maximize|\u6700\u5c0f\u5316|\u6700\u5927\u5316|paste|\u8cbc\u4e0a|file explorer|\u6a94\u6848\u7e3d\u7ba1|\u6587\u4ef6\u8d44\u6e90\u7ba1\u7406\u5668'
}

function Invoke-SafeElement([Windows.Automation.AutomationElement]$Element) {
    if ($null -eq $Element) { return 'skip-null' }
    $name = ''
    try { $name = [string]$Element.Current.Name } catch { return 'skip-stale' }
    if (Test-BlockedName $name) { return "blocked:$name" }
    $invoke = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
        try {
            ([Windows.Automation.InvokePattern]$invoke).Invoke()
            return "invoke:$name"
        } catch {
            return "invoke-failed:$name"
        }
    }
    try {
        $bounds = $Element.Current.BoundingRectangle
        if ($bounds.Width -lt 4 -or $bounds.Height -lt 4) { return "skip-tiny:$name" }
        $x = [int]($bounds.X + $bounds.Width / 2)
        $y = [int]($bounds.Y + $bounds.Height / 2)
        [void][MonkeySafeUi.Native]::SetCursorPos($x, $y)
        [MonkeySafeUi.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [MonkeySafeUi.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "click:$name"
    } catch {
        return "click-failed:$name"
    }
}

function Get-Clickable([Windows.Automation.AutomationElement]$Root) {
    $found = New-Object System.Collections.Generic.List[object]
    foreach ($type in @(
            [Windows.Automation.ControlType]::Button,
            [Windows.Automation.ControlType]::TabItem,
            [Windows.Automation.ControlType]::MenuItem,
            [Windows.Automation.ControlType]::Hyperlink,
            [Windows.Automation.ControlType]::CheckBox,
            [Windows.Automation.ControlType]::ListItem,
            [Windows.Automation.ControlType]::TreeItem
        )) {
        try {
            $condition = [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty, $type)
            $elements = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
            foreach ($element in $elements) {
                try {
                    if (-not $element.Current.IsEnabled) { continue }
                    if (Test-BlockedName $element.Current.Name) { continue }
                    $found.Add($element)
                    if ($found.Count -ge 80) { return $found }
                } catch { }
            }
        } catch { }
    }
    return $found
}

function Read-LogErrors([string]$Path) {
    $hits = @()
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $hits }
    $lines = Get-Content -LiteralPath $Path -Encoding utf8 -ErrorAction SilentlyContinue
    foreach ($line in $lines) {
        if ($line -match '(?i)panic') { $hits += $line; continue }
        if ($line -match 'severity="critical"') { $hits += $line; continue }
        if ($line -match 'severity="error"') {
            if ($line -match '(?i)FOLDER_SIZE_UNAVAILABLE|service endpoint: Overloaded|window not found|request cancellation token was set|mft_service_batch_query') {
                continue
            }
            $hits += $line
        }
    }
    return $hits
}

$actions = New-Object System.Collections.Generic.List[object]
$findings = New-Object System.Collections.Generic.List[object]
$hung = $false
$earlyExit = $null

$safeKeys = @(
    @{ name = 'Tab'; key = [byte]0x09; mods = @() },
    @{ name = 'Shift+Tab'; key = [byte]0x09; mods = @([byte]0x10) },
    @{ name = 'Left'; key = [byte]0x25; mods = @() },
    @{ name = 'Up'; key = [byte]0x26; mods = @() },
    @{ name = 'Right'; key = [byte]0x27; mods = @() },
    @{ name = 'Down'; key = [byte]0x28; mods = @() },
    @{ name = 'F5'; key = [byte]0x74; mods = @() },
    @{ name = 'Alt+Left'; key = [byte]0x25; mods = @([byte]0x12) },
    @{ name = 'Alt+Right'; key = [byte]0x27; mods = @([byte]0x12) },
    @{ name = 'Alt+Up'; key = [byte]0x26; mods = @([byte]0x12) },
    @{ name = 'Ctrl+T'; key = [byte]0x54; mods = @([byte]0x11) },
    @{ name = 'Ctrl+Tab'; key = [byte]0x09; mods = @([byte]0x11) },
    @{ name = 'Ctrl+1'; key = [byte]0x31; mods = @([byte]0x11) },
    @{ name = 'Ctrl+2'; key = [byte]0x32; mods = @([byte]0x11) },
    @{ name = 'Ctrl+3'; key = [byte]0x33; mods = @([byte]0x11) },
    @{ name = 'Ctrl+4'; key = [byte]0x34; mods = @([byte]0x11) },
    @{ name = 'Ctrl+L'; key = [byte]0x4C; mods = @([byte]0x11) },
    @{ name = 'Ctrl+E'; key = [byte]0x45; mods = @([byte]0x11) },
    @{ name = 'Ctrl+A'; key = [byte]0x41; mods = @([byte]0x11) },
    @{ name = 'Ctrl+C'; key = [byte]0x43; mods = @([byte]0x11) },
    @{ name = 'Esc'; key = [byte]0x1B; mods = @() },
    @{ name = 'Enter'; key = [byte]0x0D; mods = @() },
    @{ name = 'Space'; key = [byte]0x20; mods = @() },
    @{ name = 'Home'; key = [byte]0x24; mods = @() },
    @{ name = 'End'; key = [byte]0x23; mods = @() },
    @{ name = 'PageDown'; key = [byte]0x22; mods = @() },
    @{ name = 'PageUp'; key = [byte]0x21; mods = @() }
)

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $executable
$start.WorkingDirectory = $workspaceRoot
$start.UseShellExecute = $false
$start.Environment['EXPLORER_INITIAL_PATH'] = $fixtureRoot
$start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
$start.Environment['SUPEREXPLORER_LOCALE'] = 'zh-TW'
$process = [Diagnostics.Process]::Start($start)
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if ($process.HasExited) { throw "application exited early: $($process.ExitCode)" }
        $process.Refresh()
        $hwnd = $process.MainWindowHandle
        if ($hwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 80 }
    } while ($hwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($hwnd -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][MonkeySafeUi.Native]::SetWindowPos($hwnd, [IntPtr]::Zero, 60, 40, 1400, 900, 0x0040)
    Start-Sleep -Milliseconds 400
    [void][MonkeySafeUi.Native]::SetForegroundWindow($hwnd)
    $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $endAt = [DateTime]::UtcNow.AddSeconds($DurationSeconds)
    $step = 0
    while ([DateTime]::UtcNow -lt $endAt) {
        $step += 1
        $process.Refresh()
        if ($process.HasExited) {
            $earlyExit = $process.ExitCode
            $findings.Add([ordered]@{ kind = 'crash'; step = $step; detail = "exited with $earlyExit" })
            break
        }
        $hwnd = $process.MainWindowHandle
        if ($hwnd -eq [IntPtr]::Zero) {
            Start-Sleep -Milliseconds 50
            continue
        }
        if ([MonkeySafeUi.Native]::IsHungAppWindow($hwnd)) {
            $hung = $true
            $findings.Add([ordered]@{ kind = 'hang'; step = $step; detail = 'IsHungAppWindow' })
            break
        }
        try {
            $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
            $kind = $rng.Next(0, 10)
            $label = ''
            if ($kind -lt 6) {
                $choice = $safeKeys[$rng.Next(0, $safeKeys.Count)]
                [void][MonkeySafeUi.Native]::SetForegroundWindow($hwnd)
                Send-Key $choice.key $choice.mods
                if ($choice.extra -eq 'esc') {
                    Start-Sleep -Milliseconds 80
                    Send-Key 0x1B @()
                }
                $label = $choice.name
            } else {
                $clickable = Get-Clickable $root
                if ($clickable.Count -gt 0) {
                    $pick = $clickable[$rng.Next(0, $clickable.Count)]
                    $label = Invoke-SafeElement $pick
                } else {
                    Send-Key 0x1B @()
                    $label = 'Esc-empty-tree'
                }
            }
            $actions.Add([ordered]@{ step = $step; at = [DateTime]::UtcNow.ToString('o'); action = $label })
        } catch {
            $findings.Add([ordered]@{ kind = 'uia-error'; step = $step; detail = "$_" })
            Send-Key 0x1B @()
        }
        Start-Sleep -Milliseconds (80 + $rng.Next(0, 140))
        try {
            $dialogs = $root.FindAll(
                [Windows.Automation.TreeScope]::Children,
                [Windows.Automation.PropertyCondition]::new(
                    [Windows.Automation.AutomationElement]::ControlTypeProperty,
                    [Windows.Automation.ControlType]::Window))
            foreach ($dialog in $dialogs) {
                $dialogName = ''
                try { $dialogName = [string]$dialog.Current.Name } catch { continue }
                if ($dialogName -match '(?i)error|\u932f\u8aa4|exception|\u5931\u6557|failed') {
                    $findings.Add([ordered]@{ kind = 'error-dialog'; step = $step; detail = $dialogName })
                }
                Send-Key 0x1B @()
            }
        } catch { }
    }
} finally {
    $logErrors = Read-LogErrors (Join-Path $OutputDirectory 'explorer.log')
    $errorLogErrors = Read-LogErrors (Join-Path $OutputDirectory 'error.log')
    $stderrErrors = @()
    foreach ($line in $logErrors) {
        $findings.Add([ordered]@{ kind = 'explorer.log'; detail = $line })
    }
    foreach ($line in $errorLogErrors) {
        $findings.Add([ordered]@{ kind = 'error.log'; detail = $line })
    }
    foreach ($line in $stderrErrors) {
        $findings.Add([ordered]@{ kind = 'stderr'; detail = $line })
    }
    if ($null -ne $process -and -not $process.HasExited) {
        $hwnd = $process.MainWindowHandle
        if ($hwnd -ne [IntPtr]::Zero) {
            [void][MonkeySafeUi.Native]::PostMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        }
        if (-not $process.WaitForExit(8000)) {
            $hung = $true
            $findings.Add([ordered]@{ kind = 'hang'; detail = 'did not exit after WM_CLOSE' })
            $process.Kill()
            $process.WaitForExit()
        }
    }
    $findingRows = @()
    foreach ($item in $findings) {
        $findingRows += [pscustomobject]@{
            kind = [string]$item.kind
            step = $item.step
            detail = [string]$item.detail
        }
    }
    $actionRows = @()
    foreach ($item in @($actions | Select-Object -Last 40)) {
        $actionRows += [pscustomobject]@{
            step = $item.step
            at = [string]$item.at
            action = [string]$item.action
        }
    }
    $pass = ($findings.Count -eq 0 -and -not $hung -and $null -eq $earlyExit)
    $report = [pscustomobject]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        seed = $Seed
        duration_seconds = $DurationSeconds
        fixture = $fixtureRoot
        hung = [bool]$hung
        early_exit = $earlyExit
        action_count = $actions.Count
        finding_count = $findings.Count
        findings = $findingRows
        last_actions = $actionRows
        result = $(if ($pass) { 'PASS' } else { 'FAIL' })
    }
    ($report | ConvertTo-Json -Depth 8) | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    $resolvedFixture = [IO.Path]::GetFullPath($fixtureRoot)
    $allowedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\explorer-uitest-monkey-'
    if ($resolvedFixture.StartsWith($allowedTemp, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolvedFixture)) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$reportPath = Join-Path $OutputDirectory 'report.json'
Write-Output "Monkey safe UI finished: $reportPath"
$parsed = Get-Content -Raw -Encoding utf8 -LiteralPath $reportPath | ConvertFrom-Json
if ($parsed.result -ne 'PASS') {
    Write-Output ("FINDINGS: {0}" -f $parsed.finding_count)
    foreach ($finding in $parsed.findings) {
        Write-Output ("- [{0}] {1}" -f $finding.kind, $finding.detail)
    }
    exit 1
}
Write-Output 'Monkey safe UI passed with no hang or error findings.'
# keep ASCII-only user-facing strings for Windows PowerShell 5 parsers
exit 0
