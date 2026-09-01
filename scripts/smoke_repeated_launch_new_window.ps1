param(
    [string]$Executable = "target/debug/SuperExplorer.exe",
    [string]$ReportPath = "openspec/changes/repeated-launch-new-window/evidence/repeated-launch/report.json",
    [int]$WindowTimeoutSeconds = 20
)

$ErrorActionPreference = "Stop"
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).Path
$resolvedReport = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $ReportPath))
$reportDirectory = Split-Path -Parent $resolvedReport
New-Item -ItemType Directory -Force -Path $reportDirectory | Out-Null

$profileRoot = Join-Path $env:TEMP ("superexplorer-relaunch-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $profileRoot | Out-Null
$savedLocalAppData = $env:LOCALAPPDATA
$savedInitialPath = $env:EXPLORER_INITIAL_PATH
$first = $null
$second = $null
$closeResults = @()

function Wait-MainWindow([System.Diagnostics.Process]$Process, [int]$TimeoutSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $null = $Process.Refresh()
        if ($Process.HasExited) {
            throw "SuperExplorer process $($Process.Id) exited before opening a window"
        }
        if ($Process.MainWindowHandle -ne 0 -and -not [string]::IsNullOrWhiteSpace($Process.MainWindowTitle)) {
            return
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for SuperExplorer process $($Process.Id) to open a window"
}

try {
    $env:LOCALAPPDATA = $profileRoot
    $env:EXPLORER_INITIAL_PATH = "D:\"

    $first = Start-Process -FilePath $resolvedExecutable -PassThru
    Wait-MainWindow -Process $first -TimeoutSeconds $WindowTimeoutSeconds
    $first.Refresh()
    $firstObservation = [ordered]@{
        pid = $first.Id
        title = $first.MainWindowTitle
        window_handle = $first.MainWindowHandle.ToInt64()
        responding = $first.Responding
    }
    $second = Start-Process -FilePath $resolvedExecutable -PassThru
    Wait-MainWindow -Process $second -TimeoutSeconds $WindowTimeoutSeconds

    $second.Refresh()
    $secondObservation = [ordered]@{
        pid = $second.Id
        title = $second.MainWindowTitle
        window_handle = $second.MainWindowHandle.ToInt64()
        responding = $second.Responding
    }
    $passed = $firstObservation.title -eq "D:\" -and
        $second.MainWindowTitle -eq "C:\" -and
        $firstObservation.responding -and $second.Responding

    $report = [ordered]@{
        schema = "superexplorer.repeated-launch-smoke.v1"
        timestamp_utc = [DateTime]::UtcNow.ToString("o")
        executable = $resolvedExecutable
        profile_root = $profileRoot
        expected = [ordered]@{ first_title = "D:\"; second_title = "C:\" }
        first = $firstObservation
        second = $secondObservation
        passed = $passed
    }
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $resolvedReport -Encoding utf8
    if (-not $passed) {
        throw "Repeated-launch smoke expectations failed; see $resolvedReport"
    }
}
finally {
    foreach ($process in @($second, $first)) {
        if ($null -eq $process) { continue }
        try {
            if (-not $process.HasExited) {
                $requested = $process.CloseMainWindow()
                $exited = $process.WaitForExit(5000)
                if (-not $exited) {
                    Stop-Process -Id $process.Id -Force
                    $process.WaitForExit(5000)
                }
                $closeResults += [ordered]@{ pid = $process.Id; close_requested = $requested; exited = $process.HasExited }
            }
        }
        catch {
            $closeResults += [ordered]@{ pid = $process.Id; close_error = $_.Exception.Message }
        }
    }
    $env:LOCALAPPDATA = $savedLocalAppData
    if ($null -eq $savedInitialPath) {
        Remove-Item Env:EXPLORER_INITIAL_PATH -ErrorAction SilentlyContinue
    }
    else {
        $env:EXPLORER_INITIAL_PATH = $savedInitialPath
    }
}

if (Test-Path -LiteralPath $resolvedReport) {
    $savedReport = Get-Content -Raw -LiteralPath $resolvedReport | ConvertFrom-Json
    $savedReport | Add-Member -NotePropertyName cleanup -NotePropertyValue $closeResults -Force
    $savedReport | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $resolvedReport -Encoding utf8
}

Write-Output "Repeated-launch smoke passed: $resolvedReport"
