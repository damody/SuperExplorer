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

function Find-FileItemAny([string]$Name, [int]$TimeoutSeconds = 10) {
    Find-UitestElement -Root $context.Root -Description "file item $Name" -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
            ($element.Current.Name -eq $Name -or $element.Current.Name -like "$Name *")
    }
}

function Find-FileIcon([Windows.Automation.AutomationElement]$Row) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Image)
    $icon = $Row.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $icon) { throw "row has no image element: $($Row.Current.Name)" }
    $icon
}

function Get-IconPixels([string]$Screenshot, [Windows.Automation.AutomationElement]$Row) {
    $window = $context.Root.Current.BoundingRectangle
    $bounds = (Find-FileIcon $Row).Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::FromFile($Screenshot)
    try {
        $left = [Math]::Max(0, [int][Math]::Floor($bounds.Left - $window.Left))
        $right = [Math]::Min($bitmap.Width - 1, [int][Math]::Ceiling($bounds.Right - $window.Left) - 1)
        $top = [Math]::Max(0, [int][Math]::Floor($bounds.Top - $window.Top))
        $bottom = [Math]::Min($bitmap.Height - 1, [int][Math]::Ceiling($bounds.Bottom - $window.Top) - 1)
        if ($right -lt $left -or $bottom -lt $top) { throw "invalid icon bounds: $bounds" }
        $colors = [Collections.Generic.HashSet[int]]::new()
        $blueFallback = 0
        $pixels = 0
        for ($y = $top; $y -le $bottom; $y++) {
            for ($x = $left; $x -le $right; $x++) {
                $pixel = $bitmap.GetPixel($x, $y)
                [void]$colors.Add($pixel.ToArgb())
                $pixels++
                if ([Math]::Abs([int]$pixel.R - 145) -le 12 -and
                    [Math]::Abs([int]$pixel.G - 178) -le 12 -and
                    [Math]::Abs([int]$pixel.B - 209) -le 12) {
                    $blueFallback++
                }
            }
        }
        [ordered]@{
            name = $Row.Current.Name
            color_count = $colors.Count
            blue_fallback_pixels = $blueFallback
            pixel_count = $pixels
            blue_fallback_ratio = if ($pixels -gt 0) { $blueFallback / $pixels } else { 1.0 }
            bounds = [ordered]@{ left=$bounds.Left; top=$bounds.Top; width=$bounds.Width; height=$bounds.Height }
        }
    } finally {
        $bitmap.Dispose()
    }
}

try {
    $context = Start-UitestExplorer -InitialPath $resolvedInitial -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    $rows = @(
        Find-FileItemAny '.gitignore'
        Find-FileItemAny '.editorconfig'
    )
    $screenshot = Join-Path $output 'dotfile-shell-icons.png'
    $deadline = [DateTime]::UtcNow.AddSeconds(12)
    $icons = @()
    do {
        Start-Sleep -Milliseconds 250
        Save-UitestScreenshot -Root $context.Root -Path $screenshot
        $icons = @($rows | ForEach-Object { Get-IconPixels $screenshot $_ })
        $loaded = @($icons | Where-Object {
            $_.color_count -ge 4 -and $_.blue_fallback_ratio -lt 0.55
        }).Count -eq $icons.Count
    } while (-not $loaded -and [DateTime]::UtcNow -lt $deadline)

    if (-not $loaded) {
        $details = ($icons | ForEach-Object {
            "$($_.name): colors=$($_.color_count), blue_ratio=$($_.blue_fallback_ratio)"
        }) -join '; '
        throw "dotfile Shell icons remained fallback: $details"
    }

    [ordered]@{
        schema = 'dotfile-shell-icons-v1'
        status = 'PASS'
        initial_path = $resolvedInitial
        icons = $icons
        artifacts = @('dotfile-shell-icons.png')
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Dotfile Shell icon UITEST passed: $OutputDirectory"
