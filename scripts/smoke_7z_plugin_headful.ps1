param(
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$PluginDll = 'sdk\fixtures\rust-7z-virtual-folder\target\x86_64-pc-windows-msvc\debug\rust_7z_virtual_folder.dll',
    [string]$OutputDirectory = 'target\7z-plugin-headful'
)
$ErrorActionPreference = 'Stop'
$rootPath = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
foreach($name in 'Executable','PluginDll','OutputDirectory') {
    $value = Get-Variable -Name $name -ValueOnly
    if(-not [IO.Path]::IsPathRooted($value)) {
        Set-Variable -Name $name -Value ([IO.Path]::GetFullPath((Join-Path $rootPath $value)))
    }
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixture = Join-Path $OutputDirectory 'fixture'
if(Test-Path -LiteralPath $fixture) { throw "UITEST fixture must be fresh: $fixture" }
New-Item -ItemType Directory -Force -Path $fixture | Out-Null

& cargo.exe build -p explorer-app --features uitest-support --locked --offline
if($LASTEXITCODE) { throw 'headful application build failed' }
& cargo.exe build --manifest-path (Join-Path $rootPath 'sdk\fixtures\rust-7z-virtual-folder\Cargo.toml') --target x86_64-pc-windows-msvc --locked --offline
if($LASTEXITCODE) { throw '7z plugin build failed' }
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$PluginDll = (Resolve-Path -LiteralPath $PluginDll).Path

$oldOutput = $env:SUPEREXPLORER_7Z_SMOKE_OUTPUT
$oldPassword = $env:SUPEREXPLORER_7Z_SMOKE_PASSWORD
try {
    $env:SUPEREXPLORER_7Z_SMOKE_OUTPUT = Join-Path $fixture 'plain.7z'
    Remove-Item Env:SUPEREXPLORER_7Z_SMOKE_PASSWORD -ErrorAction SilentlyContinue
    & cargo.exe test --manifest-path (Join-Path $rootPath 'sdk\fixtures\rust-7z-virtual-folder\Cargo.toml') real_archive_enumerates_reads_and_extracts --locked --offline
    if($LASTEXITCODE) { throw 'plain archive fixture generation failed' }
    $env:SUPEREXPLORER_7Z_SMOKE_OUTPUT = Join-Path $fixture 'encrypted.7z'
    $env:SUPEREXPLORER_7Z_SMOKE_PASSWORD = 'headful-secret-7z'
    & cargo.exe test --manifest-path (Join-Path $rootPath 'sdk\fixtures\rust-7z-virtual-folder\Cargo.toml') real_archive_enumerates_reads_and_extracts --locked --offline
    if($LASTEXITCODE) { throw 'encrypted archive fixture generation failed' }
} finally {
    $env:SUPEREXPLORER_7Z_SMOKE_OUTPUT = $oldOutput
    $env:SUPEREXPLORER_7Z_SMOKE_PASSWORD = $oldPassword
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if(-not ('SevenZHeadful.Native' -as [type])) { Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
namespace SevenZHeadful { public static class Native {
[StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
[DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
[DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int hgt,bool repaint);
} }
'@ }

function Find-Id($Root,[string]$Id) {
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,$Id))
}
function Find-Name($Root,[string]$Name) {
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
}
function Find-Prefix($Root,[string]$Prefix) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    if($all.Count -eq 0) { return $null }
    0..($all.Count-1)|ForEach-Object{$all.Item($_)}|Where-Object{$_.Current.Name-like"$Prefix*"}|Select-Object -First 1
}
function Find-RowPrefix($Root,[string]$Prefix) {
    $condition=[Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem)
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)
    if($all.Count -eq 0) { return $null }
    0..($all.Count-1)|ForEach-Object{$all.Item($_)}|Where-Object{$_.Current.Name-like"$Prefix*"}|Select-Object -First 1
}
function Find-Control($Root,$Type,[string]$Name) {
    $condition=[Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,$Type)
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)
    if($all.Count -eq 0) { return $null }
    0..($all.Count-1)|ForEach-Object{$all.Item($_)}|Where-Object{$_.Current.Name-eq$Name}|Select-Object -First 1
}
function Find-ControlPrefix($Root,$Type,[string]$Prefix) {
    $condition=[Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,$Type)
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)
    if($all.Count -eq 0) { return $null }
    0..($all.Count-1)|ForEach-Object{$all.Item($_)}|Where-Object{$_.Current.Name-like"$Prefix*"}|Select-Object -First 1
}
function Wait-Element([scriptblock]$Probe,[string]$Description,[int]$Seconds=10) {
    $deadline=[DateTime]::UtcNow.AddSeconds($Seconds)
    do { $value=&$Probe; if($null-ne$value){return $value}; Start-Sleep -Milliseconds 100 } while([DateTime]::UtcNow-lt$deadline)
    throw "Timed out waiting for $Description"
}
function Click-Element($Element,[switch]$Double) {
    $pattern=$null
    if(-not $Double -and $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke(); return
    }
    if(-not $Double -and $Element.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern,[ref]$pattern)) {
        ([Windows.Automation.SelectionItemPattern]$pattern).Select(); return
    }
    if(-not $Double -and $Element.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern,[ref]$pattern)) {
        ([Windows.Automation.TogglePattern]$pattern).Toggle(); return
    }
    try{$point=$Element.GetClickablePoint()}catch{throw "Element '$($Element.Current.Name)' ($($Element.Current.ControlType.ProgrammaticName)) has no supported action or clickable point"}
    [void][SevenZHeadful.Native]::SetCursorPos([int]$point.X,[int]$point.Y)
    [SevenZHeadful.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero); [SevenZHeadful.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
    if($Double) { Start-Sleep -Milliseconds 80; [SevenZHeadful.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero); [SevenZHeadful.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero) }
}
function Send-Key([byte]$Key,[switch]$Control,[switch]$Alt) {
    if($Control){[SevenZHeadful.Native]::keybd_event(0x11,0,0,[UIntPtr]::Zero)}
    if($Alt){[SevenZHeadful.Native]::keybd_event(0x12,0,0,[UIntPtr]::Zero)}
    [SevenZHeadful.Native]::keybd_event($Key,0,0,[UIntPtr]::Zero); [SevenZHeadful.Native]::keybd_event($Key,0,2,[UIntPtr]::Zero)
    if($Alt){[SevenZHeadful.Native]::keybd_event(0x12,0,2,[UIntPtr]::Zero)}
    if($Control){[SevenZHeadful.Native]::keybd_event(0x11,0,2,[UIntPtr]::Zero)}
}
function Activate-Row($Element) {
    $Element.SetFocus()
    Start-Sleep -Milliseconds 100
    [Windows.Forms.SendKeys]::SendWait('{ENTER}')
}
function Select-Row($Element) {
    $bounds=$Element.Current.BoundingRectangle
    [void][SevenZHeadful.Native]::SetCursorPos([int]($bounds.Left+[Math]::Min(24,$bounds.Width/2)),[int]($bounds.Top+$bounds.Height/2))
    [SevenZHeadful.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero); [SevenZHeadful.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
}
function Capture($Window,[string]$Path) {
    $rect=[SevenZHeadful.Native+Rect]::new(); if(-not[SevenZHeadful.Native]::GetWindowRect($Window,[ref]$rect)){throw 'GetWindowRect failed'}
    $bitmap=[Drawing.Bitmap]::new($rect.Right-$rect.Left,$rect.Bottom-$rect.Top)
    try{$graphics=[Drawing.Graphics]::FromImage($bitmap);try{$dc=$graphics.GetHdc();try{if(-not[SevenZHeadful.Native]::PrintWindow($Window,$dc,2)){throw 'PrintWindow failed'}}finally{$graphics.ReleaseHdc($dc)}}finally{$graphics.Dispose()};$bitmap.Save($Path,[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
}

$profile=Join-Path $OutputDirectory 'profile'; $local=Join-Path $profile 'LocalAppData'; $roaming=Join-Path $profile 'AppData'; $state=Join-Path $profile 'ExtensionState'
New-Item -ItemType Directory -Force -Path $local,$roaming,$state | Out-Null
$old=@{}; foreach($name in 'LOCALAPPDATA','APPDATA','EXPLORER_UITEST_EXTENSION_STATE_ROOT','EXPLORER_INITIAL_PATH','EXPLORER_AUTO_CLOSE_MS','RUST_LOG'){$old[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
$process=$null
try {
    $env:LOCALAPPDATA=$local; $env:APPDATA=$roaming; $env:EXPLORER_UITEST_EXTENSION_STATE_ROOT=$state; $env:EXPLORER_INITIAL_PATH=$fixture; $env:EXPLORER_AUTO_CLOSE_MS='120000'; $env:RUST_LOG='explorer_shell_win::drag_drop=info'
    $process=Start-Process -FilePath $Executable -ArgumentList @('--plugin-dll',$PluginDll) -RedirectStandardOutput (Join-Path $OutputDirectory 'app.stdout.log') -RedirectStandardError (Join-Path $OutputDirectory 'app.stderr.log') -PassThru
    $deadline=[DateTime]::UtcNow.AddSeconds(30); do{$process.Refresh();if($process.MainWindowHandle-ne0){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow-lt$deadline)
    if($process.MainWindowHandle-eq0){throw 'application window did not appear'}
    [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle)
    $ui=[Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $previewPaneName=-join ([char]0x9810,[char]0x89BD,[char]0x7A97,[char]0x683C)

    $plain=Wait-Element { Find-RowPrefix $ui 'plain.7z' } 'plain archive row'
    Activate-Row $plain
    $nested=Wait-Element { Find-RowPrefix $ui 'nested' } 'archive nested folder'
    if($null-eq(Find-Prefix $ui 'plain.7z')) { $null=Wait-Element { Find-Prefix $ui 'plain.7z' } 'archive breadcrumb' }
    Activate-Row $nested
    $hello=Wait-Element { Find-RowPrefix $ui 'hello.txt' } 'archive file'

    Click-Element $hello
    [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle)
    Send-Key 0x50 -Alt
    Click-Element (Wait-Element { Find-RowPrefix $ui 'hello.txt' } 'archive preview file')
    $null=Wait-Element { Find-Name $ui $previewPaneName } 'archive preview pane'
    Capture $process.MainWindowHandle (Join-Path $OutputDirectory 'archive-preview.png')
    [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle)
    Send-Key 0x50 -Alt

    # Verify history before mutations refresh the current virtual location.
    $back=Wait-Element { Find-Control $ui ([Windows.Automation.ControlType]::Button) 'Back' } 'Back command'
    Click-Element $back
    $null=Wait-Element { Find-RowPrefix $ui 'nested' } 'archive root after Back'
    $forward=Wait-Element { Find-Control $ui ([Windows.Automation.ControlType]::Button) 'Forward' } 'Forward command'
    Click-Element $forward
    $null=Wait-Element { Find-RowPrefix $ui 'hello.txt' } 'nested folder after Forward'

    $renameSource=Wait-Element { Find-RowPrefix $ui 'hello.txt' } 'selected archive file'
    Select-Row $renameSource
    $renameSource=Wait-Element {
        $row=Find-RowPrefix $ui 'hello.txt'; $selectionPattern=$null
        if($null-ne$row -and $row.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern,[ref]$selectionPattern) -and ([Windows.Automation.SelectionItemPattern]$selectionPattern).Current.IsSelected) { $row }
    } 'selected archive file state'
    [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle)
    Send-Key 0x71
    Start-Sleep -Milliseconds 300
    $renameEditor=Wait-Element { Find-ControlPrefix $ui ([Windows.Automation.ControlType]::Edit) 'Rename ' } 'archive rename editor'
    $renameEditor.SetFocus()
    [Windows.Forms.SendKeys]::SendWait('^a'); [Windows.Forms.SendKeys]::SendWait('renamed.txt'); [Windows.Forms.SendKeys]::SendWait('{ENTER}')
    Start-Sleep -Seconds 2
    $renamed=Wait-Element { Find-RowPrefix $ui 'renamed.txt' } 'renamed archive file' 15
    Select-Row $renamed; [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle); Send-Key 0x2E
    $deadline=[DateTime]::UtcNow.AddSeconds(15); do{if($null-eq(Find-Prefix $ui 'renamed.txt')){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow-lt$deadline)
    if($null-ne(Find-Prefix $ui 'renamed.txt')){throw 'archive delete did not complete'}
    $moreName=-join ([char]0x5176,[char]0x5B83)
    $undoName=-join ([char]0x5FA9,[char]0x539F)
    $more=Wait-Element { Find-Control $ui ([Windows.Automation.ControlType]::Button) $moreName } 'More command'; Click-Element $more
    $undo=Wait-Element { Find-Control $ui ([Windows.Automation.ControlType]::MenuItem) $undoName } 'Undo command'; Click-Element $undo
    $null=Wait-Element { Find-Prefix $ui 'renamed.txt' } 'whole-container undo result' 15

    $dragDestination=Join-Path $fixture 'drag-out'; New-Item -ItemType Directory -Path $dragDestination | Out-Null
    $existingWindows=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
    if($existingWindows.Count-gt0){
        0..($existingWindows.Count-1) | ForEach-Object { $existingWindows.Item($_) } |
            Where-Object { $_.Current.Name -like '*drag-out*' } | ForEach-Object {
                $pattern=$null
                if($_.TryGetCurrentPattern([Windows.Automation.WindowPattern]::Pattern,[ref]$pattern)){([Windows.Automation.WindowPattern]$pattern).Close()}
            }
        Start-Sleep -Milliseconds 500
    }
    Start-Process explorer.exe -ArgumentList $dragDestination | Out-Null
    $explorerWindow=Wait-Element {
        $windows=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
        0..($windows.Count-1)|ForEach-Object{$windows.Item($_)}|Where-Object{$_.Current.Name-like'*drag-out*'}|Select-Object -First 1
    } 'drag-out File Explorer window' 15
    [void][SevenZHeadful.Native]::MoveWindow($process.MainWindowHandle,0,0,950,850,$true)
    [void][SevenZHeadful.Native]::MoveWindow([IntPtr]$explorerWindow.Current.NativeWindowHandle,960,0,850,850,$true)
    [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle); Start-Sleep -Milliseconds 250
    $dragSource=Wait-Element { Find-RowPrefix $ui 'renamed.txt' } 'virtual drag source'
    $sourceBounds=$dragSource.Current.BoundingRectangle
    $sourcePoint=[Drawing.Point]::new([int]($sourceBounds.Left+24),[int]($sourceBounds.Top+$sourceBounds.Height/2)); $targetBounds=$explorerWindow.Current.BoundingRectangle
    # Aim inside Explorer's file pane. The navigation tree may consume more
    # than half the window width, so the window centre is not a valid target.
    $targetX=[int]($targetBounds.Left+($targetBounds.Width*0.82)); $targetY=[int]($targetBounds.Top+($targetBounds.Height*0.62))
    [void][SevenZHeadful.Native]::SetCursorPos([int]$sourcePoint.X,[int]$sourcePoint.Y)
    [SevenZHeadful.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 120
    # Cross the GPUI drag threshold, then keep the button held while the
    # broker materializes the virtual item and enters the Shell OLE loop.
    [void][SevenZHeadful.Native]::SetCursorPos([int]$sourcePoint.X+18,[int]$sourcePoint.Y)
    $materializedRoot=Join-Path $env:TEMP 'SuperExplorer'
    $materialized=Wait-Element {
        Get-ChildItem -LiteralPath $materializedRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "virtual-drag-$($process.Id)-*" } |
            ForEach-Object { Get-Item -LiteralPath (Join-Path $_.FullName 'renamed.txt') -ErrorAction SilentlyContinue } |
            Select-Object -First 1
    } 'virtual drag materialization' 15
    if([IO.File]::ReadAllText($materialized.FullName)-ne'hello archive'){throw 'materialized virtual drag content mismatch'}
    Start-Sleep -Milliseconds 350
    [void][SevenZHeadful.Native]::SetForegroundWindow([IntPtr]$explorerWindow.Current.NativeWindowHandle)
    foreach($step in 1..12){$x=[int]($sourcePoint.X+(($targetX-$sourcePoint.X)*$step/12));$y=[int]($sourcePoint.Y+(($targetY-$sourcePoint.Y)*$step/12));[void][SevenZHeadful.Native]::SetCursorPos($x,$y);Start-Sleep -Milliseconds 50}
    # Give Explorer a real DragEnter/DragOver interval before the terminal
    # button transition; releasing on the first target mouse move is flaky.
    [void][SevenZHeadful.Native]::SetCursorPos($targetX+2,$targetY)
    Start-Sleep -Milliseconds 1000
    [SevenZHeadful.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
    $null=Wait-Element { if(Test-Path -LiteralPath (Join-Path $dragDestination 'renamed.txt')){Get-Item (Join-Path $dragDestination 'renamed.txt')} } 'materialized drag-out file' 20
    if([IO.File]::ReadAllText((Join-Path $dragDestination 'renamed.txt'))-ne'hello archive'){throw 'drag-out content mismatch'}
    $windowPattern=$null; if($explorerWindow.TryGetCurrentPattern([Windows.Automation.WindowPattern]::Pattern,[ref]$windowPattern)){([Windows.Automation.WindowPattern]$windowPattern).Close()}
    [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle)
    (Wait-Element { Find-RowPrefix $ui 'renamed.txt' } 'virtual row after drag-out').SetFocus()
    Start-Sleep -Milliseconds 250

    [void]$process.CloseMainWindow()
    if(-not$process.WaitForExit(5000)){$process.Kill();$process.WaitForExit()}
    $process=Start-Process -FilePath $Executable -ArgumentList @('--plugin-dll',$PluginDll) -RedirectStandardOutput (Join-Path $OutputDirectory 'app-relaunch.stdout.log') -RedirectStandardError (Join-Path $OutputDirectory 'app-relaunch.stderr.log') -PassThru
    $deadline=[DateTime]::UtcNow.AddSeconds(30); do{$process.Refresh();if($process.MainWindowHandle-ne0){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow-lt$deadline)
    if($process.MainWindowHandle-eq0){throw 'relaunched application window did not appear'}
    [void][SevenZHeadful.Native]::SetForegroundWindow($process.MainWindowHandle)
    $ui=[Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $encrypted=Wait-Element { Find-RowPrefix $ui 'encrypted.7z' } 'encrypted archive row'; Activate-Row $encrypted

    $credential=Wait-Element {
        $windows=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
        0..($windows.Count-1)|ForEach-Object{$windows.Item($_)}|Where-Object{$_.Current.Name-like'*archive password*'}|Select-Object -First 1
    } 'archive password dialog'
    [void][SevenZHeadful.Native]::SetForegroundWindow($credential.Current.NativeWindowHandle)
    $credentialEdits=$credential.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Edit))
    $passwordEdit=$null
    if($credentialEdits.Count-gt0){
        0..($credentialEdits.Count-1)|ForEach-Object{$credentialEdits.Item($_)}|Where-Object{$_.Current.IsPassword}|Select-Object -First 1|ForEach-Object{$passwordEdit=$_}
        if($null-eq$passwordEdit){$passwordEdit=$credentialEdits.Item($credentialEdits.Count-1)}
    }
    if($null-ne$passwordEdit){$passwordEdit.SetFocus()}else{[Windows.Forms.SendKeys]::SendWait('{TAB}')}
    [Windows.Forms.SendKeys]::SendWait('headful-secret-7z'); [Windows.Forms.SendKeys]::SendWait('{ENTER}')
    $null=Wait-Element { Find-Prefix $ui 'nested' } 'encrypted archive contents' 15
    Capture $process.MainWindowHandle (Join-Path $OutputDirectory 'encrypted-archive.png')

    $optionsName=-join ([char]0x9078,[char]0x9805)
    $more=Wait-Element { Find-Control $ui ([Windows.Automation.ControlType]::Button) $moreName } 'More command for disable'; Click-Element $more
    $options=Wait-Element { Find-Control $ui ([Windows.Automation.ControlType]::MenuItem) $optionsName } 'Folder Options command'; Click-Element $options
    $extensions=Wait-Element { $value=Find-Id $ui 'folder-options-extensions-tab'; if($null-eq$value){$value=Find-Control $ui ([Windows.Automation.ControlType]::TabItem) 'Extensions'}; if($null-eq$value){$value=Find-Control $ui ([Windows.Automation.ControlType]::Button) 'Extensions'}; $value } 'Extensions tab'; Click-Element $extensions
    $extensionsList=Wait-Element { Find-Control $ui ([Windows.Automation.ControlType]::List) 'Extensions' } 'Extensions list'
    $listBounds=$extensionsList.Current.BoundingRectangle
    $scrollPattern=$null
    if($extensionsList.TryGetCurrentPattern([Windows.Automation.ScrollPattern]::Pattern,[ref]$scrollPattern)){
        foreach($step in 1..4){([Windows.Automation.ScrollPattern]$scrollPattern).Scroll([Windows.Automation.ScrollAmount]::NoAmount,[Windows.Automation.ScrollAmount]::LargeIncrement);Start-Sleep -Milliseconds 80}
    }
    $windowRect=[SevenZHeadful.Native+Rect]::new(); if(-not[SevenZHeadful.Native]::GetWindowRect($process.MainWindowHandle,[ref]$windowRect)){throw 'GetWindowRect failed for Extensions list'}
    $rootBounds=$ui.Current.BoundingRectangle
    $sx=($windowRect.Right-$windowRect.Left)/$rootBounds.Width; $sy=($windowRect.Bottom-$windowRect.Top)/$rootBounds.Height
    $listX=[int]($windowRect.Left+(($listBounds.Left+$listBounds.Width/2)-$rootBounds.Left)*$sx)
    $listY=[int]($windowRect.Top+(($listBounds.Top+$listBounds.Height/2)-$rootBounds.Top)*$sy)
    [void][SevenZHeadful.Native]::SetCursorPos($listX,$listY)
    $wheelDown=[BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]-120),0)
    foreach($step in 1..8){[SevenZHeadful.Native]::mouse_event(0x0800,0,0,$wheelDown,[UIntPtr]::Zero);Start-Sleep -Milliseconds 80}
    Start-Sleep -Milliseconds 250
    $toggle=Wait-Element {
        $boxes=$ui.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::CheckBox))
        0..($boxes.Count-1)|ForEach-Object{$boxes.Item($_)}|Where-Object{$_.Current.Name-like'*7-Zip virtual folder*'}|Select-Object -First 1
    } '7-Zip virtual folder toggle'
    Click-Element $toggle
    $applyName=-join ([char]0x5957,[char]0x7528)
    $apply=Wait-Element { $value=Find-Id $ui 'folder-options-apply'; if($null-eq$value){$value=Find-Control $ui ([Windows.Automation.ControlType]::Button) 'Apply'}; if($null-eq$value){$value=Find-Name $ui $applyName}; $value } 'Folder Options Apply'; Click-Element $apply
    $okName=-join ([char]0x78BA,[char]0x5B9A)
    $ok=Wait-Element { $value=Find-Id $ui 'folder-options-ok'; if($null-eq$value){$value=Find-Control $ui ([Windows.Automation.ControlType]::Button) 'OK'}; if($null-eq$value){$value=Find-Name $ui $okName}; $value } 'Folder Options OK'; Click-Element $ok
    $null=Wait-Element { Find-Prefix $ui 'encrypted.7z' } 'filesystem fallback after provider disable' 15

    $passwordLeaked=$false
    foreach($profileFile in Get-ChildItem -LiteralPath $OutputDirectory -File -Recurse -ErrorAction SilentlyContinue){
        $stream=$null
        try{
            $stream=[IO.FileStream]::new($profileFile.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite-bor[IO.FileShare]::Delete)
            $bytes=[byte[]]::new([int]$stream.Length); [void]$stream.Read($bytes,0,$bytes.Length)
            if([Text.Encoding]::UTF8.GetString($bytes).Contains('headful-secret-7z')){$passwordLeaked=$true;break}
        }catch{}finally{if($null-ne$stream){$stream.Dispose()}}
    }
    if($passwordLeaked){throw 'archive password leaked into profile state'}
    [ordered]@{schema_version=1;passed=$true;navigation=$true;breadcrumb=$true;history=$true;preview=$true;drag_out=$true;rename=$true;delete=$true;whole_container_undo=$true;encrypted_prompt=$true;password_persisted=$false;disable_redirected=$true;artifacts=@('archive-preview.png','encrypted-archive.png')} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
} finally {
    if($null-ne$process-and-not$process.HasExited){$process.Kill();$process.WaitForExit()}
    foreach($name in $old.Keys){[Environment]::SetEnvironmentVariable($name,$old[$name],'Process')}
}
