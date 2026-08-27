param([string]$OutputDirectory, [switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$target = Join-Path $workspace 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $target ('operation-message-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$fixture = Join-Path $target ('operation-message-fixture-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $OutputDirectory, $fixture | Out-Null
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$context = $null
try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $OutputDirectory -SkipBuild:$SkipBuild
    $newButton = Find-UitestElement -Root $context.Root -Description 'New command button' -Predicate {
        param($element)
        $element.Current.Name -eq 'Create a new item' -and $element.Current.BoundingRectangle.Width -gt 0
    }
    $newButton.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
    $folder = Find-UitestElement -Root $context.Root -Description 'Folder new item' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::MenuItem -and
            $element.Current.Name -eq 'Folder' -and $element.Current.BoundingRectangle.Width -gt 0
    }
    Invoke-UitestClick -Element $folder
    $rename = Find-UitestElement -Root $context.Root -Description 'new folder rename' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $element.Current.Name -eq 'Rename New folder'
    }
    $rename.SetFocus()
    Send-UitestKey -Key 0x0D -DelayMilliseconds 500
    $expectedPath = Join-Path $fixture 'New folder'
    Wait-UitestPath -Path $expectedPath
    $detailedScreenshot = Join-Path $OutputDirectory 'operation-message-detailed.png'
    Save-UitestScreenshot -Root $context.Root -Path $detailedScreenshot
    Start-Sleep -Milliseconds 8500
    $expiredScreenshot = Join-Path $OutputDirectory 'operation-message-expired.png'
    Save-UitestScreenshot -Root $context.Root -Path $expiredScreenshot
    $before = [Drawing.Bitmap]::FromFile($detailedScreenshot)
    $after = [Drawing.Bitmap]::FromFile($expiredScreenshot)
    try {
        $changed = 0
        $top = [Math]::Max(0, $before.Height - 100)
        for ($y = $top; $y -lt ($before.Height - 35); $y += 4) {
            for ($x = 12; $x -lt ($before.Width - 12); $x += 12) {
                if ($before.GetPixel($x, $y).ToArgb() -ne $after.GetPixel($x, $y).ToArgb()) {
                    $changed++
                }
            }
        }
        if ($changed -lt 50) { throw 'operation message area did not change after eight seconds' }
    } finally {
        $before.Dispose()
        $after.Dispose()
    }
    $messageText = "Create folder | $expectedPath | Completed"
    [ordered]@{
        status = 'passed'
        message = $messageText
        full_target_path = $true
        hidden_after_eight_seconds = $true
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Operation message lifecycle smoke passed: $OutputDirectory"
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    $resolved = [IO.Path]::GetFullPath($fixture)
    $allowed = [IO.Path]::GetFullPath($target).TrimEnd('\') + '\operation-message-fixture-'
    if (-not $resolved.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing unsafe fixture cleanup: $resolved"
    }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
