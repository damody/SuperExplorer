param(
    [Parameter(Mandatory=$true)][ValidateSet('unfocused','focused')][string]$Mode,
    [Parameter(Mandatory=$true)][int]$DurationSeconds,
    [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference = 'Stop'
$cache = Join-Path $env:ProgramData 'SuperExplorer\MftIndex'
$fixture = Join-Path (Split-Path -Parent $PSScriptRoot) 'target\mft-installed-fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutputPath) | Out-Null

function Snapshot-Cache {
    $result = @{}
    Get-ChildItem -LiteralPath $cache -File -Force -ErrorAction SilentlyContinue | ForEach-Object {
        $result[$_.Name] = "{0}:{1}" -f $_.Length,$_.LastWriteTimeUtc.Ticks
    }
    return $result
}
function Raw-Counters {
    $rows = @{}
    Get-CimInstance Win32_PerfRawData_PerfProc_Process | Where-Object {
        $_.Name -in @('MsMpEng','superexplorer-mft-service')
    } | ForEach-Object {
        $rows[$_.Name] = [ordered]@{
            process_id = [uint64]$_.IDProcess
            cpu_ticks = [uint64]$_.PercentProcessorTime
            timestamp_100ns = [uint64]$_.Timestamp_Sys100NS
            read_bytes = [uint64]$_.IOReadBytesPersec
            write_bytes = [uint64]$_.IOWriteBytesPersec
            data_bytes = [uint64]$_.IODataBytesPersec
            working_set_private = [uint64]$_.WorkingSetPrivate
        }
    }
    return $rows
}

$started = [DateTimeOffset]::UtcNow
$before = Snapshot-Cache
$previous = $before
$counterStart = Raw-Counters
$events = [System.Collections.Generic.List[object]]::new()
$mutationCount = 0
$timer = [Diagnostics.Stopwatch]::StartNew()
while ($timer.Elapsed.TotalSeconds -lt $DurationSeconds) {
    if (($mutationCount -eq 0) -or ($timer.Elapsed.TotalSeconds -ge ($mutationCount * 2))) {
        $slot = $mutationCount % 32
        [IO.File]::WriteAllText((Join-Path $fixture ("mutation-{0:D2}.txt" -f $slot)),
            "mode=$Mode sequence=$mutationCount utc=$([DateTimeOffset]::UtcNow.ToString('O'))")
        $mutationCount++
    }
    Start-Sleep -Milliseconds 1000
    $current = Snapshot-Cache
    foreach ($name in @($previous.Keys + $current.Keys | Sort-Object -Unique)) {
        $old = $previous[$name]; $new = $current[$name]
        if ($old -ne $new) {
            $events.Add([ordered]@{elapsed_ms=[int64]$timer.ElapsedMilliseconds;name=$name;before=$old;after=$new})
        }
    }
    $previous = $current
}
$after = Snapshot-Cache
$counterEnd = Raw-Counters
$result = [ordered]@{
    schema = 'superexplorer-mft-installed-trace-v1'
    mode = $Mode
    started_utc = $started.ToString('O')
    ended_utc = [DateTimeOffset]::UtcNow.ToString('O')
    duration_seconds = [math]::Round($timer.Elapsed.TotalSeconds,3)
    mutation_count = $mutationCount
    cache_files_before = $before.Count
    cache_files_after = $after.Count
    cache_events = $events
    counters_before = $counterStart
    counters_after = $counterEnd
}
$json = $result | ConvertTo-Json -Depth 8
[IO.File]::WriteAllText($OutputPath,$json,[Text.UTF8Encoding]::new($false))
