param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][AllowEmptyString()][string]$InitialPath,
    [string]$RestoreWindowId = '',
    [ValidateRange(1, 60)][int]$WarmupSeconds = 20,
    [ValidateRange(1, 60)][int]$SampleSeconds = 10,
    [ValidateRange(1, 60)][int]$SampleCount = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$output = [IO.Path]::GetFullPath($OutputDirectory)
$context = $null

function Get-SupportCpuCounters {
    $counters = @{}
    Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "Name LIKE 'superexplorer-mft-service%' OR Name LIKE 'explorer-extension-%'" | ForEach-Object {
        $counters[[uint32]$_.IDProcess] = [uint64]$_.PercentProcessorTime
    }
    return $counters
}

try {
    $environment = @{
        SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION = '1'
    }
    if ($RestoreWindowId) { $environment['SUPEREXPLORER_RESTORE_WINDOW_ID'] = $RestoreWindowId }
    $context = Start-UitestExplorer -InitialPath $InitialPath -OutputDirectory $output -Executable $Executable -SkipBuild -AdditionalEnvironment $environment
    Start-Sleep -Seconds $WarmupSeconds
    $samples = @()
    $supportSamples = @()
    $logicalProcessors = [Environment]::ProcessorCount
    for ($index = 0; $index -lt $SampleCount; $index++) {
        $supportBefore = Get-SupportCpuCounters
        $context.Process.Refresh()
        $before = $context.Process.TotalProcessorTime.TotalSeconds
        $clock = [Diagnostics.Stopwatch]::StartNew()
        Start-Sleep -Seconds $SampleSeconds
        $context.Process.Refresh()
        if ($context.Process.HasExited) { throw 'Application exited while measuring idle CPU' }
        $elapsed = $clock.Elapsed.TotalSeconds
        $samples += 100 * ($context.Process.TotalProcessorTime.TotalSeconds - $before) / $elapsed / $logicalProcessors
        $supportAfter = Get-SupportCpuCounters
        $supportTicks = 0.0
        foreach ($id in $supportAfter.Keys) {
            if ($supportBefore.ContainsKey($id)) { $supportTicks += $supportAfter[$id] - $supportBefore[$id] }
        }
        $supportSamples += 100 * $supportTicks / 10000000 / $elapsed / $logicalProcessors
    }
    $context.Process.Refresh()
    $context.Root = [Windows.Automation.AutomationElement]::FromHandle($context.Process.MainWindowHandle)
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'visible-window.png')
    $report = [ordered]@{
        executable = [IO.Path]::GetFullPath($Executable)
        window_title = $context.Process.MainWindowTitle
        window_width = $context.Root.Current.BoundingRectangle.Width
        window_height = $context.Root.Current.BoundingRectangle.Height
        initial_path = $InitialPath
        restored_window_id = $RestoreWindowId
        logical_processors = $logicalProcessors
        warmup_seconds = $WarmupSeconds
        sample_seconds = $SampleSeconds
        cpu_percent_samples = $samples
        cpu_percent_mean = ($samples | Measure-Object -Average).Average
        cpu_percent_max = ($samples | Measure-Object -Maximum).Maximum
        support_cpu_percent_samples = $supportSamples
        combined_cpu_percent_samples = @(for ($index = 0; $index -lt $SampleCount; $index++) { $samples[$index] + $supportSamples[$index] })
    }
    $report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'report.json')
    $report | ConvertTo-Json -Depth 4
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}
