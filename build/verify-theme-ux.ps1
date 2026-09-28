[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ThemeUxNative4 {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
}
'@

function Find-Name($Root, [string]$Name) {
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $Name))
}
function Click-Element($Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke(); return
    }
    $point = $Element.GetClickablePoint()
    [void][ThemeUxNative4]::SetCursorPos([int]$point.X, [int]$point.Y)
    [ThemeUxNative4]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
    [ThemeUxNative4]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
}
function Buttons($Root) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Button)
    $all = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)
    $items = @()
    if ($all.Count -eq 0) { return $items }
    0..($all.Count - 1) | ForEach-Object { $items += $all.Item($_) }
    $items
}
function Dump-Buttons($Root, [int]$Limit = 60) {
    $n = 0
    foreach ($el in Buttons $Root) {
        $rect = $el.Current.BoundingRectangle
        Write-Output ("BUTTON name='{0}' x={1:n0} w={2:n0}" -f $el.Current.Name, $rect.X, $rect.Width)
        $n++
        if ($n -ge $Limit) { break }
    }
}

$exe = [IO.Path]::GetFullPath('C:\Program Files\SuperExplorer\SuperExplorer.exe')
$proc = Get-CimInstance Win32_Process -Filter "Name='SuperExplorer.exe'" |
    Where-Object { $_.ExecutablePath -and ([IO.Path]::GetFullPath($_.ExecutablePath) -ieq $exe) } |
    Select-Object -First 1
if (-not $proc) { throw 'installed SuperExplorer not running' }
$p = Get-Process -Id $proc.ProcessId
[void][ThemeUxNative4]::SetForegroundWindow($p.MainWindowHandle)
Start-Sleep -Milliseconds 400
$root = [Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)

$bar = Find-Name $root 'Explorer command bar'
if ($null -eq $bar) { throw 'missing Explorer command bar' }
$barButtons = Buttons $bar
if ($barButtons.Count -lt 10) { throw ("command bar button count {0}" -f $barButtons.Count) }
# 新增 剪下 複製 貼上 重新命名 分享 刪除 排序 檢視 其它
$more = $barButtons[9]
Write-Output ("clicking command-bar[{0}] name='{1}'" -f 9, $more.Current.Name)
Click-Element $more
Start-Sleep -Milliseconds 500
$root = [Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)

$popupButtons = Buttons $root | Where-Object { $_.Current.Name -and $_.Current.Name.Length -gt 0 }
$theme = $null
foreach ($el in $popupButtons) {
    # 主題 is the command with a submenu chevron near Options/About at the bottom of the more menu
    $name = $el.Current.Name
    if ($name -eq 'One Dark' -or $name -eq 'Ayu Dark') { continue }
}
# More menu order: Undo, Zip, Favorite, Bookmark, Copy path, Select all, Select none, Invert, Theme, Options, About
# After opening more, click the 9th enabled row (index 8) by matching last-three cluster: Theme, Options, About.
$moreItems = @()
foreach ($el in (Buttons $root)) {
    $name = $el.Current.Name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    if ($name -eq $more.Current.Name) { continue }
    $moreItems += $el
}
if ($moreItems.Count -lt 11) {
    Write-Output '--- buttons after opening more ---'
    Dump-Buttons $root
    throw ("expected more-menu rows, got {0}" -f $moreItems.Count)
}
$theme = $moreItems[8]
Write-Output ("clicking more-item[8] name='{0}'" -f $theme.Current.Name)
$themeRect = $theme.Current.BoundingRectangle
Click-Element $theme
Start-Sleep -Milliseconds 500
$root = [Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)

$oneDark = Find-Name $root 'One Dark'
if ($null -eq $oneDark) {
    Write-Output '--- buttons after opening theme ---'
    Dump-Buttons $root
    throw 'theme pane did not open (missing One Dark)'
}
$oneRect = $oneDark.Current.BoundingRectangle
Write-Output ("theme_item_left={0:n0} one_dark_left={1:n0}" -f $themeRect.X, $oneRect.X)
if ($oneRect.X -le ($themeRect.X + 20)) {
    throw ("theme pane is not to the right of Theme: themeX={0} oneDarkX={1}" -f $themeRect.X, $oneRect.X)
}

$ayu = Find-Name $root 'Ayu Dark'
if ($null -eq $ayu) { throw 'missing Ayu Dark' }
Click-Element $ayu
Start-Sleep -Milliseconds 700
$root = [Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
if ($null -eq (Find-Name $root 'One Dark') -or $null -eq (Find-Name $root 'Ayu Mirage')) {
    Write-Output '--- buttons after Ayu Dark ---'
    Dump-Buttons $root
    throw 'theme submenu closed after picking Ayu Dark'
}

$oneLight = Find-Name $root 'One Light'
if ($null -eq $oneLight) { throw 'missing One Light' }
Click-Element $oneLight
Start-Sleep -Milliseconds 700
$root = [Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
if ($null -eq (Find-Name $root 'One Dark') -or $null -eq (Find-Name $root 'Gruvbox Dark')) {
    Write-Output '--- buttons after One Light ---'
    Dump-Buttons $root
    throw 'theme submenu closed after picking One Light'
}

Write-Output 'theme submenu stayed open across Ayu Dark then One Light'
Write-Output 'installed theme UX PASS'
exit 0
