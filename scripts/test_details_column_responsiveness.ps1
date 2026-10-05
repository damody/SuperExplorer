param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$TargetDirectory = 'C:\portable\OpenKoikatsu',
    [string]$ChildName = 'Content',
    [int]$Cycles = 3,
    [int]$CacheRecords = 1000
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful
Add-Type -TypeDefinition @'
using System;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;
public sealed class DetailsColumnHeartbeat : IDisposable {
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
    public readonly ConcurrentQueue<long> Milliseconds = new ConcurrentQueue<long>();
    public int Timeouts;
    volatile bool stopped;
    readonly Thread worker;
    public DetailsColumnHeartbeat(IntPtr hwnd) {
        worker = new Thread(() => {
            while (!stopped) {
                IntPtr result;
                var timer = Stopwatch.StartNew();
                var sent = SendMessageTimeout(hwnd, 0, IntPtr.Zero, IntPtr.Zero, 2, 1000, out result);
                Milliseconds.Enqueue(timer.ElapsedMilliseconds);
                if (sent == IntPtr.Zero) Interlocked.Increment(ref Timeouts);
                Thread.Sleep(20);
            }
        });
        worker.IsBackground = true;
        worker.Start();
    }
    public void Dispose() { stopped = true; worker.Join(); }
}
'@

$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$TargetDirectory = [IO.Path]::GetFullPath($TargetDirectory)
$child = Join-Path $TargetDirectory $ChildName
if (-not (Test-Path -LiteralPath $child -PathType Container)) { throw 'Target child directory is missing.' }
$context = Start-UitestExplorer -InitialPath $child -OutputDirectory $OutputDirectory `
    -Executable $Executable -SkipBuild -AdditionalEnvironment @{
        EXPLORER_AUTO_CLOSE_MS = '120000'
        SUPEREXPLORER_DIRECTORY_FACTS_VALIDATION_DELAY_MS = '5000'
    }
[void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 20, 20, 1440, 880, 0x0040)
[void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
$heartbeat = [DetailsColumnHeartbeat]::new($context.Hwnd)
$measurements = @()
try {
    foreach ($cycle in 1..$Cycles) {
        # All records are in the isolated test profile. Changing the cache
        # window must prune them while the window continues processing input.
        foreach ($namespace in @('rust-code-lines', 'lua-code-lines')) {
            $cache = Join-Path $OutputDirectory "localappdata\SuperExplorer\data-column-cache\v1\$namespace"
            New-Item -ItemType Directory -Force -Path $cache | Out-Null
            foreach ($record in 1..$CacheRecords) {
                [IO.File]::WriteAllText((Join-Path $cache "probe-$cycle-$record.json"), '{"path":"C:\\outside-probe\\file.rs"}')
            }
        }
        $up = Find-UitestElement -Root $context.Root -Description 'Up toolbar button' -Predicate {
            param($element) $element.Current.Name -in @('上一層', 'Up')
        }
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $invoke = $null
        if ($up.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
            ([Windows.Automation.InvokePattern]$invoke).Invoke()
        } else {
            Invoke-UitestClick -Element $up
        }
        $row = Find-UitestFileItem -Root $context.Root -Name $ChildName
        $readyMs = $timer.ElapsedMilliseconds
        # Keep probing through the deliberately delayed directory facts query.
        Start-Sleep -Seconds 6
        $measurements += @{ cycle = $cycle; listing_ready_ms = $readyMs }
        if ($cycle -lt $Cycles) {
            $row = Find-UitestFileItem -Root $context.Root -Name $ChildName
            Invoke-UitestClick -Element $row -Double
            Start-Sleep -Milliseconds 400
        }
    }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'ready.png')
} finally {
    $heartbeat.Dispose()
    $samples = @($heartbeat.Milliseconds.ToArray())
    $remainingCacheRecords = @(Get-ChildItem -LiteralPath (Join-Path $OutputDirectory 'localappdata\SuperExplorer\data-column-cache\v1\rust-code-lines') -Filter 'probe-*' -File).Count
    $prunedCacheRecords = $Cycles * $CacheRecords - $remainingCacheRecords
    $report = [ordered]@{
        status = $(if ($heartbeat.Timeouts -eq 0 -and $measurements.Count -eq $Cycles -and $prunedCacheRecords -ge $CacheRecords) { 'PASS' } else { 'FAIL' })
        executable = $Executable
        target = $TargetDirectory
        cycles = $measurements
        cache_records_per_cycle = 2 * $CacheRecords
        pruned_rust_cache_records = $prunedCacheRecords
        directory_facts_delay_ms = 5000
        heartbeat_samples = $samples.Count
        heartbeat_max_ms = ($samples | Measure-Object -Maximum).Maximum
        heartbeat_timeouts = $heartbeat.Timeouts
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    Stop-UitestExplorer -Context $context
}
$report | ConvertTo-Json -Depth 5
if ($report.status -ne 'PASS') { throw 'Details columns blocked the window heartbeat.' }
