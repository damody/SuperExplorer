param(
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutputPath) {
    $OutputPath = Join-Path $workspaceRoot 'target\mixed-dpi-evidence\monitor-topology.json'
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputPath))
}

Add-Type -AssemblyName System.Windows.Forms
$screens = @([System.Windows.Forms.Screen]::AllScreens | ForEach-Object {
    [ordered]@{
        device_name = $_.DeviceName
        primary = $_.Primary
        bounds = [ordered]@{
            x = $_.Bounds.X
            y = $_.Bounds.Y
            width = $_.Bounds.Width
            height = $_.Bounds.Height
        }
    }
})
$activeMonitors = @(Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorID |
    Where-Object Active |
    ForEach-Object {
        [ordered]@{
            instance_name = $_.InstanceName
            manufacturer = ([Text.Encoding]::ASCII.GetString([byte[]]$_.ManufacturerName)).Trim([char]0)
            product_code = ([Text.Encoding]::ASCII.GetString([byte[]]$_.ProductCodeID)).Trim([char]0)
            serial = ([Text.Encoding]::ASCII.GetString([byte[]]$_.SerialNumberID)).Trim([char]0)
        }
    })

$audit = [ordered]@{
    captured_utc = [DateTime]::UtcNow.ToString('o')
    screen_count = $screens.Count
    active_physical_monitor_count = $activeMonitors.Count
    screens = $screens
    active_physical_monitors = $activeMonitors
    mixed_dpi_test_available = ($screens.Count -gt 1 -and $activeMonitors.Count -gt 1)
    limitation = if ($screens.Count -gt 1 -and $activeMonitors.Count -gt 1) {
        $null
    } else {
        'Only one active display is available; a cross-monitor mixed-DPI drag cannot be verified on this machine.'
    }
}

$parent = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Force -Path $parent | Out-Null
$audit | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath $OutputPath
$audit | ConvertTo-Json -Depth 6

