param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'typeahead-fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
foreach ($name in @('alpha.txt', 'data.txt', 'database.log', 'delta.txt', 'omega.txt')) {
    New-Item -ItemType File -Force -Path (Join-Path $fixture $name) | Out-Null
}
$context = $null

function Test-Selected([Windows.Automation.AutomationElement]$Item) {
    $pattern = $null
    $Item.TryGetCurrentPattern(
        [Windows.Automation.SelectionItemPattern]::Pattern,
        [ref]$pattern
    ) -and ([Windows.Automation.SelectionItemPattern]$pattern).Current.IsSelected
}

function Wait-SelectedName([string]$Name, [int]$TimeoutMilliseconds = 2500) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $item = Find-UitestFileItem -Root $context.Root -Name $Name -TimeoutSeconds 2
        if (Test-Selected $item) { return $item }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    $selected = @(Get-UitestFileItems -Root $context.Root | Where-Object { Test-Selected $_ } | ForEach-Object { $_.Current.Name }) -join ', '
    throw "type-ahead did not select $Name; selected=[$selected]"
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    $alpha = Find-UitestFileItem -Root $context.Root -Name 'alpha.txt'
    Invoke-UitestClick -Element $alpha

    Send-UitestKey -Key 0x44 -DelayMilliseconds 80
    Send-UitestKey -Key 0x41 -DelayMilliseconds 180
    $data = Wait-SelectedName 'data.txt'
    $dataShot = Join-Path $output 'typeahead-data-selected.png'
    Save-UitestScreenshot -Root $context.Root -Path $dataShot

    Send-UitestKey -Key 0x1B -DelayMilliseconds 120
    if (-not (Test-Selected $data)) {
        throw 'Escape cleared the selected item instead of only clearing the active type-ahead prefix'
    }
    Send-UitestKey -Key 0x44 -DelayMilliseconds 80
    Send-UitestKey -Key 0x45 -DelayMilliseconds 180
    [void](Wait-SelectedName 'delta.txt')

    Send-UitestKey -Key 0x1B -DelayMilliseconds 100
    Send-UitestKey -Key 0x44 -DelayMilliseconds 80
    [void](Wait-SelectedName 'data.txt')
    Send-UitestKey -Key 0x44 -DelayMilliseconds 180
    [void](Wait-SelectedName 'database.log')

    Send-UitestKey -Key 0x1B -DelayMilliseconds 100
    Send-UitestKey -Key 0x44 -DelayMilliseconds 100
    [void](Wait-SelectedName 'data.txt')
    Start-Sleep -Milliseconds 1150
    Send-UitestKey -Key 0x41 -DelayMilliseconds 180
    [void](Wait-SelectedName 'alpha.txt')
    $timeoutShot = Join-Path $output 'typeahead-timeout-reset.png'
    Save-UitestScreenshot -Root $context.Root -Path $timeoutShot

    [ordered]@{
        schema = 'file-view-typeahead-v1'
        status = 'PASS'
        initial_path = $fixture
        prefix_da_selected = 'data.txt'
        escape_preserved_selection = $true
        prefix_de_selected = 'delta.txt'
        repeated_d_cycled_to = 'database.log'
        timeout_milliseconds = 1150
        timeout_reset_selected = 'alpha.txt'
        hidden_mode = $true
        artifacts = @('typeahead-data-selected.png', 'typeahead-timeout-reset.png')
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "File-view type-ahead UITEST passed: $OutputDirectory"
