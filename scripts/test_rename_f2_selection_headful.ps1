param(
    [string]$Executable = 'D:\SuperExplorer\target\debug\SuperExplorer.exe',
    [string]$OutputDirectory = 'D:\SuperExplorer\build\rename-f2-selection'
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $OutputDirectory ('fixture-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $fixture -Force
$cases = @(
    @{ source = 'scene_split_1221371.9181824804_1221551.9181824804.bin.gz'; presses = 1; typed = 'scene-renamed'; expected = 'scene-renamed.bin.gz' },
    @{ source = 'folder.bin.gz'; presses = 1; typed = 'renamed-folder'; expected = 'renamed-folder'; is_directory = $true },
    @{ source = '5.1.4.bin.gz'; presses = 1; typed = '5.1.5'; expected = '5.1.5.bin.gz' },
    @{ source = 'compound-full.bin.gz'; presses = 2; typed = 'complete-name.zip'; expected = 'complete-name.zip' },
    @{ source = 'compound-cycle.bin.gz'; presses = 3; typed = 'cycled'; expected = 'cycled.bin.gz' },
    @{ source = 'stem.txt'; presses = 1; typed = 'stem-renamed'; expected = 'stem-renamed.txt' },
    @{ source = 'full.txt'; presses = 2; typed = 'full-renamed.log'; expected = 'full-renamed.log' },
    @{ source = 'cycle.txt'; presses = 3; typed = 'cycle-renamed'; expected = 'cycle-renamed.txt' },
    @{ source = 'edited.txt'; presses = 1; typed = 'final-renamed'; expected = 'final-renamed.txt'; edit_first = $true }
)
foreach ($case in $cases) {
    if ($case.is_directory) { $null = New-Item -ItemType Directory -Path (Join-Path $fixture $case.source) }
    else { Set-Content -LiteralPath (Join-Path $fixture $case.source) -Value 'rename-selection-test' }
}
$context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $OutputDirectory `
    -Executable $Executable -SkipBuild -AdditionalEnvironment @{ EXPLORER_AUTO_CLOSE_MS = '60000' }
try {
    foreach ($case in $cases) {
        $row = Find-UitestFileItem -Root $context.Root -Name $case.source
        Invoke-UitestClick -Element $row
        foreach ($press in 1..$case.presses) { Send-UitestKey -Key 0x71 }
        $bounds = $context.Root.Current.BoundingRectangle
        $null = Find-UitestElement -Root $context.Root -Description 'focused rename editor' -Predicate {
            param($element)
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
                $element.Current.HasKeyboardFocus -and $element.Current.BoundingRectangle.Top -gt ($bounds.Top + 175)
        }
        $null = Find-UitestElement -Root $context.Root -Description 'F2 selection hint in status bar' -Predicate {
            param($element)
            $element.Current.Name.StartsWith('F2')
        }
        if ($case.edit_first) {
            [Windows.Forms.SendKeys]::SendWait('already-edited')
            Send-UitestKey -Key 0x71
            Send-UitestKey -Key 0x71
        }
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory ($case.source + '-selection.png'))
        [Windows.Forms.SendKeys]::SendWait($case.typed)
        Send-UitestKey -Key 0x0D
        $target = Join-Path $fixture $case.expected
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while (-not (Test-Path -LiteralPath $target) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 100
        }
        if (-not (Test-Path -LiteralPath $target)) { throw "Wrong F2 selection for $($case.source): expected $($case.expected)" }
        if (Test-Path -LiteralPath (Join-Path $fixture $case.source)) { throw 'Original file was not renamed.' }
        $null = Find-UitestFileItem -Root $context.Root -Name $case.expected
        $remainingHint = @($context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
            Where-Object { $_.Current.Name.StartsWith('F2') })
        if ($remainingHint.Count -ne 0) { throw 'Rename hint remained after committing rename.' }
    }
    [ordered]@{ status = 'PASS'; executable = $Executable; cases = $cases } |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
} catch {
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'failed.png')
    throw
} finally {
    Stop-UitestExplorer -Context $context
}
