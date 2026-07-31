param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture\LevelOne\LevelTwo'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$context = $null

function Get-ThisPcLabel { -join ([char[]]@(0x672C, 0x6A5F)) }

function Find-NavigationRow([string]$Description, [scriptblock]$NamePredicate) {
    $window = $context.Root.Current.BoundingRectangle
    Find-UitestElement -Root $context.Root -Description $Description -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $bounds.Width -gt 0 -and $bounds.Height -gt 0 -and
            $bounds.Left -lt ($window.Left + 420) -and
            $bounds.Top -gt ($window.Top + 150) -and $bounds.Bottom -lt $window.Bottom -and
            (& $NamePredicate $element.Current.Name)
    }
}

function Find-RowChevron([Windows.Automation.AutomationElement]$Row, [string]$State = 'Expand') {
    $rowBounds = $Row.Current.BoundingRectangle
    Find-UitestElement -Root $context.Root -Description "$State chevron for $($Row.Current.Name)" -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq $State -and
            [Math]::Abs(($bounds.Top + $bounds.Height / 2) - ($rowBounds.Top + $rowBounds.Height / 2)) -lt 6 -and
            $bounds.Left -ge ($rowBounds.Left - 2) -and
            $bounds.Right -lt ($rowBounds.Left + 100)
    }
}

function Get-IconMetrics(
    [string]$Screenshot,
    [Windows.Automation.AutomationElement]$Row,
    [Windows.Automation.AutomationElement]$Chevron
) {
    $window = $context.Root.Current.BoundingRectangle
    $rowBounds = $Row.Current.BoundingRectangle
    $chevronBounds = $Chevron.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::FromFile($Screenshot)
    try {
        $left = [Math]::Max(0, [int][Math]::Round($chevronBounds.Right - $window.Left))
        $right = [Math]::Min($bitmap.Width - 1, $left + 25)
        $top = [Math]::Max(0, [int][Math]::Round($rowBounds.Top - $window.Top + 4))
        $bottom = [Math]::Min($bitmap.Height - 1, [int][Math]::Round($rowBounds.Bottom - $window.Top - 4))
        $colors = [Collections.Generic.HashSet[int]]::new()
        $saturated = 0
        for ($y = $top; $y -le $bottom; $y++) {
            for ($x = $left; $x -le $right; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                [void]$colors.Add($pixel.ToArgb())
                $maximum = [Math]::Max($pixel.R, [Math]::Max($pixel.G, $pixel.B))
                $minimum = [Math]::Min($pixel.R, [Math]::Min($pixel.G, $pixel.B))
                if (($maximum - $minimum) -ge 35) { $saturated++ }
            }
        }
        if ($colors.Count -lt 8 -or $saturated -lt 4) {
            throw "Shell icon pixels missing for '$($Row.Current.Name)': colors=$($colors.Count), saturated=$saturated"
        }
        [ordered]@{
            name = $Row.Current.Name
            color_count = $colors.Count
            saturated_pixels = $saturated
            bounds = [ordered]@{ left=$rowBounds.Left; top=$rowBounds.Top; width=$rowBounds.Width; height=$rowBounds.Height }
        }
    } finally {
        $bitmap.Dispose()
    }
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    Start-Sleep -Milliseconds 900

    $driveRows = @{}
    foreach ($letter in @('C','D','E')) {
        try {
            $row = Find-NavigationRow "drive $letter before This PC" { param($name) $name -match "\($letter`:\)$" }
            $driveRows[$letter] = $row
        } catch {
            if ($letter -eq 'D') { throw }
        }
    }
    $beforePath = Join-Path $output 'navigation-icons-before-this-pc.png'
    Save-UitestScreenshot -Root $context.Root -Path $beforePath
    $before = @{}
    foreach ($letter in $driveRows.Keys) {
        $before[$letter] = Get-IconMetrics $beforePath $driveRows[$letter] (Find-RowChevron $driveRows[$letter])
    }

    $thisPc = Find-NavigationRow 'This PC navigation row' { param($name) $name -eq (Get-ThisPcLabel) }
    Invoke-UitestClick -Element $thisPc
    Start-Sleep -Milliseconds 1200

    $afterPath = Join-Path $output 'navigation-icons-after-this-pc.png'
    Save-UitestScreenshot -Root $context.Root -Path $afterPath
    $after = @{}
    foreach ($letter in $driveRows.Keys) {
        $row = Find-NavigationRow "drive $letter after This PC" { param($name) $name -match "\($letter`:\)$" }
        $after[$letter] = Get-IconMetrics $afterPath $row (Find-RowChevron $row)
    }

    $driveC = Find-NavigationRow 'C drive to select' { param($name) $name -match '\(C:\)$' }
    Invoke-UitestClick -Element $driveC
    Start-Sleep -Milliseconds 1400
    $selectedDrivePath = Join-Path $output 'navigation-icons-after-selecting-c.png'
    Save-UitestScreenshot -Root $context.Root -Path $selectedDrivePath
    $afterDriveSelection = @{}
    foreach ($letter in $driveRows.Keys) {
        $row = Find-NavigationRow "drive $letter after selecting C" { param($name) $name -match "\($letter`:\)$" }
        $afterDriveSelection[$letter] = Get-IconMetrics $selectedDrivePath $row (Find-RowChevron $row)
    }

    $driveD = Find-NavigationRow 'D drive for folder expansion' { param($name) $name -match '\(D:\)$' }
    Invoke-UitestClick -Element (Find-RowChevron $driveD)
    $testFolder = Find-NavigationRow 'expanded generic folder' { param($name) $name -eq '$RECYCLE.BIN' }
    $foldersPath = Join-Path $output 'navigation-generic-folder-icons.png'
    Save-UitestScreenshot -Root $context.Root -Path $foldersPath
    $folderMetric = Get-IconMetrics $foldersPath $testFolder (Find-RowChevron $testFolder)

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        drive_shell_icons_before_this_pc = $before
        drive_shell_icons_after_this_pc = $after
        drive_shell_icons_after_selecting_c = $afterDriveSelection
        selected_c_drive_shell_icon = $afterDriveSelection['C']
        generic_folder_shell_icon = $folderMetric
        this_pc_pointer_activation = $true
        c_drive_pointer_activation = $true
        expanded_drive_pointer_activation = $true
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Navigation Shell icon smoke passed: $OutputDirectory"
