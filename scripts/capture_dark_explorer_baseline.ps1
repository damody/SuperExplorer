param(
    [string]$OutputDirectory = 'target\explorer-reference-evidence\dark-final',
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [string]$PythonExecutable = 'python',
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

if (-not ('ExplorerDarkTheme.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerDarkTheme {
    public static class Native {
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wParam,
            string lParam, uint flags, uint timeout, out IntPtr result);
    }
}
'@
}

function Broadcast-ThemeChange {
    $result = [IntPtr]::Zero
    [void][ExplorerDarkTheme.Native]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [IntPtr]::Zero,
        'ImmersiveColorSet', 2, 3000, [ref]$result)
    [void][ExplorerDarkTheme.Native]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [IntPtr]::Zero,
        'WindowsThemeElement', 2, 3000, [ref]$result)
}

$personalize = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
$original = Get-ItemProperty -Path $personalize -Name AppsUseLightTheme -ErrorAction SilentlyContinue
$originalExists = $null -ne $original
$originalValue = if ($originalExists) { [int]$original.AppsUseLightTheme } else { 1 }
$shell = New-Object -ComObject Shell.Application
try {
    $dWindow = @($shell.Windows()) | Where-Object { $_.LocationURL -eq 'file:///D:/' } | Select-Object -First 1
    if ($null -eq $dWindow) {
        Start-Process explorer.exe -ArgumentList 'D:\'
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            Start-Sleep -Milliseconds 250
            $dWindow = @($shell.Windows()) | Where-Object { $_.LocationURL -eq 'file:///D:/' } | Select-Object -First 1
        } while ($null -eq $dWindow -and [DateTime]::UtcNow -lt $deadline)
    }
    if ($null -eq $dWindow) { throw 'could not open a real Explorer D: window' }
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($dWindow)

    Set-ItemProperty -Path $personalize -Name AppsUseLightTheme -Type DWord -Value 0
    Broadcast-ThemeChange
    Start-Sleep -Seconds 3

    $explorerDirectory = Join-Path $OutputDirectory 'explorer'
    $applicationDirectory = Join-Path $OutputDirectory 'application'
    $diffDirectory = Join-Path $OutputDirectory 'diff'
    & (Join-Path $PSScriptRoot 'capture_explorer_reference.ps1') -Theme dark -OutputDirectory $explorerDirectory
    & (Join-Path $PSScriptRoot 'capture_visual_fixture.ps1') -Theme dark -ExpectedDpiPercent 175 `
        -State populated -Profile $Profile -Width 1520 -Height 919 `
        -OutputDirectory $applicationDirectory -SkipBuild
    & (Join-Path $PSScriptRoot 'compare_explorer_reference.ps1') `
        -ExplorerDirectory $explorerDirectory -ApplicationDirectory $applicationDirectory `
        -OutputDirectory $diffDirectory -PythonExecutable $PythonExecutable `
        -ExplorerRegions (Join-Path $workspaceRoot 'docs\visual\explorer-d-drive-light-175-regions.json') `
        -ApplicationDiagnostics (Join-Path $applicationDirectory 'diagnostics.json') `
        -RequireRegionPass

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        location = 'D:\'
        apps_use_light_theme_during_capture = 0
        original_apps_use_light_theme = $originalValue
        explorer = $explorerDirectory
        application = $applicationDirectory
        diff = $diffDirectory
    } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
    Write-Output "Dark Explorer/application baseline passed: $OutputDirectory"
} finally {
    if ($originalExists) {
        Set-ItemProperty -Path $personalize -Name AppsUseLightTheme -Type DWord -Value $originalValue
    } else {
        Remove-ItemProperty -Path $personalize -Name AppsUseLightTheme -ErrorAction SilentlyContinue
    }
    Broadcast-ThemeChange
    Start-Sleep -Seconds 3
    [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
}
