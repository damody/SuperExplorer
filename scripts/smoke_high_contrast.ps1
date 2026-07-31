param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [string]$OutputDirectory = 'target\high-contrast-evidence\current-system',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile
    if ($LASTEXITCODE -ne 0) { throw "artifact finalization failed: $LASTEXITCODE" }
}

if (-not ('ExplorerHighContrast.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerHighContrast {
    public static class Native {
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        public struct HighContrast {
            public uint cbSize;
            public uint dwFlags;
            public IntPtr lpszDefaultScheme;
        }
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SystemParametersInfo(uint action, uint parameter, ref HighContrast value, uint update);
        [DllImport("user32.dll")]
        public static extern uint GetSysColor(int index);
    }
}
'@
}

function Get-HighContrastState {
    $state = [ExplorerHighContrast.Native+HighContrast]::new()
    $state.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($state)
    if (-not [ExplorerHighContrast.Native]::SystemParametersInfo(0x42, $state.cbSize, [ref]$state, 0)) {
        throw "SPI_GETHIGHCONTRAST failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    return $state
}

function Set-HighContrastFlags([uint32]$Flags) {
    $state = [ExplorerHighContrast.Native+HighContrast]::new()
    $state.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($state)
    $state.dwFlags = $Flags
    $state.lpszDefaultScheme = [IntPtr]::Zero
    if (-not [ExplorerHighContrast.Native]::SystemParametersInfo(0x43, $state.cbSize, [ref]$state, 3)) {
        throw "SPI_SETHIGHCONTRAST failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
}

$original = Get-HighContrastState
$changed = (($original.dwFlags -band 1) -eq 0)
try {
    if ($changed) {
        Set-HighContrastFlags ($original.dwFlags -bor 1)
        Start-Sleep -Seconds 2
    }
    $active = Get-HighContrastState
    if (($active.dwFlags -band 1) -eq 0) { throw 'Windows did not enter high contrast mode' }

    $captureDirectory = Join-Path $OutputDirectory 'capture'
    & (Join-Path $PSScriptRoot 'capture_visual_fixture.ps1') `
        -Theme light -ExpectedDpiPercent 175 -State focused -Profile $Profile `
        -OutputDirectory $captureDirectory -SkipBuild
    if ($LASTEXITCODE -ne 0) { throw "high contrast capture failed: $LASTEXITCODE" }

    $diagnostics = Get-Content -Raw -Encoding utf8 (Join-Path $captureDirectory 'diagnostics.json') | ConvertFrom-Json
    if (-not $diagnostics.theme.high_contrast_active) { throw 'application did not select the Windows high-contrast palette' }
    $colors = @{}
    foreach ($entry in $diagnostics.theme.colors) { $colors[$entry.slot] = $entry.rgba }
    $window = [ExplorerHighContrast.Native]::GetSysColor(5)
    $windowText = [ExplorerHighContrast.Native]::GetSysColor(8)
    $highlight = [ExplorerHighContrast.Native]::GetSysColor(13)
    $rgb = {
        param([uint32]$value)
        @(
            [int]($value -band 255)
            [int](($value -shr 8) -band 255)
            [int](($value -shr 16) -band 255)
            255
        )
    }
    if ((Compare-Object $colors.Surface (& $rgb $window))) { throw 'surface does not match COLOR_WINDOW' }
    if ((Compare-Object $colors.TextPrimary (& $rgb $windowText))) { throw 'text does not match COLOR_WINDOWTEXT' }
    if ((Compare-Object $colors.SelectedActive (& $rgb $highlight))) { throw 'selection does not match COLOR_HIGHLIGHT' }
    if (($colors.TextDisabled -join ',') -eq ($colors.TextPrimary -join ',')) { throw 'disabled state is not visually distinct' }
    if (($colors.SelectedActive -join ',') -eq ($colors.Surface -join ',')) { throw 'selected state is not visually distinct' }

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        original_flags = $original.dwFlags
        active_flags = $active.dwFlags
        toggled_for_test = $changed
        system_color_assertions = 'COLOR_WINDOW, COLOR_WINDOWTEXT, COLOR_HIGHLIGHT matched'
        opaque_state_assertions = 'disabled and selected use distinct opaque system colors'
        capture = $captureDirectory
        screenshot_sha256 = (Get-FileHash -Algorithm SHA256 (Join-Path $captureDirectory 'screenshot.png')).Hash
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
    Write-Output "High contrast smoke passed: $OutputDirectory"
} finally {
    if ($changed) {
        Set-HighContrastFlags $original.dwFlags
        Start-Sleep -Seconds 2
        $restored = Get-HighContrastState
        if ($restored.dwFlags -ne $original.dwFlags) {
            throw "high contrast restore mismatch: expected $($original.dwFlags), got $($restored.dwFlags)"
        }
    }
}
