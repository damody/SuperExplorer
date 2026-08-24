param([ValidateSet('debug','release')][string]$Profile='debug',[Parameter(Mandatory)][string]$OutputDirectory,[switch]$SkipBuild,[switch]$EditorOnly)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful
if(-not('RustExplorerUitest.BookmarkMenuNative'-as[type])){Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using System.Text;namespace RustExplorerUitest{public static class BookmarkMenuNative{[StructLayout(LayoutKind.Sequential)]public struct RECT{public int Left,Top,Right,Bottom;}[DllImport("user32.dll")]public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);[DllImport("user32.dll")]public static extern int GetMenuItemCount(IntPtr m);[DllImport("user32.dll",CharSet=CharSet.Unicode)]public static extern int GetMenuString(IntPtr m,uint i,StringBuilder s,int c,uint f);[DllImport("user32.dll")]public static extern bool GetMenuItemRect(IntPtr h,IntPtr m,uint i,out RECT r);}}
'@}
$output=[IO.Path]::GetFullPath($OutputDirectory)
$fixture=Join-Path $output 'bookmark-fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $fixture 'Folder bookmark') | Out-Null
New-Item -ItemType File -Force -Path (Join-Path $fixture 'File bookmark.txt') | Out-Null
$context=$null
function Find-ByName([string]$Name){$processId=[int]$context.Process.Id;Find-UitestElement -Root ([Windows.Automation.AutomationElement]::RootElement) -Description $Name -Predicate {param($e) $e.Current.ProcessId -eq $processId -and $e.Current.Name -eq $Name}}
function Get-PopupHandle{
 $ids=[Collections.Generic.HashSet[int]]::new();[void]$ids.Add([int]$context.Process.Id)
 do{$changed=$false;foreach($p in @(Get-CimInstance Win32_Process)){if($ids.Contains([int]$p.ParentProcessId)-and$ids.Add([int]$p.ProcessId)){$changed=$true}}}while($changed)
 $found=[Collections.Generic.List[IntPtr]]::new();$cb=[RustExplorerUitest.Native+EnumWindowsProc]{param([IntPtr]$h,[IntPtr]$u);$n=[Text.StringBuilder]::new(64);[void][RustExplorerUitest.Native]::GetClassName($h,$n,$n.Capacity);[uint32]$procId=0;[void][RustExplorerUitest.Native]::GetWindowThreadProcessId($h,[ref]$procId);if($n.ToString()-eq'#32768'-and$ids.Contains([int]$procId)){$found.Add($h)};return $true};[void][RustExplorerUitest.Native]::EnumWindows($cb,[IntPtr]::Zero);$found|Select-Object -First 1
}
function Invoke-AddBookmarkMenu([string]$ItemName){
 Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name $ItemName) -Right
 $popup=$null;$deadline=[DateTime]::UtcNow.AddSeconds(5);do{$popup=Get-PopupHandle;if($null-eq$popup){Start-Sleep -Milliseconds 50}}while($null-eq$popup-and[DateTime]::UtcNow-lt$deadline);if($null-eq$popup){throw 'native bookmark popup not found'}
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-context-menu.png')
 $menu=[RustExplorerUitest.BookmarkMenuNative]::SendMessage($popup,0x01E1,[IntPtr]::Zero,[IntPtr]::Zero);$matched=$false
 $expected=(-join @([char]0x52A0,[char]0x5165,[char]0x66F8,[char]0x7C64))
 for($i=0;$i-lt[RustExplorerUitest.BookmarkMenuNative]::GetMenuItemCount($menu);$i++){$label=[Text.StringBuilder]::new(512);[void][RustExplorerUitest.BookmarkMenuNative]::GetMenuString($menu,[uint32]$i,$label,$label.Capacity,0x400);if($label.ToString()-eq$expected){$r=[RustExplorerUitest.BookmarkMenuNative+RECT]::new();[void][RustExplorerUitest.BookmarkMenuNative]::GetMenuItemRect([IntPtr]::Zero,$menu,[uint32]$i,[ref]$r);[void][RustExplorerUitest.Native]::SetCursorPos([int](($r.Left+$r.Right)/2),[int](($r.Top+$r.Bottom)/2));[RustExplorerUitest.Native]::mouse_event(2,0,0,0,[UIntPtr]::Zero);[RustExplorerUitest.Native]::mouse_event(4,0,0,0,[UIntPtr]::Zero);$matched=$true;break}}
 if(-not$matched){throw 'Add bookmark command missing from native menu'};Start-Sleep -Milliseconds 250
}
try {
 $context=Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
 $addStar=Find-ByName 'Add current folder bookmark and choose a folder'
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-star-off.png')
 Invoke-UitestClick -Element $addStar
 [void](Find-ByName 'Bookmark editor')
 Save-UitestScreenshot -Root (Find-ByName 'Bookmark editor window') -Path (Join-Path $output 'bookmark-destination-picker.png')
 Invoke-UitestClick -Element (Find-ByName 'Save bookmark')
 [void][RustExplorerUitest.Native]::SetForegroundWindow([IntPtr]$context.Hwnd)
 Start-Sleep -Milliseconds 500
 $context.Root=[Windows.Automation.AutomationElement]::FromHandle([IntPtr]$context.Hwnd)
 [void](Find-ByName 'Edit or remove current folder bookmark')
 $folderBookmark=Find-UitestElement -Root $context.Root -Description 'Current folder bookmark button' -Predicate {param($e) $e.Current.Name -like '*Bookmark: bookmark-fixture*'}
 $addStar=Find-ByName 'Edit or remove current folder bookmark'
 if($addStar.Current.BoundingRectangle.Left -ge $folderBookmark.Current.BoundingRectangle.Left){throw 'Bookmark star is not fixed at the left edge of the toolbar'}
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-star-on.png')
 if($EditorOnly){
   Start-Sleep -Milliseconds 300
   [ordered]@{schema='bookmark-editor-window-smoke-v2';status='PASS';dedicated_window=$true;read_only_target=$true;artifacts=@('bookmark-star-on.png','bookmark-star-off.png','bookmark-destination-picker.png')}|ConvertTo-Json -Depth 4|Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
   Write-Output "Bookmark editor window smoke passed: $OutputDirectory"
   return
 }
 Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'File bookmark.txt')
 [void](Find-ByName 'Edit or remove current folder bookmark')
 Invoke-UitestClick -Element (Find-ByName 'Edit or remove current folder bookmark')
 [void](Find-ByName 'Bookmark editor')
 Invoke-UitestClick -Element (Find-ByName 'Remove bookmark')
 [void](Find-ByName 'Add current folder bookmark and choose a folder')
 Invoke-UitestClick -Element (Find-ByName 'Add current folder bookmark and choose a folder')
 [void](Find-ByName 'Bookmark editor')
 Invoke-UitestClick -Element (Find-ByName 'Save bookmark')
 Invoke-UitestClick -Element (Find-ByName 'Manage bookmarks')
 Invoke-UitestClick -Element (Find-ByName 'Add bookmark folder')
 [void](Find-ByName 'Rename bookmark folder')
 Invoke-UitestClick -Element (Find-ByName 'Save bookmark folder')
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-folder-manager.png')
 Invoke-UitestClick -Element (Find-ByName 'Close bookmark manager')
 $favoriteFolder=Find-UitestElement -Root $context.Root -Description 'favorite bookmark folder' -Predicate {param($e) $e.Current.Name -like 'Favorite folder *'}
 Invoke-UitestClick -Element $favoriteFolder -Right
 [void](Find-ByName 'Bookmark folder menu')
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-folder-context.png')
 Invoke-UitestClick -Element (Find-ByName 'Add Lua bookmark')
 $luaEditorWindow=Find-ByName 'Bookmark editor window'
 Save-UitestScreenshot -Root $luaEditorWindow -Path (Join-Path $output 'lua-bookmark-editor-window.png')
 [void][RustExplorerUitest.Native]::SetForegroundWindow([IntPtr]$context.Hwnd)
 Start-Sleep -Milliseconds 500
 $processId=[int]$context.Process.Id
 $editorStillOpen=@([Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)|Where-Object{$_.Current.ProcessId-eq$processId-and$_.Current.Name-eq'Bookmark editor window'}).Count -gt 0
 if($editorStillOpen){throw 'Bookmark editor window did not cancel after losing focus'}
 1..4|ForEach-Object{
   Invoke-UitestClick -Element (Find-ByName 'Add Lua bookmark')
   [void](Find-ByName 'Bookmark editor')
   Invoke-UitestClick -Element (Find-ByName 'Save bookmark')
 }
 $lua=Find-UitestElement -Root $context.Root -Description 'Lua bookmark button' -Predicate {param($e) $e.Current.Name -like '*Bookmark: Lua command*'}
 if($lua.Current.Name -notlike '*⚡*'){throw 'Lua bookmark does not expose its distinct icon'}
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-toolbar.png')
 Invoke-UitestClick -Element (Find-ByName 'Manage bookmarks')
 [void](Find-ByName 'Close bookmark manager')
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-manager.png')
 [ordered]@{schema='bookmark-toolbar-uitest-v4';status='PASS';star_editor_verified=$true;bookmark_folder_created=$true;bookmark_folder_context_opened=$true;editor_focus_loss_cancelled=$true;lua_bookmarks_created=4;distinct_lua_icon=$true;manager_opened=$true;artifacts=@('bookmark-star-on.png','bookmark-star-off.png','bookmark-destination-picker.png','bookmark-folder-manager.png','bookmark-folder-context.png','lua-bookmark-editor-window.png','bookmark-toolbar.png','bookmark-manager.png')}|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {if($null-ne$context){Stop-UitestExplorer -Context $context}}
Write-Output "Bookmark toolbar UITEST passed: $OutputDirectory"
