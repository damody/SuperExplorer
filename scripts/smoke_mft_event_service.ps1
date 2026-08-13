param(
    [ValidateSet('unfocused', 'focused')]
    [string]$Mode = 'unfocused',
    [string]$OutputDirectory = "$PSScriptRoot\..\target\mft-event-service-evidence",
    [int]$DurationSeconds = 120
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$captureScript = Join-Path $PSScriptRoot 'capture_mft_installed_evidence.ps1'
$cache = Join-Path $env:ProgramData 'SuperExplorer\MftIndex'
$tracePath = Join-Path $OutputDirectory 'trace.json'
$reportPath = Join-Path $OutputDirectory 'report.json'

$service = Get-CimInstance Win32_Service -Filter "Name='SuperExplorerMft'"
if (-not $service -or $service.State -ne 'Running') {
    throw 'SuperExplorerMft must already be installed and running.'
}
if (-not (Test-Path -LiteralPath $cache -PathType Container)) {
    throw "MFT cache directory is missing: $cache"
}

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

function Get-CacheInventory {
    @(Get-ChildItem -LiteralPath $cache -File -Force | Sort-Object Name | ForEach-Object {
        [ordered]@{
            name = $_.Name
            bytes = $_.Length
            last_write_utc = $_.LastWriteTimeUtc.ToString('O')
        }
    })
}

$before = Get-CacheInventory
$canonicalBefore = @($before | Where-Object { $_.name -match '^[A-Z]\.mft\.sqlite3(?:-wal|-shm)?$' })
if ($canonicalBefore.Count -eq 0) {
    throw 'No canonical per-volume SQLite MFT store is present.'
}

& powershell -NoProfile -ExecutionPolicy Bypass -File $captureScript `
    -Mode $Mode -DurationSeconds $DurationSeconds -OutputPath $tracePath
if ($LASTEXITCODE -ne 0) { throw 'Installed evidence capture failed.' }

$trace = Get-Content -Raw -Encoding UTF8 -LiteralPath $tracePath | ConvertFrom-Json
$after = Get-CacheInventory
$beforeNames = @($before | ForEach-Object name)
$afterNames = @($after | ForEach-Object name)
$created = @($afterNames | Where-Object { $_ -notin $beforeNames })
$removed = @($beforeNames | Where-Object { $_ -notin $afterNames })
$legacyEvents = @($trace.cache_events | Where-Object {
    $_.name -match '\.(?:semftcp|semftdelta|semftstatus)$'
})
$sqliteEvents = @($trace.cache_events | Where-Object {
    $_.name -match '^(?<volume>[A-Z])\.mft\.sqlite3(?:-wal|-shm)?$'
})

# Consecutive file notifications from one SQLite transaction form one burst. A
# 30-second quiet gap separates bursts while remaining far below the 10-minute
# persistence deadline.
$attemptsByVolume = [ordered]@{}
foreach ($group in @($sqliteEvents | Group-Object { if ($_.name -match '^([A-Z])\.') { $Matches[1] } })) {
    $starts = [Collections.Generic.List[int64]]::new()
    $last = $null
    foreach ($event in @($group.Group | Sort-Object elapsed_ms)) {
        $elapsed = [int64]$event.elapsed_ms
        if ($null -eq $last -or ($elapsed - $last) -gt 30000) { $starts.Add($elapsed) }
        $last = $elapsed
    }
    $attemptsByVolume[$group.Name] = @($starts)
}

$cadenceViolations = [Collections.Generic.List[object]]::new()
foreach ($volume in $attemptsByVolume.Keys) {
    $starts = @($attemptsByVolume[$volume])
    for ($index = 1; $index -lt $starts.Count; $index++) {
        $interval = [int64]$starts[$index] - [int64]$starts[$index - 1]
        if ($interval -lt 600000) {
            $cadenceViolations.Add([ordered]@{
                volume = $volume
                previous_elapsed_ms = $starts[$index - 1]
                elapsed_ms = $starts[$index]
                interval_ms = $interval
            })
        }
    }
}

$report = [ordered]@{
    schema = 'superexplorer-mft-sqlite-smoke-v1'
    mode = $Mode
    service_name = $service.Name
    service_account = $service.StartName
    service_path = $service.PathName
    duration_seconds = $trace.duration_seconds
    mutation_count = $trace.mutation_count
    fixed_file_set = $created.Count -eq 0 -and $removed.Count -eq 0
    created_files = $created
    removed_files = $removed
    legacy_file_event_count = $legacyEvents.Count
    sqlite_file_event_count = $sqliteEvents.Count
    attempts_by_volume = $attemptsByVolume
    cadence_violations = @($cadenceViolations)
    unfocused_zero_sqlite_writes = $Mode -ne 'unfocused' -or $sqliteEvents.Count -eq 0
    trace = (Resolve-Path -LiteralPath $tracePath).Path
}
$report | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -LiteralPath $reportPath

if (-not $report.fixed_file_set) { throw 'The cache active file set changed during the trace.' }
if ($legacyEvents.Count -ne 0) { throw 'A legacy sidecar was rewritten during the trace.' }
if (-not $report.unfocused_zero_sqlite_writes) { throw 'SQLite was written while Super Explorer was unfocused.' }
if ($cadenceViolations.Count -ne 0) { throw 'A volume had more than one SQLite write attempt inside a ten-minute interval.' }

Write-Output "MFT SQLite $Mode smoke PASS: $reportPath"
