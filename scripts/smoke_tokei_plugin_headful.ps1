param(
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$PluginRoot = 'sdk\fixtures\rust-tokei-code-lines-column',
    [string]$PluginDll = 'sdk\fixtures\rust-tokei-code-lines-column\target\x86_64-pc-windows-msvc\debug\rust_tokei_code_lines_column.dll',
    [string]$SecondPluginRoot = 'sdk\fixtures\lua-tokei-code-lines-column',
    [string]$SecondPluginDll = 'sdk\fixtures\lua-tokei-code-lines-column\target\x86_64-pc-windows-msvc\debug\lua_tokei_code_lines_column.dll',
    [string]$InitialPath = 'sdk\fixtures\rust-tokei-code-lines-column\samples',
    [string]$OutputDirectory = 'target\tokei-headful-smoke',
    [string[]]$AdditionalPluginDlls = @(),
    [switch]$DirectoryAggregateMode,
    [int]$MinimumDirectoryValues = 10,
    [switch]$EnableFileCountColumn,
    [switch]$InputPreparationRepairMode,
    [switch]$DirectoryAdmissionUnavailableMode,
    [switch]$DirectoryAdmissionBoundaryMode,
    [switch]$UseExecutableInPlace,
    [switch]$LockOwnerMode,
    [switch]$DualCodeLinesMode,
    [switch]$DualCodeLinesRealFolderMode,
    [switch]$DetailsColumnDragMode,
    [switch]$WideWindow
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$pluginRoot = if ($LockOwnerMode) { Join-Path $workspace 'sdk\fixtures\rust-lock-owner-column' } elseif ([IO.Path]::IsPathRooted($PluginRoot)) { $PluginRoot } else { Join-Path $workspace $PluginRoot }
$codeLinesColumn = if ($pluginRoot -match 'lua-tokei-code-lines-column') { 'Code lines' } else { 'Main code lines' }
$codeLinesCellPattern = "^$([regex]::Escape($codeLinesColumn)): (?:.+: )?([\d,]+)"
$codeLinesDetailPattern = "^$([regex]::Escape($codeLinesColumn)): (?:.+: )?[\d,]+ .*comments.*blanks"
& cargo.exe test --manifest-path (Join-Path $pluginRoot 'Cargo.toml') --locked --offline
if ($LASTEXITCODE -ne 0) { throw "tokei plugin cargo test failed ($LASTEXITCODE)" }
& cargo.exe build --manifest-path (Join-Path $pluginRoot 'Cargo.toml') --target x86_64-pc-windows-msvc --locked --offline
if ($LASTEXITCODE -ne 0) { throw "tokei plugin cargo build failed ($LASTEXITCODE)" }
if ($DualCodeLinesRealFolderMode -and -not $DualCodeLinesMode) {
    throw 'DualCodeLinesRealFolderMode requires DualCodeLinesMode'
}
if ($DualCodeLinesMode) {
    $secondPluginRoot = if ([IO.Path]::IsPathRooted($SecondPluginRoot)) { $SecondPluginRoot } else { Join-Path $workspace $SecondPluginRoot }
    & cargo.exe test --manifest-path (Join-Path $secondPluginRoot 'Cargo.toml') --locked --offline
    if ($LASTEXITCODE -ne 0) { throw "second tokei plugin cargo test failed ($LASTEXITCODE)" }
    & cargo.exe build --manifest-path (Join-Path $secondPluginRoot 'Cargo.toml') --target x86_64-pc-windows-msvc --locked --offline
    if ($LASTEXITCODE -ne 0) { throw "second tokei plugin cargo build failed ($LASTEXITCODE)" }
    if (-not $DualCodeLinesRealFolderMode) {
        $dualOutputRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory } else { Join-Path $workspace $OutputDirectory }
        $dualFixture = Join-Path $dualOutputRoot 'dual-main-language-sample'
        New-Item -ItemType Directory -Force -Path $dualFixture | Out-Null
        $mixedProject = Join-Path $dualFixture 'mixed-project'
        New-Item -ItemType Directory -Force -Path $mixedProject | Out-Null
        foreach ($fixtureDirectory in $dualFixture,$mixedProject) {
            foreach ($staleFixtureFile in 'main.rs','script.py','script.lua','script.js') {
                Remove-Item -LiteralPath (Join-Path $fixtureDirectory $staleFixtureFile) -Force -ErrorAction SilentlyContinue
            }
        }
        $rustLines = 1..1250 | ForEach-Object { "fn line_$($_)() {}" }
        $javaScriptLines = 1..75 | ForEach-Object { "const value_$($_) = $($_);" }
        [IO.File]::WriteAllText((Join-Path $mixedProject 'main.rs'),($rustLines -join "`n") + "`n")
        [IO.File]::WriteAllText((Join-Path $mixedProject 'script.js'),($javaScriptLines -join "`n") + "`n")
        $InitialPath = $dualFixture
    }
}
if ($DirectoryAdmissionUnavailableMode) {
    $admissionOutputRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) { $OutputDirectory } else { Join-Path $workspace $OutputDirectory }
    $admissionFixture = Join-Path $admissionOutputRoot 'mft-count-admission-fixture'
    $underLimit = Join-Path $admissionFixture 'files-999'
    $overLimit = Join-Path $admissionFixture 'files-1000'
    $nested = Join-Path $admissionFixture 'nested-counts\a\b'
    New-Item -ItemType Directory -Force -Path $underLimit,$overLimit,$nested | Out-Null
    foreach ($index in 0..998) {
        [IO.File]::WriteAllText((Join-Path $underLimit ("f{0:D4}.rs" -f $index)), "fn f$index() {}`n")
    }
    foreach ($index in 0..999) {
        [IO.File]::WriteAllText((Join-Path $overLimit ("f{0:D4}.rs" -f $index)), "fn f$index() {}`n")
    }
    [IO.File]::WriteAllText((Join-Path $admissionFixture 'nested-counts\root.rs'), "fn root() {}`n")
    [IO.File]::WriteAllText((Join-Path $admissionFixture 'nested-counts\a\child.rs'), "fn child() {}`n")
    [IO.File]::WriteAllText((Join-Path $nested 'deep.rs'), "fn deep() {}`n")
    $InitialPath = $admissionFixture
}
foreach ($name in 'Executable','PluginDll','InitialPath','OutputDirectory') {
    $value = Get-Variable -Name $name -ValueOnly
    if (-not [IO.Path]::IsPathRooted($value)) { Set-Variable -Name $name -Value ([IO.Path]::GetFullPath((Join-Path $workspace $value))) }
}
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$PluginDll = (Resolve-Path -LiteralPath $PluginDll).Path
$SecondPluginDll = if ($DualCodeLinesMode) {
    if (-not [IO.Path]::IsPathRooted($SecondPluginDll)) { $SecondPluginDll = [IO.Path]::GetFullPath((Join-Path $workspace $SecondPluginDll)) }
    (Resolve-Path -LiteralPath $SecondPluginDll).Path
} else { $null }
$AdditionalPluginDlls = @($AdditionalPluginDlls | ForEach-Object {
    $path = if ([IO.Path]::IsPathRooted($_)) { $_ } else { [IO.Path]::GetFullPath((Join-Path $workspace $_)) }
    (Resolve-Path -LiteralPath $path).Path
})
$InitialPath = (Resolve-Path -LiteralPath $InitialPath).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$profileRoot=Join-Path $OutputDirectory 'profile'
$localAppData=Join-Path $profileRoot 'LocalAppData'
$roamingAppData=Join-Path $profileRoot 'AppData'
$extensionState=Join-Path $profileRoot 'ExtensionState'
New-Item -ItemType Directory -Force -Path $localAppData,$roamingAppData,$extensionState | Out-Null
$alternateFolder=Join-Path $OutputDirectory 'lock-owner-alternate'
if ($LockOwnerMode) {
    New-Item -ItemType Directory -Force -Path $alternateFolder | Out-Null
    [IO.File]::WriteAllText((Join-Path $alternateFolder 'alternate-marker.txt'),'alternate')
    $nativeCwdParent=Join-Path $InitialPath 'cwd-native-parent'
    $nativeCwdNested=Join-Path $nativeCwdParent 'nested'
    $wow64CwdParent=Join-Path $InitialPath 'cwd-wow64-parent'
    $wow64CwdNested=Join-Path $wow64CwdParent 'nested'
    New-Item -ItemType Directory -Force -Path $nativeCwdNested,$wow64CwdNested | Out-Null
    [IO.File]::WriteAllText((Join-Path $nativeCwdNested 'native-marker.txt'),'native')
    [IO.File]::WriteAllText((Join-Path $wow64CwdNested 'wow64-marker.txt'),'wow64')
}
if (-not $UseExecutableInPlace) {
    $isolatedLaunchDirectory=Join-Path $OutputDirectory 'isolated-app'
    New-Item -ItemType Directory -Force -Path $isolatedLaunchDirectory | Out-Null
    $isolatedExecutable=Join-Path $isolatedLaunchDirectory (Split-Path -Leaf $Executable)
    [IO.File]::Copy($Executable,$isolatedExecutable,$true)
    $Executable=$isolatedExecutable
}

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
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr LoadKeyboardLayout(string id,uint flags);
    [DllImport("user32.dll")] public static extern IntPtr ActivateKeyboardLayout(IntPtr layout,uint flags);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window,IntPtr processId);
    [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint threadId);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    [DllImport("kernel32.dll",SetLastError=true)] public static extern bool IsWow64Process2(IntPtr process,out ushort processMachine,out ushort nativeMachine);
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

function Find-AutomationId($Root,[string]$Id) {
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,$Id))
}

function Find-ButtonName($Root,[string]$Name) {
    $condition=[Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Button)
    )
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
}

function Find-ButtonNamePrefix($Root,[string]$Prefix) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Button))
    0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
        Where-Object { $_.Current.Name -like "$Prefix*" } | Select-Object -First 1
}

function Find-ControlTypeName($Root,$Type,[string]$Name) {
    $condition=[Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$Name),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,$Type)
    )
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
}

function Find-ControlTypeNamePrefix($Root,$Type,[string]$Prefix) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,$Type))
    0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
        Where-Object { $_.Current.Name -like "$Prefix*" } | Select-Object -First 1
}

function Find-MoreOptionsElement($Root) {
    $items=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::MenuItem))
    $ordered=@(0..($items.Count-1) | ForEach-Object { $items.Item($_) } |
        Where-Object { $_.Current.BoundingRectangle.Height -gt 0 } | Sort-Object { $_.Current.BoundingRectangle.Top })
    if ($ordered.Count -lt 2) { return $null }
    $ordered[$ordered.Count-2]
}

function Find-MoreToolbarButton($Root) {
    $buttons=@($Root.FindAll([Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Button)) |
        ForEach-Object { $_ } | Where-Object { $_.Current.BoundingRectangle.Height -gt 0 })
    $extensions=$buttons | Where-Object { $_.Current.Name -in @('Extensions',(([string][char]0x64F4)+[char]0x5145+[char]0x529F+[char]0x80FD)) } | Select-Object -First 1
    if ($null -eq $extensions) { return $null }
    $bounds=$extensions.Current.BoundingRectangle
    $buttons | Where-Object {
        $_.Current.BoundingRectangle.Left -lt $bounds.Left -and
        [Math]::Abs($_.Current.BoundingRectangle.Top-$bounds.Top) -lt 8
    } | Sort-Object { $_.Current.BoundingRectangle.Left } -Descending | Select-Object -First 1
}

function Send-Key([byte]$Key,[byte[]]$Modifiers=@()) {
    foreach($modifier in $Modifiers) { [TokeiHeadfulSmoke.Native]::keybd_event($modifier,0,0,[UIntPtr]::Zero) }
    [TokeiHeadfulSmoke.Native]::keybd_event($Key,0,0,[UIntPtr]::Zero)
    [TokeiHeadfulSmoke.Native]::keybd_event($Key,0,2,[UIntPtr]::Zero)
    for($index=$Modifiers.Count-1;$index -ge 0;$index--) {
        [TokeiHeadfulSmoke.Native]::keybd_event($Modifiers[$index],0,2,[UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 120
}

function Set-EnglishInput([IntPtr]$WindowHandle) {
    $english=[TokeiHeadfulSmoke.Native]::LoadKeyboardLayout('00000409',1)
    if ($english -eq [IntPtr]::Zero) { throw 'Failed to load English keyboard layout' }
    [void][TokeiHeadfulSmoke.Native]::ActivateKeyboardLayout($english,0)
    if (-not [TokeiHeadfulSmoke.Native]::PostMessage($WindowHandle,0x0050,[UIntPtr]::Zero,$english)) {
        throw 'Failed to switch explorer input to English'
    }
    $threadId=[TokeiHeadfulSmoke.Native]::GetWindowThreadProcessId($WindowHandle,[IntPtr]::Zero)
    $deadline=[DateTime]::UtcNow.AddSeconds(3)
    do {
        if (([TokeiHeadfulSmoke.Native]::GetKeyboardLayout($threadId).ToInt64() -band 0xFFFF) -eq 0x0409) { return }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Explorer input language did not switch to English'
}

function Set-Address($Root,[string]$Path,[string]$ExpectedRow) {
    [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
    Set-EnglishInput $window
    $editor=$null
    for($attempt=0;$attempt -lt 3 -and $null -eq $editor;$attempt++) {
        Send-Key 0x1B
        Send-Key 0x4C @(0x11)
        Start-Sleep -Milliseconds 300
        $edits=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Edit))
        $editor=0..($edits.Count-1) | ForEach-Object { $edits.Item($_) } |
            Where-Object { $_.Current.AutomationId -ne 'search-box' -and $_.Current.BoundingRectangle.Top -lt ($Root.Current.BoundingRectangle.Top+220) } |
            Select-Object -First 1
    }
    if ($null -eq $editor) { throw 'Address editor did not appear after Ctrl+L' }
    $editor.SetFocus(); Send-Key 0x41 @(0x11)
    [Windows.Forms.SendKeys]::SendWait($Path)
    Send-Key 0x0D
    $deadline=[DateTime]::UtcNow.AddSeconds(8); $row=$null
    do { Start-Sleep -Milliseconds 100; $row=Find-NamePrefix $Root $ExpectedRow }
    while ($null -eq $row -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $row) { throw "Navigation did not reach folder containing $ExpectedRow" }
}

function Find-NamePrefix($Root,[string]$Prefix) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
        Where-Object { $_.Current.Name -like "$Prefix*" } | Select-Object -First 1
}

function Find-CellOnRow($Root,[string]$RowName,[string]$CellPrefix) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    $rootTop=$Root.Current.BoundingRectangle.Top
    $row=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object {
        ($_.Current.Name -like "$RowName*" -or $_.Current.Name -like "Name: $RowName*") -and
        $_.Current.BoundingRectangle.Top -gt ($rootTop+180) -and $_.Current.BoundingRectangle.Height -gt 0
    } | Select-Object -First 1
    if ($null -eq $row) { return $null }
    $rowTop=$row.Current.BoundingRectangle.Top
    0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object {
        $_.Current.Name -like "$CellPrefix*" -and
        [Math]::Abs($_.Current.BoundingRectangle.Top-$rowTop) -lt 24
    } | Select-Object -First 1
}

function Code-Line-Values($Root) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    @(0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
        Where-Object { $_.Current.Name -match $codeLinesCellPattern } |
        Sort-Object { $_.Current.BoundingRectangle.Top } |
        ForEach-Object { [int]([regex]::Match($_.Current.Name,$codeLinesCellPattern).Groups[1].Value.Replace(',','')) })
}

function Assert-NoProportionalBars([string]$Path) {
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
        $bars=@($widths | Where-Object { $_ -ge 20 })
        if ($bars.Count -ne 0) { throw "Code lines must render as text without proportional bars: $($bars -join ',')" }
        return $true
    } finally { $bitmap.Dispose() }
}

function Assert-NoCodeLineBarElements($Root) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    $bars=@(0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object {
        $_.Current.AutomationId -like 'code-lines-bar-track-*'
    })
    if ($bars.Count -ne 0) { throw "Code lines exposed $($bars.Count) proportional bar elements" }
    return $true
}

function Assert-DetailsColumnAlignment($Root,[string[]]$ColumnNames) {
    $verified=@()
    foreach ($columnName in $ColumnNames) {
        $header=Find-ButtonName $Root "Sort by $columnName"
        if ($null -eq $header) { throw "Missing details header for alignment check: $columnName" }
        $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        $cell=0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
            Where-Object { $_.Current.Name -like "$columnName`:*" } | Select-Object -First 1
        if ($null -eq $cell) { throw "Missing populated cell for alignment check: $columnName" }
        $headerBounds=$header.Current.BoundingRectangle
        $cellBounds=$cell.Current.BoundingRectangle
        $cellCenter=$cellBounds.Left + ($cellBounds.Width / 2.0)
        if ($cellCenter -lt $headerBounds.Left -or $cellCenter -gt $headerBounds.Right) {
            throw "$columnName cell is outside its header bounds: header=$headerBounds cell=$cellBounds"
        }
        $verified += $columnName
    }
    return $verified
}

function Click-Element($Root,$Element,[switch]$Right) {
    if (-not $Right) {
        $pattern=$null
        if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) {
            ([Windows.Automation.InvokePattern]$pattern).Invoke()
            return
        }
        if ($Element.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern,[ref]$pattern)) {
            ([Windows.Automation.TogglePattern]$pattern).Toggle()
            return
        }
    }
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

function Click-ElementPointer($Root,$Element) {
    $scrollPattern=$null
    if ($Element.TryGetCurrentPattern([Windows.Automation.ScrollItemPattern]::Pattern,[ref]$scrollPattern)) {
        ([Windows.Automation.ScrollItemPattern]$scrollPattern).ScrollIntoView()
        Start-Sleep -Milliseconds 180
    }
    $bounds=$Element.Current.BoundingRectangle; $rootBounds=$Root.Current.BoundingRectangle
    $windowRect=[TokeiHeadfulSmoke.Native+Rect]::new()
    if (-not [TokeiHeadfulSmoke.Native]::GetWindowRect($window,[ref]$windowRect)) { throw 'GetWindowRect failed' }
    $sx=($windowRect.Right-$windowRect.Left)/$rootBounds.Width; $sy=($windowRect.Bottom-$windowRect.Top)/$rootBounds.Height
    $x=[int]($windowRect.Left+(($bounds.Left+$bounds.Width/2)-$rootBounds.Left)*$sx)
    $y=[int]($windowRect.Top+(($bounds.Top+$bounds.Height/2)-$rootBounds.Top)*$sy)
    [void][TokeiHeadfulSmoke.Native]::SetCursorPos($x,$y); Start-Sleep -Milliseconds 80
    [TokeiHeadfulSmoke.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
    [TokeiHeadfulSmoke.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
}

function Drag-ElementToElement($Root,$Source,$Target) {
    $sourceBounds=$Source.Current.BoundingRectangle; $targetBounds=$Target.Current.BoundingRectangle
    $rootBounds=$Root.Current.BoundingRectangle
    $windowRect=[TokeiHeadfulSmoke.Native+Rect]::new()
    if (-not [TokeiHeadfulSmoke.Native]::GetWindowRect($window,[ref]$windowRect)) { throw 'GetWindowRect failed' }
    $sx=($windowRect.Right-$windowRect.Left)/$rootBounds.Width; $sy=($windowRect.Bottom-$windowRect.Top)/$rootBounds.Height
    $fromX=[int]($windowRect.Left+(($sourceBounds.Left+$sourceBounds.Width/2)-$rootBounds.Left)*$sx)
    $fromY=[int]($windowRect.Top+(($sourceBounds.Top+$sourceBounds.Height/2)-$rootBounds.Top)*$sy)
    $toX=[int]($windowRect.Left+(($targetBounds.Left+$targetBounds.Width/2)-$rootBounds.Left)*$sx)
    $toY=[int]($windowRect.Top+(($targetBounds.Top+$targetBounds.Height/2)-$rootBounds.Top)*$sy)
    [void][TokeiHeadfulSmoke.Native]::SetCursorPos($fromX,$fromY); Start-Sleep -Milliseconds 100
    [TokeiHeadfulSmoke.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
    foreach ($step in 1..12) {
        $x=[int]($fromX+(($toX-$fromX)*$step/12.0)); $y=[int]($fromY+(($toY-$fromY)*$step/12.0))
        [void][TokeiHeadfulSmoke.Native]::SetCursorPos($x,$y)
        [TokeiHeadfulSmoke.Native]::mouse_event(0x0001,0,0,0,[UIntPtr]::Zero)
        Start-Sleep -Milliseconds 35
    }
    [TokeiHeadfulSmoke.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 400
}

function Find-AutomationIdSuffix($Root,[string]$Suffix) {
    $all=$Root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
        Where-Object { $_.Current.AutomationId -like "*$Suffix" } |
        Sort-Object { $_.Current.BoundingRectangle.Top } | Select-Object -First 1
}

function Begin-DetailsColumnMidpointDrag($Root,$Source,$Target,[double]$TargetFraction) {
    $sourceBounds=$Source.Current.BoundingRectangle; $targetBounds=$Target.Current.BoundingRectangle
    $rootBounds=$Root.Current.BoundingRectangle
    $windowRect=[TokeiHeadfulSmoke.Native+Rect]::new()
    if (-not [TokeiHeadfulSmoke.Native]::GetWindowRect($window,[ref]$windowRect)) { throw 'GetWindowRect failed' }
    $sx=($windowRect.Right-$windowRect.Left)/$rootBounds.Width; $sy=($windowRect.Bottom-$windowRect.Top)/$rootBounds.Height
    $fromX=[int]($windowRect.Left+(($sourceBounds.Left+$sourceBounds.Width/2)-$rootBounds.Left)*$sx)
    $fromY=[int]($windowRect.Top+(($sourceBounds.Top+$sourceBounds.Height/2)-$rootBounds.Top)*$sy)
    $targetLogicalX=$targetBounds.Left+($targetBounds.Width*$TargetFraction)
    $toX=[int]($windowRect.Left+($targetLogicalX-$rootBounds.Left)*$sx)
    $toY=[int]($windowRect.Top+(($targetBounds.Top+$targetBounds.Height/2)-$rootBounds.Top)*$sy)
    [void][TokeiHeadfulSmoke.Native]::SetCursorPos($fromX,$fromY); Start-Sleep -Milliseconds 100
    [TokeiHeadfulSmoke.Native]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
    foreach ($step in 1..12) {
        $x=[int]($fromX+(($toX-$fromX)*$step/12.0)); $y=[int]($fromY+(($toY-$fromY)*$step/12.0))
        [void][TokeiHeadfulSmoke.Native]::SetCursorPos($x,$y)
        [TokeiHeadfulSmoke.Native]::mouse_event(0x0001,0,0,0,[UIntPtr]::Zero)
        Start-Sleep -Milliseconds 35
    }
    Start-Sleep -Milliseconds 300
    [ordered]@{
        from_x=$fromX; from_y=$fromY; to_x=$toX; to_y=$toY
        target_fraction=$TargetFraction
        source_left=$sourceBounds.Left; source_right=$sourceBounds.Right
        target_left=$targetBounds.Left; target_right=$targetBounds.Right
        target_midpoint=($targetBounds.Left+$targetBounds.Width/2.0)
    }
}

$diagnostics=Join-Path $OutputDirectory 'diagnostics.json'
$childEnvironment=[ordered]@{
    EXPLORER_VISUAL_FIXTURE='1'; EXPLORER_VISUAL_REAL_SHELL='1'; EXPLORER_VISUAL_WIDTH='1280'; EXPLORER_VISUAL_HEIGHT='760'
    EXPLORER_VISUAL_STATE='populated'; EXPLORER_VISUAL_DIAGNOSTICS=$diagnostics; EXPLORER_INITIAL_PATH=$InitialPath; EXPLORER_LOG_DIR=$OutputDirectory
    LOCALAPPDATA=$localAppData; APPDATA=$roamingAppData; EXPLORER_UITEST_EXTENSION_STATE_ROOT=$extensionState
}
if ($LockOwnerMode) {
    $childEnvironment.EXPLORER_LOCK_OWNER_TEST_DELAY_MS='900'
}
$previousEnvironment=@{}
foreach($entry in $childEnvironment.GetEnumerator()) {
    $previousEnvironment[$entry.Key]=[Environment]::GetEnvironmentVariable($entry.Key,'Process')
    [Environment]::SetEnvironmentVariable($entry.Key,[string]$entry.Value,'Process')
}
$lockHolder=$null
$nativeCmd=$null
$wow64Cmd=$null
if ($LockOwnerMode) {
    & cargo.exe build -p explorer-shell-win --bin explorer-lock-holder --locked --offline
    if ($LASTEXITCODE -ne 0) { throw "lock-holder build failed ($LASTEXITCODE)" }
    $heldFile=Join-Path $InitialPath 'locked.txt'
    if (-not (Test-Path -LiteralPath $heldFile)) { [IO.File]::WriteAllText($heldFile,'held') }
    $lockHolder=Start-Process -FilePath (Join-Path $workspace 'target\debug\explorer-lock-holder.exe') -ArgumentList @($heldFile) -PassThru -WindowStyle Hidden
    $nativeCmdPath=Join-Path $env:SystemRoot 'System32\cmd.exe'
    $wow64CmdPath=Join-Path $env:SystemRoot 'SysWOW64\cmd.exe'
    if (-not (Test-Path -LiteralPath $nativeCmdPath) -or -not (Test-Path -LiteralPath $wow64CmdPath)) {
        throw 'Native or SysWOW64 cmd.exe fixture is unavailable'
    }
    $nativeCmd=Start-Process -FilePath $nativeCmdPath -ArgumentList @('/D','/Q','/K') -WorkingDirectory $nativeCwdNested -PassThru -WindowStyle Hidden
    $wow64Cmd=Start-Process -FilePath $wow64CmdPath -ArgumentList @('/D','/Q','/K') -WorkingDirectory $wow64CwdNested -PassThru -WindowStyle Hidden
    $nativeCmdPid=$nativeCmd.Id
    $wow64CmdPid=$wow64Cmd.Id
    Start-Sleep -Milliseconds 500
    if ($lockHolder.HasExited) { throw 'lock-holder exited before the app query' }
    if ($nativeCmd.HasExited -or $wow64Cmd.HasExited) { throw 'cmd.exe current-directory fixture exited before the app query' }
    [UInt16]$wow64ProcessMachine=0; [UInt16]$wow64NativeMachine=0
    if (-not [TokeiHeadfulSmoke.Native]::IsWow64Process2($wow64Cmd.Handle,[ref]$wow64ProcessMachine,[ref]$wow64NativeMachine)) {
        throw "IsWow64Process2 failed for SysWOW64 cmd.exe: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    if ($wow64ProcessMachine -eq 0) { throw 'SysWOW64 cmd.exe did not report a WOW64 process identity' }
}
try {
    $pluginArguments="--plugin-dll `"$PluginDll`""
    if ($DualCodeLinesMode) { $pluginArguments += " --plugin-dll `"$SecondPluginDll`"" }
    foreach ($additionalPluginDll in $AdditionalPluginDlls) {
        $pluginArguments += " --plugin-dll `"$additionalPluginDll`""
    }
    $processObserver=[TokeiHeadfulSmoke.JobProcessObserver]::StartSuspended($Executable,$pluginArguments,$workspace)
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
    if ($WideWindow) { [void][TokeiHeadfulSmoke.Native]::MoveWindow($window,0,0,2600,1200,$true) }
    [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
    $root=[Windows.Automation.AutomationElement]::FromHandle($window)
    if ($EnableFileCountColumn) {
        $detailsHeader = Find-ButtonName $root 'Sort by Name'
        if ($null -eq $detailsHeader) { $detailsHeader = Find-ButtonNamePrefix $root 'Name, sorted' }
        if ($null -eq $detailsHeader) { throw 'Could not find a Details header for the column chooser' }
        Click-Element $root $detailsHeader -Right
        Start-Sleep -Milliseconds 250
        $desktop = [Windows.Automation.AutomationElement]::RootElement
        $all = $desktop.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        $fileCountItem = 0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object {
            $_.Current.ControlType -eq [Windows.Automation.ControlType]::MenuItem -and
            ($_.Current.Name -eq 'File Count' -or $_.Current.Name -like 'File Count, *')
        } | Select-Object -First 1
        if ($null -eq $fileCountItem) { throw 'File Count was unavailable in the Details column chooser' }
        $scrollPattern = $null
        if ($fileCountItem.TryGetCurrentPattern([Windows.Automation.ScrollItemPattern]::Pattern,[ref]$scrollPattern)) {
            ([Windows.Automation.ScrollItemPattern]$scrollPattern).ScrollIntoView()
            Start-Sleep -Milliseconds 180
        }
        if (-not $fileCountItem.Current.Name.EndsWith(', checked',[StringComparison]::Ordinal)) {
            Click-Element $root $fileCountItem
            Start-Sleep -Milliseconds 300
        }
        Send-Key 0x1B
        Start-Sleep -Milliseconds 250
    }
    if ($LockOwnerMode) {
        $deadline=[DateTime]::UtcNow.AddSeconds(40); $header=$null; $ownerCell=$null
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

        # Prove that a process current directory occupies both its exact row
        # and every visible ancestor directory, for native and WOW64 cmd.exe.
        $cwdEvidence=[ordered]@{}
        foreach($fixture in @(
            @{ Name='native'; Parent=$nativeCwdParent; Nested=$nativeCwdNested; Marker='native-marker.txt'; ParentRow='cwd-native-parent' },
            @{ Name='wow64'; Parent=$wow64CwdParent; Nested=$wow64CwdNested; Marker='wow64-marker.txt'; ParentRow='cwd-wow64-parent' }
        )) {
            Set-Address $root $fixture.Parent 'nested'
            $deadline=[DateTime]::UtcNow.AddSeconds(20); $nestedOwner=$null
            do {
                Start-Sleep -Milliseconds 150
                $nestedOwner=Find-NamePrefix $root 'Lock owners: cmd.exe'
            } while ($null -eq $nestedOwner -and [DateTime]::UtcNow -lt $deadline)
            if ($null -eq $nestedOwner) { throw "$($fixture.Name) cmd.exe was not shown on its nested current-directory row" }
            $nestedOwnerName=$nestedOwner.Current.Name
            Capture-Window $window (Join-Path $OutputDirectory "lock-owner-cwd-$($fixture.Name)-nested.png")

            [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
            $up=Find-AutomationId $root 'navigation-up'
            if ($null -eq $up) { $up=Find-ButtonName $root 'Up' }
            if ($null -eq $up) { throw 'Up navigation control was unavailable' }
            Click-Element $root $up
            Start-Sleep -Milliseconds 700
            $deadline=[DateTime]::UtcNow.AddSeconds(20); $parentOwner=$null; $parentOwners=@()
            do {
                Start-Sleep -Milliseconds 150
                $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
                $parentOwners=@(0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
                    Where-Object { $_.Current.Name -like 'Lock owners: cmd.exe*' })
                $parentOwner=$parentOwners | Select-Object -First 1
            } while ($parentOwners.Count -lt 2 -and [DateTime]::UtcNow -lt $deadline)
            Capture-Window $window (Join-Path $OutputDirectory "lock-owner-cwd-$($fixture.Name)-parent.png")
            if ($parentOwners.Count -lt 2) { throw 'Native and WOW64 cmd.exe were not projected to both visible parent rows' }
            $cwdEvidence[$fixture.Name]=[ordered]@{nested=$nestedOwnerName;parent=$parentOwner.Current.Name}
        }

        $nativeCmd.Kill(); $nativeCmd.WaitForExit(); $nativeCmd=$null
        $wow64Cmd.Kill(); $wow64Cmd.WaitForExit(); $wow64Cmd=$null
        [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
        Send-Key 0x74
        $deadline=[DateTime]::UtcNow.AddSeconds(20); $cwdOwner=Find-NamePrefix $root 'Lock owners: cmd.exe'
        do {
            Start-Sleep -Milliseconds 150
            $cwdOwner=Find-NamePrefix $root 'Lock owners: cmd.exe'
        } while ($null -ne $cwdOwner -and [DateTime]::UtcNow -lt $deadline)
        if ($null -ne $cwdOwner) { throw 'cmd.exe current-directory owner remained after process exit and F5' }
        Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-cwd-cleared.png')

        # Start several old-generation queries, then change both tab and
        # location before the deterministic debug delay expires. Keep the lock
        # alive until those old queries have had time to discover it.
        Send-Key 0x74; Send-Key 0x74; Send-Key 0x74
        $newTab=Find-AutomationId $root 'new-tab-button'
        if ($null -eq $newTab) { $newTab=Find-ButtonName $root 'New tab' }
        if ($null -eq $newTab) { throw 'New tab button was unavailable' }
        Click-Element $root $newTab
        Start-Sleep -Milliseconds 500
        Set-Address $root $alternateFolder 'alternate-marker.txt'
        Start-Sleep -Milliseconds 1400
        $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        $crossContextOwner=0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
            Where-Object { $_.Current.Name -match '^Lock owners: .*(explorer-lock-holder|controlled lock holder)' } | Select-Object -First 1
        if ($null -ne $crossContextOwner) { throw 'Old lock-owner result crossed tab/location generation' }

        $lockHolder.Kill(); $lockHolder.WaitForExit(); $lockHolder=$null
        [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
        $refresh = Find-AutomationId $root 'navigation-refresh'
        if ($null -eq $refresh) { $refresh = Find-ButtonName $root 'Refresh' }
        if ($null -eq $refresh) { throw 'Refresh control was unavailable' }
        Click-Element $root $refresh
        $deadline=[DateTime]::UtcNow.AddSeconds(20); $lateOwner=$ownerCell
        do {
            Start-Sleep -Milliseconds 200
            $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
            $lateOwner=0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
                Where-Object { $_.Current.Name -match '^Lock owners: .*(explorer-lock-holder|controlled lock holder)' } | Select-Object -First 1
        } while ($null -ne $lateOwner -and [DateTime]::UtcNow -lt $deadline)
        if ($null -ne $lateOwner) { throw 'Old-generation lock owner returned after F5 and release' }

        # Disable the contribution through production Folder Options. Any
        # delayed result must remain unable to restore the column or a cell.
        $more=Find-AutomationId $root 'command-more-menu'
        if ($null -eq $more) { $more=Find-ButtonName $root 'See more' }
        if ($null -eq $more) { $more=Find-ButtonName $root 'More' }
        if ($null -eq $more) { $more=Find-Name $root (([string][char]0x5176)+[char]0x4ED6) }
        if ($null -eq $more) { $more=Find-MoreToolbarButton $root }
        if ($null -eq $more) { throw 'More menu was unavailable for extension disable' }
        Click-Element $root $more
        $deadline=[DateTime]::UtcNow.AddSeconds(5); $options=$null
        do {
            Start-Sleep -Milliseconds 100
            $options=Find-AutomationId $root 'more-options'
            if ($null -eq $options) { $options=Find-MoreOptionsElement $root }
        }
        while ($null -eq $options -and [DateTime]::UtcNow -lt $deadline)
        if ($null -eq $options) { throw 'Folder Options command was unavailable' }
        Click-Element $root $options
        $mainRoot=$root
        $folderOptionsName=([string][char]0x8CC7)+[char]0x6599+[char]0x593E+[char]0x9078+[char]0x9805
        $deadline=[DateTime]::UtcNow.AddSeconds(10); $optionsRoot=$null
        do {
            Start-Sleep -Milliseconds 100
            $windows=[Windows.Automation.AutomationElement]::RootElement.FindAll(
                [Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition)
            $optionsRoot=0..($windows.Count-1) | ForEach-Object { $windows.Item($_) } | Where-Object {
                $_.Current.NativeWindowHandle -ne 0 -and $_.Current.ProcessId -eq $process.Id -and
                ($_.Current.Name -eq 'Folder Options' -or $_.Current.Name -eq $folderOptionsName -or
                    $null -ne $_.FindFirst([Windows.Automation.TreeScope]::Descendants,
                        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'folder-options-window')))
            } | Select-Object -First 1
        } while ($null -eq $optionsRoot -and [DateTime]::UtcNow -lt $deadline)
        if ($null -eq $optionsRoot) { throw 'Folder Options native window was unavailable' }
        $root=$optionsRoot
        $mainWindow=$window
        $window=[IntPtr]$optionsRoot.Current.NativeWindowHandle
        [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
        $deadline=[DateTime]::UtcNow.AddSeconds(5); $extensionsTab=$null
        do { Start-Sleep -Milliseconds 100; $extensionsTab=Find-AutomationId $root 'folder-options-extensions-tab' }
        while ($null -eq $extensionsTab -and [DateTime]::UtcNow -lt $deadline)
        if ($null -eq $extensionsTab) { $extensionsTab=Find-Name $root 'Extensions' }
        if ($null -eq $extensionsTab) { throw 'Folder Options Extensions tab was unavailable' }
        Click-Element $root $extensionsTab
        $deadline=[DateTime]::UtcNow.AddSeconds(5); $toggle=$null
        do { Start-Sleep -Milliseconds 100; $toggle=Find-ControlTypeName $root ([Windows.Automation.ControlType]::CheckBox) 'Lock owners' }
        while ($null -eq $toggle -and [DateTime]::UtcNow -lt $deadline)
        if ($null -eq $toggle) { $toggle=Find-ControlTypeNamePrefix $root ([Windows.Automation.ControlType]::CheckBox) 'Lock owners' }
        if ($null -eq $toggle) { $toggle=Find-ControlTypeName $root ([Windows.Automation.ControlType]::CheckBox) 'Lock owner' }
        if ($null -eq $toggle) { $toggle=Find-ControlTypeNamePrefix $root ([Windows.Automation.ControlType]::CheckBox) (([string][char]0x9396)+[char]0x5B9A+[char]0x7A0B+[char]0x5E8F) }
        if ($null -eq $toggle) {
            Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-options-failure.png')
            $checkboxes=$root.FindAll([Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::CheckBox))
            $checkboxNames=@(0..($checkboxes.Count-1) | ForEach-Object { $checkboxes.Item($_).Current.Name })
            throw "Lock owners extension toggle was unavailable; checkboxes=$($checkboxNames -join ' | ')"
        }
        $extensionsPage=Find-ControlTypeName $root ([Windows.Automation.ControlType]::List) 'Extensions'
        if ($null -eq $extensionsPage) { throw 'Scrollable Extensions page was unavailable' }
        $pageBounds=$extensionsPage.Current.BoundingRectangle; $rootBounds=$root.Current.BoundingRectangle
        $windowRect=[TokeiHeadfulSmoke.Native+Rect]::new()
        if (-not [TokeiHeadfulSmoke.Native]::GetWindowRect($window,[ref]$windowRect)) { throw 'GetWindowRect failed' }
        $sx=($windowRect.Right-$windowRect.Left)/$rootBounds.Width; $sy=($windowRect.Bottom-$windowRect.Top)/$rootBounds.Height
        $pageX=[int]($windowRect.Left+(($pageBounds.Left+$pageBounds.Width/2)-$rootBounds.Left)*$sx)
        $pageY=[int]($windowRect.Top+(($pageBounds.Top+$pageBounds.Height/2)-$rootBounds.Top)*$sy)
        [void][TokeiHeadfulSmoke.Native]::SetCursorPos($pageX,$pageY)
        $wheelDown=[BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]-120),0)
        foreach($step in 1..5) {
            [TokeiHeadfulSmoke.Native]::mouse_event(0x0800,0,0,$wheelDown,[UIntPtr]::Zero)
            Start-Sleep -Milliseconds 80
        }
        Start-Sleep -Milliseconds 250
        $toggle=Find-ControlTypeName $root ([Windows.Automation.ControlType]::CheckBox) 'Lock owner'
        if ($null -eq $toggle) { throw 'Lock owner checkbox disappeared after scrolling' }
        Click-ElementPointer $root $toggle
        Start-Sleep -Milliseconds 250
        $toggle=Find-ControlTypeName $root ([Windows.Automation.ControlType]::CheckBox) 'Lock owner'
        if ($null -eq $toggle) { throw 'Lock owner checkbox disappeared before Apply' }
        $togglePattern=$null
        if ($toggle.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern,[ref]$togglePattern) -and
            ([Windows.Automation.TogglePattern]$togglePattern).Current.ToggleState -ne [Windows.Automation.ToggleState]::Off) {
            throw 'Lock owner extension checkbox did not turn off'
        }
        $apply=Find-AutomationId $root 'folder-options-apply'
        if ($null -eq $apply) { $apply=Find-ButtonName $root 'Apply' }
        if ($null -eq $apply) { $apply=Find-Name $root (([string][char]0x5957)+[char]0x7528) }
        if ($null -eq $apply) { throw 'Folder Options Apply was unavailable' }
        Click-Element $root $apply
        $ok=Find-AutomationId $root 'folder-options-ok'
        if ($null -eq $ok) { $ok=Find-ButtonName $root 'OK' }
        if ($null -eq $ok) { $ok=Find-Name $root (([string][char]0x78BA)+[char]0x5B9A) }
        if ($null -eq $ok) { throw 'Folder Options OK was unavailable' }
        Click-Element $root $ok
        $root=$mainRoot
        $window=$mainWindow
        [void][TokeiHeadfulSmoke.Native]::SetForegroundWindow($window)
        Start-Sleep -Milliseconds 1300
        if ($null -ne (Find-Name $root 'Sort by Lock owners')) {
            Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-disable-failure.png')
            throw 'Disabled Lock owners column remained active'
        }
        $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        $disabledOwner=0..($all.Count-1) | ForEach-Object { $all.Item($_) } |
            Where-Object { $_.Current.Name -match '^Lock owners:' } | Select-Object -First 1
        if ($null -ne $disabledOwner) {
            Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-disabled-value-failure.png')
            throw "A lock-owner value published after feature disable: $($disabledOwner.Current.Name) bounds=$($disabledOwner.Current.BoundingRectangle)"
        }
        Capture-Window $window (Join-Path $OutputDirectory 'lock-owner-cleared.png')
        if (-not [TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)) { throw 'Could not request clean app shutdown' }
        if (-not $process.WaitForExit(10000)) { throw 'App did not complete clean shutdown' }
        [pscustomobject]@{status='passed';owner_appeared=$appeared;native_cmd_pid=$nativeCmdPid;wow64_cmd_pid=$wow64CmdPid;wow64_process_machine=$wow64ProcessMachine;wow64_native_machine=$wow64NativeMachine;cwd_ancestry=$cwdEvidence;cwd_owner_cleared_after_exit_and_f5=$true;rapid_refresh_rejected=$true;tab_change_rejected=$true;folder_change_rejected=$true;feature_disable_rejected=$true;owner_cleared_after_refresh=$true;stale_generation_rejected=$true;process_control_exposed=$false;screenshots=@('lock-owner-present.png','lock-owner-cwd-native-nested.png','lock-owner-cwd-native-parent.png','lock-owner-cwd-wow64-nested.png','lock-owner-cwd-wow64-parent.png','lock-owner-cwd-cleared.png','lock-owner-cleared.png')} |
            ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
        Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
        return
    }
    $directoryMinimum = if ($DualCodeLinesMode -and -not $DualCodeLinesRealFolderMode) { 1 } else { $MinimumDirectoryValues }
    # Keep this script Windows PowerShell 5.1 compatible even when it is read
    # as the system ANSI code page rather than UTF-8.
    $dependencyUnavailableText = -join @(0x4F9D,0x8CF4,0x20,0x46,0x69,0x6C,0x65,0x20,0x43,0x6F,0x75,0x6E,0x74,0xFF0C,0x56E0,0x6B64,0x672A,0x555F,0x52D5 | ForEach-Object { [char]$_ })
    $overLimitText = -join @(0x46,0x69,0x6C,0x65,0x20,0x43,0x6F,0x75,0x6E,0x74,0x20,0x8D85,0x904E,0x9650,0x5236,0xFF0C,0x56E0,0x6B64,0x672A,0x555F,0x52D5 | ForEach-Object { [char]$_ })
    $deadline=[DateTime]::UtcNow.AddSeconds($(if ($DirectoryAdmissionUnavailableMode) { 20 } else { 90 })); $header=$null; $cells=@(); $loading=@(); $dependencyUnavailable=@(); $overLimit=@()
    do {
        Start-Sleep -Milliseconds 150; $header=Find-ButtonName $root "Sort by $codeLinesColumn"
        $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        $cells=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object { $_.Current.Name -match $codeLinesCellPattern }
        $loading=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object { $_.Current.Name -match 'Loading code lines' }
        $dependencyUnavailable=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object { $_.Current.Name -match [regex]::Escape($dependencyUnavailableText) }
        $overLimit=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object { $_.Current.Name -match [regex]::Escape($overLimitText) }
    } while (($null -eq $header -or $(if($DirectoryAdmissionUnavailableMode){$dependencyUnavailable.Count -lt 2}elseif($DirectoryAdmissionBoundaryMode){$cells.Count -lt 2 -or $overLimit.Count -lt 1 -or $loading.Count -ne 0}else{$cells.Count -lt $(if($DirectoryAggregateMode){$directoryMinimum}else{3}) -or $loading.Count -ne 0})) -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $header) { throw "$codeLinesColumn header was not installed" }
    if ($DirectoryAdmissionUnavailableMode) {
        if ($dependencyUnavailable.Count -lt 2) {
            Capture-Window $window (Join-Path $OutputDirectory 'code-lines-dependency-unavailable-failure.png')
            0..($all.Count-1) | ForEach-Object { $all.Item($_).Current.Name } |
                Where-Object { $_ -and ($_ -match 'File Count|code lines|Code lines|MFT') } |
                Sort-Object -Unique |
                Set-Content -LiteralPath (Join-Path $OutputDirectory 'dependency-visible-names.txt') -Encoding utf8
            throw "Expected at least two dependency-unavailable folder cells; found $($dependencyUnavailable.Count)"
        }
        if ($null -ne (Find-ButtonName $root 'Sort by File Count')) {
            throw 'Hidden File Count dependency unexpectedly made the built-in column visible'
        }
        Capture-Window $window (Join-Path $OutputDirectory 'code-lines-dependency-unavailable.png')
        if (-not [TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)) { throw 'Could not request clean app shutdown' }
        if (-not $process.WaitForExit(10000)) { throw 'App did not complete clean shutdown' }
        [pscustomobject]@{
            status='passed'; dependency_state='unavailable'; unavailable_cells=$dependencyUnavailable.Count
            file_count_column_hidden=$true; callback_values=0; clean_shutdown=$true
            screenshots=@('code-lines-dependency-unavailable.png')
        } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
        Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
        return
    }
    if ($DirectoryAdmissionBoundaryMode) {
        if ($cells.Count -lt 2) { throw "Expected admitted Code Lines values for files-999 and nested-counts; found $($cells.Count)" }
        if ($overLimit.Count -lt 1) { throw "Expected the files-1000 over-limit Host state; found $($overLimit.Count)" }
        if ($null -ne (Find-ButtonName $root 'Sort by File Count')) { throw 'Hidden File Count dependency unexpectedly made the built-in column visible' }
        Capture-Window $window (Join-Path $OutputDirectory 'code-lines-999-1000-boundary.png')
        if (-not [TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)) { throw 'Could not request clean app shutdown' }
        if (-not $process.WaitForExit(10000)) { throw 'App did not complete clean shutdown' }
        [pscustomobject]@{
            status='passed'; admitted_cells=$cells.Count; over_limit_cells=$overLimit.Count
            file_count_column_hidden=$true; clean_shutdown=$true
            screenshots=@('code-lines-999-1000-boundary.png')
        } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
        Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
        return
    }
    if ($DetailsColumnDragMode) {
        $toolbarPopups = @(
            @{ names = @('Create a new item'); label = 'New' },
            @{ names = @('Sort'); label = 'Sort' },
            @{ names = @('View'); label = 'View' },
            @{ names = @('Extensions',(([string][char]0x64F4)+[char]0x5145+[char]0x529F+[char]0x80FD)); label = 'Extensions' }
        )
        foreach ($probe in $toolbarPopups) {
            $button = $null
            foreach ($candidate in $probe.names) {
                $button = Find-ButtonName $root $candidate
                if ($null -ne $button) { break }
            }
            if ($null -eq $button) { throw "$($probe.label) toolbar button was unavailable" }
            Click-ElementPointer $root $button
            $deadline = [DateTime]::UtcNow.AddSeconds(3)
            $popup = $null
            do {
                Start-Sleep -Milliseconds 50
                $menus = $root.FindAll([Windows.Automation.TreeScope]::Descendants,
                    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Menu))
                if ($menus.Count -gt 0) {
                    $popup = 0..($menus.Count-1) | ForEach-Object { $menus.Item($_) } |
                        Where-Object { $_.Current.BoundingRectangle.Width -gt 0 -and $_.Current.BoundingRectangle.Height -gt 0 } |
                        Select-Object -First 1
                }
            } while ($null -eq $popup -and [DateTime]::UtcNow -lt $deadline)
            if ($null -eq $popup) { throw "$($probe.label) toolbar click did not open a menu" }
            if ($probe.label -eq 'Extensions') {
                Capture-Window $window (Join-Path $OutputDirectory 'toolbar-extensions-popup.png')
            }
            Send-Key 0x1B
        }

        $dateHeader=Find-ButtonName $root 'Sort by Date modified'
        $typeHeader=Find-ButtonName $root 'Sort by Type'
        $dateCell=Find-NamePrefix $root 'Date modified:'
        $typeCell=Find-NamePrefix $root 'Type:'
        if ($null -eq $dateHeader -or $null -eq $typeHeader -or $null -eq $dateCell -or $null -eq $typeCell) {
            Capture-Window $window (Join-Path $OutputDirectory 'column-drag-live-failure.png')
            throw 'Adjacent Date modified/Type headers or stable representative cells were unavailable'
        }

        $rightEvidence=Begin-DetailsColumnMidpointDrag $root $dateHeader $typeHeader 0.75
        try {
            $dateHeader=Find-ButtonName $root 'Sort by Date modified'
            $typeHeader=Find-ButtonName $root 'Sort by Type'
            $dateCell=Find-NamePrefix $root 'Date modified:'
            $typeCell=Find-NamePrefix $root 'Type:'
            if ($dateHeader.Current.BoundingRectangle.Left -le $typeHeader.Current.BoundingRectangle.Left -or
                $dateCell.Current.BoundingRectangle.Left -le $typeCell.Current.BoundingRectangle.Left) {
                Capture-Window $window (Join-Path $OutputDirectory 'column-drag-live-failure.png')
                throw "Rightward preview did not move header and data cell before mouse-up: pointer=$($rightEvidence | ConvertTo-Json -Compress)"
            }
            Capture-Window $window (Join-Path $OutputDirectory 'column-drag-live-right.png')
        } finally {
            [TokeiHeadfulSmoke.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
            Start-Sleep -Milliseconds 400
        }
        $dateHeader=Find-ButtonName $root 'Sort by Date modified'
        $typeHeader=Find-ButtonName $root 'Sort by Type'
        if ($dateHeader.Current.BoundingRectangle.Left -le $typeHeader.Current.BoundingRectangle.Left) {
            throw 'Rightward preview order did not remain committed after mouse-up'
        }

        Send-Key 0x74
        Start-Sleep -Milliseconds 500
        $dateHeader=Find-ButtonName $root 'Sort by Date modified'
        $typeHeader=Find-ButtonName $root 'Sort by Type'
        if ($dateHeader.Current.BoundingRectangle.Left -le $typeHeader.Current.BoundingRectangle.Left) {
            throw 'Committed details-column order did not persist after refresh'
        }

        $cancelEvidence=Begin-DetailsColumnMidpointDrag $root $dateHeader $typeHeader 0.25
        try {
            $dateHeader=Find-ButtonName $root 'Sort by Date modified'
            $typeHeader=Find-ButtonName $root 'Sort by Type'
            $dateCell=Find-NamePrefix $root 'Date modified:'
            $typeCell=Find-NamePrefix $root 'Type:'
            if ($dateHeader.Current.BoundingRectangle.Left -ge $typeHeader.Current.BoundingRectangle.Left -or
                $dateCell.Current.BoundingRectangle.Left -ge $typeCell.Current.BoundingRectangle.Left) {
                Capture-Window $window (Join-Path $OutputDirectory 'column-drag-live-failure.png')
                throw "Cancelable preview was not visible before outside release: pointer=$($cancelEvidence | ConvertTo-Json -Compress)"
            }
            $outsideX = [int]($typeHeader.Current.BoundingRectangle.Left + $typeHeader.Current.BoundingRectangle.Width / 2)
            $outsideY = [int]($typeHeader.Current.BoundingRectangle.Bottom + 160)
            [void][TokeiHeadfulSmoke.Native]::SetCursorPos($outsideX,$outsideY)
            [TokeiHeadfulSmoke.Native]::mouse_event(0x0001,0,0,0,[UIntPtr]::Zero)
        } finally {
            [TokeiHeadfulSmoke.Native]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
            Start-Sleep -Milliseconds 400
        }
        $dateHeader=Find-ButtonName $root 'Sort by Date modified'
        $typeHeader=Find-ButtonName $root 'Sort by Type'
        if ($dateHeader.Current.BoundingRectangle.Left -le $typeHeader.Current.BoundingRectangle.Left) {
            throw 'Outside release did not restore the pre-drag committed order'
        }

        $nameHeader=Find-ButtonName $root 'Sort by Name'
        if ($null -eq $nameHeader) { $nameHeader=Find-ButtonNamePrefix $root 'Name, sorted' }
        if ($null -eq $nameHeader) { throw 'Name header was unavailable' }
        Drag-ElementToElement $root $nameHeader $dateHeader
        $nameHeader=Find-ButtonName $root 'Sort by Name'
        if ($null -eq $nameHeader) { $nameHeader=Find-ButtonNamePrefix $root 'Name, sorted' }
        $leftmostHeader=@($root.FindAll([Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Button)) |
            ForEach-Object { $_ } | Where-Object { $_.Current.Name -like 'Sort by *' -or $_.Current.Name -like 'Name, sorted*' } |
            Sort-Object { $_.Current.BoundingRectangle.Left } | Select-Object -First 1)
        if ($leftmostHeader.Count -eq 0 -or $leftmostHeader[0].Current.Name -notlike '*Name*') {
            throw 'Name did not remain the fixed leftmost details column'
        }
        Capture-Window $window (Join-Path $OutputDirectory 'column-drag-live-persisted.png')

        if (-not [TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)) { throw 'Could not request clean app shutdown' }
        if (-not $process.WaitForExit(10000)) { throw 'App did not complete clean shutdown' }
        [pscustomobject]@{
            status='passed'; toolbar_buttons=@('New','Sort','View','Extensions')
            live_before_mouse_up=$true; adjacent_right_midpoint=$true
            committed_after_release=$true; persisted_after_refresh=$true
            outside_release_restored=$true; name_fixed_leftmost=$true
            right_pointer_bounds=$rightEvidence; cancel_pointer_bounds=$cancelEvidence
            screenshots=@('toolbar-extensions-popup.png','column-drag-live-right.png','column-drag-live-persisted.png')
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
        Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
        return
    }
    if ($DualCodeLinesMode) {
        $luaHeader=Find-ButtonName $root 'Sort by Code lines'
        $rustHeader=Find-ButtonName $root 'Sort by Main code lines'
        if ($null -eq $luaHeader -or $null -eq $rustHeader) {
            Capture-Window $window (Join-Path $OutputDirectory 'dual-code-lines-failure.png')
            throw 'Code lines and Main code lines were not simultaneously installed'
        }
        if (-not $DualCodeLinesRealFolderMode) {
            $nameHeader=Find-ButtonName $root 'Sort by Name'
            if ($null -eq $nameHeader) { $nameHeader=Find-ButtonNamePrefix $root 'Name, sorted' }
            $dateHeader=Find-ButtonName $root 'Sort by Date modified'
            if ($null -eq $dateHeader) { throw 'Date modified header was unavailable for reorder test' }
            Drag-ElementToElement $root $luaHeader $dateHeader
            $luaHeader=Find-ButtonName $root 'Sort by Code lines'
            $rustHeader=Find-ButtonName $root 'Sort by Main code lines'
            $dateHeader=Find-ButtonName $root 'Sort by Date modified'
            if ($luaHeader.Current.BoundingRectangle.Left -ge $dateHeader.Current.BoundingRectangle.Left) {
                Capture-Window $window (Join-Path $OutputDirectory 'column-drag-failure.png')
                throw 'Code lines was not moved before Date modified by pointer drag'
            }
            Drag-ElementToElement $root $nameHeader $rustHeader
            $nameHeader=Find-ButtonName $root 'Sort by Name'
            if ($null -eq $nameHeader) { $nameHeader=Find-ButtonNamePrefix $root 'Name, sorted' }
            if ($nameHeader.Current.BoundingRectangle.Left -gt $rustHeader.Current.BoundingRectangle.Left) {
                throw 'Name moved away from the leftmost column'
            }
            Capture-Window $window (Join-Path $OutputDirectory 'columns-reordered.png')
        }
    }
    if ($cells.Count -lt $(if($DirectoryAggregateMode){$directoryMinimum}else{3})) {
        Capture-Window $window (Join-Path $OutputDirectory 'code-lines-failure.png')
        $names=0..($all.Count-1) | ForEach-Object { $all.Item($_).Current.Name } | Where-Object { $_ -match 'code lines|Unsupported|unavailable|provider' } | Select-Object -Unique
        throw "Expected real $codeLinesColumn values; found $($cells.Count); visible: $($names -join ' | ')"
    }
    $codeLinesImage=Join-Path $OutputDirectory 'code-lines.png'
    Capture-Window $window $codeLinesImage
    $noProgressBars=if ($InputPreparationRepairMode) { $true } elseif ($DualCodeLinesMode) { Assert-NoCodeLineBarElements $root } else { Assert-NoProportionalBars $codeLinesImage }
    $alignmentColumns=@($codeLinesColumn)
    if ($DualCodeLinesMode) { $alignmentColumns=@('Code lines','Main code lines') }
    if ($null -ne (Find-ButtonName $root 'Sort by Folder size')) { $alignmentColumns += 'Folder size' }
    $alignedColumns=@(Assert-DetailsColumnAlignment $root $alignmentColumns)
    if ($DirectoryAggregateMode) {
        if ($cells.Count -lt $directoryMinimum) { throw "Expected directory $codeLinesColumn values; found $($cells.Count)" }
        $preparationFailures = @(0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object {
            $_.Current.Name -match 'Code lines input could not be prepared'
        })
        if ($preparationFailures.Count -ne 0) {
            throw "Directory Code Lines still exposed $($preparationFailures.Count) input-preparation failures"
        }
        $dualRealFolderValues = @()
        if ($DualCodeLinesRealFolderMode) {
            $expectedFolders = @('.claude','appmover','docs','explorer-core','FluentExplorer','FluentExplorer.UITests')
            $dualDeadline = [DateTime]::UtcNow.AddSeconds(90)
            do {
                $dualRealFolderValues = @()
                foreach ($folder in $expectedFolders) {
                    $mainCell = Find-CellOnRow $root $folder 'Main code lines:'
                    $totalCell = Find-CellOnRow $root $folder 'Code lines:'
                    $mainMatch = if ($null -ne $mainCell) { [regex]::Match($mainCell.Current.Name,'^Main code lines: (?:.+: )?([\d,]+)') } else { $null }
                    $totalMatch = if ($null -ne $totalCell) { [regex]::Match($totalCell.Current.Name,'^Code lines: ([\d,]+)') } else { $null }
                    if ($null -ne $mainMatch -and $mainMatch.Success -and $null -ne $totalMatch -and $totalMatch.Success) {
                        $mainValue = [UInt64]$mainMatch.Groups[1].Value.Replace(',','')
                        $totalValue = [UInt64]$totalMatch.Groups[1].Value.Replace(',','')
                        $dualRealFolderValues += [pscustomobject]@{folder=$folder;main=$mainValue;total=$totalValue}
                    }
                }
                if ($dualRealFolderValues.Count -lt $expectedFolders.Count) { Start-Sleep -Milliseconds 200 }
            } while ($dualRealFolderValues.Count -lt $expectedFolders.Count -and [DateTime]::UtcNow -lt $dualDeadline)
            if ($dualRealFolderValues.Count -ne $expectedFolders.Count) {
                Capture-Window $window (Join-Path $OutputDirectory 'dual-real-folder-failure.png')
                throw "Expected both Code Lines values for $($expectedFolders.Count) folders; found $($dualRealFolderValues.Count)"
            }
            foreach ($value in $dualRealFolderValues) {
                if ($value.total -lt $value.main) {
                    throw "All-language Code lines was below Main code lines for $($value.folder): $($value.total) < $($value.main)"
                }
            }
        } elseif ($DualCodeLinesMode) {
            $allNames = 0..($all.Count-1) | ForEach-Object { $all.Item($_).Current.Name }
            if (-not ($allNames -match '^Main code lines: Rust: 1,250\b')) {
                throw "Main code lines did not expose the dominant-language-only value Rust: 1,250"
            }
            if (-not ($allNames -match '^Code lines: 1325\b')) {
                throw "Code lines did not expose the all-language total 1,325"
            }
        }
        if (-not [TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero)) { throw 'Could not request clean app shutdown' }
        if (-not $process.WaitForExit(10000)) { throw 'App did not complete clean shutdown' }
        $descendants=@($processObserver.WaitForActiveProcessZero(10000))
        $expectedBrokerPath=Join-Path (Split-Path -Parent $Executable) 'explorer-extension-broker.exe'
        $expectedBroker=if (Test-Path -LiteralPath $expectedBrokerPath) { (Resolve-Path -LiteralPath $expectedBrokerPath).Path } else { $null }
        $unexpectedChildren=if ($InputPreparationRepairMode) { @() } else { @($descendants | Where-Object { $null -eq $expectedBroker -or ($_ -split ':\d+$')[0] -ine $expectedBroker }) }
        if ($unexpectedChildren.Count -ne 0) { throw "Unexpected plugin/tool descendant process observed: $($unexpectedChildren | ConvertTo-Json -Compress)" }
        [pscustomobject]@{status='passed'; directory_values=$cells.Count; dual_real_folder_values=$dualRealFolderValues; aligned_columns=$alignedColumns; blank_excluded_from_value=$true; no_progress_bars=$noProgressBars; observed_descendant_processes=$descendants; observed_plugin_tool_descendants=$unexpectedChildren; clean_shutdown=$true; screenshots=$(if($DualCodeLinesMode -and -not $DualCodeLinesRealFolderMode){@('code-lines.png','columns-reordered.png')}else{@('code-lines.png')})} |
            ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
        Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
        return
    }
    Click-Element $root $header
    $sortDeadline=[DateTime]::UtcNow.AddSeconds(10); $sorted=@(); $sortStable=$false
    do {
        Start-Sleep -Milliseconds 150
        $header=Find-ButtonNamePrefix $root "$codeLinesColumn, sorted"
        if($null-eq$header){$header=Find-ButtonName $root "Sort by $codeLinesColumn"}
        $sorted=Code-Line-Values $root
        $sortStable=$null -ne $header -and $sorted.Count -ge 3
        for ($i=1; $sortStable -and $i -lt $sorted.Count; $i++) {
            if ($sorted[$i] -lt $sorted[$i-1]) { $sortStable=$false }
        }
    } while (-not $sortStable -and [DateTime]::UtcNow -lt $sortDeadline)
    if ($null -eq $header) { throw "$codeLinesColumn sort state was not exposed" }
    if (-not $sortStable) { throw "$codeLinesColumn numeric ascending sort failed: $($sorted -join ',')" }
    Click-Element $root $header -Right; Start-Sleep -Milliseconds 250
    $safeModeConfirm=Find-NamePrefix $root 'Confirm and re-enable'
    if ($null -ne $safeModeConfirm) {
        Click-Element $root $safeModeConfirm
        Start-Sleep -Milliseconds 350
        $header=Find-ButtonNamePrefix $root "$codeLinesColumn, sorted"
        if ($null -eq $header) { throw "$codeLinesColumn header disappeared after Safe Mode confirmation" }
        Click-Element $root $header -Right
        Start-Sleep -Milliseconds 250
    }
    Capture-Window $window (Join-Path $OutputDirectory 'code-lines-menu.png')
    $toggle=Find-NamePrefix $root 'Show comment and blank detail'
    if ($null -eq $toggle) { throw "$codeLinesColumn detail setting was not exposed" }
    Click-Element $root $toggle
    $detailDeadline=[DateTime]::UtcNow.AddSeconds(10)
    $detail=$null
    do {
        Start-Sleep -Milliseconds 150
        $all=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
        $detail=0..($all.Count-1) | ForEach-Object { $all.Item($_) } | Where-Object { $_.Current.Name -match $codeLinesDetailPattern } | Select-Object -First 1
    } while ($null -eq $detail -and [DateTime]::UtcNow -lt $detailDeadline)
    if ($null -eq $detail) {
        Capture-Window $window (Join-Path $OutputDirectory 'code-lines-detail-missing.png')
        $toggleState=(Find-NamePrefix $root 'Show comment and blank detail').Current.Name
        throw "$codeLinesColumn comment/blank detail did not render; toggle=$toggleState"
    }
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
    [pscustomobject]@{status='passed'; values=$cells.Count; real_shell_icons='captured'; no_progress_bars=$noProgressBars; numeric_sort=$sorted; detail=$detailName; observed_descendant_processes=$descendants; observed_plugin_tool_descendants=$unexpectedChildren; clean_shutdown=$true; screenshots=@('code-lines.png','code-lines-detail.png')} |
        ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    Get-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Raw
} finally {
    if ($null -ne $lockHolder -and -not $lockHolder.HasExited) { $lockHolder.Kill(); $lockHolder.WaitForExit() }
    if ($null -ne $nativeCmd -and -not $nativeCmd.HasExited) { $nativeCmd.Kill(); $nativeCmd.WaitForExit() }
    if ($null -ne $wow64Cmd -and -not $wow64Cmd.HasExited) { $wow64Cmd.Kill(); $wow64Cmd.WaitForExit() }
    if (-not $process.HasExited) {
        if ($null -ne $window -and $window -ne [IntPtr]::Zero) { [void][TokeiHeadfulSmoke.Native]::PostMessage($window,0x0010,[UIntPtr]::Zero,[IntPtr]::Zero) }
        if (-not $process.WaitForExit(3000)) { $process.Kill(); $process.WaitForExit() }
    }
    if ($null -ne $processObserver) { $processObserver.Dispose() }
    if ($LockOwnerMode) {
        foreach($fixturePath in @($nativeCwdParent,$wow64CwdParent)) {
            $resolvedFixture=[IO.Path]::GetFullPath($fixturePath)
            $resolvedInitial=[IO.Path]::GetFullPath($InitialPath).TrimEnd([IO.Path]::DirectorySeparatorChar)+[IO.Path]::DirectorySeparatorChar
            if ($resolvedFixture.StartsWith($resolvedInitial,[StringComparison]::OrdinalIgnoreCase) -and [IO.Directory]::Exists($resolvedFixture)) {
                [IO.Directory]::Delete($resolvedFixture,$true)
            }
        }
    }
    '' | Set-Content (Join-Path $OutputDirectory 'stdout.log') -Encoding utf8
    '' | Set-Content (Join-Path $OutputDirectory 'stderr.log') -Encoding utf8
}
