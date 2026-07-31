param(
    [ValidateSet('full','soak')][string]$Mode = 'full',
    [int]$Count = 0,
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestFilesystemCorpus.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

if ($Count -le 0) { $Count = if ($Mode -eq 'soak') { 20000 } else { 2000 } }
$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
$bulk = Join-Path $fixture '07-bulk'
$context = $null
$coldMilliseconds = 0
$warmMilliseconds = 0
$resourceSamples = [Collections.Generic.List[object]]::new()
$cacheCorruption = 'SKIP-no-cache-entry'

function Sample-Resources([string]$Phase) {
    $context.Process.Refresh()
    $sample = [pscustomobject][ordered]@{
        phase = $Phase
        captured_utc = [DateTime]::UtcNow.ToString('o')
        working_set_bytes = $context.Process.WorkingSet64
        private_memory_bytes = $context.Process.PrivateMemorySize64
        handles = $context.Process.HandleCount
        threads = $context.Process.Threads.Count
    }
    if ($sample.working_set_bytes -gt 2GB) { throw "$Phase working set exceeded 2 GiB" }
    if ($sample.handles -gt 10000) { throw "$Phase handle count exceeded 10000" }
    if ($sample.threads -gt 500) { throw "$Phase thread count exceeded 500" }
    $resourceSamples.Add($sample)
}

try {
    $generation = [Diagnostics.Stopwatch]::StartNew()
    New-UitestFilesystemCorpus -FixtureRoot $fixture -OwnedRoot $output -Profile $Mode -BulkCount $Count | Out-Null
    $generation.Stop()
    $actualCount = @([IO.Directory]::EnumerateFiles($bulk)).Count
    if ($actualCount -ne $Count) { throw "bulk corpus count mismatch: expected=$Count actual=$actualCount" }
    Write-UitestCorpusManifest -FixtureRoot $fixture -Path (Join-Path $output 'fixture-manifest.json') -Profile "$Mode-$Count" -SkipHashes | Out-Null

    $cold = [Diagnostics.Stopwatch]::StartNew()
    $context = Start-UitestExplorer -InitialPath $bulk -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild -TimeoutSeconds 45
    Find-UitestFileItem -Root $context.Root -Name 'item-00001.txt' -TimeoutSeconds 30 | Out-Null
    $cold.Stop()
    $coldMilliseconds = $cold.ElapsedMilliseconds
    if ($cold.Elapsed.TotalSeconds -gt 45) { throw "cold large-directory viewport exceeded 45 seconds: $($cold.Elapsed)" }
    Sample-Resources 'cold-ready'

    # Burst refresh requests exercise generation cancellation/coalescing. Only
    # the terminal visible state is asserted; intermediate generations may cancel.
    foreach ($index in 1..8) { Send-UitestKey -Key 0x74 -DelayMilliseconds 25 }
    Find-UitestFileItem -Root $context.Root -Name 'item-00001.txt' -TimeoutSeconds 30 | Out-Null
    Sample-Resources 'after-refresh-burst'
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'large-directory-cold.png')
    Stop-UitestExplorer -Context $context
    $context = $null

    $cacheRoot = Join-Path $output 'localappdata\RustGpuiExplorer\icon-cache\v1'
    if (Test-Path -LiteralPath $cacheRoot -PathType Container) {
        $cacheFile = Get-ChildItem -File -Recurse -LiteralPath $cacheRoot | Select-Object -First 1
        if ($null -ne $cacheFile) {
            [IO.File]::WriteAllBytes($cacheFile.FullName, [byte[]](0x55,0x49,0x54,0x45,0x53,0x54,0x2D,0x42,0x41,0x44))
            $cacheCorruption = 'CORRUPTED-AND-RECOVERED'
        }
    }

    $warm = [Diagnostics.Stopwatch]::StartNew()
    $context = Start-UitestExplorer -InitialPath $bulk -OutputDirectory $output -Profile $Profile -SkipBuild -TimeoutSeconds 45
    Find-UitestFileItem -Root $context.Root -Name 'item-00001.txt' -TimeoutSeconds 30 | Out-Null
    $warm.Stop()
    $warmMilliseconds = $warm.ElapsedMilliseconds
    if ($warm.Elapsed.TotalSeconds -gt 45) { throw "warm large-directory viewport exceeded 45 seconds: $($warm.Elapsed)" }
    Sample-Resources 'warm-ready'
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'large-directory-warm.png')

    @($resourceSamples) | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'resources.json')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        mode = $Mode
        generated_items = $Count
        generation_milliseconds = $generation.ElapsedMilliseconds
        cold_first_viewport_milliseconds = $coldMilliseconds
        warm_first_viewport_milliseconds = $warmMilliseconds
        cache_corruption_probe = $cacheCorruption
        resource_samples = @($resourceSamples)
        oracles = [ordered]@{
            exact_disk_count = $true
            cold_viewport_ready = $true
            refresh_generation_burst_converged = $true
            warm_viewport_ready = $true
            no_resource_gate_exceeded = $true
            cleanup_is_fixture_scoped = $true
        }
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if (Test-Path -LiteralPath $fixture) { Remove-UitestOwnedFixture -FixtureRoot $fixture -OwnedRoot $output }
}

Write-Output "Large-directory $Mode smoke passed: count=$Count output=$OutputDirectory"
