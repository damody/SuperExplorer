param(
    [Parameter(Mandatory)][string]$ParentPath,
    [Parameter(Mandatory)][string]$ChildName,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Executable = 'D:\SuperExplorer\target\debug\SuperExplorer.exe',
    [ValidateSet('DoubleClick','Enter')][string]$OpenMethod = 'DoubleClick'
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$childPath = Join-Path $ParentPath $ChildName
$expectedNames = @(Get-ChildItem -LiteralPath $childPath | ForEach-Object Name)
if ($expectedNames.Count -eq 0) { throw 'Select a populated child folder for this probe.' }
$context = Start-UitestExplorer -InitialPath $ParentPath -OutputDirectory $OutputDirectory `
    -Executable $Executable -SkipBuild -AdditionalEnvironment @{ EXPLORER_AUTO_CLOSE_MS = '45000' }
$results = @()
try {
    # Bind physical gestures to this probe when another app owns foreground focus.
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 0, 0, 0, 0, 0x0003)
    foreach ($cycle in 1..3) {
        $row = Find-UitestFileItem -Root $context.Root -Name $ChildName
        $timer = [Diagnostics.Stopwatch]::StartNew()
        if ($OpenMethod -eq 'DoubleClick') {
            Invoke-UitestClick -Element $row -Double
        } else {
            Invoke-UitestClick -Element $row
            Send-UitestKey -Key 0x0D
        }
        $ready = $false
        do {
            $names = @(Get-UitestFileItems -Root $context.Root | ForEach-Object { $_.Current.Name })
            $missing = @($expectedNames | Where-Object {
                $expected = $_
                -not @($names | Where-Object { $_ -eq $expected -or $_ -like "$expected *" }).Count
            })
            $ready = $missing.Count -eq 0
            if (-not $ready) { Start-Sleep -Milliseconds 20 }
        } while (-not $ready -and $timer.Elapsed.TotalSeconds -lt 10)
        if (-not $ready) {
            $names | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'visible-items.json')
            Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'failed.png')
            throw 'Child folder did not show its file rows within ten seconds.'
        }
        $results += [ordered]@{ cycle = $cycle; visible_rows = $names.Count; ready_ms = $timer.ElapsedMilliseconds }
        if ($cycle -lt 3) { Send-UitestKey -Key 0x26 -Modifiers @(0x12) }
    }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'folder-ready.png')
    [ordered]@{ status = 'PASS'; executable = $Executable; parent = $ParentPath; child = $childPath; cycles = $results } |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
    $results | ConvertTo-Json
} finally {
    Stop-UitestExplorer -Context $context
}
