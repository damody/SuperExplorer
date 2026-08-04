param(
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$PluginDll = 'sdk\fixtures\rust-tokei-code-lines-column\target\x86_64-pc-windows-msvc\debug\rust_tokei_code_lines_column.dll',
    [string]$InitialPath = 'sdk\fixtures\rust-tokei-code-lines-column\samples',
    [string]$OutputDirectory = 'target\tokei-headful-smoke',
    [switch]$LockOwnerMode
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$pluginRoot = Join-Path $workspace $(if ($LockOwnerMode) { 'sdk\fixtures\rust-lock-owner-column' } else { 'sdk\fixtures\rust-tokei-code-lines-column' })
& cargo.exe test --manifest-path (Join-Path $pluginRoot 'Cargo.toml') --locked --offline
if ($LASTEXITCODE -ne 0) { throw "tokei plugin cargo test failed ($LASTEXITCODE)" }
& cargo.exe build --manifest-path (Join-Path $pluginRoot 'Cargo.toml') --target x86_64-pc-windows-msvc --locked --offline
if ($LASTEXITCODE -ne 0) { throw "tokei plugin cargo build failed ($LASTEXITCODE)" }
foreach ($name in 'Executable','PluginDll','InitialPath','OutputDirectory') {
    $value = Get-Variable -Name $name -ValueOnly
    if (-not [IO.Path]::IsPathRooted($value)) { Set-Variable -Name $name -Value ([IO.Path]::GetFullPath((Join-Path $workspace $value))) }
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$PluginDll = (Resolve-Path -LiteralPath $PluginDll).Path
$InitialPath = (Resolve-Path -LiteralPath $InitialPath).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$profileRoot=Join-Path $OutputDirectory 'profile'
$localAppData=Join-Path $profileRoot 'LocalAppData'
$roamingAppData=Join-Path $profileRoot 'AppData'
$extensionState=Join-Path $profileRoot 'ExtensionState'
New-Item -ItemType Directory -Force -Path $localAppData,$roamingAppData,$extensionState | Out-Null
$isolatedLaunchDirectory=Join-Path $OutputDirectory 'isolated-app'
New-Item -ItemType Directory -Force -Path $isolatedLaunchDirectory | Out-Null
$isolatedExecutable=Join-Path $isolatedLaunchDirectory (Split-Path -Leaf $Executable)
[IO.File]::Copy($Executable,$isolatedExecutable,$true)
$Executable=$isolatedExecutable

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
if (-not ('TokeiHeadfulSmoke.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
namespace TokeiHeadfulSmoke {
  public static class Native {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
  }
  public sealed class JobProcessObserver : IDisposable {
    const uint CREATE_SUSPENDED=0x00000004, DETACHED_PROCESS=0x00000008, JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO=4, JOB_OBJECT_MSG_NEW_PROCESS=6;
    const int JobObjectAssociateCompletionPortInformation=7;
    const uint PROCESS_QUERY_LIMITED_INFORMATION=0x1000;
    [StructLayout(LayoutKind.Sequential)] struct Association { public IntPtr Key; public IntPtr Port; }
    [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct STARTUPINFO { public uint cb; public string reserved,desktop,title; public uint x,y,xSize,ySize,xChars,yChars,fill,flags; public ushort show; public ushort reserved2; public IntPtr reservedPtr,stdInput,stdOutput,stdError; }
    [StructLayout(LayoutKind.Sequential)] struct PROCESS_INFORMATION { public IntPtr process,thread; public uint processId,threadId; }
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern IntPtr CreateJobObject(IntPtr attributes,string name);
    [DllImport("kernel32.dll")] static extern bool AssignProcessToJobObject(IntPtr job,IntPtr process);
    [DllImport("kernel32.dll")] static extern IntPtr CreateIoCompletionPort(IntPtr file,IntPtr existing,UIntPtr key,uint threads);
    [DllImport("kernel32.dll")] static extern bool SetInformationJobObject(IntPtr job,int infoClass,ref Association info,uint length);
    [DllImport("kernel32.dll")] static extern bool GetQueuedCompletionStatus(IntPtr port,out uint message,out UIntPtr key,out IntPtr value,uint milliseconds);
    [DllImport("kernel32.dll")] static extern bool PostQueuedCompletionStatus(IntPtr port,uint message,UIntPtr key,IntPtr value);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcess(string application,StringBuilder command,IntPtr processAttributes,IntPtr threadAttributes,bool inheritHandles,uint flags,IntPtr environment,string directory,ref STARTUPINFO startup,out PROCESS_INFORMATION information);
    [DllImport("kernel32.dll")] static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll")] static extern bool TerminateProcess(IntPtr process,uint code);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access,bool inherit,uint processId);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder path,ref uint size);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    readonly IntPtr job,port;
    readonly uint rootPid;
    readonly List<string> paths=new List<string>();
    readonly ManualResetEventSlim activeZero=new ManualResetEventSlim(false);
    readonly Task monitor;
    bool stopped;
    public Process Process { get; private set; }
    JobProcessObserver(IntPtr jobHandle,IntPtr completionPort,uint processId) {
      job=jobHandle; port=completionPort; rootPid=processId;
      Process=Process.GetProcessById((int)processId);
      monitor=Task.Run((Action)Observe);
    }
    public static JobProcessObserver StartSuspended(string executable,string arguments,string directory) {
      IntPtr job=CreateJobObject(IntPtr.Zero,null);
      IntPtr port=CreateIoCompletionPort(new IntPtr(-1),IntPtr.Zero,UIntPtr.Zero,1);
      if(job==IntPtr.Zero || port==IntPtr.Zero) throw new InvalidOperationException("Could not create process-observation job");
      var association=new Association { Key=IntPtr.Zero,Port=port };
      if(!SetInformationJobObject(job,JobObjectAssociateCompletionPortInformation,ref association,(uint)Marshal.SizeOf(typeof(Association)))) throw new InvalidOperationException("Could not attach process-observation completion port");
      var startup=new STARTUPINFO(); startup.cb=(uint)Marshal.SizeOf(typeof(STARTUPINFO));
      PROCESS_INFORMATION information;
      var command=new StringBuilder("\""+executable+"\" "+arguments);
      if(!CreateProcess(executable,command,IntPtr.Zero,IntPtr.Zero,false,CREATE_SUSPENDED|DETACHED_PROCESS,IntPtr.Zero,directory,ref startup,out information)) throw new InvalidOperationException("Could not create suspended app: "+Marshal.GetLastWin32Error());
      try {
        if(!AssignProcessToJobObject(job,information.process)) throw new InvalidOperationException("Could not assign suspended app to process-observation job");
        var observer=new JobProcessObserver(job,port,information.processId);
        if(ResumeThread(information.thread)==UInt32.MaxValue) throw new InvalidOperationException("Could not resume observed app");
        return observer;
      } catch { TerminateProcess(information.process,1); CloseHandle(job); CloseHandle(port); throw; }
      finally { CloseHandle(information.thread); CloseHandle(information.process); }
    }
    void Observe() {
      while(true) {
        uint message; UIntPtr key; IntPtr value;
        if(!GetQueuedCompletionStatus(port,out message,out key,out value,250)) continue;
        if(message==UInt32.MaxValue) return;
        if(message==JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO) { activeZero.Set(); return; }
        uint pid=unchecked((uint)value.ToInt64());
        if(message==JOB_OBJECT_MSG_NEW_PROCESS && pid!=rootPid) {
          string path=ProcessPath(pid);
          lock(paths) paths.Add(String.IsNullOrEmpty(path) ? "<unresolved>:"+pid : path+":"+pid);
        }
      }
    }
    static string ProcessPath(uint pid) {
      IntPtr process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid);
      if(process==IntPtr.Zero) return null;
      try { var text=new StringBuilder(32768); uint length=(uint)text.Capacity; return QueryFullProcessImageName(process,0,text,ref length) ? text.ToString() : null; }
      finally { CloseHandle(process); }
    }
    public string[] WaitForActiveProcessZero(int milliseconds) {
      if(!activeZero.Wait(milliseconds)) throw new TimeoutException("Observed process job did not become empty");
      monitor.Wait(milliseconds); stopped=true;
      lock(paths) return paths.ToArray();
    }
    public void Dispose() { if(!stopped){PostQueuedCompletionStatus(port,UInt32.MaxValue,UIntPtr.Zero,IntPtr.Zero);monitor.Wait(2000);} activeZero.Dispose(); CloseHandle(port); CloseHandle(job); }
  }
}
'@
}

function Capture-Window([IntPtr]$Window, [string]$Path) {
    [void][TokeiHeadfulSmoke.Native]::DwmFlush()
    $rect = [TokeiHeadfulSmoke.Native+Rect]::new()
    if (-not [TokeiHeadfulSmoke.Native]::GetWindowRect($Window, [ref]$rect)) { throw 'GetWindowRect failed' }
    $bitmap = [Drawing.Bitmap]::new($rect.Right-$rect.Left, $rect.Bottom-$rect.Top)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try { if (-not [TokeiHeadfulSmoke.Native]::PrintWindow($Window,$dc,2)) { throw 'PrintWindow failed' } }
            finally { $graphics.ReleaseHdc($dc) }
        } finally { $graphics.Dispose() }
        $bitmap.Save($Path,[Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
}

function Find-Name($Root,[string]$Name) {
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name))
}

function Find-NamePrefix($Root,[string]$Prefix) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
        Where-Object { $_.Current.Name -like "$Prefix*" } | Select-Object -First 1
}

function Code-Line-Values($Root) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    @(0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
        Where-Object { $_.Current.Name -match '^Code lines: (\d+)' } |
        ForEach-Object { [int]([regex]::Match($_.Current.Name,'^Code lines: (\d+)').Groups[1].Value) })
}

function Assert-ProportionalBars([string]$Path) {
    $bitmap=[Drawing.Bitmap]::new($Path)
    try {
        $widths=@()
        $x0=[int]($bitmap.Width*0.70); $x1=[int]($bitmap.Width*0.90)
        $y0=[int]($bitmap.Height*0.18); $y1=[int]($bitmap.Height*0.72)
        for ($y=$y0; $y -lt $y1; $y++) {
            $count=0
            for ($x=$x0; $x -lt $x1; $x++) {
                $pixel=$bitmap.GetPixel($x,$y)
                if ($pixel.B -gt ($pixel.R+45) -and $pixel.B -gt 170 -and $pixel.G -gt 120) { $count++ }
            }
            if ($count -ge 8) { $widths += $count }
        }
        if ($widths.Count -eq 0) { throw 'No proportional bars were visible' }
        $small=@($widths | Where-Object { $_ -ge 20 -and $_ -le 45 })
        $large=@($widths | Where-Object { $_ -ge 70 })
        if ($small.Count -eq 0 -or $large.Count -eq 0) { throw "Proportional bars did not expose distinct 1-line and 3-line widths: $($widths -join ',')" }
        return [pscustomobject]@{one_line=($small | Measure-Object -Maximum).Maximum;three_lines=($large | Measure-Object -Maximum).Maximum}
    } finally { $bitmap.Dispose() }
}

function Click-Element($Root,$Element,[switch]$Right) {
    $bounds=$Element.Current.BoundingRectangle; $rootBounds=$Root.Current.BoundingRectangle
    $windowRect=[TokeiHeadfulSmoke.Native+Rect]::new()
    if (-not [TokeiHeadfulSmoke.Native]::GetWindowRect($window,[ref]$windowRect)) { throw 'GetWindowRect failed' }
    $sx=($windowRect.Right-$windowRect.Left)/$rootBounds.Width; $sy=($windowRect.Bottom-$windowRect.Top)/$rootBounds.Height
    $x=[int]($windowRect.Left+(($bounds.Left+$bounds.Width/2)-$rootBounds.Left)*$sx)
    $y=[int]($windowRect.Top+(($bounds.Top+$bounds.Height/2)-$rootBounds.Top)*$sy)
    [void][TokeiHeadfulSmoke.Native]::SetCursorPos($x,$y); Start-Sleep -Milliseconds 80
    if ($Right) { [TokeiHeadfulSmoke.Native]::mouse_event(0x0008,0,0,0,[UIntPtr]::Zero); [TokeiHeadfulSmoke.Native]::mouse_event(0x0010,0,0,0,[UIntPtr]::Zero) }
    else { [TokeiHeadfulSmoke.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero); [TokeiHeadfulSmoke.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero) }
}

$diagnostics=Join-Path $OutputDirectory 'diagnostics.json'
$childEnvironment=[ordered]@{
    EXPLORER_VISUAL_FIXTURE='1'; EXPLORER_VISUAL_REAL_SHELL='1'; EXPLORER_VISUAL_WIDTH='1280'; EXPLORER_VISUAL_HEIGHT='760'
    EXPLORER_VISUAL_STATE='populated'; EXPLORER_VISUAL_DIAGNOSTICS=$diagnostics; EXPLORER_INITIAL_PATH=$InitialPath; EXPLORER_LOG_DIR=$OutputDirectory
    LOCALAPPDATA=$localAppData; APPDATA=$roamingAppData; EXPLORER_UITEST_EXTENSION_STATE_ROOT=$extensionState
}
$previousEnvironment=@{}
foreach($entry in $childEnvironment.GetEnumerator()) {
    $previousEnvironment[$entry.Key]=[Environment]::GetEnvironmentVariable($entry.Key,'Process')
    [Environment]::SetEnvironmentVariable($entry.Key,[string]$entry.Value,'Process')
}
$lockHolder=$null
if ($LockOwnerMode) {
    & cargo.exe build -p explorer-shell-win --bin explorer-lock-holder --locked --offline
    if ($LASTEXITCODE -ne 0) { throw "lock-holder build failed ($LASTEXITCODE)" }
    $heldFile=Join-Path $InitialPath 'locked.txt'
    if (-not (Test-Path -LiteralPath $heldFile)) { [IO.File]::WriteAllText($heldFile,'held') }
    $lockHolder=Start-Process -FilePath (Join-Path $workspace 'target\debug\explorer-lock-holder.exe') -ArgumentList @($heldFile) -PassThru -WindowStyle Hidden
    Start-Sleep -Milliseconds 500
    if ($lockHolder.HasExited) { throw 'lock-holder exited before the app query' }
}
try {
    $processObserver=[TokeiHeadfulSmoke.JobProcessObserver]::StartSuspended($Executable,"--plugin-dll `"$PluginDll`"",$workspace)
} finally {
    foreach($entry in $childEnvironment.GetEnumerator()) { [Environment]::SetEnvironmentVariable($entry.Key,$previousEnvironment[$entry.Key],'Process') }
}
$process=$processObserver.Process
try {
    $deadline=[DateTime]::UtcNow.AddSeconds(35)
    [IntPtr]$window=[IntPtr]::Zero
    do {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        if ($process.HasExited) {
            throw "SuperExplorer exited before opening a window (exit code $($process.ExitCode)); inspect $OutputDirectory\error.log"
        }
        [IntPtr]$window=$process.MainWindowHandle
    }
    while (($window -eq [IntPtr]::Zero -or -not (Test-Path $diagnostics)) -and [DateTime]::UtcNow -lt $deadline)
    if ($window -eq [IntPtr]::Zero) { throw 'Timed out waiting for SuperExplorer' }
    [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
    $root=[Windows.Automation.AutomationElement]::FromHandle($window)
    if ($LockOwnerMode) {
        $deadline=[DateTime]::UtcNow.AddSeconds(20); $header=$null; $ownerCell=$null
        do {
            Start-Sleep -Milliseconds 150
            $header=Find-Name $root 'Sort by Lock owners'
            $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
            $ownerCell=0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
                Where-Object { $_.Current.Name -match '^Lock owners: .*(explorer-lock-holder|controlled lock holder)' } | Select-Object -First 1
        } while (($null -eq $header -or $null -eq $ownerCell) -and [DateTime]::UtcNow -lt $deadline)
        if ($null -eq $header -or $null -eq $ownerCell) {
            Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-failure.png')
            $names=0..($all.Count-1) | ForEach-Object { $all.Item($_).Current.Name } | Where-Object { $_ -match 'Lock owners|lock|owner' } | Select-Object -Unique
            throw "Lock owner did not appear; visible: $($names -join ' | ')"
        }
        $appeared=$ownerCell.Current.Name
        Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-present.png')
        $lockHolder.Kill(); $lockHolder.WaitForExit(); $lockHolder=$null
        [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
        [Windows.Forms.SendKeys]::SendWait('{F5}')
        $deadline=[DateTime]::UtcNow.AddSeconds(20); $lateOwner=$ownerCell
        do {
            Start-Sleep -Milliseconds 200
            $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
            $lateOwner=0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
                Where-Object { $_.Current.Name -match '^Lock owners: .*(explorer-lock-holder|controlled lock holder)' } | Select-Object -First 1
        } while ($null -ne $lateOwner -and [DateTime]::UtcNow -lt $deadline)
        if ($null -ne $lateOwner) { throw 'Old-generation lock owner returned after F5 and release' }
        Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-cleared.png')
        if (-not [TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)) { throw 'Could not request clean app shutdown' }
        if (-not $process.WaitForExit(10000)) { throw 'App did not complete clean shutdown' }
        [pscustomobject]@{status='passed';owner_appeared=$appeared;owner_cleared_after_f5=$true;stale_generation_rejected=$true;process_control_exposed=$false;screenshots=@('lock-owner-present.png','lock-owner-cleared.png')} |
            ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
        Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
        return
    }
    $deadline=[DateTime]::UtcNow.AddSeconds(20); $header=$null; $cells=@()
    do {
        Start-Sleep -Milliseconds 150; $header=Find-Name $root 'Sort by Code lines'
        $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        $cells=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object { $_.Current.Name -match '^Code lines: \d+' }
    } while (($null -eq $header -or $cells.Count -lt 3) -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $header) { throw 'Code lines header was not installed' }
    if ($cells.Count -lt 3) {
        Capture-Window $window (Join-Path $OutputDirectory 'code-lines-failure.png')
        $names=0..($all.Count-1) | ForEach-Object { $all.Item($_).Current.Name } | Where-Object { $_ -match 'Code lines|Unsupported|unavailable|provider' } | Select-Object -Unique
        throw "Expected real Code lines values; found $($cells.Count); visible: $($names -join ' | ')"
    }
    $codeLinesImage=Join-Path $OutputDirectory 'code-lines.png'
    Capture-Window $window $codeLinesImage
    $barWidths=Assert-ProportionalBars $codeLinesImage
    Click-Element $root $header; Start-Sleep -Milliseconds 350
    $sorted=Code-Line-Values $root
    for ($i=1; $i -lt $sorted.Count; $i++) {
        if ($sorted[$i] -lt $sorted[$i-1]) { throw "Code lines numeric ascending sort failed: $($sorted -join ',')" }
    }
    $header=Find-NamePrefix $root 'Code lines, sorted'
    if ($null -eq $header) { throw 'Code lines sort state was not exposed' }
    Click-Element $root $header -Right; Start-Sleep -Milliseconds 250
    Capture-Window $window (Join-Path $OutputDirectory 'code-lines-menu.png')
    $toggle=Find-NamePrefix $root 'Show comment and blank detail'
    if ($null -eq $toggle) { throw 'Code lines detail setting was not exposed' }
    Click-Element $root $toggle; Start-Sleep -Milliseconds 350
    $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    $detail=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object { $_.Current.Name -match '^Code lines: \d+ .*comments.*blanks' } | Select-Object -First 1
    if ($null -eq $detail) { throw 'Code lines comment/blank detail did not render' }
    $detailName=$detail.Current.Name
    Capture-Window $window (Join-Path $OutputDirectory 'code-lines-detail.png')
    Start-Sleep -Milliseconds 250
    if (-not [TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)) { throw 'Could not request clean app shutdown' }
    if (-not $process.WaitForExit(10000)) { throw 'App did not complete clean shutdown' }
    $descendants=@($processObserver.WaitForActiveProcessZero(10000))
    $expectedBrokerPath=Join-Path (Split-Path -Parent $Executable) 'explorer-extension-broker.exe'
    $expectedBroker=if (Test-Path -LiteralPath $expectedBrokerPath) { (Resolve-Path -LiteralPath $expectedBrokerPath).Path } else { $null }
    $unexpectedChildren=@($descendants | Where-Object { $null -eq $expectedBroker -or ($_ -split ':\d+$')[0] -ine $expectedBroker })
    if ($unexpectedChildren.Count -ne 0) { throw "Unexpected plugin/tool descendant process observed: $($unexpectedChildren | ConvertTo-Json -Compress)" }
    [pscustomobject]@{status='passed'; values=$cells.Count; real_shell_icons='captured'; proportional_bar_widths=$barWidths; numeric_sort=$sorted; detail=$detailName; observed_descendant_processes=$descendants; observed_plugin_tool_descendants=$unexpectedChildren; clean_shutdown=$true; screenshots=@('code-lines.png','code-lines-detail.png')} |
        ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
} finally {
    if ($null -ne $lockHolder -and -not $lockHolder.HasExited) { $lockHolder.Kill(); $lockHolder.WaitForExit() }
    if (-not $process.HasExited) {
        if ($null -ne $window -and $window -ne [IntPtr]::Zero) { [void][TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero) }
        if (-not $process.WaitForExit(3000)) { $process.Kill(); $process.WaitForExit() }
    }
    if ($null -ne $processObserver) { $processObserver.Dispose() }
    '' | Set-Content (Join-Path $OutputDirectory 'stdout.log') -Encoding utf8
    '' | Set-Content (Join-Path $OutputDirectory 'stderr.log') -Encoding utf8
}
