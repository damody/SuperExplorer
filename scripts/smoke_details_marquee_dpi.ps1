param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [string]$InitialPath = 'D:\UE_5.7',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$resolvedInitial = (Resolve-Path -LiteralPath $InitialPath).Path
$context = $null
$mouseDown = $false

function Test-MarqueeBlue([Drawing.Color]$Pixel) {
    $Pixel.R -le 80 -and $Pixel.B -ge 150 -and
        $Pixel.B -ge ($Pixel.R + 90) -and $Pixel.B -ge ($Pixel.G + 45)
}

function Measure-VerticalEdge(
    [Drawing.Bitmap]$Bitmap,
    [int]$ExpectedX,
    [int]$Top,
    [int]$Bottom,
    [int]$Tolerance = 8
) {
    $best = 0
    $bestX = -1
    foreach ($x in ([Math]::Max(0, $ExpectedX - $Tolerance))..([Math]::Min($Bitmap.Width - 1, $ExpectedX + $Tolerance))) {
        $count = 0
        foreach ($y in ([Math]::Max(0, $Top))..([Math]::Min($Bitmap.Height - 1, $Bottom))) {
            if (Test-MarqueeBlue ($Bitmap.GetPixel($x, $y))) { $count++ }
        }
        if ($count -gt $best) { $best = $count; $bestX = $x }
    }
    [ordered]@{ expected=$ExpectedX; detected=$bestX; blue_pixels=$best; delta=if ($bestX -ge 0) { [Math]::Abs($bestX - $ExpectedX) } else { -1 } }
}

function Measure-HorizontalEdge(
    [Drawing.Bitmap]$Bitmap,
    [int]$ExpectedY,
    [int]$Left,
    [int]$Right,
    [int]$Tolerance = 8
) {
    $best = 0
    $bestY = -1
    foreach ($y in ([Math]::Max(0, $ExpectedY - $Tolerance))..([Math]::Min($Bitmap.Height - 1, $ExpectedY + $Tolerance))) {
        $count = 0
        foreach ($x in ([Math]::Max(0, $Left))..([Math]::Min($Bitmap.Width - 1, $Right))) {
            if (Test-MarqueeBlue ($Bitmap.GetPixel($x, $y))) { $count++ }
        }
        if ($count -gt $best) { $best = $count; $bestY = $y }
    }
    [ordered]@{ expected=$ExpectedY; detected=$bestY; blue_pixels=$best; delta=if ($bestY -ge 0) { [Math]::Abs($bestY - $ExpectedY) } else { -1 } }
}

try {
    $context = Start-UitestExplorer -InitialPath $resolvedInitial -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    $startRow = Find-UitestFileItem -Root $context.Root -Name '.editorconfig'
    $endRow = Find-UitestFileItem -Root $context.Root -Name '.gitignore'
    $startBounds = $startRow.Current.BoundingRectangle
    $endBounds = $endRow.Current.BoundingRectangle
    $startX = [int]($startBounds.Left + $startBounds.Width * 0.58)
    $startY = [int]($startBounds.Top + $startBounds.Height / 2)
    $endX = [int]($endBounds.Left + $endBounds.Width * 0.80)
    $endY = [int]($endBounds.Top + $endBounds.Height / 2)

    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($startX, $startY)) {
        throw "could not position marquee start at ($startX,$startY)"
    }
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    $mouseDown = $true
    foreach ($step in 1..12) {
        $x = [int]($startX + ($endX - $startX) * $step / 12)
        $y = [int]($startY + ($endY - $startY) * $step / 12)
        if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($x, $y)) {
            throw "could not move marquee pointer to ($x,$y)"
        }
        Start-Sleep -Milliseconds 35
    }
    Start-Sleep -Milliseconds 350

    $selectedWhileHeld = Get-UitestSelectedCount -Root $context.Root
    if ($selectedWhileHeld -lt 2) {
        throw "non-name Details drag did not marquee-select multiple rows: selected=$selectedWhileHeld"
    }

    $screenshot = Join-Path $output 'details-marquee-held.png'
    Save-UitestScreenshot -Root $context.Root -Path $screenshot
    $window = $context.Root.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::FromFile($screenshot)
    try {
        $left = [Math]::Min($startX, $endX) - [int]$window.Left
        $right = [Math]::Max($startX, $endX) - [int]$window.Left
        $top = [Math]::Min($startY, $endY) - [int]$window.Top
        $bottom = [Math]::Max($startY, $endY) - [int]$window.Top
        $edges = [ordered]@{
            left = Measure-VerticalEdge $bitmap $left $top $bottom
            right = Measure-VerticalEdge $bitmap $right $top $bottom
            top = Measure-HorizontalEdge $bitmap $top $left $right
            bottom = Measure-HorizontalEdge $bitmap $bottom $left $right
        }
    } finally {
        $bitmap.Dispose()
    }

    foreach ($name in @('left','right','top','bottom')) {
        $edge = $edges[$name]
        if ($edge.detected -lt 0 -or $edge.delta -gt 8 -or $edge.blue_pixels -lt 6) {
            throw "marquee $name edge is not pointer-aligned: $($edge | ConvertTo-Json -Compress)"
        }
    }

    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    $mouseDown = $false
    Start-Sleep -Milliseconds 250
    $selectedAfterRelease = Get-UitestSelectedCount -Root $context.Root
    if ($selectedAfterRelease -lt 2) {
        throw "marquee selection did not survive pointer release: selected=$selectedAfterRelease"
    }

    [ordered]@{
        schema = 'details-marquee-dpi-v1'
        status = 'PASS'
        initial_path = $resolvedInitial
        start = [ordered]@{ x=$startX; y=$startY; row=$startRow.Current.Name }
        end = [ordered]@{ x=$endX; y=$endY; row=$endRow.Current.Name }
        selected_while_held = $selectedWhileHeld
        selected_after_release = $selectedAfterRelease
        edges = $edges
        artifacts = @('details-marquee-held.png')
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($mouseDown) { [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) }
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Details marquee DPI UITEST passed: $OutputDirectory"
