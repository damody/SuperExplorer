param(
    [Parameter(Mandatory)][string]$InitialPath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Executable = '',
    [ValidateRange(4, 1000)][int]$Cycles = 24,
    [double]$MaximumGrowthMB = 512,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$output = [IO.Path]::GetFullPath($OutputDirectory)
$rootPath = (Resolve-Path -LiteralPath $InitialPath).Path
$children = @(Get-ChildItem -LiteralPath $rootPath -Directory | Where-Object {
    @(Get-ChildItem -LiteralPath $_.FullName -File -Filter '*.png').Count -gt 0
} | Select-Object -First 3 -ExpandProperty FullName)
$paths = @($rootPath) + $children

function Invoke-ThumbnailTestControl($Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
    } else {
        Invoke-UitestClick -Element $Element
    }
    Start-Sleep -Milliseconds 200
}

function Set-LargeIcons($Context) {
    $view = Find-UitestElement -Root $Context.Root -Description 'View button' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq 'View'
    }
    Invoke-ThumbnailTestControl $view
    Start-Sleep -Milliseconds 100
    $menu = Find-UitestElement -Root $Context.Root -Description 'View menu' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Menu -and
            $element.Current.BoundingRectangle.Height -gt 0
    }
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Button)
    $items = @($menu.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
        Where-Object { $_.Current.BoundingRectangle.Height -gt 0 } |
        Sort-Object { $_.Current.BoundingRectangle.Top })
    if ($items.Count -eq 0) { throw 'View menu contained no view choices' }
    Invoke-ThumbnailTestControl $items[0]
    Start-Sleep -Milliseconds 350
}

function Send-Wheel($Context, [int]$Delta, [bool]$Zoom) {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($Context.Hwnd)
    Start-Sleep -Milliseconds 100
    if ([RustExplorerUitest.Native]::GetForegroundWindow() -ne $Context.Hwnd) {
        throw 'Test window lost foreground; do not send wheel input to another application'
    }
    $bounds = $Context.Root.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware(
        [int]($bounds.Left + $bounds.Width * 0.6),
        [int]($bounds.Top + $bounds.Height * 0.55))
    if ($Zoom) { [RustExplorerUitest.Native]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero) }
    try {
        $data = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]$Delta), 0)
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $data, [UIntPtr]::Zero)
    } finally {
        if ($Zoom) { [RustExplorerUitest.Native]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero) }
    }
    Start-Sleep -Milliseconds 150
}

$context = $null
$samples = @()
try {
    $context = Start-UitestExplorer -InitialPath $rootPath -OutputDirectory $output `
        -Executable $Executable -SkipBuild:$SkipBuild -AdditionalEnvironment @{
            SUPEREXPLORER_LOCALE = 'en'
            SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION = '1'
        }
    # Optimized builds can replace the bootstrap window after the helper returns.
    Start-Sleep -Seconds 2
    $context.Process.Refresh()
    $context.Hwnd = $context.Process.MainWindowHandle
    $context.Root = [Windows.Automation.AutomationElement]::FromHandle($context.Hwnd)
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr]::Zero, 20, 20, 1440, 880, 0x0040)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 300
    foreach ($cycle in 0..($Cycles - 1)) {
        $path = $paths[$cycle % $paths.Count]
        Set-UitestAddress -Context $context -Path $path
        Set-LargeIcons $context
        foreach ($delta in @(120,120,-120,-120)) { Send-Wheel $context $delta $true }
        foreach ($step in 1..8) { Send-Wheel $context -360 $false }
        Start-Sleep -Milliseconds 250
        if ($context.Process.HasExited) { throw 'Explorer exited during memory stress' }
        $context.Process.Refresh()
        $sample = [ordered]@{
            cycle = $cycle
            path = $path
            private_mb = [Math]::Round($context.Process.PrivateMemorySize64 / 1MB, 1)
            working_set_mb = [Math]::Round($context.Process.WorkingSet64 / 1MB, 1)
        }
        $samples += $sample
        $samples | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'memory.json') -Encoding utf8
        Write-Output ($sample | ConvertTo-Json -Compress)
    }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'final.png')
    $growth = $null
    if ($samples.Count -ge $paths.Count * 3) {
        $warm = ($samples[$paths.Count..($paths.Count * 2 - 1)].private_mb |
            Measure-Object -Average).Average
        $last = ($samples[($samples.Count - $paths.Count)..($samples.Count - 1)].private_mb |
            Measure-Object -Average).Average
        $growth = [Math]::Round($last - $warm, 1)
    }
    $summary = [ordered]@{
        executable = $context.Process.StartInfo.FileName
        cycles = $samples.Count
        peak_private_mb = ($samples.private_mb | Measure-Object -Maximum).Maximum
        final_private_mb = $samples[-1].private_mb
        post_warmup_growth_mb = $growth
        maximum_growth_mb = $MaximumGrowthMB
    }
    $summary | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'summary.json') -Encoding utf8
    if ($MaximumGrowthMB -gt 0 -and $null -ne $growth -and $growth -gt $MaximumGrowthMB) {
        throw "Thumbnail memory grew by $growth MB after warmup; limit is $MaximumGrowthMB MB"
    }
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}
