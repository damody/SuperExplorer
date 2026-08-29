param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $output | Out-Null
if (@(Get-Process SuperExplorer -ErrorAction SilentlyContinue).Count -gt 0) {
    throw 'close existing SuperExplorer windows before the current-profile appverif smoke'
}
if (-not $SkipBuild) {
    $release = if ($Profile -eq 'release') { @('--release') } else { @() }
    & cargo.exe build -p explorer-app -p explorer-extension-broker --locked @release
    if ($LASTEXITCODE -ne 0) { throw "product build failed: $LASTEXITCODE" }
}

function Get-LaunchedProcessIds($Context) {
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add([int]$Context.Process.Id)
    do {
        $changed = $false
        foreach ($process in @(Get-CimInstance Win32_Process)) {
            if ($ids.Contains([int]$process.ParentProcessId) -and $ids.Add([int]$process.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    return ,$ids
}

function Wait-OwnedPopup($Context, [int]$TimeoutSeconds = 12) {
    $allowed = Get-LaunchedProcessIds $Context
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $handles = [Collections.Generic.List[IntPtr]]::new()
        $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
            param([IntPtr]$hwnd, [IntPtr]$unused)
            if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
                $className = [Text.StringBuilder]::new(128)
                [void][RustExplorerUitest.Native]::GetClassName($hwnd, $className, $className.Capacity)
                [uint32]$processId = 0
                [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($hwnd, [ref]$processId)
                if ($className.ToString() -eq 'SuperExplorer.ImmersivePopup.v1' -and
                    $allowed.Contains([int]$processId)) {
                    $handles.Add($hwnd)
                }
            }
            return $true
        }
        [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
        if ($handles.Count -eq 1) { return $handles[0] }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'C:\appverifUI.dll did not open exactly one application-owned popup'
}

$context = Start-UitestExplorer -InitialPath '' -OutputDirectory $output -Profile $Profile `
    -SkipBuild -UseCurrentProfile
try {
    $item = Find-UitestElement -Root $context.Root -Description 'appverifUI.dll file row' `
        -TimeoutSeconds 20 -Predicate {
            param($element)
            $bounds = $element.Current.BoundingRectangle
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
                $element.Current.Name -like 'appverifUI.dll*' -and
                $bounds.Width -gt 0 -and $bounds.Height -gt 0
        }
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $point = Get-UitestPhysicalPoint -Element $item -HorizontalOffset 80
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)
    [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    $popup = Wait-OwnedPopup $context
    $rect = [RustExplorerUitest.Native+RECT]::new()
    if (-not [RustExplorerUitest.Native]::GetWindowRect($popup, [ref]$rect)) {
        throw 'GetWindowRect failed for application-owned appverif popup'
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $screenshot = Join-Path $output 'appverif-owned-popup.png'
    $bitmap = [Drawing.Bitmap]::new($width, $height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($screenshot, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
    [ordered]@{
        schema = 'superexplorer.appverif-owned-context.v1'
        status = 'passed'
        target = 'C:\appverifUI.dll'
        popup_class = 'SuperExplorer.ImmersivePopup.v1'
        application_owned = $true
        width = $width
        height = $height
        reference_width = 495
        width_delta = $width - 495
        screenshot = $screenshot
    } | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $output 'report.json')
    Send-UitestKey -Key 0x1B
} finally {
    Stop-UitestExplorer -Context $context
}

Get-Content -Raw (Join-Path $output 'report.json')
