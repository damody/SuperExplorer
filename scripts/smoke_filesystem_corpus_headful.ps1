param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestFilesystemCorpus.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
$context = $null
$measurements = [Collections.Generic.List[object]]::new()

function Find-NamedElement([string[]]$Names, [Windows.Automation.ControlType]$Type, [string]$Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        foreach ($name in $Names) {
            $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $name)
            foreach ($element in $context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)) {
                if ($element.Current.ControlType -eq $Type -and $element.Current.BoundingRectangle.Width -gt 0) { return $element }
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA named element not found: $Description names=[$($Names -join ', ')]"
}

function Select-ViewMode([string[]]$MenuNames, [string]$Mode) {
    $viewLabel = -join ([char[]](0x6AA2,0x8996))
    $button = Find-NamedElement -Names @('View', $viewLabel) -Type ([Windows.Automation.ControlType]::Button) -Description 'View command button'
    $invoke = $null
    if ($button.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
        ([Windows.Automation.InvokePattern]$invoke).Invoke()
        Start-Sleep -Milliseconds 350
    } else {
        Invoke-UitestClick -Element $button
    }
    $menu = Find-NamedElement -Names $MenuNames -Type ([Windows.Automation.ControlType]::MenuItem) -Description "View menu item $Mode"
    Invoke-UitestClick -Element $menu
    Start-Sleep -Milliseconds 400
    $sentinel = Find-UitestFileItem -Root $context.Root -Name '00-empty-folder'
    $items = @(Get-UitestFileItems -Root $context.Root)
    $bounds = $sentinel.Current.BoundingRectangle
    $measurements.Add([pscustomobject][ordered]@{
        mode = $Mode
        visible_items = $items.Count
        sentinel_width = $bounds.Width
        sentinel_height = $bounds.Height
    })
}

function Open-Folder([string]$Name, [string]$ExpectedItem) {
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name $Name)
    Send-UitestKey -Key 0x0D -DelayMilliseconds 450
    Find-UitestFileItem -Root $context.Root -Name $ExpectedItem | Out-Null
}

function Return-To-Parent([string]$FocusItem, [string]$ExpectedItem) {
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name $FocusItem)
    Send-UitestKey -Key 0x08 -DelayMilliseconds 600
    Find-UitestFileItem -Root $context.Root -Name $ExpectedItem | Out-Null
}

try {
    New-UitestFilesystemCorpus -FixtureRoot $fixture -OwnedRoot $output -Profile small | Out-Null
    $manifestItems = @(Write-UitestCorpusManifest -FixtureRoot $fixture -Path (Join-Path $output 'fixture-manifest.json') -Profile small)
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

    foreach ($name in @('00-empty-folder','01-nested-empty','02-unicode','03-content','04-search','05-mutation','06-deep','08-attributes','corpus-generator.txt')) {
        Find-UitestFileItem -Root $context.Root -Name $name | Out-Null
    }

    $small = -join ([char[]](0x5C0F,0x5716,0x793A))
    $medium = -join ([char[]](0x4E2D,0x5716,0x793A))
    $large = -join ([char[]](0x5927,0x5716,0x793A))
    $list = -join ([char[]](0x6E05,0x55AE))
    $details = -join ([char[]](0x8A73,0x7D30,0x8CC7,0x6599))
    foreach ($mode in @(
        [pscustomobject]@{ names = @($small, 'Small icons'); id = 'small-icons' }
        [pscustomobject]@{ names = @($medium, 'Medium icons'); id = 'medium-icons' }
        [pscustomobject]@{ names = @($large, 'Large icons'); id = 'large-icons' }
        [pscustomobject]@{ names = @($list, 'List'); id = 'list' }
        [pscustomobject]@{ names = @($details, 'Details'); id = 'details' }
    )) { Select-ViewMode -MenuNames $mode.names -Mode $mode.id }

    Open-Folder -Name '02-unicode' -ExpectedItem 'spaces (round) #hash %percent.txt'
    $unicodeNames = @(Get-UitestFileItems -Root $context.Root | ForEach-Object { $_.Current.Name })
    if ($unicodeNames.Count -lt 7) { throw "Unicode folder exposed only $($unicodeNames.Count) visible items" }
    Return-To-Parent -FocusItem 'spaces (round) #hash %percent.txt' -ExpectedItem '02-unicode'

    Open-Folder -Name '04-search' -ExpectedItem 'Needle-Alpha.txt'
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Send-UitestKey -Key 0x46 -Modifiers @(0x11)
    $window = $context.Root.Current.BoundingRectangle
    $searchEditor = Find-UitestElement -Root $context.Root -Description 'search editor' -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $bounds.Top -lt ($window.Top + 180) -and $bounds.Left -gt ($window.Left + $window.Width * 0.58)
    }
    $searchEditor.SetFocus()
    Send-UitestKey -Key 0x41 -Modifiers @(0x11)
    Set-UitestClipboardText -Text 'Needle'
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 250
    Send-UitestKey -Key 0x0D -DelayMilliseconds 700
    # Full-suite runs can begin while the lazy local index is still draining work from earlier
    # headful cases. Match Explorer's asynchronous search contract and wait for the terminal
    # filtered projection, rather than treating an intermediate stale accessibility row as final.
    $searchDeadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $searchNames = @(Get-UitestFileItems -Root $context.Root | ForEach-Object { $_.Current.Name })
        if (@($searchNames | Where-Object { $_ -like '*Needle*' }).Count -ge 3 -and
            @($searchNames | Where-Object { $_ -like '*no-match*' }).Count -eq 0) { break }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $searchDeadline)
    if (@($searchNames | Where-Object { $_ -like '*Needle*' }).Count -lt 3) {
        throw "search did not expose the three corpus matches: [$($searchNames -join ', ')]"
    }
    if (@($searchNames | Where-Object { $_ -like '*no-match*' }).Count -gt 0) {
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'search-filter-failure.png')
        throw "search retained the no-match control item: [$($searchNames -join ', ')]"
    }

    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'Needle-Alpha.txt')
    Send-UitestKey -Key 0x08 -DelayMilliseconds 600
    Find-UitestFileItem -Root $context.Root -Name '06-deep' | Out-Null

    $deep = Join-Path $fixture '06-deep'
    Open-Folder -Name '06-deep' -ExpectedItem 'segment-01-abcdefghijklmnop'
    foreach ($index in 1..18) {
        $segment = 'segment-{0:D2}-abcdefghijklmnop' -f $index
        $deep = Join-Path $deep $segment
        $next = if ($index -eq 18) { 'deep-leaf.txt' } else { 'segment-{0:D2}-abcdefghijklmnop' -f ($index + 1) }
        Open-Folder -Name $segment -ExpectedItem $next
    }

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'filesystem-corpus.png')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        fixture_item_count = $manifestItems.Count
        root_items_verified = 9
        view_modes = @($measurements)
        unicode_visible_items = $unicodeNames.Count
        deep_path_characters = $deep.Length
        search_matches = @($searchNames)
        oracles = [ordered]@{
            real_filesystem_enumeration = $true
            view_mode_switching = $true
            unicode_round_trip = $true
            extended_length_address = $true
            search_results_refresh = $true
        }
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if (Test-Path -LiteralPath $fixture) { Remove-UitestOwnedFixture -FixtureRoot $fixture -OwnedRoot $output }
}

Write-Output "Filesystem corpus headful smoke passed: $OutputDirectory"
