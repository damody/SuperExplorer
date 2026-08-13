param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [string]$Executable = '',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild,
    [switch]$CountColumnsPresenceMode,
    [switch]$CountColumnsValueMode,
    [switch]$UseCurrentProfile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'alpha.txt') -Value 'alpha' -Encoding utf8
if ($CountColumnsValueMode) {
    $counted = Join-Path $fixture 'counted-folder'
    $nested = Join-Path $counted 'nested-folder'
    New-Item -ItemType Directory -Force -Path $nested | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $fixture 'empty-folder') | Out-Null
    Set-Content -LiteralPath (Join-Path $counted 'one.txt') -Value 'one' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $nested 'two.txt') -Value 'two' -Encoding utf8
}
$context = $null

function Find-DetailsHeader {
    Find-UitestElement -Root $context.Root -Description 'Details header' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            ($element.Current.Name -like 'Sort by *' -or $element.Current.Name -like '*sorted*') -and
            $element.Current.BoundingRectangle.Width -gt 40
    }
}

function Invoke-RightClick([Windows.Automation.AutomationElement]$Element) {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $point = Get-UitestPhysicalPoint -Element $Element -HorizontalOffset 30
    [void][RustExplorerUitest.Native]::SetPhysicalCursorPos($point.X, $point.Y)
    [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 220
}

function Find-Chooser {
    Find-UitestElement -Root $context.Root -Description 'Details column chooser' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Menu -and
            $element.Current.Name -eq 'Choose details columns' -and
            -not $element.Current.IsOffscreen -and
            $element.Current.BoundingRectangle.Height -gt 0
    }
}

function Find-ChooserItem([string]$Name) {
    Find-UitestElement -Root $context.Root -Description "Details chooser row '$Name'" -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::MenuItem -and
            ($element.Current.Name -eq $Name -or $element.Current.Name -like "$Name, *")
    }
}

function Wait-ChooserItemState([string]$Name, [bool]$Checked, [bool]$Visible = $true) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    $suffix = if ($Checked) { ', checked' } else { ', unchecked' }
    do {
        try {
            $item = Find-ChooserItem -Name $Name
            $bounds = $item.Current.BoundingRectangle
            $chooserBounds = (Find-Chooser).Current.BoundingRectangle
            $isVisible = -not $item.Current.IsOffscreen -and $bounds.Bottom -gt $chooserBounds.Top -and $bounds.Top -lt $chooserBounds.Bottom
            if ($item.Current.Name.EndsWith($suffix, [StringComparison]::Ordinal) -and $isVisible -eq $Visible) {
                return $item
            }
        } catch { }
        Start-Sleep -Milliseconds 80
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "chooser row '$Name' did not become checked=$Checked visible=$Visible"
}

function Send-ChooserWheel([int]$Delta, [int]$Count) {
    $chooserBounds = (Find-Chooser).Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetPhysicalCursorPos(
        [int]($chooserBounds.Left + $chooserBounds.Width / 2),
        [int]($chooserBounds.Top + $chooserBounds.Height / 2))
    $wheelData = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]$Delta), 0)
    foreach ($unused in 1..$Count) {
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $wheelData, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
}

function Find-CellOnRow([string]$RowName, [string]$CellName) {
    $all = $context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition)
    $rootTop = $context.Root.Current.BoundingRectangle.Top
    $row = 0..($all.Count - 1) | ForEach-Object { $all.Item($_) } | Where-Object {
        ($_.Current.Name -eq $RowName -or $_.Current.Name -like "Name: $RowName*") -and
        $_.Current.BoundingRectangle.Top -gt ($rootTop + 180) -and
        $_.Current.BoundingRectangle.Height -gt 0
    } | Select-Object -First 1
    if ($null -eq $row) { return $null }
    $rowTop = $row.Current.BoundingRectangle.Top
    0..($all.Count - 1) | ForEach-Object { $all.Item($_) } | Where-Object {
        $_.Current.Name -eq $CellName -and
        [Math]::Abs($_.Current.BoundingRectangle.Top - $rowTop) -lt 8
    } | Select-Object -First 1
}

function Wait-CellOnRow([string]$RowName, [string]$CellName) {
    # A first query can promote a partial on-disk MFT snapshot to an exact
    # index. Large volumes may need substantially longer than an ordinary UI
    # repaint, so keep the window (and its visible-column demand) alive while
    # the service completes that bounded rebuild.
    $deadline = [DateTime]::UtcNow.AddSeconds(120)
    do {
        $cell = Find-CellOnRow -RowName $RowName -CellName $CellName
        if ($null -ne $cell) { return $cell }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "cell '$CellName' did not appear on '$RowName'"
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -Executable $Executable -SkipBuild:$SkipBuild -UseCurrentProfile:$UseCurrentProfile
    [void](Find-UitestFileItem -Root $context.Root -Name 'alpha.txt')
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr]::Zero, 40, 40, 1100, 880, 0x0040)
    Start-Sleep -Milliseconds 350

    Invoke-RightClick -Element (Find-DetailsHeader)
    $chooser = Find-Chooser
    $windowBounds = $context.Root.Current.BoundingRectangle
    $chooserBounds = $chooser.Current.BoundingRectangle
    if ($chooserBounds.Bottom -gt $windowBounds.Bottom + 1) {
        throw "chooser exceeded the window bottom: chooser=$chooserBounds window=$windowBounds"
    }

    if ($CountColumnsPresenceMode) {
        $fileCount = Find-ChooserItem -Name 'File Count'
        $folderCount = Find-ChooserItem -Name 'Folder Count'
        foreach ($item in $fileCount,$folderCount) {
            if (-not $item.Current.Name.EndsWith(', unchecked', [StringComparison]::Ordinal)) {
                throw "restored count column was not default-hidden: $($item.Current.Name)"
            }
        }
        $scrollPattern = $null
        if ($folderCount.TryGetCurrentPattern([Windows.Automation.ScrollItemPattern]::Pattern,[ref]$scrollPattern)) {
            ([Windows.Automation.ScrollItemPattern]$scrollPattern).ScrollIntoView()
            Start-Sleep -Milliseconds 250
        }
        Send-ChooserWheel -Delta -120 -Count 12
        [void](Wait-ChooserItemState -Name 'Folder Count' -Checked $false -Visible $true)
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'restored-count-columns-in-chooser.png')
        [ordered]@{
            schema_version = 1
            status = 'PASS'
            file_count = $fileCount.Current.Name
            folder_count = $folderCount.Current.Name
            existing_layout_preserved = $true
            screenshots = @('restored-count-columns-in-chooser.png')
        } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
        return
    }

    if ($CountColumnsValueMode) {
        Send-ChooserWheel -Delta -120 -Count 12
        foreach ($name in 'File Count','Folder Count') {
            $item = Find-ChooserItem -Name $name
            $scrollPattern = $null
            if ($item.TryGetCurrentPattern([Windows.Automation.ScrollItemPattern]::Pattern,[ref]$scrollPattern)) {
                ([Windows.Automation.ScrollItemPattern]$scrollPattern).ScrollIntoView()
                Start-Sleep -Milliseconds 150
            }
            $item = Find-ChooserItem -Name $name
            if (-not $item.Current.Name.EndsWith(', checked', [StringComparison]::Ordinal)) {
                $item = Wait-ChooserItemState -Name $name -Checked $false -Visible $true
                Invoke-UitestClick -Element $item
                [void](Wait-ChooserItemState -Name $name -Checked $true -Visible $true)
            }
        }
        Send-UitestKey -Key 0x1B -DelayMilliseconds 200
        [void](Wait-CellOnRow -RowName 'counted-folder' -CellName 'File Count: 2')
        [void](Wait-CellOnRow -RowName 'counted-folder' -CellName 'Folder Count: 1')
        [void](Wait-CellOnRow -RowName 'empty-folder' -CellName 'File Count: 0')
        [void](Wait-CellOnRow -RowName 'empty-folder' -CellName 'Folder Count: 0')
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'visible-count-columns-populated.png')
        [ordered]@{
            schema_version = 1
            status = 'PASS'
            activation_required_refresh = $false
            counted_folder_file_count = 2
            counted_folder_folder_count = 1
            empty_folder_file_count = 0
            empty_folder_folder_count = 0
            source = 'MFT service runtime'
            screenshots = @('visible-count-columns-populated.png')
        } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
        return
    }

    $size = Find-ChooserItem -Name 'Size'
    $initiallyChecked = $size.Current.Name.EndsWith(', checked', [StringComparison]::Ordinal)
    $states = @(!$initiallyChecked, $initiallyChecked, !$initiallyChecked, $initiallyChecked)
    foreach ($expected in $states) {
        Invoke-UitestClick -Element (Find-ChooserItem -Name 'Size')
        [void](Wait-ChooserItemState -Name 'Size' -Checked $expected)
        [void](Find-Chooser)
    }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'details-column-chooser-persistent.png')

    Send-UitestKey -Key 0x1B -DelayMilliseconds 200
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr]::Zero, 40, 40, 1100, 620, 0x0040)
    Start-Sleep -Milliseconds 350
    Invoke-RightClick -Element (Find-DetailsHeader)
    $chooser = Find-Chooser
    $windowBounds = $context.Root.Current.BoundingRectangle
    $chooserBounds = $chooser.Current.BoundingRectangle
    if ($chooserBounds.Bottom -gt $windowBounds.Bottom + 1) {
        throw "short-window chooser exceeded the window bottom: chooser=$chooserBounds window=$windowBounds"
    }

    $title = Find-ChooserItem -Name 'Title'
    $titleInitiallyChecked = $title.Current.Name.EndsWith(', checked', [StringComparison]::Ordinal)
    Send-ChooserWheel -Delta -120 -Count 10
    $title = Wait-ChooserItemState -Name 'Title' -Checked $titleInitiallyChecked -Visible $true
    Invoke-UitestClick -Element $title
    [void](Wait-ChooserItemState -Name 'Title' -Checked (!$titleInitiallyChecked) -Visible $true)
    [void](Find-Chooser)
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'details-column-chooser-scrolled-bottom.png')

    Send-ChooserWheel -Delta 120 -Count 10
    [void](Wait-ChooserItemState -Name 'Size' -Checked $initiallyChecked -Visible $true)
    Send-UitestKey -Key 0x1B -DelayMilliseconds 200
    $menus = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Menu)) | Where-Object {
                try { -not $_.Current.IsOffscreen -and $_.Current.BoundingRectangle.Height -gt 0 } catch { $false }
            })
    if ($menus.Count -ne 0) { throw 'Escape did not dismiss the Details column chooser' }

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        repeated_toggle_states = $states
        menu_remained_open = $true
        bounded_inside_window = $true
        wheel_reached_final_builtin_row = $true
        bottom_toggle_retained_scroll = $true
        upward_scroll_preserved_earlier_state = $true
        escape_dismissed = $true
        screenshots = @('details-column-chooser-persistent.png','details-column-chooser-scrolled-bottom.png')
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    $resolvedFixture = [IO.Path]::GetFullPath($fixture)
    $ownedPrefix = $output.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedFixture.StartsWith($ownedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to remove fixture outside evidence directory: $resolvedFixture"
    }
    if (Test-Path -LiteralPath $resolvedFixture) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}

Write-Output "Persistent scrollable Details column chooser smoke passed: $OutputDirectory"
