param(
    [string]$OutputDirectory = "",
    [switch]$SkipBuild,
    [ValidateRange(2, 20)][int]$RestartRuns = 10
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $workspaceRoot 'target\roadmap-session-evidence'
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

$testLog = Join-Path $OutputDirectory 'cargo-test.log'
$savedErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
& cargo test -p explorer-model -p explorer-app session --locked 2>&1 | Tee-Object -FilePath $testLog
$cargoTestExitCode = $LASTEXITCODE
$ErrorActionPreference = $savedErrorActionPreference
if ($cargoTestExitCode -ne 0) { throw "session tests failed with exit code $cargoTestExitCode" }

if (-not $SkipBuild) {
    & cargo build -p explorer-app --locked
    if ($LASTEXITCODE -ne 0) { throw "explorer-app build failed with exit code $LASTEXITCODE" }
}

$targetRoot = if ($env:CARGO_TARGET_DIR) {
    [IO.Path]::GetFullPath((Join-Path $workspaceRoot $env:CARGO_TARGET_DIR))
} else {
    Join-Path $workspaceRoot 'target'
}
$executable = Join-Path $targetRoot 'debug\SuperExplorer.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "explorer-app executable not found: $executable"
}

& (Join-Path $PSScriptRoot 'smoke_session_restore_headful.ps1') `
    -OutputDirectory $OutputDirectory `
    -Executable $executable `
    -RestartRuns $RestartRuns
$headfulPath = Join-Path $OutputDirectory 'headful-report.json'
if (-not (Test-Path -LiteralPath $headfulPath -PathType Leaf)) {
    throw 'headful session validation did not produce headful-report.json'
}
$headful = Get-Content -Raw -Encoding UTF8 -LiteralPath $headfulPath | ConvertFrom-Json
if ($headful.result -ne 'PASS' -or $headful.full_oracle_per_run -ne $true) {
    throw 'headful session validation did not pass its complete before/after oracle'
}
if ($headful.restored_active_auto_loaded -ne $true -or
    $headful.restored_background_auto_loaded -ne $true -or
    $headful.persistent_disconnected_seen -ne $false) {
    throw 'restored tabs did not automatically connect to the directory service'
}
$report = [ordered]@{
    schema = 'roadmap-session-validation-v2'
    result = 'PASS'
    restart_runs = $headful.restart_runs
    clean_runs = $headful.clean_runs
    crash_runs = $headful.crash_runs
    tab_count = $headful.tab_count
    active_tab_index = $headful.active_tab_index
    mixed_locations = $headful.mixed_locations
    cross_volume = $headful.cross_volume
    full_oracle_per_run = $headful.full_oracle_per_run
    restored_active_auto_loaded = $headful.restored_active_auto_loaded
    restored_background_auto_loaded = $headful.restored_background_auto_loaded
    persistent_disconnected_seen = $headful.persistent_disconnected_seen
    restart_results = $headful.results
    artifacts = @(
        'cargo-test.log',
        'headful-report.json',
        'before-uia.json',
        'before.png',
        'before-payload.json',
        'restored-active-loaded.png',
        'restored-background-loaded.png'
    )
}
$report | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
Write-Host "Session roadmap validation PASS: $OutputDirectory"
