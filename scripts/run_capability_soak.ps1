param(
    [ValidateRange(1, 20)]
    [int]$Runs = 3,
    [string]$OutputDirectory,
    [string]$WorkloadFilter
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $workspaceRoot ('target\capability-soak-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not ('CapabilitySoak.GuiResources' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace CapabilitySoak {
    public static class GuiResources {
        [DllImport("user32.dll")]
        public static extern uint GetGuiResources(IntPtr process, uint flags);
    }
}
'@
}

$cargo = (Get-Command cargo -ErrorAction Stop).Source
$workloads = @(
    [ordered]@{ name = 'multi-tab'; arguments = @('test','-p','explorer-shell-win','sta::tests::end_to_end_two_tabs_navigation_history_and_watcher_are_isolated','--','--nocapture','--exact') },
    [ordered]@{ name = 'folder-100k'; arguments = @('test','-p','explorer-shell-win','sta::tests::real_100k_dataset_reports_latency_memory_count_and_batch_depth','--','--ignored','--nocapture','--exact') },
    [ordered]@{ name = 'file-operations'; arguments = @('test','-p','explorer-shell-win','sta::tests::real_file_operations_match_safe_disk_oracle','--','--nocapture','--exact') },
    [ordered]@{ name = 'clipboard-ole'; arguments = @('test','-p','explorer-shell-win','sta::tests::real_ole_clipboard_copy_cut_paste_crosses_tabs_and_matches_disk','--','--nocapture','--exact') },
    [ordered]@{ name = 'ole-drag'; arguments = @('test','-p','explorer-shell-win','drag_drop::tests::real_do_drag_drop_cancel_soak_releases_process_resources','--','--nocapture','--exact') },
    [ordered]@{ name = 'context-menu'; arguments = @('test','-p','explorer-shell-win','context_menu::tests::real_popup_cancel_soak_forwards_messages_and_releases_menu_resources','--','--nocapture','--exact') },
    [ordered]@{ name = 'search-100k'; arguments = @('test','-p','explorer-search','engine::tests::measures_one_hundred_thousand_real_items','--','--ignored','--nocapture','--exact') }
    [ordered]@{ name = 'roadmap-combined'; arguments = @('test','-p','explorer-app','--test','roadmap_combined','--locked') }
    [ordered]@{ name = 'namespace-roadmap'; arguments = @('test','-p','explorer-shell-win','real_namespace_root_fixture_matrix','--locked','--','--nocapture') }
    [ordered]@{ name = 'thumbnail-roadmap'; arguments = @('test','-p','explorer-jobs','thousand_scroll_zoom_resize_navigation_replacements_stay_bounded','--locked') }
    [ordered]@{ name = 'broker-preview-roadmap'; arguments = @('test','-p','explorer-extension-broker','--test','process_boundary','persistent_preview_session_attaches_resizes_focuses_and_unloads_by_generation','--locked') }
)
if (-not [string]::IsNullOrWhiteSpace($WorkloadFilter)) {
    $workloadNames = [string[]]@($WorkloadFilter.Split(',') | ForEach-Object Trim | Where-Object Length)
    $requested = [Collections.Generic.HashSet[string]]::new(
        $workloadNames,
        [StringComparer]::OrdinalIgnoreCase
    )
    $workloads = @($workloads | Where-Object { $requested.Contains($_.name) })
    if ($workloads.Count -ne $requested.Count) {
        throw "unknown workload requested; valid names: multi-tab, folder-100k, file-operations, clipboard-ole, ole-drag, context-menu, search-100k, roadmap-combined, namespace-roadmap, thumbnail-roadmap, broker-preview-roadmap"
    }
}

function Get-DescendantProcessIds([int]$RootId, [DateTime]$RootStarted) {
    $known = [Collections.Generic.HashSet[int]]::new()
    [void]$known.Add($RootId)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($candidate in Get-CimInstance Win32_Process) {
            $created = [DateTime]$candidate.CreationDate
            $providerOwned = $candidate.Name -in @('explorer.exe','dllhost.exe','conhost.exe')
            if (-not $providerOwned -and $created -ge $RootStarted.AddSeconds(-1) -and $known.Contains([int]$candidate.ParentProcessId) -and $known.Add([int]$candidate.ProcessId)) {
                $changed = $true
            }
        }
    }
    return @($known)
}

function Get-Percentile([double[]]$Values, [int]$Percent) {
    $sorted = @($Values | Sort-Object)
    $rank = [math]::Ceiling($sorted.Count * $Percent / 100.0)
    return $sorted[[math]::Max(0, $rank - 1)]
}

$results = @()
foreach ($workload in $workloads) {
    $runResults = @()
    for ($run = 1; $run -le $Runs; $run++) {
        $stdoutPath = Join-Path $OutputDirectory ("{0}-run-{1}-stdout.log" -f $workload.name, $run)
        $stderrPath = Join-Path $OutputDirectory ("{0}-run-{1}-stderr.log" -f $workload.name, $run)
        $started = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $cargo -ArgumentList $workload.arguments `
            -WorkingDirectory $workspaceRoot -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
        # Windows PowerShell 5 can lose ExitCode for a fast process unless its
        # native handle is materialized before the process exits.
        $processHandle = $process.Handle
        $process.Refresh()
        $rootStarted = $process.StartTime
        $peakProcesses = 1
        $peakThreads = $process.Threads.Count
        $peakHandles = $process.HandleCount
        $peakWorkingSet = $process.WorkingSet64
        $peakGdi = [CapabilitySoak.GuiResources]::GetGuiResources($process.Handle, 0)
        $peakUser = [CapabilitySoak.GuiResources]::GetGuiResources($process.Handle, 1)
        # A long-running soak can observe a PID that Windows later reuses for an unrelated
        # process. Keep the creation timestamp as part of the identity so PID reuse is not
        # reported as a leaked descendant.
        $observedProcesses = [Collections.Generic.Dictionary[int,long]]::new()
        while (-not $process.HasExited) {
            $ids = @(Get-DescendantProcessIds $process.Id $rootStarted)
            $peakProcesses = [math]::Max($peakProcesses, $ids.Count)
            $threads = 0
            $handles = 0
            $workingSet = 0L
            $gdi = 0
            $user = 0
            foreach ($id in $ids) {
                $sample = Get-Process -Id $id -ErrorAction SilentlyContinue
                if ($null -ne $sample) {
                    if (-not $observedProcesses.ContainsKey([int]$id)) {
                        try {
                            $observedProcesses.Add(
                                [int]$id,
                                $sample.StartTime.ToUniversalTime().Ticks
                            )
                        } catch {
                            # The process may exit before StartTime is readable.
                        }
                    }
                    $threads += $sample.Threads.Count
                    $handles += $sample.HandleCount
                    $workingSet += $sample.WorkingSet64
                    try {
                        $gdi += [CapabilitySoak.GuiResources]::GetGuiResources($sample.Handle, 0)
                        $user += [CapabilitySoak.GuiResources]::GetGuiResources($sample.Handle, 1)
                    } catch {
                        # A short-lived child may exit between sampling and querying GUI counters.
                    }
                    $sample.Dispose()
                }
            }
            $peakThreads = [math]::Max($peakThreads, $threads)
            $peakHandles = [math]::Max($peakHandles, $handles)
            $peakWorkingSet = [math]::Max($peakWorkingSet, $workingSet)
            $peakGdi = [math]::Max($peakGdi, $gdi)
            $peakUser = [math]::Max($peakUser, $user)
            Start-Sleep -Milliseconds 100
            $process.Refresh()
        }
        $process.WaitForExit()
        $started.Stop()
        $process.Refresh()
        $exitCode = $process.ExitCode
        $rootId = $process.Id
        $process.Dispose()
        $reapDeadline = [DateTime]::UtcNow.AddSeconds(2)
        do {
            $residual = @($observedProcesses.Keys | Where-Object {
                if ($_ -eq $rootId) { return $false }
                $candidate = Get-Process -Id $_ -ErrorAction SilentlyContinue
                if ($null -eq $candidate) { return $false }
                try {
                    if ($candidate.StartTime.ToUniversalTime().Ticks -ne $observedProcesses[$_]) {
                        return $false
                    }
                } catch {
                    return $false
                }
                # Shell tests can activate Windows-owned out-of-process providers. Explorer,
                # dllhost and conhost are not app descendants to reap and may intentionally be
                # reparented to the session. App/broker/worker/test processes remain strict.
                return $candidate.ProcessName -notin @('explorer','dllhost','conhost')
            })
            if ($residual.Count -gt 0) {
                Start-Sleep -Milliseconds 100
            }
        } while ($residual.Count -gt 0 -and [DateTime]::UtcNow -lt $reapDeadline)
        $combinedOutput = ((Get-Content -Raw -ErrorAction SilentlyContinue $stdoutPath) +
            (Get-Content -Raw -ErrorAction SilentlyContinue $stderrPath))
        $runResult = [pscustomobject][ordered]@{
            run = $run
            duration_ms = [math]::Round($started.Elapsed.TotalMilliseconds, 3)
            exit_code = $exitCode
            peak_processes = $peakProcesses
            peak_threads = $peakThreads
            peak_handles = $peakHandles
            peak_working_set_bytes = $peakWorkingSet
            peak_gdi_objects = $peakGdi
            peak_user_objects = $peakUser
            residual_descendant_pids = $residual
            max_batch = if ($combinedOutput -match 'max_batch=(\d+)') { [int]$Matches[1] } else { $null }
            max_queue = if ($combinedOutput -match 'max_queue=(\d+)') { [int]$Matches[1] } else { $null }
            memory_delta_bytes = if ($combinedOutput -match 'memory_delta=(\d+)') { [int64]$Matches[1] } elseif ($combinedOutput -match 'delta=(\d+)') { [int64]$Matches[1] } else { $null }
            stdout = [IO.Path]::GetFileName($stdoutPath)
            stderr = [IO.Path]::GetFileName($stderrPath)
        }
        $runResults += $runResult
        Write-Output ("{0} run {1}/{2}: exit={3}, {4} ms, peak threads={5}, handles={6}" -f $workload.name, $run, $Runs, $exitCode, $runResult.duration_ms, $peakThreads, $peakHandles)
        if ($exitCode -ne 0 -or $residual.Count -ne 0) {
            throw "capability soak failed or leaked descendants: $($workload.name) run $run"
        }
    }
    $durations = [double[]]@($runResults | ForEach-Object duration_ms)
    $results += [pscustomobject][ordered]@{
        name = $workload.name
        command = 'cargo ' + ($workload.arguments -join ' ')
        runs = $runResults
        summary = [ordered]@{
            median_ms = Get-Percentile $durations 50
            p95_ms = Get-Percentile $durations 95
            max_peak_threads = ($runResults | Measure-Object peak_threads -Maximum).Maximum
            max_peak_handles = ($runResults | Measure-Object peak_handles -Maximum).Maximum
            max_peak_working_set_bytes = ($runResults | Measure-Object peak_working_set_bytes -Maximum).Maximum
            max_peak_gdi_objects = ($runResults | Measure-Object peak_gdi_objects -Maximum).Maximum
            max_peak_user_objects = ($runResults | Measure-Object peak_user_objects -Maximum).Maximum
            leak_oracle = 'all exits 0; no residual descendant process; in-test resource counters/assertions passed'
        }
    }
}

$report = [ordered]@{
    schema_version = 1
    captured_utc = [DateTime]::UtcNow.ToString('o')
    windows_build = (Get-CimInstance Win32_OperatingSystem).BuildNumber
    rustc = (& rustc -V).Trim()
    cargo = (& cargo -V).Trim()
    repetitions_per_workload = $Runs
    percentile_method = 'nearest rank; with three runs p95 equals the maximum'
    debugger_attached = $false
    workloads = $results
}
$report | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')

$markdown = @('# Capability soak report', '', "- Captured UTC: $($report.captured_utc)", "- Runs per workload: $Runs", '- Percentile: nearest rank', '', '| Workload | Median ms | p95 ms | Peak threads | Peak handles | Peak working set | Leak oracle |', '|---|---:|---:|---:|---:|---:|---|')
foreach ($result in $results) {
    $summary = $result.summary
    $markdown += "| $($result.name) | $($summary.median_ms) | $($summary.p95_ms) | $($summary.max_peak_threads) | $($summary.max_peak_handles) | $($summary.max_peak_working_set_bytes) | pass |"
}
$markdown | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.md')
Write-Output "Capability soak completed: $OutputDirectory"
