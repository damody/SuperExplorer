[CmdletBinding()]
param(
    [string]$Executable = 'C:\Program Files\SuperExplorer\SuperExplorer.exe',
    [int]$Seconds = 20
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Wait-Until([scriptblock]$Probe, [string]$Description, [int]$TimeoutSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $value = & $Probe
        if ($null -ne $value -and $value -ne $false) { return $value }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Description"
}

function Find-Id($Root, [string]$Id) {
    $Root.FindFirst(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::AutomationIdProperty, $Id))
}

function Find-Name($Root, [string]$Name) {
    $Root.FindFirst(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty, $Name))
}

function Click-Element($Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
        return
    }
    $point = $Element.GetClickablePoint()
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ThemeVerifyNative {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
}
'@
    [void][ThemeVerifyNative]::SetCursorPos([int]$point.X, [int]$point.Y)
    [ThemeVerifyNative]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [ThemeVerifyNative]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Dump-Ids($Root) {
    $all = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    $ids = @()
    if ($all.Count -eq 0) { return $ids }
    0..($all.Count - 1) | ForEach-Object {
        $item = $all.Item($_)
        $id = $item.Current.AutomationId
        $name = $item.Current.Name
        if (-not [string]::IsNullOrWhiteSpace($id) -or -not [string]::IsNullOrWhiteSpace($name)) {
            $ids += ("id={0}; name={1}" -f $id, $name)
        }
    }
    $ids
}

$exe = [IO.Path]::GetFullPath($Executable)
if (-not (Test-Path -LiteralPath $exe)) { throw "missing $exe" }

$bytes = [IO.File]::ReadAllBytes($exe)
$ascii = [Text.Encoding]::ASCII.GetString($bytes)
foreach ($needle in @('more-theme', 'ToggleMoreThemeSubmenu', 'follow-windows', 'one-dark', 'gruvbox-dark')) {
    if (-not $ascii.Contains($needle)) { throw "installed binary missing $needle" }
}
if ($ascii.Contains('more-handoff-file-explorer') -and -not $ascii.Contains('command-handoff-file-explorer')) {
    throw 'installed binary still uses more-menu handoff without toolbar handoff'
}

$process = Get-CimInstance Win32_Process -Filter "Name='SuperExplorer.exe'" |
    Where-Object { $_.ExecutablePath -and ([IO.Path]::GetFullPath($_.ExecutablePath) -ieq $exe) } |
    Select-Object -First 1
if (-not $process) { throw "SuperExplorer is not running from $exe" }
$uiProcess = Get-Process -Id $process.ProcessId
$hwnd = Wait-Until { if ($uiProcess.MainWindowHandle -ne [IntPtr]::Zero) { $uiProcess.MainWindowHandle } } 'main window' $Seconds
$root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
if ($null -eq $root) { throw 'UIA root unavailable' }

$more = Wait-Until {
    $byId = Find-Id $root 'command-more-menu'
    if ($byId) { return $byId }
    Find-Name $root '其它'
} 'more menu button' $Seconds
Click-Element $more
Start-Sleep -Milliseconds 400
$root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
$theme = Wait-Until {
    $byId = Find-Id $root 'more-theme'
    if ($byId) { return $byId }
    Find-Name $root '主題'
} 'theme more-menu item' $Seconds
Click-Element $theme
Start-Sleep -Milliseconds 400
$root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)

$required = @(
    @{ Id = 'more-theme-follow-windows'; Name = '跟隨 Windows' },
    @{ Id = 'more-theme-windows-light'; Name = '淺色' },
    @{ Id = 'more-theme-windows-dark'; Name = '深色' },
    @{ Id = 'more-theme-one-dark'; Name = 'One Dark' },
    @{ Id = 'more-theme-one-light'; Name = 'One Light' },
    @{ Id = 'more-theme-ayu-dark'; Name = 'Ayu Dark' },
    @{ Id = 'more-theme-ayu-mirage'; Name = 'Ayu Mirage' },
    @{ Id = 'more-theme-gruvbox-dark'; Name = 'Gruvbox Dark' }
)
$missing = @()
foreach ($item in $required) {
    $found = Find-Id $root $item.Id
    if (-not $found) { $found = Find-Name $root $item.Name }
    if (-not $found) { $missing += $item.Id }
}
$forbidden = @('more-handoff-file-explorer')
$leaked = @()
foreach ($id in $forbidden) {
    if (Find-Id $root $id) { $leaked += $id }
}
$dump = Dump-Ids $root
if ($missing.Count -gt 0 -or $leaked.Count -gt 0) {
    Write-Output '--- UIA dump ---'
    $dump | Select-Object -First 80
    throw ("theme menu verification failed missing=[{0}] leaked=[{1}]" -f ($missing -join ','), ($leaked -join ','))
}

Write-Output 'installed SuperExplorer theme more-menu PASS'
exit 0
