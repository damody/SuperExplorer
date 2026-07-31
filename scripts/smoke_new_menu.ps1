param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'owned-new-menu-fixture'
$context = $null
$passed = $false

function Find-NamedElement([Windows.Automation.ControlType]$Type, [string]$Name, [string]$Description) {
    Find-UitestElement -Root $context.Root -Description $Description -Predicate {
        param($element)
        $element.Current.ControlType -eq $Type -and $element.Current.Name -eq $Name
    }
}

function Invoke-Element([Windows.Automation.AutomationElement]$Element) {
    $invoke = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
        throw "element does not expose InvokePattern: $($Element.Current.Name)"
    }
    ([Windows.Automation.InvokePattern]$invoke).Invoke()
    Start-Sleep -Milliseconds 300
}

function Open-NewMenu {
    Invoke-Element (Find-NamedElement ([Windows.Automation.ControlType]::Button) 'Create a new item' 'New command')
}

try {
    New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'New Text Document.txt') -Value 'collision sentinel'
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

    Open-NewMenu
    foreach ($name in @('Folder', 'Text Document', 'Bitmap image', 'Compressed (zipped) Folder')) {
        Find-NamedElement ([Windows.Automation.ControlType]::MenuItem) $name "New menu item $name" | Out-Null
    }
    Invoke-UitestClick -Element (Find-NamedElement ([Windows.Automation.ControlType]::MenuItem) 'Text Document' 'Text Document item')
    Wait-UitestPath -Path (Join-Path $fixture 'New Text Document (2).txt')

    Open-NewMenu
    Invoke-UitestClick -Element (Find-NamedElement ([Windows.Automation.ControlType]::MenuItem) 'Bitmap image' 'Bitmap image item')
    $bitmap = Join-Path $fixture 'New Bitmap Image.bmp'
    Wait-UitestPath -Path $bitmap
    $bitmapBytes = [IO.File]::ReadAllBytes($bitmap)
    if ($bitmapBytes.Length -lt 2 -or $bitmapBytes[0] -ne 0x42 -or $bitmapBytes[1] -ne 0x4d) {
        throw 'Bitmap ShellNew Data recipe did not write the expected header'
    }

    Open-NewMenu
    Invoke-UitestClick -Element (Find-NamedElement ([Windows.Automation.ControlType]::MenuItem) 'Folder' 'Folder item')
    Wait-UitestPath -Path (Join-Path $fixture 'New folder')

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'new-menu.png')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        menu_population = @('Folder', 'Text Document', 'Bitmap image', 'Compressed (zipped) Folder')
        collision_safe_text_name = 'New Text Document (2).txt'
        bitmap_data_recipe = $true
        folder_disk_effect = $true
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
    $passed = $true
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if ($passed -and (Test-Path -LiteralPath $fixture)) {
        $resolvedFixture = [IO.Path]::GetFullPath($fixture)
        $resolvedOutput = [IO.Path]::GetFullPath($output).TrimEnd([IO.Path]::DirectorySeparatorChar)
        if (-not $resolvedFixture.StartsWith($resolvedOutput + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing to remove fixture outside evidence directory: $resolvedFixture"
        }
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

if (-not $passed) { throw 'New menu smoke did not reach PASS' }
Write-Output "New menu smoke passed: $output"
