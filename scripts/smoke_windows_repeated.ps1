param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [ValidateRange(2, 100)]
    [int]$Runs = 10,
    [int]$TimeoutSeconds = 20,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = if ($env:CARGO_TARGET_DIR) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $env:CARGO_TARGET_DIR))
    }
} else {
    Join-Path $workspaceRoot 'target'
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) {
        throw "artifact finalization failed with exit code $LASTEXITCODE"
    }
}

$samples = @()
for ($run = 1; $run -le $Runs; $run++) {
    $output = @(& (Join-Path $PSScriptRoot 'smoke_windows_lifecycle.ps1') `
        -Profile $Profile -TimeoutSeconds $TimeoutSeconds -SkipBuild)
    $passedLine = $output | Where-Object { $_ -like 'Headful lifecycle smoke passed:*' } | Select-Object -Last 1
    if (-not $passedLine) {
        throw "run $run did not return an evidence directory"
    }
    $evidenceDirectory = $passedLine.Substring('Headful lifecycle smoke passed:'.Length).Trim()
    $summaryPath = Join-Path $evidenceDirectory 'summary.json'
    $summary = Get-Content -Raw -Encoding utf8 -LiteralPath $summaryPath | ConvertFrom-Json
    $samples += [pscustomobject]@{
        run = $run
        process_id = $summary.process_id
        ready_duration_ms = [double]$summary.ready_duration_ms
        thread_count = $summary.ready_resource_sample.thread_count
        process_handle_count = $summary.ready_resource_sample.process_handle_count
        gdi_handle_count = $summary.ready_resource_sample.gdi_handle_count
        user_handle_count = $summary.ready_resource_sample.user_handle_count
        working_set_bytes = $summary.ready_resource_sample.working_set_bytes
        peak_working_set_bytes = $summary.ready_resource_sample.peak_working_set_bytes
        evidence = $evidenceDirectory
    }
    if (Get-Process -Id $summary.process_id -ErrorAction SilentlyContinue) {
        throw "run $run left process $($summary.process_id) alive"
    }
}

$reportDirectory = Join-Path $targetRoot ('smoke-repeat-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
$warmDurations = @($samples | Select-Object -Skip 1 | ForEach-Object { [double]$_.ready_duration_ms } | Sort-Object)
$allDurations = @($samples | ForEach-Object { [double]$_.ready_duration_ms } | Sort-Object)
function Get-Median([double[]]$values) {
    if ($values.Count -eq 0) { return $null }
    $middle = [int][math]::Floor($values.Count / 2)
    if ($values.Count % 2 -eq 1) { return $values[$middle] }
    return ($values[$middle - 1] + $values[$middle]) / 2
}
function Get-P95([double[]]$values) {
    if ($values.Count -eq 0) { return $null }
    $index = [math]::Max(0, [math]::Ceiling($values.Count * 0.95) - 1)
    return $values[$index]
}
$report = [ordered]@{
    profile = $Profile
    run_count = $Runs
    all_processes_exited = $true
    lifecycle_and_diagnostics_flush_validated_each_run = $true
    startup = [ordered]@{
        cold_first_process_ms = [double]$samples[0].ready_duration_ms
        warm_sample_count = $warmDurations.Count
        warm_median_ms = Get-Median $warmDurations
        warm_p95_ms = Get-P95 $warmDurations
        all_process_median_ms = Get-Median $allDurations
        all_process_p95_ms = Get-P95 $allDurations
        definition = 'cold is the first post-build process; warm samples are fresh processes with OS/GPU caches retained'
    }
    samples = $samples
    ranges = [ordered]@{
        thread_count = [ordered]@{ min = ($samples.thread_count | Measure-Object -Minimum).Minimum; max = ($samples.thread_count | Measure-Object -Maximum).Maximum }
        process_handle_count = [ordered]@{ min = ($samples.process_handle_count | Measure-Object -Minimum).Minimum; max = ($samples.process_handle_count | Measure-Object -Maximum).Maximum }
        gdi_handle_count = [ordered]@{ min = ($samples.gdi_handle_count | Measure-Object -Minimum).Minimum; max = ($samples.gdi_handle_count | Measure-Object -Maximum).Maximum }
        user_handle_count = [ordered]@{ min = ($samples.user_handle_count | Measure-Object -Minimum).Minimum; max = ($samples.user_handle_count | Measure-Object -Maximum).Maximum }
        peak_working_set_bytes = [ordered]@{ min = ($samples.peak_working_set_bytes | Measure-Object -Minimum).Minimum; max = ($samples.peak_working_set_bytes | Measure-Object -Maximum).Maximum }
    }
    completed_utc = [DateTime]::UtcNow.ToString('o')
}
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $reportDirectory 'summary.json')
$samples | Export-Csv -NoTypeInformation -Encoding utf8 (Join-Path $reportDirectory 'samples.csv')

Write-Output "Repeated headful smoke passed: $reportDirectory"
Write-Output "Runs: $Runs; crashes: 0; residual processes: 0; lifecycle/diagnostics flush: $Runs/$Runs"
Write-Output "Startup: cold first $($report.startup.cold_first_process_ms) ms; warm median $($report.startup.warm_median_ms) ms; warm p95 $($report.startup.warm_p95_ms) ms"
Write-Output "Threads: $($report.ranges.thread_count.min)-$($report.ranges.thread_count.max); process handles: $($report.ranges.process_handle_count.min)-$($report.ranges.process_handle_count.max); GDI: $($report.ranges.gdi_handle_count.min)-$($report.ranges.gdi_handle_count.max); User: $($report.ranges.user_handle_count.min)-$($report.ranges.user_handle_count.max)"
