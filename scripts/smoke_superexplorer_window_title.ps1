param(
    [switch]$SkipBuild,
    [string]$OutputDirectory,
    [int]$TimeoutSeconds = 45
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspace 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('window-title-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$runId = 'superexplorer-title-' + [guid]::NewGuid().ToString('N')
$cParent = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Temp\RustGpuiExplorerUITest'
$dParent = Join-Path $targetRoot 'uitest-drive-fixtures'
$cFixture = Join-Path $cParent $runId
$dFixture = Join-Path $dParent $runId
$context = $null

function Assert-OwnedPath([string]$Path, [string]$Parent) {
    $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $fullParent = [IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
    if (-not $fullPath.StartsWith($fullParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing non-owned fixture path: $fullPath"
    }
}

function Wait-WindowTitle([Parameter(Mandatory)]$Context, [Parameter(Mandatory)][string]$Expected) {
    $deadline = [DateTime]::UtcNow.AddSeconds(12)
    do {
        $Context.Process.Refresh()
        $processTitle = $Context.Process.MainWindowTitle
        $uiaTitle = $Context.Root.Current.Name
        if ($processTitle.Equals($Expected, [StringComparison]::OrdinalIgnoreCase) -and
            $uiaTitle.Equals($Expected, [StringComparison]::OrdinalIgnoreCase)) {
            return [ordered]@{ expected = $Expected; process_title = $processTitle; uia_title = $uiaTitle }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "window title mismatch: expected='$Expected' process='$processTitle' uia='$uiaTitle'"
}

Assert-OwnedPath $cFixture $cParent
Assert-OwnedPath $dFixture $dParent
try {
    New-Item -ItemType Directory -Force -Path $cFixture, $dFixture | Out-Null
    Set-Content -LiteralPath (Join-Path $cFixture 'c-title-marker.txt') -Value 'C title fixture' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $dFixture 'd-title-marker.txt') -Value 'D title fixture' -Encoding utf8

    $context = Start-UitestExplorer -InitialPath $cFixture -OutputDirectory $OutputDirectory -SkipBuild:$SkipBuild -TimeoutSeconds $TimeoutSeconds
    $cResult = Wait-WindowTitle -Context $context -Expected ([IO.Path]::GetFullPath($cFixture))
    Set-UitestAddress -Context $context -Path $dFixture -ExpectedItem 'd-title-marker.txt'
    $dResult = Wait-WindowTitle -Context $context -Expected ([IO.Path]::GetFullPath($dFixture))
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'window-title-cross-drive.png')

    [ordered]@{
        test = 'superexplorer-window-title'
        executable = Join-Path $workspace 'target\debug\SuperExplorer.exe'
        initial = $cResult
        navigated = $dResult
        result = 'passed'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    foreach ($owned in @(@($cFixture, $cParent), @($dFixture, $dParent))) {
        Assert-OwnedPath $owned[0] $owned[1]
        if (Test-Path -LiteralPath $owned[0]) { Remove-Item -LiteralPath $owned[0] -Recurse -Force }
    }
}

Write-Output "SuperExplorer cross-drive window title smoke passed: $OutputDirectory"
