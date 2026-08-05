param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'UitestFilesystemCorpus.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
$context = $null
New-Item -ItemType Directory -Force -Path $fixture | Out-Null

function Find-ById([string]$Id, [string]$Description, [string]$AccessibleName = '', [int]$TimeoutSeconds = 10) {
    Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        ($element.Current.AutomationId -eq $Id -or
            ($AccessibleName -and $element.Current.Name -eq $AccessibleName)) -and
            $bounds.Width -gt 0 -and $bounds.Height -gt 0
    }
}

function Find-OptionalById([string]$Id) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
    $context.Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Find-OptionalVisible([string]$Id, [string]$AccessibleName) {
    $all = $context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    0..($all.Count - 1) | ForEach-Object { $all.Item($_) } | Where-Object {
        $bounds = $_.Current.BoundingRectangle
        ($_.Current.AutomationId -eq $Id -or $_.Current.Name -eq $AccessibleName) -and
            $bounds.Width -gt 0 -and $bounds.Height -gt 0
    } | Select-Object -First 1
}

function Find-RoleName([Windows.Automation.ControlType]$Type, [string]$Name, [string]$Description, [int]$TimeoutSeconds = 10) {
    Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq $Type -and $element.Current.Name -eq $Name -and
            $bounds.Width -gt 0 -and $bounds.Height -gt 0
    }
}

function Find-NameContains([string]$Needle, [string]$Description, [int]$TimeoutSeconds = 10) {
    Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.Name -like "*$Needle*" -and $bounds.Width -gt 0 -and $bounds.Height -gt 0
    }
}

function Invoke-Control([Windows.Automation.AutomationElement]$Element, [int]$DelayMilliseconds = 400) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
    } else {
        Invoke-UitestClick -Element $Element
    }
    if ($DelayMilliseconds -gt 0) { Start-Sleep -Milliseconds $DelayMilliseconds }
}

function Wait-Path([string]$Path, [int]$TimeoutSeconds = 15) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if (Test-Path -LiteralPath $Path -PathType Container) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "folder operation did not complete: $Path"
}

try {
    [IO.File]::WriteAllText((Join-Path $fixture 'keep.txt'), 'unchanged')
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

    $extensionsName = [string]([char]0x64F4) + [char]0x5145 + [char]0x529F + [char]0x80FD
    $extensions = Find-ById -Id 'command-extensions-menu' -Description 'Extensions button' -AccessibleName $extensionsName
    Invoke-Control -Element $extensions
    $popup = Find-RoleName -Type ([Windows.Automation.ControlType]::Menu) -Name $extensionsName -Description 'bounded Extensions popup'
    $bulk = Find-ById -Id 'extension-command-lua-bulk-folder-button-v2' -Description 'Bulk folder command' -AccessibleName 'Bulk folder generator'

    $popupBounds = $popup.Current.BoundingRectangle
    $bulkBounds = $bulk.Current.BoundingRectangle
    if ($bulkBounds.Left -lt $popupBounds.Left -or $bulkBounds.Right -gt $popupBounds.Right) {
        throw "extension command text escaped popup bounds: command=$bulkBounds popup=$popupBounds"
    }

    Invoke-Control -Element $bulk
    $panelNodes = $context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    @($panelNodes | ForEach-Object { "$($_.Current.AutomationId)|$($_.Current.Name)|$($_.Current.ControlType.ProgrammaticName)" }) |
        Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'command-panel-uia.txt')
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'command-panel-debug.png')
    Find-NameContains -Needle 'Folder-001' -Description 'Create ten preview action' | Out-Null
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'bulk-folder-panel.png')

    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 400
    Find-ById -Id 'extension-command-lua-bulk-folder-button-v2' -Description 'command list restored after Escape' -AccessibleName 'Bulk folder generator' | Out-Null
    if (@(Get-ChildItem -LiteralPath $fixture -Directory).Count -ne 0) {
        throw 'Escape caused a folder operation before confirmation'
    }

    $bulk = Find-ById -Id 'extension-command-lua-bulk-folder-button-v2' -Description 'Bulk folder command after Escape' -AccessibleName 'Bulk folder generator'
    Invoke-Control -Element $bulk
    $createTen = Find-NameContains -Needle 'Folder-001' -Description 'confirmed create ten action'
    Invoke-Control -Element $createTen
    Wait-Path -Path (Join-Path $fixture 'Folder-001')
    Wait-Path -Path (Join-Path $fixture 'Folder-010')
    if (@(Get-ChildItem -LiteralPath $fixture -Directory -Filter 'Folder-*').Count -ne 10) {
        throw 'confirmed bulk-folder operation did not create exactly ten folders'
    }

    $exif = Find-OptionalVisible -Id 'extension-command-rust-exif-rename-button-v2' -AccessibleName 'Rename from EXIF'
    if ($null -eq $exif) {
        $extensions = Find-ById -Id 'command-extensions-menu' -Description 'Extensions button after folder refresh' -AccessibleName $extensionsName
        Invoke-Control -Element $extensions
        $exif = Find-ById -Id 'extension-command-rust-exif-rename-button-v2' -Description 'EXIF rename command' -AccessibleName 'Rename from EXIF'
    }
    Invoke-Control -Element $exif -DelayMilliseconds 20
    Find-NameContains -Needle '20260805_123456' -Description 'EXIF date naming choice' | Out-Null
    $cancelName = [string]([char]0x53D6) + [char]0x6D88
    Find-RoleName -Type ([Windows.Automation.ControlType]::MenuItem) -Name $cancelName -Description 'command panel Cancel action' | Out-Null
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'exif-rename-panel.png')

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        created_folders = @(Get-ChildItem -LiteralPath $fixture -Directory -Filter 'Folder-*').Count
        oracles = [ordered]@{
            popup_content_is_bounded = $true
            bulk_command_opens_interaction_panel = $true
            escape_returns_without_mutation = $true
            confirmed_bulk_action_executes = $true
            exif_command_exposes_choices_and_cancel = $true
        }
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if (Test-Path -LiteralPath $fixture) { Remove-UitestOwnedFixture -FixtureRoot $fixture -OwnedRoot $output }
}

Write-Output "Extension command interaction smoke passed: $OutputDirectory"
