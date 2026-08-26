[CmdletBinding()]
param(
    [string]$EvidenceRoot = "evidence",
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
$changeRoot = Split-Path -Parent $PSScriptRoot
$workspaceRoot = [System.IO.Path]::GetFullPath((Join-Path $changeRoot "..\..\.."))

function Get-TaskIds {
    $taskPath = Join-Path $changeRoot "tasks.md"
    $ids = [System.Collections.Generic.List[string]]::new()
    foreach ($line in Get-Content -LiteralPath $taskPath) {
        if ($line -match '^- \[[ x]\] (?<id>\d+\.\d+\.\d+) ') {
            $ids.Add($Matches.id)
        }
    }
    return @($ids)
}

function Test-IndexData($Baseline, $Index, [string[]]$KnownTaskIds, [switch]$SkipFileHashes) {
    $errors = [System.Collections.Generic.List[string]]::new()
    $locationIds = @($Baseline.unsafe_locations.id)
    if (($locationIds | Sort-Object -Unique).Count -ne $locationIds.Count) {
        $errors.Add("duplicate baseline location id")
    }
    foreach ($location in $Baseline.unsafe_locations) {
        if ($location.disposition -notin @("removed-unnecessary", "safe-api", "expected-reviewed")) {
            $errors.Add("location $($location.id) lacks one terminal disposition")
        }
    }
    foreach ($taskId in $KnownTaskIds) {
        $current = @($Index.tasks | Where-Object { $_.task_id -eq $taskId -and $_.current })
        if ($current.Count -ne 1) {
            $errors.Add("task $taskId has $($current.Count) current records")
            continue
        }
        if ($current[0].status -ne "passed") {
            $errors.Add("mandatory task $taskId is not passed")
        }
    }
    foreach ($record in $Index.tasks) {
        if ($record.task_id -notin $KnownTaskIds) {
            $errors.Add("unknown task id $($record.task_id)")
        }
        if ($record.status -in @("stale", "superseded") -and [string]::IsNullOrWhiteSpace($record.replacement_task_id)) {
            $errors.Add("nonterminal record $($record.task_id) lacks replacement")
        }
        foreach ($entry in $record.source_hashes.psobject.Properties) {
            if ($entry.Value -notmatch '^[0-9a-f]{64}$') {
                $errors.Add("invalid source hash for $($record.task_id):$($entry.Name)")
            }
        }
    }
    if (-not $SkipFileHashes) {
        foreach ($entry in $Index.current_files.psobject.Properties) {
            $path = Join-Path $workspaceRoot $entry.Name
            if (-not (Test-Path -LiteralPath $path)) {
                $errors.Add("current file missing: $($entry.Name)")
                continue
            }
            $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
            if ($actual -ne $entry.Value) {
                $errors.Add("current file hash mismatch: $($entry.Name)")
            }
        }
    }
    return @($errors)
}

if ($SelfTest) {
    $hash = "0" * 64
    $baseline = [pscustomobject]@{ unsafe_locations = @([pscustomobject]@{ id = "UCG-0001"; disposition = "expected-reviewed" }) }
    $validIndex = [pscustomobject]@{
        tasks = @([pscustomobject]@{ task_id = "1.1.1"; status = "passed"; current = $true; replacement_task_id = $null; source_hashes = [pscustomobject]@{ "a.rs" = $hash } })
        current_files = [pscustomobject]@{}
    }
    if ((Test-IndexData $baseline $validIndex @("1.1.1") -SkipFileHashes).Count -ne 0) {
        throw "valid evidence fixture was rejected"
    }
    $duplicate = [pscustomobject]@{ unsafe_locations = @($baseline.unsafe_locations[0], $baseline.unsafe_locations[0]) }
    if ((Test-IndexData $duplicate $validIndex @("1.1.1") -SkipFileHashes).Count -eq 0) {
        throw "duplicate location fixture was accepted"
    }
    $unknownIndex = [pscustomobject]@{
        tasks = @($validIndex.tasks[0], [pscustomobject]@{ task_id = "9.9.9"; status = "passed"; current = $false; replacement_task_id = $null; source_hashes = [pscustomobject]@{} })
        current_files = [pscustomobject]@{}
    }
    if ((Test-IndexData $baseline $unknownIndex @("1.1.1") -SkipFileHashes).Count -eq 0) {
        throw "unknown task fixture was accepted"
    }
    Write-Output "PASS: evidence validator fixtures"
    exit 0
}

$root = [System.IO.Path]::GetFullPath((Join-Path $changeRoot $EvidenceRoot))
$baselinePath = Join-Path $root "baseline.json"
$indexPath = Join-Path $root "index.json"
if (-not (Test-Path -LiteralPath $baselinePath) -or -not (Test-Path -LiteralPath $indexPath)) {
    throw "baseline.json and index.json are required"
}
$baselineData = Get-Content -Raw -LiteralPath $baselinePath | ConvertFrom-Json -Depth 50
$indexData = Get-Content -Raw -LiteralPath $indexPath | ConvertFrom-Json -Depth 50
$validationErrors = Test-IndexData $baselineData $indexData (Get-TaskIds)
if ($validationErrors.Count -gt 0) {
    $validationErrors | ForEach-Object { Write-Error $_ }
    exit 1
}
Write-Output "PASS: unsafe governance evidence is complete and current"
