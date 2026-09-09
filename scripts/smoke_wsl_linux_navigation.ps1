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

function Get-LeftNavButtons {
    $window = $context.Root.Current.BoundingRectangle
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button
        )
    ) | Where-Object {
        try {
            $bounds = $_.Current.BoundingRectangle
            $bounds.Width -gt 0 -and $bounds.Height -gt 0 -and
                $bounds.Left -lt ($window.Left + 420) -and
                $bounds.Top -gt ($window.Top + 120)
        } catch { $false }
    } | ForEach-Object {
        [ordered]@{
            name = $_.Current.Name
            left = [int]$_.Current.BoundingRectangle.Left
            top = [int]$_.Current.BoundingRectangle.Top
            width = [int]$_.Current.BoundingRectangle.Width
            height = [int]$_.Current.BoundingRectangle.Height
        }
    })
}

function Find-LeftNavButton([string]$Name) {
    $window = $context.Root.Current.BoundingRectangle
    Find-UitestElement -Root $context.Root -Description "left nav $Name" -TimeoutSeconds 4 -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq $Name -and
            $bounds.Width -gt 0 -and $bounds.Height -gt 0 -and
            $bounds.Left -lt ($window.Left + 420) -and
            $bounds.Top -gt ($window.Top + 120)
    }
}

function Scroll-LeftNav([int]$Steps = 8) {
    $window = $context.Root.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetCursorPos(
        [int]($window.Left + 180),
        [int]($window.Top + $window.Height * 0.72)
    )
    Start-Sleep -Milliseconds 120
    foreach ($step in 1..$Steps) {
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, [uint32](unchecked([uint32]-120)), [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
}

try {
    $context = Start-UitestExplorer -InitialPath 'C:\' -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    Start-Sleep -Milliseconds 1200
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output '01-launch.png')
    $namesBefore = @(Get-LeftNavButtons)
    $namesBefore | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $output 'nav-buttons-before.json')

    $linux = $null
    $ubuntu = $null
    foreach ($attempt in 1..6) {
        try { $linux = Find-LeftNavButton 'Linux' } catch { $linux = $null }
        try { $ubuntu = Find-LeftNavButton 'Ubuntu-24.04' } catch { $ubuntu = $null }
        if ($null -ne $linux -and $null -ne $ubuntu) { break }
        Scroll-LeftNav -Steps 6
        Start-Sleep -Milliseconds 250
        Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output ("02-scroll-{0}.png" -f $attempt))
    }

    $namesAfter = @(Get-LeftNavButtons)
    $namesAfter | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $output 'nav-buttons-after.json')
    if ($null -eq $linux) { throw "Linux navigation row not found. names=$($namesAfter.name -join ', ')" }
    if ($null -eq $ubuntu) { throw "Ubuntu-24.04 navigation row not found. names=$($namesAfter.name -join ', ')" }

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output '03-linux-visible.png')
    Invoke-UitestClick -Element $linux
    Start-Sleep -Milliseconds 900
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output '04-linux-selected.png')
    Invoke-UitestClick -Element $ubuntu
    Start-Sleep -Milliseconds 1800
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output '05-ubuntu-selected.png')

    $window = $context.Root.Current.BoundingRectangle
    $headerNames = @(
        $context.Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition
        ) | Where-Object {
            try {
                $bounds = $_.Current.BoundingRectangle
                $name = $_.Current.Name
                $bounds.Width -gt 0 -and $bounds.Height -gt 0 -and
                    $bounds.Left -gt ($window.Left + 420) -and
                    $bounds.Top -gt ($window.Top + 180) -and
                    $bounds.Top -lt ($window.Top + 320) -and
                    -not [string]::IsNullOrWhiteSpace($name)
            } catch { $false }
        } | ForEach-Object { $_.Current.Name }
    )
    $headerNames | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $output 'details-headers.json')
    $joined = $headerNames -join ' | '
    foreach ($forbidden in @('File Count','Folder Count','Folder size','Lock owners','Lock Owners')) {
        if ($joined -match [regex]::Escape($forbidden)) {
            throw "WSL details still shows unsupported column '$forbidden'. headers=$joined"
        }
    }
    $required = @('名稱','修改日期','類型','大小','Name','Date modified','Type','Size')
    $foundRequired = @($required | Where-Object { $joined -like "*$_*" })
    if ($foundRequired.Count -lt 4) {
        throw "WSL details missing universal columns. headers=$joined"
    }

    $navAfterWsl = @(Get-LeftNavButtons)
    $navAfterWsl | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $output 'nav-buttons-after-wsl.json')
    $wslUnderThisPc = @($navAfterWsl | Where-Object { $_.name -match 'wsl\.localhost|wsl\$' })
    if ($wslUnderThisPc.Count -gt 0) {
        throw "This PC still shows WSL UNC rows: $($wslUnderThisPc.name -join ', ')"
    }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output '06-this-pc-without-wsl-unc.png')

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        linux_name = $linux.Current.Name
        ubuntu_name = $ubuntu.Current.Name
        details_headers = $headerNames
        linux_bounds = [ordered]@{
            left = [int]$linux.Current.BoundingRectangle.Left
            top = [int]$linux.Current.BoundingRectangle.Top
            width = [int]$linux.Current.BoundingRectangle.Width
            height = [int]$linux.Current.BoundingRectangle.Height
        }
        ubuntu_bounds = [ordered]@{
            left = [int]$ubuntu.Current.BoundingRectangle.Left
            top = [int]$ubuntu.Current.BoundingRectangle.Top
            width = [int]$ubuntu.Current.BoundingRectangle.Width
            height = [int]$ubuntu.Current.BoundingRectangle.Height
        }
        nav_names = @($namesAfter | ForEach-Object { $_.name })
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "WSL Linux navigation smoke passed: $OutputDirectory"
