param(
    [string]$Executable = 'target\debug\SuperExplorer.exe',
    [string]$PluginDll = 'sdk\fixtures\rust-folder-size-visual-column\target\x86_64-pc-windows-msvc\debug\rust_folder_size_visual_column.dll',
    [string]$InitialPath = '',
    [string]$OutputDirectory = 'target\folder-size-ttl-headful',
    [int]$ExpiryWaitSeconds = 62
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

if ([string]::IsNullOrWhiteSpace($InitialPath)) {
    $InitialPath = Join-Path $OutputDirectory 'fixture'
    if (-not (Test-Path $InitialPath)) {
        New-Item -ItemType Directory -Force -Path $InitialPath | Out-Null
        foreach ($index in 0..29) {
            $dir = Join-Path $InitialPath ("dir-{0:D2}" -f $index)
            New-Item -ItemType Directory -Force -Path $dir | Out-Null
            $size = 1024 + ($index * 512)
            [IO.File]::WriteAllBytes((Join-Path $dir 'payload.bin'), [byte[]]::new($size))
        }
    }
} else { $InitialPath = (Resolve-Path $InitialPath).Path }

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
if (-not ('FolderSizeTtlHeadful.Native' -as [type])) {
    Add-Type @'
using System; using System.Runtime.InteropServices;
namespace FolderSizeTtlHeadful { public static class Native {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("dwmapi.dll")] public static extern int DwmFlush();
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e);
 [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern IntPtr CreateFileW(string n,uint access,uint share,IntPtr sec,uint creation,uint flags,IntPtr templ);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetFileTime(IntPtr h,IntPtr c,IntPtr a,ref long w);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
} }
'@
}
function Capture([IntPtr]$window,[string]$path) {
    [FolderSizeTtlHeadful.Native]::DwmFlush(); $r=[FolderSizeTtlHeadful.Native+Rect]::new()
    if (-not [FolderSizeTtlHeadful.Native]::GetWindowRect($window,[ref]$r)) { throw 'GetWindowRect failed' }
    $b=[Drawing.Bitmap]::new($r.Right-$r.Left,$r.Bottom-$r.Top); try { $g=[Drawing.Graphics]::FromImage($b); try { $dc=$g.GetHdc(); try { if (-not [FolderSizeTtlHeadful.Native]::PrintWindow($window,$dc,2)) { throw 'PrintWindow failed' } } finally {$g.ReleaseHdc($dc)} } finally {$g.Dispose()}; $b.Save($path,[Drawing.Imaging.ImageFormat]::Png) } finally {$b.Dispose()}
}
function Find-Name($root,[string]$name) { $root.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty,$name)) }
function Find-Prefix($root,[string]$prefix) { $a=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition); 0..($a.Count-1) | % {$a.Item($_)} | ? {$_.Current.Name -like "$prefix*"} | select -First 1 }
function Key([byte]$key) { [FolderSizeTtlHeadful.Native]::keybd_event($key,0,0,[UIntPtr]::Zero); [FolderSizeTtlHeadful.Native]::keybd_event($key,0,2,[UIntPtr]::Zero) }
function Chord([byte]$modifier,[byte]$key) { [FolderSizeTtlHeadful.Native]::keybd_event($modifier,0,0,[UIntPtr]::Zero); Key $key; [FolderSizeTtlHeadful.Native]::keybd_event($modifier,0,2,[UIntPtr]::Zero) }

$StatusTexts = @('Folder size: Host cache','Folder size: Host cache...','Folder size: MFT service','Folder size: MFT service...','Folder size: MFT unavailable')
function Get-Cells($root) {
    $a=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    $cells=New-Object System.Collections.Generic.List[object]
    for ($i=0; $i -lt $a.Count; $i++) {
        $e=$a.Item($i); $n=$e.Current.Name
        if ($n -like 'Folder size: *' -and ($StatusTexts -notcontains $n)) { $cells.Add($e) }
    }
    return ,$cells
}
function Get-BackendStatus($root) {
    $byId=$root.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,'status-folder-size-backend'))
    if ($byId -and $byId.Current.Name) { return $byId.Current.Name }
    $a=$root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)
    for ($i=0; $i -lt $a.Count; $i++) {
        $n=$a.Item($i).Current.Name
        foreach ($s in $StatusTexts) { if ($n -eq $s) { return $n } }
        if ($n -like '*Folder size: Host cache*') { return $n }
        if ($n -like '*Folder size: MFT*') { return $n }
    }
    return $null
}
function Wait-CellsExact($root,[int]$seconds,[switch]$ReturnLoadingSeen) {
    $deadline=[DateTime]::UtcNow.AddSeconds($seconds)
    $loadingSeen=$false
    do {
        Start-Sleep -Milliseconds 150
        $cells=Get-Cells $root
        $loading = @($cells | Where-Object { $_.Current.Name -match 'Loading|Calculating' }).Count
        if ($loading -gt 0) { $loadingSeen=$true }
        $exact = ($cells.Count -gt 0 -and $loading -eq 0)
    } while (-not $exact -and [DateTime]::UtcNow -lt $deadline)
    [pscustomobject]@{ exact=$exact; loading_seen=$loadingSeen; cell_count=$cells.Count; first=if($cells.Count){$cells[0].Current.Name}else{$null} }
}
function Bump-Mtimes([string]$root) {
    Get-ChildItem -LiteralPath $root -Directory | ForEach-Object {
        $h=[FolderSizeTtlHeadful.Native]::CreateFileW($_.FullName,0x100,7,[IntPtr]::Zero,3,0x02000000,[IntPtr]::Zero)
        if ($h -eq [IntPtr]::new(-1)) { throw "CreateFileW failed for $($_.FullName)" }
        try { $ft=[DateTime]::UtcNow.ToFileTimeUtc(); [FolderSizeTtlHeadful.Native]::SetFileTime($h,[IntPtr]::Zero,[IntPtr]::Zero,[ref]$ft) | Out-Null }
        finally { [FolderSizeTtlHeadful.Native]::CloseHandle($h) | Out-Null }
    }
}

$diag=Join-Path $OutputDirectory 'diagnostics.json'
$psi=[Diagnostics.ProcessStartInfo]::new()
$psi.FileName=$Executable
$psi.Arguments="--plugin-dll `"$PluginDll`""
$psi.WorkingDirectory=$workspace
$psi.UseShellExecute=$false
$psi.EnvironmentVariables['EXPLORER_VISUAL_FIXTURE']='1'
$psi.EnvironmentVariables['EXPLORER_VISUAL_REAL_SHELL']='1'
$psi.EnvironmentVariables['EXPLORER_VISUAL_STATE']='populated'
$psi.EnvironmentVariables['EXPLORER_VISUAL_DIAGNOSTICS']=$diag
$psi.EnvironmentVariables['EXPLORER_INITIAL_PATH']=$InitialPath
$psi.EnvironmentVariables['EXPLORER_LOG_DIR']=$OutputDirectory
# Requires the SuperExplorer MFT Service to be running so folder sizes resolve
# through the MFT folder-aggregate pipeline. When the app is launched under the
# installer the service is started automatically as LocalSystem.
$process=[Diagnostics.Process]::Start($psi)
try {
    $until=[DateTime]::UtcNow.AddSeconds(40)
    do { Start-Sleep -Milliseconds 200; $process.Refresh(); $window=$process.MainWindowHandle } while (($window -eq [IntPtr]::Zero -or -not (Test-Path $diag)) -and [DateTime]::UtcNow -lt $until)
    if ($window -eq [IntPtr]::Zero) { throw 'Timed out waiting for SuperExplorer' }
    [FolderSizeTtlHeadful.Native]::SetForegroundWindow($window) | Out-Null
    $root=[Windows.Automation.AutomationElement]::FromHandle($window)

    # Phase 1: cold compute baseline.
    $header=Find-Name $root 'Sort by Folder size'
    if ($null -eq $header) { throw 'Folder size column header was not exposed' }
    $cold=[Diagnostics.Stopwatch]::StartNew()
    $coldResult=Wait-CellsExact $root 60
    $cold.Stop()
    if (-not $coldResult.exact) { throw 'Cold folder-size computation did not complete' }
    $coldImage=Join-Path $OutputDirectory 'ttl-cold.png'; Capture $window $coldImage
    $coldStatus=Get-BackendStatus $root

    # Phase 2: simulate "C: keeps being edited" (mtime bump) then open tab B inside TTL.
    Bump-Mtimes $InitialPath
    Start-Sleep -Milliseconds 800
    $ttlHit=[Diagnostics.Stopwatch]::StartNew()
    Chord 0x11 0x54
    $ttlResult=Wait-CellsExact $root 60
    $ttlHit.Stop()
    $ttlStatus=Get-BackendStatus $root
    $hostCacheHit = ($null -ne $ttlStatus -and $ttlStatus -match 'Host cache')
    $reuseImage=Join-Path $OutputDirectory 'ttl-reuse.png'; Capture $window $reuseImage

    # Phase 3: past the TTL window a changed mtime must rescan.
    Start-Sleep -Seconds $ExpiryWaitSeconds
    Bump-Mtimes $InitialPath
    Start-Sleep -Milliseconds 800
    $expired=[Diagnostics.Stopwatch]::StartNew()
    Chord 0x11 0x54
    $expiredResult=Wait-CellsExact $root 60 -ReturnLoadingSeen
    $expired.Stop()
    $expiredStatus=Get-BackendStatus $root
    $expiredImage=Join-Path $OutputDirectory 'ttl-expired.png'; Capture $window $expiredImage

    $passed = ($coldResult.exact -and $hostCacheHit -and $expiredResult.exact -and $expiredResult.loading_seen)
    $passed = ($passed -and -not $ttlResult.loading_seen)
    [pscustomobject]@{
        status=if($passed){'passed'}else{'failed'}
        case_id='folder-size-cache-ttl-headful'
        schema='superexplorer.folder-size-cache-ttl.v1'
        fixture_path=$InitialPath
        cold_ms=$cold.ElapsedMilliseconds
        cold_cell_count=$coldResult.cell_count
        cold_backend_status=$coldStatus
        ttl_hit_ms=$ttlHit.ElapsedMilliseconds
        ttl_hit_cell_count=$ttlResult.cell_count
        ttl_host_cache_status=$hostCacheHit
        ttl_backend_status=$ttlStatus
        expired_ms=$expired.ElapsedMilliseconds
        expired_loading_seen=$expiredResult.loading_seen
        expired_backend_status=$expiredStatus
        passed=@($coldResult.exact,$hostCacheHit,$expiredResult.exact,$expiredResult.loading_seen,(-not $ttlResult.loading_seen))
        screenshots=@('ttl-cold.png','ttl-reuse.png','ttl-expired.png')
    } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    $report = Get-Content (Join-Path $OutputDirectory 'report.json') -Raw
    Write-Output $report
} finally {
    if ($process -and -not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
