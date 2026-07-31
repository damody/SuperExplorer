param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$context = $null
$remainingLabel = -join ([char[]](0x5269,0x9918))
$totalJoinLabel = [char]0x5171
$noMediaLabel = -join ([char[]](0x6C92,0x6709,0x5A92,0x9AD4))
$disconnectedLabel = -join ([char[]](0x5DF2,0x4E2D,0x65B7,0x9023,0x7DDA))
$accessDeniedLabel = -join ([char[]](0x62D2,0x7D55,0x5B58,0x53D6))
$capacityUnavailableLabel = -join ([char[]](0x7121,0x6CD5,0x53D6,0x5F97,0x5BB9,0x91CF))

function Find-NamedElement([string[]]$Names, [Windows.Automation.ControlType]$Type, [string]$Description) {
    foreach ($name in $Names) {
        try {
            return Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds 8 -Predicate {
                param($element)
                $element.Current.ControlType -eq $Type -and
                    $element.Current.Name -eq $name -and
                    $element.Current.BoundingRectangle.Width -gt 0
            }
        } catch {
            continue
        }
    }
    throw "UIA element not found: $Description names=[$($Names -join ', ')]"
}

function Select-ViewMode([string[]]$Names) {
    $viewLabel = -join ([char[]](0x6AA2,0x8996))
    $button = Find-NamedElement -Names @('View', $viewLabel) -Type ([Windows.Automation.ControlType]::Button) -Description 'View command'
    Invoke-UitestClick -Element $button
    $item = Find-NamedElement -Names $Names -Type ([Windows.Automation.ControlType]::MenuItem) -Description 'View mode menu item'
    Invoke-UitestClick -Element $item
    Start-Sleep -Milliseconds 500
}

function Get-DriveRows {
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::ListItem
        )
    ) | Where-Object {
        try { $_.Current.Name -match '\([A-Z]:\)' -and $_.Current.BoundingRectangle.Width -gt 0 } catch { $false }
    })
}

function Get-CapacityTexts {
    $localizedCapacity = "($([regex]::Escape($remainingLabel)).+$([regex]::Escape([string]$totalJoinLabel))|$([regex]::Escape($noMediaLabel))|$([regex]::Escape($disconnectedLabel))|$([regex]::Escape($accessDeniedLabel))|$([regex]::Escape($capacityUnavailableLabel)))"
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    ) | Where-Object {
        try {
            $_.Current.Name -match "$localizedCapacity|free of|No media|Disconnected|Access denied|Capacity unavailable"
        } catch { $false }
    })
}

function Test-HeaderLabel([string]$Label) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty,
        $Label
    )
    @($context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)).Count -gt 0
}

function Test-HeaderContract([string[]]$Labels) {
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    ) | Where-Object {
        try {
            $name = $_.Current.Name
            ($Labels | Where-Object { $name -notlike "*$_*" }).Count -eq 0
        } catch { $false }
    }).Count -gt 0
}

function Capture-Mode([string]$Id, [string[]]$Names, [ValidateSet('details','icons','content')][string]$Family) {
    Select-ViewMode -Names $Names
    $drives = @(Get-DriveRows)
    if ($drives.Count -lt 1) { throw "$Id exposed no drive rows" }
    $capacity = @(Get-CapacityTexts)
    if ($capacity.Count -lt $drives.Count) {
        throw "$Id exposed $($capacity.Count) capacity labels for $($drives.Count) drives"
    }
    $window = $context.Root.Current.BoundingRectangle
    $bounds = @($drives | ForEach-Object { $_.Current.BoundingRectangle })
    $minimumWidth = ($bounds | Measure-Object -Property Width -Minimum).Minimum
    $minimumHeight = ($bounds | Measure-Object -Property Height -Minimum).Minimum
    if ($Family -in @('details','content') -and $minimumWidth -lt ($window.Width * 0.58)) {
        throw "$Id rows are not Explorer full-width rows: minimum=$minimumWidth window=$($window.Width)"
    }
    if ($Family -eq 'icons' -and $minimumWidth -ge ($window.Width * 0.58)) {
        throw "$Id did not use bounded Explorer drive cards: minimum=$minimumWidth window=$($window.Width)"
    }
    $nameHeader = -join ([char[]](0x540D,0x7A31))
    $typeHeader = -join ([char[]](0x985E,0x578B))
    $totalHeader = -join ([char[]](0x5927,0x5C0F,0x7E3D,0x8A08))
    $freeHeader = -join ([char[]](0x53EF,0x7528,0x7A7A,0x9593))
    $headerContract = $Family -ne 'details' -or (
        (Test-HeaderLabel $nameHeader) -and (Test-HeaderLabel $typeHeader) -and
        (Test-HeaderLabel $totalHeader) -and (Test-HeaderLabel $freeHeader)
    ) -or (Test-HeaderContract @($nameHeader, $typeHeader, $totalHeader, $freeHeader))
    $screenshot = Join-Path $output "this-pc-$Id.png"
    Save-UitestScreenshot -Root $context.Root -Path $screenshot
    [ordered]@{
        id = $Id
        family = $Family
        drive_count = $drives.Count
        capacity_statuses = $capacity.Count
        minimum_row_width = $minimumWidth
        minimum_row_height = $minimumHeight
        full_width = $Family -in @('details','content')
        accessibility_header_contract = $headerContract
    }
}

try {
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    $context = Start-UitestExplorer -InitialPath $output -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    $firstDrive = $null
    foreach ($attempt in 1..2) {
        Set-UitestAddress -Context $context -Path 'shell:MyComputerFolder'
        try {
            $firstDrive = Find-UitestElement -Root $context.Root -Description 'This PC drive item' -TimeoutSeconds 10 -Predicate {
                param($element)
                $element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
                    $element.Current.Name -match '\([A-Z]:\)'
            }
            break
        } catch {
            if ($attempt -eq 2) { throw }
        }
    }
    $small = -join ([char[]](0x5C0F,0x5716,0x793A))
    $medium = -join ([char[]](0x4E2D,0x5716,0x793A))
    $large = -join ([char[]](0x5927,0x5716,0x793A))
    $details = -join ([char[]](0x8A73,0x7D30,0x8CC7,0x6599))
    $content = -join ([char[]](0x5167,0x5BB9))
    $modes = @(
        Capture-Mode -Id 'details' -Names @($details, 'Details') -Family details
        Capture-Mode -Id 'small-icons' -Names @($small, 'Small icons') -Family icons
        Capture-Mode -Id 'medium-icons' -Names @($medium, 'Medium icons') -Family icons
        Capture-Mode -Id 'large-icons' -Names @($large, 'Large icons') -Family icons
        Capture-Mode -Id 'content' -Names @($content, 'Content') -Family content
    )
    $drives = @(Get-DriveRows)
    $capacityTexts = @(Get-CapacityTexts)
    $driveName = $drives[0].Current.Name
    $firstDrive = $drives[0]
    Invoke-UitestClick -Element $firstDrive
    Send-UitestKey -Key 0x0D -DelayMilliseconds 700
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'this-pc-drive-status.png')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        drives = $drives.Count
        capacity_statuses = $capacityTexts.Count
        view_modes = $modes
        activated_drive = $driveName
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "This PC drive-status smoke passed: $OutputDirectory"
