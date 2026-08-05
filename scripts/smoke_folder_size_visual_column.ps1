param(
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$PluginDll = 'sdk\fixtures\rust-folder-size-visual-column\target\x86_64-pc-windows-msvc\debug\rust_folder_size_visual_column.dll',
    [string]$InitialPath = 'sdk\fixtures\rust-folder-size-visual-column\fixtures\sample',
    [string]$OutputDirectory = 'target\rust-folder-size-visual-column-headful'
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
foreach ($name in 'Executable','PluginDll','OutputDirectory') {
    $value = Get-Variable $name -ValueOnly
    if (-not [IO.Path]::IsPathRooted($value)) { Set-Variable $name ([IO.Path]::GetFullPath((Join-Path $workspace $value))) }
}
$Executable = (Resolve-Path $Executable).Path
$PluginDll = (Resolve-Path $PluginDll).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$extensionState = Join-Path $OutputDirectory 'extension-state'
New-Item -ItemType Directory -Force -Path $extensionState | Out-Null

# Keep the fixture deterministic and outside the repository.  The values are
# deliberately different so the Details header's numeric sort is observable.
if ([string]::IsNullOrWhiteSpace($InitialPath)) {
    $InitialPath = Join-Path $OutputDirectory 'sample'
    New-Item -ItemType Directory -Force -Path $InitialPath | Out-Null
    foreach ($index in 0..999) {
        $dir = Join-Path $InitialPath ("item-{0:D4}" -f $index)
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        $size = 256 + (($index % 64) * 128)
        [IO.File]::WriteAllBytes((Join-Path $dir 'payload.bin'), [byte[]]::new($size))
    }
} else { $InitialPath = (Resolve-Path $InitialPath).Path }
$fixtureItemCount = @(Get-ChildItem -LiteralPath $InitialPath -Directory).Count
if ($fixtureItemCount -ne 1000) { throw "folder-size final slice requires exactly 1,000 fixture items, found $fixtureItemCount" }

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ('FolderSizeHeadful.Native' -as [type])) {
    Add-Type @'
using System; using System.Runtime.InteropServices;
namespace FolderSizeHeadful { public static class Native {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e);
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("dwmapi.dll")] public static extern int DwmFlush();
} }
'@
}
function Capture([IntPtr]$window,[string]$path) {
    [FolderSizeHeadful.Native]::DwmFlush(); $r=[FolderSizeHeadful.Native+Rect]::new()
    if (-not [FolderSizeHeadful.Native]::GetWindowRect($window,[ref]$r)) { throw 'GetWindowRect failed' }
    $b=[Drawing.Bitmap]::new($r.Right-$r.Left,$r.Bottom-$r.Top); try { $g=[Drawing.Graphics]::FromImage($b); try { $dc=$g.GetHdc(); try { if (-not [FolderSizeHeadful.Native]::PrintWindow($window,$dc,2)) { throw 'PrintWindow failed' } } finally {$g.ReleaseHdc($dc)} } finally {$g.Dispose()}; $b.Save($path,[Drawing.Imaging.ImageFormat]::Png) } finally {$b.Dispose()}
}
function Find-Name($root,[string]$name) { $root.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$name)) }
function Find-Prefix($root,[string]$prefix) { $a=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition); 0..($a.Count-1) | % {$a.Item($_)} | ? {$_.Current.Name -like "$prefix*"} | select -First 1 }
function Click($root,$element,[switch]$Right) {
    $b=$element.Current.BoundingRectangle; $rb=$root.Current.BoundingRectangle; $wr=[FolderSizeHeadful.Native+Rect]::new(); [FolderSizeHeadful.Native]::GetWindowRect($window,[ref]$wr) | Out-Null
    $x=[int]($wr.Left+(($b.Left+$b.Width/2)-$rb.Left)*($wr.Right-$wr.Left)/$rb.Width); $y=[int]($wr.Top+(($b.Top+$b.Height/2)-$rb.Top)*($wr.Bottom-$wr.Top)/$rb.Height)
    [FolderSizeHeadful.Native]::SetCursorPos($x,$y); if ($Right) {$down=8;$up=16} else {$down=2;$up=4}; [FolderSizeHeadful.Native]::mouse_event($down,0,0,0,[UIntPtr]::Zero); [FolderSizeHeadful.Native]::mouse_event($up,0,0,0,[UIntPtr]::Zero)
}
function Key([byte]$key) { [FolderSizeHeadful.Native]::keybd_event($key,0,0,[UIntPtr]::Zero); [FolderSizeHeadful.Native]::keybd_event($key,0,2,[UIntPtr]::Zero) }
function Chord([byte]$modifier,[byte]$key) { [FolderSizeHeadful.Native]::keybd_event($modifier,0,0,[UIntPtr]::Zero); Key $key; [FolderSizeHeadful.Native]::keybd_event($modifier,0,2,[UIntPtr]::Zero) }

$diag=Join-Path $OutputDirectory 'diagnostics.json'; $psi=[Diagnostics.ProcessStartInfo]::new(); $psi.FileName=$Executable; $psi.Arguments="--plugin-dll `"$PluginDll`""; $psi.WorkingDirectory=$workspace; $psi.UseShellExecute=$false; $psi.EnvironmentVariables['EXPLORER_VISUAL_FIXTURE']='1'; $psi.EnvironmentVariables['EXPLORER_VISUAL_REAL_SHELL']='1'; $psi.EnvironmentVariables['EXPLORER_VISUAL_STATE']='populated'; $psi.EnvironmentVariables['EXPLORER_VISUAL_DIAGNOSTICS']=$diag; $psi.EnvironmentVariables['EXPLORER_INITIAL_PATH']=$InitialPath; $psi.EnvironmentVariables['EXPLORER_LOG_DIR']=$OutputDirectory; $psi.EnvironmentVariables['EXPLORER_UITEST_EXTENSION_STATE_ROOT']=$extensionState
$process=[Diagnostics.Process]::Start($psi); try {
    $until=[DateTime]::UtcNow.AddSeconds(35); do { Start-Sleep -Milliseconds 150; $process.Refresh(); $window=$process.MainWindowHandle } while (($window -eq [IntPtr]::Zero -or -not (Test-Path $diag)) -and [DateTime]::UtcNow -lt $until)
    if ($window -eq [IntPtr]::Zero) { throw 'Timed out waiting for SuperExplorer' }; [FolderSizeHeadful.Native]::SetForegroundWindow($window) | Out-Null; $root=[Windows.Automation.AutomationElement]::FromHandle($window)
    $until=[DateTime]::UtcNow.AddSeconds(30); do { Start-Sleep -Milliseconds 200; $header=Find-Name $root 'Sort by Folder size'; $cells=Find-Prefix $root 'Folder size: ' } while (($null -eq $header -or $null -eq $cells -or $cells.Current.Name -match 'Loading|Calculating') -and [DateTime]::UtcNow -lt $until)
    if ($null -eq $header -or $null -eq $cells) { throw 'Folder size column did not produce completed exact values' }
    $exactValue = $cells.Current.Name
    $columnImage=Join-Path $OutputDirectory 'folder-size-column.png'; Capture $window $columnImage
    $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    # GPUI element IDs are intentionally not accessibility nodes for this
    # decorative bar.  The setting toggle and screenshot delta below prove
    # that the public render plan changes the visible surface.
    $tracks=@(0..($all.Count-1) | % { $all.Item($_) } | ? { $_.Current.AutomationId -like 'folder-size-bar-track-*' })
    $rows=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::DataItem)); if ($rows.Count -eq 0) { throw 'No real shell DataItem rows were loaded' }
    Click $root $header; Start-Sleep -Milliseconds 400; $sorted=Find-Prefix $root 'Folder size, sorted'; if ($null -eq $sorted) { throw 'Folder size numeric sort state was not exposed' }
    Click $root $sorted -Right; Start-Sleep -Milliseconds 250; $toggle=Find-Prefix $root 'Show proportional bar'; if ($null -eq $toggle) { throw 'Show proportional bar setting was not exposed' }
    $before=Join-Path $OutputDirectory 'folder-size-bar-on.png'; Capture $window $before; Click $root $toggle; Start-Sleep -Milliseconds 400; $after=Join-Path $OutputDirectory 'folder-size-bar-off.png'; Capture $window $after
    if ((Get-FileHash $before).Hash -eq (Get-FileHash $after).Hash) { throw 'Toggling proportional bar did not change the rendered surface' }
    Key 0x74; Start-Sleep -Milliseconds 1200; if ($null -eq (Find-Prefix $root 'Folder size: ')) { throw 'F5 lost the completed folder-size values' }
    Chord 0x12 0x26; Start-Sleep -Milliseconds 600
    $cacheTimer=[Diagnostics.Stopwatch]::StartNew(); Chord 0x12 0x25
    $until=[DateTime]::UtcNow.AddSeconds(3); $cachedCell=$null
    do { Start-Sleep -Milliseconds 50; $cachedCell=Find-Prefix $root 'Folder size: ' } while (($null -eq $cachedCell -or $cachedCell.Current.Name -match 'Loading|Calculating') -and [DateTime]::UtcNow -lt $until)
    $cacheTimer.Stop(); if ($null -eq $cachedCell -or $cachedCell.Current.Name -match 'Loading|Calculating') { throw 'Returning to an unchanged folder did not reuse the host cache' }
    [pscustomobject]@{status='passed';case_id='rust-folder-size-visual-column-headful';fixture_items=$fixtureItemCount;exact_value=$exactValue;data_items=$rows.Count;decorative_bar_automation_nodes=$tracks.Count;proportional_bar_visual_delta=$true;numeric_sort=$true;proportional_bar_toggle=$true;f5_generation_recovery=$true;unchanged_folder_host_cache=$true;cache_roundtrip_millis=$cacheTimer.ElapsedMilliseconds;screenshots=@('folder-size-column.png','folder-size-bar-on.png','folder-size-bar-off.png')} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OutputDirectory 'report.json') -Encoding utf8
} finally { if ($process -and -not $process.HasExited) {$process.Kill();$process.WaitForExit()} }
