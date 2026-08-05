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
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path (Join-Path $fixture '00-folder') | Out-Null
Set-Content -LiteralPath (Join-Path $fixture '01-plain.txt') -Value 'Shell icon fixture' -Encoding utf8
Set-Content -LiteralPath (Join-Path $fixture '02-clip.mp4') -Value 'unsupported video fixture keeps its Shell fallback' -Encoding utf8
foreach ($index in 0..39) {
    Set-Content -LiteralPath (Join-Path $fixture ('filler-{0:D3}.dat' -f $index)) -Value $index -Encoding ascii
}

function New-MagentaImage([string]$Path) {
    $bitmap = [Drawing.Bitmap]::new(320, 180)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.Clear([Drawing.Color]::Magenta)
        $graphics.FillRectangle([Drawing.Brushes]::Cyan, 30, 30, 80, 80)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Bmp)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}
New-MagentaImage (Join-Path $fixture '00-photo.bmp')
New-MagentaImage (Join-Path $fixture 'filler-020.bmp')
New-MagentaImage (Join-Path $fixture 'zz-tail.bmp')

function Find-Named([Windows.Automation.AutomationElement]$Root, [Windows.Automation.ControlType]$Type, [string]$Name) {
    Find-UitestElement -Root $Root -Description "$Type $Name" -Predicate {
        param($element)
        $element.Current.ControlType -eq $Type -and $element.Current.Name -eq $Name
    }
}

function Send-Key([byte]$VirtualKey) {
    [RustExplorerUitest.Native]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::keybd_event($VirtualKey, 0, 2, [UIntPtr]::Zero)
}

function Send-CtrlWheel([Windows.Automation.AutomationElement]$Root, [int]$Delta) {
    $row = Get-UitestFileItems -Root $Root | Select-Object -First 1
    if ($null -eq $row) { throw 'Ctrl+wheel requires a realized file item' }
    $bounds = $row.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware(
        [int]($bounds.Left + [Math]::Min($bounds.Width / 2, 80)),
        [int]($bounds.Top + $bounds.Height / 2))
    [RustExplorerUitest.Native]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    try {
        $data = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]$Delta), 0)
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $data, [UIntPtr]::Zero)
    } finally {
        [RustExplorerUitest.Native]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 250
}

function Set-ViewMode([Windows.Automation.AutomationElement]$Root, [int]$Index) {
    Invoke-UitestClick -Element (Find-Named $Root ([Windows.Automation.ControlType]::Button) 'View')
    Start-Sleep -Milliseconds 250
    $menuCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Menu)
    $menu = $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $menuCondition) | Where-Object {
        $_.Current.BoundingRectangle.Width -gt 0 -and $_.Current.BoundingRectangle.Height -gt 0
    } | Sort-Object { $_.Current.BoundingRectangle.Top } | Select-Object -First 1
    if ($null -eq $menu) { throw 'View menu was not exposed to UI Automation' }
    $buttonCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Button)
    $items = @($menu.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition) | Where-Object {
        $_.Current.BoundingRectangle.Width -gt 0 -and $_.Current.BoundingRectangle.Height -gt 0
    } | Sort-Object { $_.Current.BoundingRectangle.Top })
    if ($items.Count -le $Index) { throw "View menu index $Index missing; count=$($items.Count)" }
    Invoke-UitestClick -Element $items[$Index]
    Start-Sleep -Milliseconds 700
}

function Get-IconMetrics([Windows.Automation.AutomationElement]$Root, [string]$Screenshot, [string]$Name) {
    $row = Find-UitestFileItem -Root $Root -Name $Name
    $imageCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Image)
    $icon = $row.FindFirst([Windows.Automation.TreeScope]::Descendants, $imageCondition)
    if ($null -eq $icon) { throw "$Name did not expose an icon" }
    $window = $Root.Current.BoundingRectangle
    $bounds = $icon.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::FromFile($Screenshot)
    try {
        $left = [Math]::Max(0, [int][Math]::Floor($bounds.Left - $window.Left))
        $right = [Math]::Min($bitmap.Width - 1, [int][Math]::Ceiling($bounds.Right - $window.Left) - 1)
        $top = [Math]::Max(0, [int][Math]::Floor($bounds.Top - $window.Top))
        $bottom = [Math]::Min($bitmap.Height - 1, [int][Math]::Ceiling($bounds.Bottom - $window.Top) - 1)
        $colors = [Collections.Generic.HashSet[int]]::new()
        $blue = 0
        $magenta = 0
        $foreground = 0
        $pixels = 0
        for ($y = $top; $y -le $bottom; $y++) {
            for ($x = $left; $x -le $right; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                [void]$colors.Add($pixel.ToArgb())
                $pixels++
                if ([Math]::Abs([int]$pixel.R - 145) -le 14 -and [Math]::Abs([int]$pixel.G - 178) -le 14 -and [Math]::Abs([int]$pixel.B - 209) -le 14) { $blue++ }
                if ($pixel.R -ge 180 -and $pixel.B -ge 180 -and $pixel.G -le 100) { $magenta++ }
                if ($pixel.R -lt 245 -or $pixel.G -lt 245 -or $pixel.B -lt 245) { $foreground++ }
            }
        }
        [ordered]@{
            name=$Name
            colors=$colors.Count
            blue_ratio=if ($pixels) { $blue / $pixels } else { 1.0 }
            magenta_ratio=if ($pixels) { $magenta / $pixels } else { 0.0 }
            foreground_ratio=if ($pixels) { $foreground / $pixels } else { 0.0 }
            bounds=[ordered]@{left=$bounds.Left;top=$bounds.Top;width=$bounds.Width;height=$bounds.Height}
        }
    } finally {
        $bitmap.Dispose()
    }
}

$context = $null
try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    $modes = @(
        [ordered]@{index=0; id='extra-large'; thumbnail=$true; maximum=$true},
        [ordered]@{index=1; id='large'; thumbnail=$true; maximum=$false},
        [ordered]@{index=2; id='medium'; thumbnail=$true; maximum=$false},
        [ordered]@{index=3; id='small'; thumbnail=$false; maximum=$false},
        [ordered]@{index=6; id='tiles'; thumbnail=$false; maximum=$false}
    )
    $results = @()
    $maximumFolder = $null
    $maximumThumbnail = $null
    foreach ($mode in $modes) {
        Set-ViewMode $context.Root $mode.index
        $windowBounds = $context.Root.Current.BoundingRectangle
        $fillerProbe = Get-UitestFileItems -Root $context.Root | Where-Object {
            $candidateBounds = $_.Current.BoundingRectangle
            $_.Current.Name -like '*filler-*.dat*' -and
                $candidateBounds.Left -lt $windowBounds.Right -and
                $candidateBounds.Right -gt $windowBounds.Left -and
                $candidateBounds.Top -lt $windowBounds.Bottom -and
                $candidateBounds.Bottom -gt $windowBounds.Top
        } | Select-Object -First 1
        if ($null -eq $fillerProbe) { throw "$($mode.id) did not realize a generic file probe" }
        $fillerName = [regex]::Match($fillerProbe.Current.Name, 'filler-[0-9]+\.dat').Value
        $screenshot = Join-Path $output ("$($mode.id).png")
        $deadline = [DateTime]::UtcNow.AddSeconds(12)
        do {
            Save-UitestScreenshot -Root $context.Root -Path $screenshot
            $items = @('00-folder','00-photo.bmp','01-plain.txt','02-clip.mp4',$fillerName) | ForEach-Object {
                Get-IconMetrics $context.Root $screenshot $_
            }
            $photo = $items | Where-Object name -eq '00-photo.bmp' | Select-Object -First 1
            $loaded = @($items | Where-Object {
                $minimumForeground = if ($_.name -eq '00-folder') { 0.01 } else { 0.05 }
                $_.colors -ge 3 -and $_.blue_ratio -lt 0.60 -and $_.foreground_ratio -ge $minimumForeground
            }).Count -eq $items.Count -and
                (-not $mode.thumbnail -or $photo.magenta_ratio -ge 0.03)
            if (-not $loaded) { Start-Sleep -Milliseconds 250 }
        } while (-not $loaded -and [DateTime]::UtcNow -lt $deadline)
        if (-not $loaded) {
            throw "$($mode.id) retained a fallback or missed image-content pixels: $($items | ConvertTo-Json -Compress)"
        }
        $results += [ordered]@{mode=$mode.id; thumbnail_expected=$mode.thumbnail; items=$items; screenshot=[IO.Path]::GetFileName($screenshot)}
        if ($mode.maximum) {
            # Extra Large opens at 256 logical px. Two Explorer Ctrl+wheel notches reach the
            # maximum 512 px size where the former fixed yellow fallback was visible.
            Send-CtrlWheel $context.Root 120
            Send-CtrlWheel $context.Root 120
            $maximumScreenshot = Join-Path $output 'maximum-folder.png'
            $deadline = [DateTime]::UtcNow.AddSeconds(12)
            do {
                Save-UitestScreenshot -Root $context.Root -Path $maximumScreenshot
                $maximumFolder = Get-IconMetrics $context.Root $maximumScreenshot '00-folder'
                $maximumThumbnail = Get-IconMetrics $context.Root $maximumScreenshot '00-photo.bmp'
                $folderRowBounds = (Find-UitestFileItem -Root $context.Root -Name '00-folder').Current.BoundingRectangle
                $photoRowBounds = (Find-UitestFileItem -Root $context.Root -Name '00-photo.bmp').Current.BoundingRectangle
                $folderWidthRatio = $maximumFolder.bounds.width / [Math]::Max(1.0, $folderRowBounds.Width)
                $photoWidthRatio = $maximumThumbnail.bounds.width / [Math]::Max(1.0, $photoRowBounds.Width)
                $maximumLoaded = $maximumFolder.bounds.width -ge 480 -and
                    $maximumFolder.colors -ge 3 -and
                    $maximumFolder.blue_ratio -lt 0.60 -and
                    $maximumFolder.foreground_ratio -ge 0.02 -and
                    $maximumThumbnail.magenta_ratio -ge 0.03 -and
                    $photoWidthRatio -ge 0.985 -and
                    $maximumThumbnail.bounds.width -ge ($maximumFolder.bounds.width * 1.05) -and
                    $folderWidthRatio -lt $photoWidthRatio
                if (-not $maximumLoaded) { Start-Sleep -Milliseconds 250 }
            } while (-not $maximumLoaded -and [DateTime]::UtcNow -lt $deadline)
            if (-not $maximumLoaded) {
                throw "maximum zoom did not edge-fit the real thumbnail while bounding the folder icon: folder=$($maximumFolder | ConvertTo-Json -Compress) thumbnail=$($maximumThumbnail | ConvertTo-Json -Compress)"
            }
        }
    }

    Set-ViewMode $context.Root 2
    $firstVisible = Get-UitestFileItems -Root $context.Root | Select-Object -First 1
    $bounds = $firstVisible.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware(
        [int]($bounds.Left + [Math]::Min(80, $bounds.Width / 2)),
        [int]($bounds.Top + $bounds.Height / 2))
    $wheelDown = [uint32]4294967176
    foreach ($step in 1..20) {
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $wheelDown, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 35
    }
    Start-Sleep -Milliseconds 300
    $windowBounds = $context.Root.Current.BoundingRectangle
    $scrolledRow = Get-UitestFileItems -Root $context.Root | Where-Object {
        $candidateBounds = $_.Current.BoundingRectangle
        $_.Current.Name -like '*filler-*.dat*' -and
            $candidateBounds.Left -lt $windowBounds.Right -and
            $candidateBounds.Right -gt $windowBounds.Left -and
            $candidateBounds.Top -lt $windowBounds.Bottom -and
            $candidateBounds.Bottom -gt $windowBounds.Top
    } | Select-Object -First 1
    if ($null -eq $scrolledRow) { throw 'scrolling did not expose a newly realized filler item' }
    $scrolledName = [regex]::Match($scrolledRow.Current.Name, 'filler-[0-9]+\.dat').Value
    if (-not $scrolledName) { throw "could not recover scrolled item name: $($scrolledRow.Current.Name)" }
    $tailScreenshot = Join-Path $output 'scrolled-icon.png'
    Save-UitestScreenshot -Root $context.Root -Path $tailScreenshot
    $tail = Get-IconMetrics $context.Root $tailScreenshot $scrolledName
    if ($tail.colors -lt 3 -or $tail.blue_ratio -ge 0.60 -or $tail.foreground_ratio -lt 0.05) {
        throw "newly realized scrolled Shell icon did not load: $($tail | ConvertTo-Json -Compress)"
    }

    [ordered]@{
        schema='icon-view-visual-loading-v1'
        status='PASS'
        fixture=$fixture
        modes=$results
        maximum_folder=$maximumFolder
        maximum_thumbnail=$maximumThumbnail
        scrolled_tail=$tail
        artifacts=@('extra-large.png','maximum-folder.png','large.png','medium.png','small.png','tiles.png','scrolled-icon.png')
    } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Icon-view visual loading UITEST passed: $OutputDirectory"
