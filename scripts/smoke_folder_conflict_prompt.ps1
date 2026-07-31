param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
$sourceParent = Join-Path $fixture 'source'
$destinationParent = Join-Path $fixture 'destination'
$sourceFolder = Join-Path $sourceParent 'SameFolder'
$destinationFolder = Join-Path $destinationParent 'SameFolder'
$context = $null

try {
    New-Item -ItemType Directory -Force -Path $sourceFolder, $destinationFolder | Out-Null
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $sourceFolder 'source-only.txt') -Value 'source only'
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $sourceFolder 'conflict.txt') -Value 'new bytes'
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $destinationFolder 'destination-only.txt') -Value 'destination only'
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $destinationFolder 'conflict.txt') -Value 'old bytes'

    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'source')
    Send-UitestKey -Key 0x0D -DelayMilliseconds 500
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'SameFolder')
    Send-UitestKey -Key 0x43 -Modifiers @(0x11) -DelayMilliseconds 200
    # Alt+Left is Explorer's history navigation and preserves the CF_HDROP clipboard payload.
    Send-UitestKey -Key 0x25 -Modifiers @(0x12) -DelayMilliseconds 500
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'destination')
    Send-UitestKey -Key 0x0D -DelayMilliseconds 500
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 300

    $desktop = [Windows.Automation.AutomationElement]::RootElement
    $dialogMarker = Find-UitestElement -Root $desktop -Description 'native same-name folder conflict chooser' -TimeoutSeconds 15 -Predicate {
        param($element)
        $element.Current.Name -eq 'Confirm Folder Replace'
    }
    # The common-controls TaskDialog is modal to the app HWND and may be surfaced by UIA as an
    # owned subtree rather than a separate top-level Window. Capture that owner to preserve the
    # complete prompt in either representation.
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'folder-conflict-prompt.png')
    $buttonNames = @($context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
        Where-Object { $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button } |
        ForEach-Object { $_.Current.Name })
    $dialogName = $dialogMarker.Current.Name

    # Escape maps to Explorer's Cancel choice and must leave both trees untouched.
    Send-UitestKey -Key 0x1B -DelayMilliseconds 500
    if (Test-Path -LiteralPath (Join-Path $destinationFolder 'source-only.txt')) {
        throw 'cancelling the folder conflict chooser copied source-only content'
    }
    if ((Get-Content -Raw -LiteralPath (Join-Path $destinationFolder 'conflict.txt')).Trim() -ne 'old bytes') {
        throw 'cancelling the folder conflict chooser changed the destination conflict file'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $destinationFolder 'destination-only.txt'))) {
        throw 'cancelling the folder conflict chooser removed destination-only content'
    }

    [ordered]@{
        schema = 'superexplorer.folder-conflict-prompt.v1'
        folder_name = 'SameFolder'
        native_dialog_name = $dialogName
        buttons = $buttonNames
        cancel_preserved_destination = $true
    } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Same-name folder conflict prompt smoke passed: $output"
