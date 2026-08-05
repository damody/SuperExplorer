param(
    [Parameter(Mandatory=$true)][string]$PluginRoot,
    [Parameter(Mandatory=$true)][string]$PluginDll,
    [Parameter(Mandatory=$true)][string]$ExpectedContribution,
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$InitialPath = '.',
    [string]$OutputDirectory = 'target\new-plugin-headful'
)
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
foreach($name in 'PluginRoot','PluginDll','Executable','InitialPath','OutputDirectory'){
    $value=Get-Variable -Name $name -ValueOnly
    if(-not [IO.Path]::IsPathRooted($value)){Set-Variable -Name $name -Value ([IO.Path]::GetFullPath((Join-Path $workspace $value)))}
}
& cargo.exe test --manifest-path (Join-Path $PluginRoot 'Cargo.toml') --locked --offline
if($LASTEXITCODE -ne 0){throw 'example functional tests failed'}
& cargo.exe build -p explorer-app --features uitest-support --locked --offline
if($LASTEXITCODE -ne 0){throw 'headful application build failed'}
& cargo.exe build --manifest-path (Join-Path $PluginRoot 'Cargo.toml') --target x86_64-pc-windows-msvc --locked --offline
if($LASTEXITCODE -ne 0){throw 'example DLL build failed'}
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$PluginDll=(Resolve-Path -LiteralPath $PluginDll).Path
$InitialPath=(Resolve-Path -LiteralPath $InitialPath).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory|Out-Null
$profile=Join-Path $OutputDirectory 'profile'; $local=Join-Path $profile 'LocalAppData'; $roaming=Join-Path $profile 'AppData'; $state=Join-Path $profile 'ExtensionState'
New-Item -ItemType Directory -Force -Path $local,$roaming,$state|Out-Null
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if(-not ('NewPluginSmoke.Native' -as [type])){Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
namespace NewPluginSmoke { public static class Native {
[StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
[DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
} }
'@}
$old=@{}; foreach($name in 'LOCALAPPDATA','APPDATA','EXPLORER_UITEST_EXTENSION_STATE_ROOT','EXPLORER_INITIAL_PATH','EXPLORER_AUTO_CLOSE_MS'){$old[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
$process=$null
try{
    $env:LOCALAPPDATA=$local; $env:APPDATA=$roaming; $env:EXPLORER_UITEST_EXTENSION_STATE_ROOT=$state; $env:EXPLORER_INITIAL_PATH=$InitialPath; $env:EXPLORER_AUTO_CLOSE_MS='60000'
    $process=Start-Process -FilePath $Executable -ArgumentList @('--plugin-dll',$PluginDll) -PassThru
    $deadline=[DateTime]::UtcNow.AddSeconds(30); do{$process.Refresh(); if($process.MainWindowHandle -ne 0){break}; Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $deadline)
    if($process.MainWindowHandle -eq 0){throw 'application window did not appear'}
    [void][NewPluginSmoke.Native]::SetForegroundWindow($process.MainWindowHandle)
    $root=[Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $button=$root.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'command-extensions-menu'))
    if($null -eq $button){
        $zhExtensions=([string][char]0x64F4)+([char]0x5145)+([char]0x529F)+([char]0x80FD)
        foreach($label in @($zhExtensions,'Extensions')){
            $button=$root.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$label))
            if($null -ne $button){break}
        }
    }
    if($null -eq $button){
        $visible=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        for($i=0;$i -lt $visible.Count;$i++){ $item=$visible.Item($i); Write-Host ("UIA {0} | {1} | {2}" -f $item.Current.AutomationId,$item.Current.Name,$item.Current.ControlType.ProgrammaticName) }
        throw 'production Extensions button was not exposed to UIA'
    }
    $invoke=$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern); $invoke.Invoke()
    $summary=$null; $summaryDeadline=[DateTime]::UtcNow.AddSeconds(4)
    do {
        foreach($searchRoot in @($root)) {
            $all=$searchRoot.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
            for($i=0;$i -lt $all.Count;$i++){if($all.Item($i).Current.Name -like "*$ExpectedContribution*"){$summary=$all.Item($i);break}}
            if($null -ne $summary){break}
        }
        if($null -eq $summary) {
            $topLevel=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
            for($windowIndex=0;$windowIndex -lt $topLevel.Count;$windowIndex++) {
                $candidate=$topLevel.Item($windowIndex)
                if($candidate.Current.ProcessId -ne $process.Id){continue}
                $all=$candidate.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
                for($i=0;$i -lt $all.Count;$i++){if($all.Item($i).Current.Name -like "*$ExpectedContribution*"){$summary=$all.Item($i);break}}
                if($null -ne $summary){break}
            }
        }
        if($null -eq $summary){Start-Sleep -Milliseconds 100}
    } while($null -eq $summary -and [DateTime]::UtcNow -lt $summaryDeadline)
    if($null -eq $summary){
        $dump=New-Object Collections.Generic.List[string]
        $topLevel=[Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
        for($windowIndex=0;$windowIndex -lt $topLevel.Count;$windowIndex++) {
            $candidate=$topLevel.Item($windowIndex); if($candidate.Current.ProcessId -ne $process.Id){continue}
            $dump.Add(("WINDOW {0} | {1}" -f $candidate.Current.AutomationId,$candidate.Current.Name))
            $all=$candidate.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
            for($i=0;$i -lt $all.Count;$i++){ $item=$all.Item($i); $dump.Add(("UIA {0} | {1} | {2}" -f $item.Current.AutomationId,$item.Current.Name,$item.Current.ControlType.ProgrammaticName)) }
        }
        [IO.File]::WriteAllLines((Join-Path $OutputDirectory 'uia-after-extensions.txt'),$dump,[Text.UTF8Encoding]::new($false))
        throw "loaded contribution was absent from production Extensions UI: $ExpectedContribution"
    }
    $rect=[NewPluginSmoke.Native+Rect]::new(); if(-not [NewPluginSmoke.Native]::GetWindowRect($process.MainWindowHandle,[ref]$rect)){throw 'GetWindowRect failed'}
    $bitmap=[Drawing.Bitmap]::new($rect.Right-$rect.Left,$rect.Bottom-$rect.Top)
    try{$graphics=[Drawing.Graphics]::FromImage($bitmap);try{$dc=$graphics.GetHdc();try{if(-not [NewPluginSmoke.Native]::PrintWindow($process.MainWindowHandle,$dc,2)){throw 'PrintWindow failed'}}finally{$graphics.ReleaseHdc($dc)}}finally{$graphics.Dispose()};$bitmap.Save((Join-Path $OutputDirectory 'extensions-contribution.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
    [ordered]@{schema_version=1;passed=$true;plugin_dll=$PluginDll;expected_contribution=$ExpectedContribution;summary=$summary.Current.Name}|ConvertTo-Json -Depth 4|Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
}finally{
    if($null -ne $process -and -not $process.HasExited){$process.Kill();$process.WaitForExit()}
    foreach($name in $old.Keys){[Environment]::SetEnvironmentVariable($name,$old[$name],'Process')}
}
