param([ValidateSet('debug','release')][string]$Profile='debug',[Parameter(Mandatory)][string]$OutputDirectory,[switch]$SkipBuild)
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
function Find-ByName([string]$Name){Find-UitestElement -Root $context.Root -Description $Name -Predicate {param($e) $e.Current.Name -eq $Name}}
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
 Invoke-AddBookmarkMenu 'Folder bookmark'
 $addStar=Find-ByName 'Add current folder to bookmarks'
 $folderBookmark=Find-UitestElement -Root $context.Root -Description 'Folder bookmark button' -Predicate {param($e) $e.Current.Name -like '*Bookmark: Folder bookmark*'}
 if($addStar.Current.BoundingRectangle.Left -ge $folderBookmark.Current.BoundingRectangle.Left){throw 'Bookmark star is not fixed at the left edge of the toolbar'}
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-star-off.png')
 Invoke-UitestClick -Element $addStar
 [void](Find-ByName 'Remove current folder from bookmarks')
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-star-on.png')
 Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'File bookmark.txt')
 [void](Find-ByName 'Remove current folder from bookmarks')
 Invoke-UitestClick -Element (Find-ByName 'Remove current folder from bookmarks')
 [void](Find-ByName 'Add current folder to bookmarks')
 Invoke-UitestClick -Element (Find-ByName 'Add current folder to bookmarks')
 1..12|ForEach-Object{
   Invoke-UitestClick -Element (Find-ByName 'Add Lua bookmark')
   [void](Find-ByName 'Bookmark editor')
   Invoke-UitestClick -Element (Find-ByName 'Save bookmark')
 }
 $lua=Find-UitestElement -Root $context.Root -Description 'Lua bookmark button' -Predicate {param($e) $e.Current.Name -like '*Bookmark: Lua command*'}
 if($lua.Current.Name -notlike '*⚡*'){throw 'Lua bookmark does not expose its distinct icon'}
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-toolbar.png')
 $more=Find-UitestElement -Root $context.Root -Description 'More Bookmarks' -Predicate {param($e) $e.Current.Name -like 'More Bookmarks,*'}
 Invoke-UitestClick -Element $more
 [void](Find-UitestElement -Root $context.Root -Description 'overflow Lua bookmark' -Predicate {param($e) $e.Current.Name -like '*Bookmark: Lua command*' -and $e.Current.BoundingRectangle.Top -gt $more.Current.BoundingRectangle.Bottom})
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-overflow.png')
 Invoke-UitestClick -Element $more
 Invoke-UitestClick -Element (Find-ByName 'Manage bookmarks')
 [void](Find-ByName 'Close bookmark manager')
 Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bookmark-manager.png')
 [ordered]@{schema='bookmark-toolbar-uitest-v2';status='PASS';native_context_bookmarks=@('Folder bookmark');star_toggle_bookmarks=@('Folder bookmark','File bookmark.txt');lua_bookmarks_created=12;distinct_lua_icon=$true;overflow_opened=$true;manager_opened=$true;artifacts=@('bookmark-context-menu.png','bookmark-star-on.png','bookmark-star-off.png','bookmark-toolbar.png','bookmark-overflow.png','bookmark-manager.png')}|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {if($null-ne$context){Stop-UitestExplorer -Context $context}}
Write-Output "Bookmark toolbar UITEST passed: $OutputDirectory"
