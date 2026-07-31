param(
    [string]$OutputDirectory = "",
    [ValidateRange(1, 20)][int]$SoakRuns = 3
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $workspaceRoot 'target\roadmap-thumbnail-evidence'
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

function Invoke-CargoCase([string]$Name, [string[]]$Arguments) {
    $log = Join-Path $OutputDirectory "$Name.log"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & cargo @Arguments 2>&1 | Tee-Object -FilePath $log | Out-Host
    $code = $LASTEXITCODE
    $ErrorActionPreference = $saved
    $watch.Stop()
    if ($code -ne 0) { throw "$Name failed with exit code $code" }
    return [ordered]@{ name = $Name; status = 'PASS'; duration_ms = [math]::Round($watch.Elapsed.TotalMilliseconds, 3); log = [IO.Path]::GetFileName($log) }
}

$checks = @()
$checks += Invoke-CargoCase 'contracts' @('test','-p','explorer-model','thumbnail','--locked')
$checks += Invoke-CargoCase 'scheduler-cache' @('test','-p','explorer-jobs','thumbnail','--locked')
$checks += Invoke-CargoCase 'real-shell-matrix' @('test','-p','explorer-shell-win','real_shell_retrieval_matrix','--locked','--','--nocapture')
$checks += Invoke-CargoCase 'cache-modes-benchmark' @('test','-p','explorer-shell-win','thumbnail_cache_modes_emit_comparable_benchmark','--locked','--','--nocapture')
$checks += Invoke-CargoCase 'progressive-ui' @('test','-p','explorer-ui','thumbnail','--locked')

$soak = @()
for ($run = 1; $run -le $SoakRuns; $run++) {
    $result = Invoke-CargoCase "soak-$run" @('test','-p','explorer-jobs','thumbnail','--locked')
    $soak += $result
}

$visual = [ordered]@{
    status = 'SKIP'
    prerequisite = 'Set EXPLORER_ROADMAP_VISUAL=1 on an interactive desktop to capture the physical DPI/theme matrix.'
    requested_dpi = @(96,120,144,168,192)
}
if ($env:EXPLORER_ROADMAP_VISUAL -eq '1') {
    $visualDirectory = Join-Path $OutputDirectory 'visual'
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'capture_dpi_matrix.ps1') -SkipBuild -OutputDirectory $visualDirectory
    if ($LASTEXITCODE -ne 0) { throw "thumbnail visual matrix failed with exit code $LASTEXITCODE" }
    $visual = [ordered]@{ status = 'PASS'; artifacts = @('visual\report.json', 'visual\*.png') }
}

$benchmarkLog = Get-Content -Raw -LiteralPath (Join-Path $OutputDirectory 'cache-modes-benchmark.log')
$benchmarkMatch = [regex]::Match($benchmarkLog, 'thumbnail-cache-benchmark no_disk_us=(?<noDisk>\d+) no_disk_source=(?<noDiskSource>[\w-]+) windows_cache_us=(?<windows>\d+) windows_cache_source=(?<windowsSource>[\w-]+) project_disk_us=(?<project>\d+) project_disk_source=(?<projectSource>[\w-]+)')
if (-not $benchmarkMatch.Success) { throw 'Comparable thumbnail cache benchmark output was not found.' }

$report = [ordered]@{
    schema = 'roadmap-thumbnail-validation-v1'
    result = 'PASS'
    captured_utc = [DateTime]::UtcNow.ToString('o')
    windows_build = (Get-CimInstance Win32_OperatingSystem).BuildNumber
    checks = $checks
    repeated_soak = $soak
    cache_mode_benchmark = [ordered]@{
        no_project_disk = [ordered]@{ duration_us = [int64]$benchmarkMatch.Groups['noDisk'].Value; source = $benchmarkMatch.Groups['noDiskSource'].Value }
        windows_cache_only = [ordered]@{ duration_us = [int64]$benchmarkMatch.Groups['windows'].Value; source = $benchmarkMatch.Groups['windowsSource'].Value }
        project_disk_hit = [ordered]@{ duration_us = [int64]$benchmarkMatch.Groups['project'].Value; source = $benchmarkMatch.Groups['projectSource'].Value }
        decision = 'Keep the project disk cache: this same-process comparison isolates each mode; the disk hit is provider-independent and entries remain bounded, checksummed, atomic, versioned, and resettable.'
    }
    visual = $visual
    limitations = @(
        'Provider availability is Windows-build and installed-handler dependent; unsupported PDF/document/media/archive providers are recorded as typed fallback.',
        'Cloud placeholder hydration prevention is covered by metadata-only attribute and cache-only contract tests; no cloud account is required for PASS.'
    )
}
$report | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
Write-Output "Thumbnail roadmap validation PASS: $OutputDirectory"
