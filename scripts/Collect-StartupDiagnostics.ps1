param(
    [string]$Executable = "$env:ProgramFiles\SuperExplorer\SuperExplorer.exe",
    [string]$OutputDirectory,
    [switch]$CollectOnly
)
$ErrorActionPreference = 'Stop'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path ([Environment]::GetFolderPath('Desktop')) ('SuperExplorer-Diagnostics-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$report = [Collections.Generic.List[string]]::new()
$report.Add('SuperExplorer startup diagnostics ' + [DateTime]::UtcNow.ToString('o'))
$report.Add('Executable: ' + $Executable)
function Capture([string]$Label, [scriptblock]$Action) {
    $report.Add("`r`n[$Label]")
    try { $report.Add((& $Action | Out-String -Width 240)) }
    catch { $report.Add($_.Exception.ToString()) }
}
Capture 'Windows' { Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture }
Capture 'CPU' { Get-CimInstance Win32_Processor | Select-Object Name,Architecture }
Capture 'GPU' { Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,DriverDate,Status }
Capture 'Executable version and hash' {
    (Get-Item -LiteralPath $Executable).VersionInfo | Format-List FileVersion,ProductVersion | Out-String
    Get-FileHash -LiteralPath $Executable -Algorithm SHA256 | Format-List Algorithm,Hash,Path | Out-String
}
Capture 'MFT service' { Get-Service SuperExplorerMft -ErrorAction Stop | Select-Object Name,Status }
if (-not $CollectOnly) {
    Capture 'Launch attempt' {
        $resolved = (Resolve-Path -LiteralPath $Executable).Path
        $existing = @(Get-CimInstance Win32_Process -Filter "Name='SuperExplorer.exe'" | Where-Object { $_.ExecutablePath -eq $resolved })
        if ($existing.Count -gt 0) {
            'Already running; close normally and run diagnostics again to capture a fresh startup.'
        } else {
            # Own the Process before starting it: Windows PowerShell's
            # Start-Process -PassThru can return null for a very fast failure.
            $info = [Diagnostics.ProcessStartInfo]::new($resolved, '--diagnostics-console')
            $info.WorkingDirectory = Split-Path -Parent $resolved
            $info.UseShellExecute = $false
            $info.RedirectStandardOutput = $true
            $info.RedirectStandardError = $true
            $process = [Diagnostics.Process]::new()
            $process.StartInfo = $info
            if (-not $process.Start()) { throw 'Process could not start.' }
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            'Started PID: ' + $process.Id
            if ($process.WaitForExit(15000)) {
                $process.WaitForExit()
                'Exit code: ' + $process.ExitCode
                'Exit code (hex): 0x' + $process.ExitCode.ToString('X8')
                [IO.File]::WriteAllText((Join-Path $outputRoot 'stdout.txt'), $stdout.GetAwaiter().GetResult(), [Text.UTF8Encoding]::new($true))
                [IO.File]::WriteAllText((Join-Path $outputRoot 'stderr.txt'), $stderr.GetAwaiter().GetResult(), [Text.UTF8Encoding]::new($true))
            } else {
                'Still running after 15 seconds; left open. No process was terminated.'
                'Console streams are still open; persistent log files were collected instead.'
            }
            Get-CimInstance Win32_Process -Filter "Name='SuperExplorer.exe'" | Select-Object ProcessId,ExecutablePath
        }
    }
}
Capture 'Recent Windows crash events (last 30 minutes)' {
    Get-WinEvent -FilterHashtable @{ LogName='Application'; Id=1000,1001,1002; StartTime=(Get-Date).AddMinutes(-30) } -ErrorAction Stop |
        Where-Object { $_.Message -match 'SuperExplorer|superexplorer|extension-broker' } |
        Select-Object -First 20 TimeCreated,Id,ProviderName,Message
}
$logCandidates = @(
    (Join-Path (Split-Path -Parent $Executable) 'error.log'),
    (Join-Path (Split-Path -Parent $Executable) 'startup-launch.log'),
    (Join-Path $env:LOCALAPPDATA 'RustGpuiExplorer\logs\explorer.log'),
    (Join-Path $env:LOCALAPPDATA 'RustGpuiExplorer\logs\error.log'),
    (Join-Path $env:TEMP 'RustGpuiExplorer\logs\error.log')
)
$index = 0
foreach ($log in ($logCandidates | Select-Object -Unique)) {
    $index++
    Capture ('Error log ' + $index) {
        if (Test-Path -LiteralPath $log -PathType Leaf) {
            Get-Item -LiteralPath $log | Select-Object FullName,Length,LastWriteTime
            Get-Content -LiteralPath $log -Tail 300 -Encoding UTF8 | Set-Content -LiteralPath (Join-Path $outputRoot "error-$index.txt") -Encoding UTF8
        } else { 'Not found: ' + $log }
    }
}
$reportPath = Join-Path $outputRoot 'report.txt'
[IO.File]::WriteAllText($reportPath, ($report -join "`r`n"), [Text.UTF8Encoding]::new($true))
Write-Output "Diagnostics saved: $outputRoot"
Write-Output 'Send report.txt, stderr.txt and error-*.txt from this folder. Existing settings and tabs were not changed.'
