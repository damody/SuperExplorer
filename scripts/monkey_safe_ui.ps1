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
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool IsHungAppWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);

        public static void KeyToWindow(IntPtr hwnd, byte key) {
            // Posted only to the app window. Does not move the cursor or change the real keyboard state.
            IntPtr down = new IntPtr(1);
            IntPtr up = new IntPtr(unchecked((int)(1 | (1 << 30) | (1u << 31))));
            PostMessage(hwnd, 0x0408, new IntPtr(key), down);
            PostMessage(hwnd, 0x0101, new IntPtr(key), up);
        }
    }
}
'@
}

function Send-WindowKey([IntPtr]$Hwnd, [byte]$Key) {
    if ($Hwnd -eq [IntPtr]::Zero) { return }
    # Never Delete (0x2E), F2 (0x71), or letters. Those can rename, type, or remove files.
    if ($Key -eq 0x2E -or $Key -eq 0x71) { return }
    [MonkeySafeUi.Native]::KeyToWindow($Hwnd, $Key)
}

function Test-BlockedName([string]$Name) {
    if ([string]::IsNullOrEmpty($Name)) { return $false }
    return $Name -match '(?i)delete|\u522a\u9664|\u5220\u9664|recycle|\u56de\u6536|empty recycle|\u6e05\u7a7a|\u6c38\u4e45\u522a|shift\+del|format|\u683c\u5f0f\u5316|rename|\u91cd\u65b0\u547d\u540d|\u91cd\u547d\u540d|\u6539\u540d|^close$|close tab|\u95dc\u9589|\u5173\u95ed|minimize|maximize|\u6700\u5c0f\u5316|\u6700\u5927\u5316|paste|\u8cbc\u4e0a|\u7c98\u8d34|cut|\u526a\u4e0b|\u526a\u5207|\u79fb\u9664|remove|\u78ba\u5b9a|\u786e\u5b9a|\byes\b|\bok\b|file explorer|\u6a94\u6848\u7e3d\u7ba1|\u6587\u4ef6\u8d44\u6e90\u7ba1\u7406\u5668|command-delete|command-paste|command-rename|command-cut|navigation-up'
}

function Get-ElementLabel([Windows.Automation.AutomationElement]$Element) {
    $name = ''
    $autoId = ''
    try { $name = [string]$Element.Current.Name } catch { return $null }
    try { $autoId = [string]$Element.Current.AutomationId } catch { $autoId = '' }
    return [pscustomobject]@{ Name = $name; AutomationId = $autoId }
}

function Invoke-SafeElement([Windows.Automation.AutomationElement]$Element) {
    if ($null -eq $Element) { return 'skip-null' }
    $label = Get-ElementLabel $Element
    if ($null -eq $label) { return 'skip-stale' }
    if ((Test-BlockedName $label.Name) -or (Test-BlockedName $label.AutomationId)) {
        return "blocked:$($label.Name)"
    }
    # InvokePattern only. Never move the cursor or synthesize a mouse click.
    $invoke = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
        try {
            ([Windows.Automation.InvokePattern]$invoke).Invoke()
            return "invoke:$($label.Name)"
        } catch {
            return "invoke-failed:$($label.Name)"
        }
    }
    return "skip-no-invoke:$($label.Name)"
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
                    $label = Get-ElementLabel $element
                    if ($null -eq $label) { continue }
                    if ((Test-BlockedName $label.Name) -or (Test-BlockedName $label.AutomationId)) { continue }
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

# Unmodified navigation only. No Enter, Space, Delete, or shortcuts that confirm or remove files.
$safeKeys = @(
    @{ name = 'Tab'; key = [byte]0x09 },
    @{ name = 'Left'; key = [byte]0x25 },
    @{ name = 'Up'; key = [byte]0x26 },
    @{ name = 'Right'; key = [byte]0x27 },
    @{ name = 'Down'; key = [byte]0x28 },
    @{ name = 'F5'; key = [byte]0x74 },
    @{ name = 'Esc'; key = [byte]0x1B },
    @{ name = 'Home'; key = [byte]0x24 },
    @{ name = 'End'; key = [byte]0x23 },
    @{ name = 'PageDown'; key = [byte]0x22 },
    @{ name = 'PageUp'; key = [byte]0x21 }
)

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $executable
$start.WorkingDirectory = $workspaceRoot
$start.UseShellExecute = $false
$start.Environment['EXPLORER_INITIAL_PATH'] = $fixtureRoot
$start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
$start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
$start.Environment['SUPEREXPLORER_LOCALE'] = 'zh-TW'
$start.Environment['RUST_BACKTRACE'] = '1'
$start.Environment['SUPEREXPLORER_BACKGROUND'] = '1'
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
    # HWND_BOTTOM + SWP_NOACTIVATE: keep the window behind others and do not take focus.
    [void][MonkeySafeUi.Native]::SetWindowPos($hwnd, [IntPtr]1, 60, 40, 1400, 900, 0x0010)
    Start-Sleep -Milliseconds 400
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
                Send-WindowKey $hwnd $choice.key
                $label = $choice.name
            } else {
                $clickable = Get-Clickable $root
                if ($clickable.Count -gt 0) {
                    $pick = $clickable[$rng.Next(0, $clickable.Count)]
                    $label = Invoke-SafeElement $pick
                } else {
                    Send-WindowKey $hwnd 0x1B
                    $label = 'Esc-empty-tree'
                }
            }
            $actions.Add([ordered]@{ step = $step; at = [DateTime]::UtcNow.ToString('o'); action = $label })
        } catch {
            $findings.Add([ordered]@{ kind = 'uia-error'; step = $step; detail = "$_" })
            Send-WindowKey $hwnd 0x1B
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
                Send-WindowKey $hwnd 0x1B
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
