param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Executable = '',
    [switch]$SkipBuild
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

# Reuse wheel and view controls without running the memory scenario.
$tokens = $null
$parseErrors = $null
$memorySource = [Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $PSScriptRoot 'smoke_thumbnail_memory.ps1'), [ref]$tokens, [ref]$parseErrors)
foreach ($function in $memorySource.FindAll({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst]
}, $false)) {
    . ([scriptblock]::Create($function.Extent.Text))
}

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
$bitmap = [Drawing.Bitmap]::new(1024, 1024)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
try {
    $graphics.Clear([Drawing.Color]::Magenta)
    $graphics.FillRectangle([Drawing.Brushes]::Cyan, 200, 200, 300, 300)
    foreach ($index in 0..119) {
        $bitmap.Save((Join-Path $fixture ('shot-{0:D3}.png' -f $index)), [Drawing.Imaging.ImageFormat]::Png)
    }
} finally {
    $graphics.Dispose()
    $bitmap.Dispose()
}
$context = $null
$results = @()
try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output `
        -Executable $Executable -SkipBuild:$SkipBuild -AdditionalEnvironment @{
            SUPEREXPLORER_LOCALE = 'en'
            SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION = '1'
        }
    Start-Sleep -Seconds 2
    $context.Process.Refresh()
    $context.Hwnd = $context.Process.MainWindowHandle
    $context.Root = [Windows.Automation.AutomationElement]::FromHandle($context.Hwnd)
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr]::Zero, 20, 20, 1440, 880, 0x0040)
    [void][RustExplorerUitest.Native]::PostMessage($context.Hwnd, 0x0112, [IntPtr]0xF030, [IntPtr]::Zero)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 500
    Set-LargeIcons $context
    Send-Wheel $context 120 $true
    Send-Wheel $context 120 $true
    foreach ($cycle in 0..7) {
        $delta = if ($cycle -lt 4) { -360 } else { 360 }
        foreach ($step in 1..5) { Send-Wheel $context $delta $false }
        $screenshot = Join-Path $output ("viewport-$cycle.png")
        $deadline = [DateTime]::UtcNow.AddSeconds(12)
        $lastProbeError = ''
        do {
          try {
            $context.Process.Refresh()
            $context.Hwnd = $context.Process.MainWindowHandle
            $context.Root = [Windows.Automation.AutomationElement]::FromHandle($context.Hwnd)
            [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
            Start-Sleep -Milliseconds 150
            $window = $context.Root.Current.BoundingRectangle
            $probes = @()
            $itemCondition = [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)
            $imageCondition = [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Image)
            foreach ($row in $context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, $itemCondition)) {
                try {
                    $bounds = $row.Current.BoundingRectangle
                    if ($row.Current.Name -notlike '*shot-*.png*' -or
                        $bounds.Bottom -le $window.Top + 300 -or $bounds.Top -ge $window.Bottom - 120) { continue }
                    $icon = $row.FindFirst([Windows.Automation.TreeScope]::Descendants, $imageCondition)
                    if ($null -ne $icon) {
                        $probes += [pscustomobject]@{
                            name = [regex]::Match($row.Current.Name, 'shot-[0-9]+\.png').Value
                            bounds = $icon.Current.BoundingRectangle
                        }
                    }
                } catch { }
            }
            Save-UitestScreenshot -Root $context.Root -Path $screenshot
            $metrics = @()
            $capture = [Drawing.Bitmap]::FromFile($screenshot)
            try {
                foreach ($probe in $probes) {
                    $bounds = $probe.bounds
                    $left = [Math]::Max(0, [int]($bounds.Left - $window.Left))
                    $top = [Math]::Max(0, [int]($bounds.Top - $window.Top))
                    $right = [Math]::Min($capture.Width - 1, [int]($bounds.Right - $window.Left))
                    $bottom = [Math]::Min($capture.Height - 1, [int]($bounds.Bottom - $window.Top))
                    $magenta = 0
                    $pixels = 0
                    for ($y = $top; $y -le $bottom; $y += 16) {
                        for ($x = $left; $x -le $right; $x += 16) {
                            $pixel = $capture.GetPixel($x, $y)
                            $pixels++
                            if ($pixel.R -ge 180 -and $pixel.B -ge 180 -and $pixel.G -le 100) { $magenta++ }
                        }
                    }
                    $metrics += [ordered]@{name=$probe.name; magenta_ratio=if ($pixels) { $magenta / $pixels } else { 0 }}
                }
            } finally { $capture.Dispose() }
            $loaded = $metrics.Count -ge 3 -and @($metrics | Where-Object {
                $_.magenta_ratio -lt 0.04
            }).Count -eq 0
          } catch {
            # Virtualized rows can disappear while UIA is enumerating them.
            $metrics = @()
            $loaded = $false
            $lastProbeError = $_.Exception.Message
          }
            if (-not $loaded) { Start-Sleep -Milliseconds 300 }
        } while (-not $loaded -and [DateTime]::UtcNow -lt $deadline)
        if (-not $loaded) { throw "Visible thumbnails missing at cycle ${cycle}: $($metrics | ConvertTo-Json -Compress -Depth 5); probe=$lastProbeError" }
        $results += [ordered]@{cycle=$cycle; visible=$metrics; screenshot=$screenshot}
        $results | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
        Write-Output "cycle=$cycle all $($metrics.Count) visible thumbnails contain image pixels"
    }
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}
