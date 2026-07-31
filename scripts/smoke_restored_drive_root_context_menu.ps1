param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'mapped-drive-root'
$localAppData = Join-Path $output 'localappdata'
$sessionPath = Join-Path $localAppData 'RustGpuiExplorer\state\v1\session.json'
$profileDirectory = if ($Profile -eq 'release') { 'release' } else { 'debug' }
$executable = Join-Path $workspace "target\$profileDirectory\SuperExplorer.exe"
$fixtureWriter = Join-Path $workspace "target\$profileDirectory\explorer-session-fixture.exe"

if (-not $SkipBuild) {
    $profileArguments = if ($Profile -eq 'release') { @('--release') } else { @() }
    $productBuildArguments = @(
        'build', '--locked',
        '-p', 'explorer-app',
        '-p', 'explorer-extension-broker'
    ) + $profileArguments
    & cargo.exe @productBuildArguments
    if ($LASTEXITCODE -ne 0) { throw "product $Profile build failed" }

    # The manifest-driven case runs inside explorer-uitest.exe. Build only the
    # fixture binary so Cargo never attempts to replace the active runner.
    $fixtureBuildArguments = @(
        'build', '--locked',
        '-p', 'explorer-uitest', '--bin', 'explorer-session-fixture'
    ) + $profileArguments
    & cargo.exe @fixtureBuildArguments
    if ($LASTEXITCODE -ne 0) { throw "session fixture $Profile build failed" }
}
foreach ($path in @($executable, $fixtureWriter)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required binary is missing: $path"
    }
}

New-Item -ItemType Directory -Force -Path $fixture, $localAppData | Out-Null
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture '00-first-sentinel.txt') -Value 'sentinel'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'Alpha.txt') -Value 'alpha'
Set-Content -Encoding utf8 -LiteralPath (Join-Path $fixture 'Beta.txt') -Value 'beta'

$driveLetter = $null
foreach ($letter in [char[]]'ZYXWVUTSRQPONMLKJIHGF') {
    if (-not (Test-Path -LiteralPath "$letter`:\")) {
        $driveLetter = [string]$letter
        break
    }
}
if ($null -eq $driveLetter) { throw 'no unused drive letter is available for the restored-session fixture' }
$driveDesignator = "$driveLetter`:"
$driveRoot = "$driveLetter`:\"
$context = $null
$mappingCreated = $false

function Get-LaunchedProcessTreeIds {
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add([int]$context.Process.Id)
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

function Get-ProcessBoundPopups {
    $handles = [Collections.Generic.List[IntPtr]]::new()
    $allowed = Get-LaunchedProcessTreeIds
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($hwnd)) {
            $className = [Text.StringBuilder]::new(64)
            [void][RustExplorerUitest.Native]::GetClassName($hwnd, $className, $className.Capacity)
            [uint32]$processId = 0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($hwnd, [ref]$processId)
            if ($className.ToString() -eq '#32768' -and $allowed.Contains([int]$processId)) {
                $handles.Add($hwnd)
            }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    @($handles | Select-Object -Unique)
}

function Wait-OnePopup([int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $popups = @(Get-ProcessBoundPopups)
        if ($popups.Count -eq 1) { return $popups[0] }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'restored drive-root right click did not open exactly one process-bound native popup'
}

function Invoke-ManualRightClick([Windows.Automation.AutomationElement]$Element) {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 100
    $point = Get-UitestPhysicalPoint -Element $Element -HorizontalOffset 100
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)) {
        throw "DPI-aware cursor positioning failed at ($($point.X),$($point.Y))"
    }
    [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 120
    [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

try {
    & subst.exe $driveDesignator $fixture
    if ($LASTEXITCODE -ne 0) { throw "failed to map $driveDesignator to the owned fixture" }
    $mappingCreated = $true

    & $fixtureWriter $sessionPath $driveDesignator $fixture
    if ($LASTEXITCODE -ne 0) { throw 'failed to write the restored-session fixture' }

    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $executable
    $start.WorkingDirectory = $workspace
    $start.UseShellExecute = $false
    $start.Environment['LOCALAPPDATA'] = $localAppData
    $start.Environment['EXPLORER_LOG_DIR'] = $output
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if ($process.HasExited) { throw "application exited during restored-session startup: $($process.ExitCode)" }
        $process.Refresh()
        Start-Sleep -Milliseconds 100
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'restored-session window did not appear' }
    $context = [pscustomobject]@{
        Process = $process
        Hwnd = $process.MainWindowHandle
        Root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    }

    # Activate once like a user selecting the app. Deliberately do not use HWND_TOPMOST.
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $context.Root.SetFocus()
    Start-Sleep -Milliseconds 900
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'restored-drive-root.png')
    $beta = Find-UitestElement -Root $context.Root -Description 'visible Beta.txt file row' -TimeoutSeconds 15 -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
            $element.Current.Name -like 'Beta.txt*' -and
            $bounds.Width -gt 0 -and $bounds.Height -gt 0
    }
    Invoke-ManualRightClick -Element $beta
    $popup = Wait-OnePopup

    $selectionPattern = $null
    if (-not $beta.TryGetCurrentPattern(
        [Windows.Automation.SelectionItemPattern]::Pattern,
        [ref]$selectionPattern
    ) -or -not ([Windows.Automation.SelectionItemPattern]$selectionPattern).Current.IsSelected) {
        throw 'physical right click did not select the exact non-first restored drive-root item'
    }
    $listItemCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem
    )
    $selected = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        $listItemCondition
    ) | Where-Object {
        $bounds = $_.Current.BoundingRectangle
        $pattern = $null
        $bounds.Width -gt 0 -and $bounds.Height -gt 0 -and
            $_.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern) -and
            ([Windows.Automation.SelectionItemPattern]$pattern).Current.IsSelected
    })
    if ($selected.Count -ne 1 -or $selected[0].Current.Name -notlike 'Beta.txt*') {
        throw "restored right click selected the wrong row set: $(@($selected | ForEach-Object Current | ForEach-Object Name) -join ', ')"
    }
    $selectedItemName = $selected[0].Current.Name

    Send-UitestKey -Key 0x1B
    Start-Sleep -Milliseconds 300
    if (@(Get-ProcessBoundPopups).Count -ne 0) { throw 'Escape did not dismiss the restored drive-root popup' }
    Stop-UitestExplorer -Context $context
    $context = $null

    $session = Get-Content -Raw -Encoding utf8 -LiteralPath $sessionPath | ConvertFrom-Json
    $activeTab = @($session.payload.tabs | Where-Object { $_.tab_id -eq $session.payload.active_tab_id })
    if ($activeTab.Count -ne 1 -or $activeTab[0].current.location.FileSystem -ne $driveRoot) {
        throw "restored drive root was not persisted canonically as $driveRoot"
    }

    [pscustomobject]@{
        schema = 'superexplorer.restored-drive-root-context-menu.v1'
        profile = $Profile
        restored_tabs = @($session.payload.tabs).Count
        source_dpi = $session.payload.window.source_dpi
        input_location = $driveDesignator
        persisted_location = $activeTab[0].current.location.FileSystem
        selected_item = $selectedItemName
        popup_hwnd = $popup.ToInt64()
        topmost_assistance = $false
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'result.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if ($mappingCreated) {
        & subst.exe $driveDesignator /D
    }
}
