param(
    [string]$Executable = "$PSScriptRoot\..\target\release\SuperExplorer.exe",
    [string]$OutputDirectory = "$PSScriptRoot\..\build\reopen-closed-headful"
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$output = [IO.Path]::GetFullPath($OutputDirectory)
$contexts = [Collections.Generic.List[object]]::new()
$folders = @{}
function Send-UitestKey {
    param([byte]$Key, [byte[]]$Modifiers=@(), [int]$DelayMilliseconds=300)
    foreach ($modifier in $Modifiers) { [RustExplorerUitest.Native]::keybd_event($modifier,0,0,[UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 50
    [RustExplorerUitest.Native]::keybd_event($Key,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 50
    [RustExplorerUitest.Native]::keybd_event($Key,0,2,[UIntPtr]::Zero)
    foreach ($modifier in @($Modifiers | Sort-Object -Descending)) { [RustExplorerUitest.Native]::keybd_event($modifier,0,2,[UIntPtr]::Zero) }
    Start-Sleep -Milliseconds $DelayMilliseconds
}
foreach ($name in @('a','b','c','d','e')) {
    $folders[$name] = Join-Path $output "fixture\folder-$name"
    New-Item -ItemType Directory -Force -Path $folders[$name] | Out-Null
    Set-Content -LiteralPath (Join-Path $folders[$name] "marker-$name.txt") -Value $name
}
function Tab-Count($context) {
    @($context.Root.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::TabItem))).Count
}
function Check-Tab($context, $count, $marker) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $actual = Tab-Count $context
        if ($actual -eq $count) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($actual -ne $count) { throw "Tab count mismatch: expected=$count actual=$actual" }
    Find-UitestFileItem -Root $context.Root -Name "marker-$marker.txt" | Out-Null
}
function Focus-Window($context) {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 250
}
function Restore-Window($owner, $oldPid) {
    Focus-Window $owner
    Send-UitestKey -Key 0x54 -Modifiers @(0x11,0x10)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $child = Get-CimInstance Win32_Process -Filter "Name='SuperExplorer.exe' AND ParentProcessId=$($owner.Process.Id)" |
            Where-Object ProcessId -ne $oldPid | Sort-Object CreationDate -Descending | Select-Object -First 1
        if ($child) {
            $process = Get-Process -Id $child.ProcessId
            if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
                $context = [pscustomobject]@{
                    Process=$process; Hwnd=$process.MainWindowHandle
                    Root=[Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
                }
                $contexts.Add($context)
                return $context
            }
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Ctrl+Shift+T did not restore a closed window'
}
try {
    $environment = @{ EXPLORER_AUTO_CLOSE_MS='180000'; SUPEREXPLORER_LOCALE='en' }
    $primary = Start-UitestExplorer -InitialPath $folders.a -Executable $Executable -SkipBuild -OutputDirectory $output -AdditionalEnvironment $environment
    $contexts.Add($primary)
    Focus-Window $primary
    Check-Tab $primary 1 'a'
    Send-UitestKey -Key 0x54 -Modifiers @(0x11)
    Check-Tab $primary 2 'a'
    Set-UitestAddress -Context $primary -Path $folders.b -ExpectedItem 'marker-b.txt'
    Focus-Window $primary
    Send-UitestKey -Key 0x1B
    Send-UitestKey -Key 0x54 -Modifiers @(0x11)
    Check-Tab $primary 3 'b'
    Set-UitestAddress -Context $primary -Path $folders.c -ExpectedItem 'marker-c.txt'
    Focus-Window $primary
    Send-UitestKey -Key 0x1B
    Send-UitestKey -Key 0x57 -Modifiers @(0x11)
    Check-Tab $primary 2 'b'
    Send-UitestKey -Key 0x57 -Modifiers @(0x11)
    Check-Tab $primary 1 'a'
    Send-UitestKey -Key 0x54 -Modifiers @(0x11,0x10)
    Check-Tab $primary 2 'b'
    Send-UitestKey -Key 0x54 -Modifiers @(0x11,0x10)
    Check-Tab $primary 3 'c'
    Send-UitestKey -Key 0x57 -Modifiers @(0x11)
    Send-UitestKey -Key 0x4C -Modifiers @(0x11)
    Send-UitestKey -Key 0x54 -Modifiers @(0x11,0x10)
    Check-Tab $primary 3 'c'
    Send-UitestKey -Key 0x25 -Modifiers @(0x12)
    Check-Tab $primary 3 'b'
    Send-UitestKey -Key 0x27 -Modifiers @(0x12)
    Check-Tab $primary 3 'c'

    $second = Start-UitestExplorer -InitialPath $folders.d -Executable $Executable -SkipBuild -OutputDirectory $output -AdditionalEnvironment $environment
    $contexts.Add($second)
    Focus-Window $second
    Send-UitestKey -Key 0x54 -Modifiers @(0x11)
    Set-UitestAddress -Context $second -Path $folders.e -ExpectedItem 'marker-e.txt'
    Focus-Window $second
    Send-UitestKey -Key 0x1B
    Check-Tab $second 2 'e'
    Send-UitestKey -Key 0x73 -Modifiers @(0x12)
    if (-not $second.Process.WaitForExit(15000)) { throw 'Closed window process did not exit' }
    $restored = Restore-Window $primary $second.Process.Id
    Check-Tab $restored 2 'e'
    Focus-Window $primary
    Send-UitestKey -Key 0x54 -Modifiers @(0x11,0x10)
    Start-Sleep -Milliseconds 700
    $spawned = @(Get-CimInstance Win32_Process -Filter "Name='SuperExplorer.exe' AND ParentProcessId=$($primary.Process.Id)")
    if ($spawned.Count -ne 1) { throw 'Repeated restore duplicated an already restored window' }
    Focus-Window $restored
    Send-UitestKey -Key 0x57 -Modifiers @(0x11)
    Check-Tab $restored 1 'd'
    Send-UitestKey -Key 0x57 -Modifiers @(0x11)
    if (-not $restored.Process.WaitForExit(15000)) { throw 'Last-tab close process did not exit' }
    $lastTabWindow = Restore-Window $primary $restored.Process.Id
    Check-Tab $lastTabWindow 1 'd'
    Save-UitestScreenshot -Root $lastTabWindow.Root -Path (Join-Path $output 'restored-window.png')
    [ordered]@{passed=$true; scenarios=@('tab_lifo','original_position','address_editor_shortcut','back_forward_history','whole_window_all_tabs','no_duplicate_restore','last_tab_window_restore')} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'report.json')
} catch {
    if ($primary) { Save-UitestScreenshot -Root $primary.Root -Path (Join-Path $output 'failure.png') }
    throw
} finally {
    foreach ($context in $contexts) { Stop-UitestExplorer -Context $context }
}
