param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = "",
    [ValidateSet("all", "ancestor-delete", "paste-into-selected-folder", "drag-between-folders", "ancestor-context-menu")]
    [string] $Case = "all",
    [switch] $Worker
)

# Runner-owned Columns command acceptance.
# Every filesystem change stays under this invocation's -OutputDirectory.
# Delete, paste, and drag each run in their own process and never open a
# context menu. The ancestor context-menu case runs last, also in its own
# process. A parent watchdog stops that process when its wall clock expires,
# because a whole-window UIA scan after a native menu can block forever.
# Menu names are read with SendMessageTimeout only. The popup is not scanned
# through UIA.
$ErrorActionPreference = "Stop"

$CaseOrder = @(
    "ancestor-delete",
    "paste-into-selected-folder",
    "drag-between-folders",
    "ancestor-context-menu"
)
$CaseWallClockSeconds = @{
    "ancestor-delete" = 120
    "paste-into-selected-folder" = 120
    "drag-between-folders" = 120
    "ancestor-context-menu" = 120
}
$ContextMenuLimitation = "After a native context menu, a whole-window UIA scan on this host can block indefinitely. This case does not query the popup or the main window through UIA once the menu is open. Item names come only from SendMessageTimeout(MN_GETHMENU, 800ms). Delete, paste, and drag never open a context menu, so a blocked menu provider cannot prevent those filesystem checks."

function Assert-RunnerDirectory([string] $Path, [string] $Label) {
    $full = [IO.Path]::GetFullPath($Path)
    if ([string]::IsNullOrWhiteSpace($full)) { throw "$Label is empty" }
    $rootPath = [IO.Path]::GetPathRoot($full)
    if ($full.TrimEnd("\") -eq $rootPath.TrimEnd("\")) {
        throw "refusing to use a drive root as ${Label}: $full"
    }
    $profileRoot = [Environment]::GetFolderPath("UserProfile")
    if (-not [string]::IsNullOrWhiteSpace($profileRoot) -and $full.TrimEnd("\") -eq $profileRoot.TrimEnd("\")) {
        throw "refusing to use the user profile as ${Label}"
    }
    return $full
}

function Format-ProcessArgument([string] $Value) {
    return '"' + ($Value -replace '"', '\"') + '"'
}

$OutputDirectory = Assert-RunnerDirectory $OutputDirectory "OutputDirectory"
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
if (-not $Executable) {
    $Executable = "D:\SuperExplorer\target\debug\SuperExplorer.exe"
}
$Executable = [IO.Path]::GetFullPath($Executable)

if (-not $Worker) {
    $requested = if ($Case -eq "all") { $CaseOrder } else { @($Case) }
    $aggregateSteps = New-Object System.Collections.Generic.List[object]
    $ps51 = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        foreach ($caseName in $requested) {
            [void]$aggregateSteps.Add([ordered]@{
                id = $caseName
                status = "SKIP"
                evidence = @(
                    "SuperExplorer.exe was not built, so this case was not started.",
                    "executable=$Executable",
                    "wallClockSec=$($CaseWallClockSeconds[$caseName])"
                )
            })
        }
        $skippedReport = [ordered]@{
            status = "SKIP"
            reason = "SuperExplorer.exe was not built, so no Columns command case was started."
            fixture = $OutputDirectory
            executable = $Executable
            context_menu_limitation = $ContextMenuLimitation
            steps = $aggregateSteps.ToArray()
        }
        $skippedReport | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory "report.json") -Encoding utf8
        exit 2
    }
    foreach ($caseName in $requested) {
        $caseDirectory = Assert-RunnerDirectory (Join-Path $OutputDirectory (Join-Path "cases" $caseName)) "case directory"
        if (-not $caseDirectory.StartsWith($OutputDirectory, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "case directory escaped OutputDirectory: $caseDirectory"
        }
        New-Item -ItemType Directory -Force -Path $caseDirectory | Out-Null
        $watchSeconds = [int]$CaseWallClockSeconds[$caseName]
        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = $ps51
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true
        $psi.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
        $psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File $(Format-ProcessArgument $PSCommandPath) -OutputDirectory $(Format-ProcessArgument $caseDirectory) -Executable $(Format-ProcessArgument $Executable) -Case $caseName -Worker"
        $started = [DateTime]::UtcNow
        $workerProcess = $null
        try {
            $workerProcess = [Diagnostics.Process]::Start($psi)
        } catch {
            [void]$aggregateSteps.Add([ordered]@{
                id = $caseName
                status = "FAIL"
                evidence = @("worker process did not start: $($_.Exception.Message)")
            })
            continue
        }
        $finished = $workerProcess.WaitForExit($watchSeconds * 1000)
        $elapsedMs = [int]([DateTime]::UtcNow - $started).TotalMilliseconds
        if (-not $finished) {
            # Only the worker started above, and the SuperExplorer process it owns.
            & "$env:SystemRoot\System32\taskkill.exe" /PID $workerProcess.Id /T /F 2>$null | Out-Null
            try { $workerProcess.WaitForExit(5000) | Out-Null } catch { }
            [void]$aggregateSteps.Add([ordered]@{
                id = $caseName
                status = "FAIL"
                evidence = @(
                    "case exceeded the ${watchSeconds}s wall clock and the owned worker was stopped",
                    "workerPid=$($workerProcess.Id)",
                    "elapsedMs=$elapsedMs",
                    "caseDirectory=$caseDirectory",
                    $ContextMenuLimitation
                )
            })
            continue
        }
        $caseReportPath = Join-Path $caseDirectory "report.json"
        if (-not (Test-Path -LiteralPath $caseReportPath)) {
            [void]$aggregateSteps.Add([ordered]@{
                id = $caseName
                status = "FAIL"
                evidence = @(
                    "worker exited $($workerProcess.ExitCode) without report.json",
                    "elapsedMs=$elapsedMs",
                    "caseDirectory=$caseDirectory"
                )
            })
            continue
        }
        $parsed = Get-Content -LiteralPath $caseReportPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $parsedSteps = @()
        if ($null -ne $parsed.steps) { $parsedSteps = @($parsed.steps) }
        if ($parsedSteps.Count -eq 0) {
            [void]$aggregateSteps.Add([ordered]@{
                id = $caseName
                status = "FAIL"
                evidence = @("worker report contained no steps", "elapsedMs=$elapsedMs", "caseDirectory=$caseDirectory")
            })
            continue
        }
        foreach ($parsedStep in $parsedSteps) {
            $evidence = New-Object System.Collections.Generic.List[string]
            if ($null -ne $parsedStep.evidence) {
                foreach ($line in @($parsedStep.evidence)) {
                    if ($null -ne $line -and "$line" -ne "") { [void]$evidence.Add([string]$line) }
                }
            }
            [void]$evidence.Add("elapsedMs=$elapsedMs")
            [void]$evidence.Add("wallClockSec=$watchSeconds")
            [void]$evidence.Add("caseDirectory=$caseDirectory")
            [void]$aggregateSteps.Add([ordered]@{
                id = [string]$parsedStep.id
                status = [string]$parsedStep.status
                evidence = $evidence.ToArray()
            })
        }
    }
    $failed = New-Object System.Collections.Generic.List[string]
    $skipped = New-Object System.Collections.Generic.List[string]
    foreach ($step in $aggregateSteps) {
        if ($step.status -eq "FAIL") { [void]$failed.Add($step.id) }
        elseif ($step.status -eq "SKIP") { [void]$skipped.Add($step.id) }
    }
    $overall = "PASS"
    $reason = "Requested Columns command cases finished inside their wall clocks: $($requested -join ', ')."
    $exitCode = 0
    if ($failed.Count -gt 0) {
        $overall = "FAIL"
        $reason = "failed: " + ($failed.ToArray() -join ", ")
        $exitCode = 1
    } elseif ($skipped.Count -gt 0) {
        $overall = "SKIP"
        $reason = "skipped: " + ($skipped.ToArray() -join ", ")
        $exitCode = 2
    }
    $aggregate = [ordered]@{
        status = $overall
        reason = $reason
        fixture = $OutputDirectory
        executable = $Executable
        context_menu_limitation = $ContextMenuLimitation
        steps = $aggregateSteps.ToArray()
    }
    $aggregate | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit $exitCode
}

if ($Case -eq "all") { throw "a worker must be started with one -Case" }

$fixture = Join-Path $OutputDirectory "fixture"
if (-not $fixture.StartsWith($OutputDirectory, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "fixture escaped OutputDirectory"
}
$opened = Join-Path $fixture "opened"
$pasteDest = Join-Path $fixture "paste-dest"
$dragSrc = Join-Path $fixture "drag-src"
$dragDst = Join-Path $fixture "drag-dst"
New-Item -ItemType Directory -Force -Path $opened, $pasteDest, $dragSrc, $dragDst | Out-Null
$fixtureFiles = @{
    "ancestor-file.txt" = "ancestor-token"
    "delete-me.txt" = "delete-token"
    "paste-me.txt" = "paste-token"
    "keep-root.txt" = "keep-root-token"
    "opened\decoy-selected.txt" = "decoy-token"
    "opened\keep-opened.txt" = "keep-opened-token"
    "paste-dest\dest-marker.txt" = "dest-marker-token"
    "drag-src\drag-me.txt" = "drag-token"
    "drag-src\not-dragged.txt" = "not-dragged-token"
    "drag-dst\dst-marker.txt" = "dst-marker-token"
}
foreach ($relative in $fixtureFiles.Keys) {
    $path = Join-Path $fixture $relative
    if (-not $path.StartsWith($fixture, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing to write outside the fixture: $path"
    }
    Set-Content -LiteralPath $path -Value $fixtureFiles[$relative] -Encoding utf8
}

$steps = New-Object System.Collections.Generic.List[object]
$script:OwnedProcess = $null
$script:Hwnd = [IntPtr]::Zero

function Save-Report([string] $Status, [string] $Reason) {
    $report = [ordered]@{
        status = $Status
        reason = $Reason
        fixture = $fixture
        executable = $Executable
        context_menu_limitation = $ContextMenuLimitation
        steps = $steps.ToArray()
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory "report.json") -Encoding utf8
}

function Add-Step([string] $Id, [string] $Status, [string[]] $Evidence) {
    [void]$steps.Add([ordered]@{
        id = $Id
        status = $Status
        evidence = $Evidence
    })
}

function Stop-OwnedExplorer {
    if ($script:OwnedProcess) {
        try {
            if (-not $script:OwnedProcess.HasExited) {
                Stop-Process -Id $script:OwnedProcess.Id -Force -ErrorAction SilentlyContinue
            }
        } catch {
        }
    }
}

function Get-FixtureRelativeFiles {
    $root = $fixture.TrimEnd("\")
    $names = New-Object System.Collections.Generic.List[string]
    Get-ChildItem -LiteralPath $root -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
        [void]$names.Add($_.FullName.Substring($root.Length).TrimStart("\"))
    }
    $sorted = @($names.ToArray() | Sort-Object)
    Write-Output -NoEnumerate $sorted
}

function Get-FixtureChanges($Before, $After) {
    $removed = New-Object System.Collections.Generic.List[string]
    $added = New-Object System.Collections.Generic.List[string]
    if ($null -eq $Before) { $Before = @() }
    if ($null -eq $After) { $After = @() }
    Compare-Object -ReferenceObject @($Before) -DifferenceObject @($After) | ForEach-Object {
        if ($_.SideIndicator -eq "<=") { [void]$removed.Add([string]$_.InputObject) }
        elseif ($_.SideIndicator -eq "=>") { [void]$added.Add([string]$_.InputObject) }
    }
    Write-Output -NoEnumerate @{ Removed = $removed.ToArray(); Added = $added.ToArray() }
}

if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    Add-Step $Case "SKIP" @("SuperExplorer.exe was not built, so this case was not started.", "executable=$Executable")
    Save-Report "SKIP" "SuperExplorer.exe was not built."
    exit 2
}

Import-Module (Join-Path $PSScriptRoot "UitestHeadful.psm1") -Force
Initialize-UitestHeadful
Add-Type -AssemblyName System.Drawing
if (-not ("ColumnCommandAcceptance.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace ColumnCommandAcceptance {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
        [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
        [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT point);
        [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
        [DllImport("user32.dll", SetLastError = true)] public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam, uint flags, uint timeoutMs, out IntPtr result);
        [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int count, uint flags);
        [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int key);
    }
}
"@
}

function Get-ProcessTreeIds([int] $RootId) {
    $ids = New-Object "System.Collections.Generic.HashSet[int]"
    [void]$ids.Add($RootId)
    for ($pass = 0; $pass -lt 4; $pass++) {
        $changed = $false
        foreach ($candidate in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)) {
            if ($ids.Contains([int]$candidate.ParentProcessId) -and $ids.Add([int]$candidate.ProcessId)) {
                $changed = $true
            }
        }
        if (-not $changed) { break }
    }
    Write-Output -NoEnumerate $ids
}

function Get-OwnedWindows([System.Collections.Generic.HashSet[int]] $ProcessIds, [string[]] $Classes) {
    $script:EnumClasses = $Classes
    $script:EnumProcessIds = $ProcessIds
    $script:EnumHandles = New-Object System.Collections.Generic.List[IntPtr]
    $callback = [RustExplorerUitest.Native+EnumWindowsProc]{
        param([IntPtr] $Hwnd, [IntPtr] $Unused)
        if ([RustExplorerUitest.Native]::IsWindowVisible($Hwnd)) {
            $className = [Text.StringBuilder]::new(128)
            [void][RustExplorerUitest.Native]::GetClassName($Hwnd, $className, $className.Capacity)
            [uint32] $owner = 0
            [void][RustExplorerUitest.Native]::GetWindowThreadProcessId($Hwnd, [ref]$owner)
            if ($script:EnumClasses -contains $className.ToString() -and $script:EnumProcessIds.Contains([int]$owner)) {
                [void]$script:EnumHandles.Add($Hwnd)
            }
        }
        return $true
    }
    [void][RustExplorerUitest.Native]::EnumWindows($callback, [IntPtr]::Zero)
    Write-Output -NoEnumerate $script:EnumHandles.ToArray()
}

function Get-WindowTitle([IntPtr] $Hwnd) {
    $text = [Text.StringBuilder]::new(512)
    [void][ColumnCommandAcceptance.Native]::GetWindowText($Hwnd, $text, $text.Capacity)
    return $text.ToString()
}

function Get-BlockingDialog([System.Collections.Generic.HashSet[int]] $ProcessIds) {
    $dialogs = Get-OwnedWindows $ProcessIds @("#32770")
    if ($dialogs.Length -eq 0) { return [IntPtr]::Zero }
    return $dialogs[0]
}

function Send-Escape {
    if ($script:Hwnd -ne [IntPtr]::Zero) {
        [void][RustExplorerUitest.Native]::SetForegroundWindow($script:Hwnd)
    }
    [RustExplorerUitest.Native]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
}

function Close-Popups([System.Collections.Generic.HashSet[int]] $ProcessIds) {
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        $popups = Get-OwnedWindows $ProcessIds @("#32768", "SuperExplorer.ImmersivePopup.v1")
        if ($popups.Length -eq 0) { return $true }
        Send-Escape
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

function Find-ColumnRow([Windows.Automation.AutomationElement] $Root, [string] $Token) {
    $window = $Root.Current.BoundingRectangle
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem)
    $pattern = "(^|\s)" + [regex]::Escape($Token) + "(\s|$)"
    $foundRows = New-Object System.Collections.Generic.List[object]
    foreach ($row in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)) {
        $bounds = $row.Current.BoundingRectangle
        if ($bounds.Width -le 1 -or $bounds.Height -le 1) { continue }
        if ($bounds.Left -lt ($window.Left + 180)) { continue }
        if ($bounds.Right -gt ($window.Right + 8) -or $bounds.Bottom -gt ($window.Bottom + 8)) { continue }
        if ($row.Current.Name -match $pattern) { [void]$foundRows.Add($row) }
    }
    Write-Output -NoEnumerate $foundRows.ToArray()
}

function Wait-ColumnRow([Windows.Automation.AutomationElement] $Root, [string] $Token, [int] $TimeoutMs) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        $rows = Find-ColumnRow $Root $Token
        if ($rows.Length -eq 1) { return $rows[0] }
        if ($rows.Length -gt 1) { throw "more than one column row matched ${Token}" }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Find-NamedElement([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Save-WindowShot([IntPtr] $Hwnd, [string] $Name) {
    try {
        if ($Hwnd -eq [IntPtr]::Zero) { return }
        $rect = [ColumnCommandAcceptance.Native+RECT]::new()
        if (-not [ColumnCommandAcceptance.Native]::GetWindowRect($Hwnd, [ref]$rect)) { return }
        $width = [Math]::Max(1, $rect.Right - $rect.Left)
        $height = [Math]::Max(1, $rect.Bottom - $rect.Top)
        $shot = New-Object System.Drawing.Bitmap $width, $height
        $graphics = [System.Drawing.Graphics]::FromImage($shot)
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size $width, $height))
        $shot.Save((Join-Path $OutputDirectory $Name))
        $graphics.Dispose()
        $shot.Dispose()
    } catch {
    }
}

function Start-ColumnsSession {
    $profileDir = Join-Path $OutputDirectory "localappdata"
    New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
    $start = [Diagnostics.ProcessStartInfo]::new($Executable)
    $start.WorkingDirectory = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
    $start.UseShellExecute = $false
    $start.Environment["LOCALAPPDATA"] = $profileDir
    $start.Environment["EXPLORER_INITIAL_PATH"] = $fixture
    $start.Environment["EXPLORER_LOG_DIR"] = $OutputDirectory
    $start.Environment["SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION"] = "1"
    $start.Environment["SUPEREXPLORER_LOCALE"] = "zh-TW"
    $script:OwnedProcess = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if ($script:OwnedProcess.HasExited) { throw "application exited before showing a window: $($script:OwnedProcess.ExitCode)" }
        $script:OwnedProcess.Refresh()
        if ($script:OwnedProcess.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($script:OwnedProcess.MainWindowHandle -eq [IntPtr]::Zero) { throw "application window did not appear within 25s" }
    $script:Hwnd = $script:OwnedProcess.MainWindowHandle
    # SWP_ASYNCWINDOWPOS | SWP_SHOWWINDOW. Do not wait on the window procedure.
    [void][ColumnCommandAcceptance.Native]::SetWindowPos($script:Hwnd, [IntPtr]::Zero, 40, 40, 1280, 800, 0x4040)
    Start-Sleep -Milliseconds 200
    [void][RustExplorerUitest.Native]::SetForegroundWindow($script:Hwnd)
    Start-Sleep -Milliseconds 700
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $ancestor = Wait-ColumnRow $root "ancestor-file.txt" 8000
    if ($null -eq $ancestor) { throw "initial address did not show the runner fixture ancestor-file.txt" }
    $view = $null
    foreach ($name in @(([string]::new(@([char]0x6AA2, [char]0x8996))), "View")) {
        $view = Find-NamedElement $root $name
        if ($null -ne $view) { break }
    }
    if ($null -eq $view) { throw "View control missing" }
    Invoke-UitestClick -Element $view
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $columns = $null
    $menuDeadline = [DateTime]::UtcNow.AddSeconds(6)
    do {
        foreach ($name in @(([string]::new(@([char]0x5206, [char]0x6B04))), "Columns")) {
            $columns = Find-NamedElement $root $name
            if ($null -ne $columns) { break }
        }
        if ($null -ne $columns) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $menuDeadline)
    if ($null -eq $columns) {
        Save-WindowShot $script:Hwnd "columns-menu-missing.png"
        throw "Columns menu item missing"
    }
    Invoke-UitestClick -Element $columns
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $surface = $null
    $surfaceDeadline = [DateTime]::UtcNow.AddSeconds(6)
    do {
        foreach ($name in @(([string]::new(@([char]0x5206, [char]0x6B04, [char]0x6AA2, [char]0x8996))), "Column view")) {
            $surface = Find-NamedElement $root $name
            if ($null -ne $surface) { break }
        }
        if ($null -ne $surface) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $surfaceDeadline)
    if ($null -eq $surface) { throw "Columns surface missing" }
    $openedRow = Wait-ColumnRow $root "opened" 6000
    if ($null -eq $openedRow) { throw "opened folder row missing in Columns" }
    Invoke-UitestClick -Element $openedRow
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $decoy = Wait-ColumnRow $root "decoy-selected.txt" 8000
    $ancestor = Wait-ColumnRow $root "ancestor-file.txt" 2000
    if ($null -eq $decoy -or $null -eq $ancestor) { throw "opening the child folder did not keep the ancestor file and reveal decoy-selected.txt" }
    $ancestorBounds = $ancestor.Current.BoundingRectangle
    $decoyBounds = $decoy.Current.BoundingRectangle
    if ($ancestorBounds.Left -ge ($decoyBounds.Left - 20)) {
        throw "ancestor row is not left of the child row; Columns branch was not shown"
    }
    Write-Output -NoEnumerate @{
        Root = $root
        Surface = $surface.Current.Name
        Ancestor = $ancestor
        Decoy = $decoy
        AncestorBounds = $ancestorBounds
        DecoyBounds = $decoyBounds
        ProcessIds = (Get-ProcessTreeIds $script:OwnedProcess.Id)
    }
}

function Test-MenuVerb([string[]] $Labels, [string[]] $Verbs) {
    foreach ($label in $Labels) {
        foreach ($verb in $Verbs) {
            if ($label -match ("(^|\s|&)" + [regex]::Escape($verb) + "(\s|\(|$)")) { return $true }
        }
    }
    return $false
}

function Get-MenuLabelsBounded([IntPtr] $Popup) {
    $labels = New-Object System.Collections.Generic.List[string]
    $menu = [IntPtr]::Zero
    # SMTO_ABORTIFHUNG. Do not call UIA or unbounded SendMessage on the popup.
    $returned = [ColumnCommandAcceptance.Native]::SendMessageTimeout(
        $Popup, 0x01E1, [IntPtr]::Zero, [IntPtr]::Zero, 0x0002, 800, [ref]$menu)
    if ($returned -eq [IntPtr]::Zero -or $menu -eq [IntPtr]::Zero) {
        Write-Output -NoEnumerate $labels.ToArray()
        return
    }
    $count = [ColumnCommandAcceptance.Native]::GetMenuItemCount($menu)
    if ($count -lt 0 -or $count -gt 80) {
        Write-Output -NoEnumerate $labels.ToArray()
        return
    }
    for ($index = 0; $index -lt $count; $index++) {
        $text = [Text.StringBuilder]::new(512)
        [void][ColumnCommandAcceptance.Native]::GetMenuString($menu, [uint32]$index, $text, $text.Capacity, 0x0400)
        $label = $text.ToString().Trim()
        if ($label) { [void]$labels.Add($label) }
    }
    Write-Output -NoEnumerate $labels.ToArray()
}

function Invoke-AncestorDelete {
    $session = Start-ColumnsSession
    $before = Get-FixtureRelativeFiles
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $deleteRow = Wait-ColumnRow $root "delete-me.txt" 4000
    if ($null -eq $deleteRow) { throw "delete-me.txt was not visible in the ancestor column" }
    $deleteName = $deleteRow.Current.Name
    $deleteLeft = $deleteRow.Current.BoundingRectangle.Left
    Invoke-UitestClick -Element $deleteRow
    [void][RustExplorerUitest.Native]::SetForegroundWindow($script:Hwnd)
    Send-UitestKey -Key 0x2E
    $dialogReason = $null
    $gone = $false
    $rowGone = $false
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $dialog = Get-BlockingDialog $session.ProcessIds
        if ($dialog -ne [IntPtr]::Zero) {
            $dialogReason = "external dialog class=#32770 title=$(Get-WindowTitle $dialog)"
            Send-Escape
            if (Test-Path -LiteralPath (Join-Path $fixture "delete-me.txt")) {
                throw "SKIP $dialogReason; Delete was not confirmed"
            }
        }
        $gone = -not (Test-Path -LiteralPath (Join-Path $fixture "delete-me.txt"))
        $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
        $rowGone = $null -eq (Wait-ColumnRow $root "delete-me.txt" 400)
        if ($gone -and $rowGone) { break }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    $changes = Get-FixtureChanges $before (Get-FixtureRelativeFiles)
    $evidence = @(
        "clicked=$deleteName left=$deleteLeft",
        "surface=$($session.Surface)",
        "ancestorLeft=$($session.AncestorBounds.Left) decoyLeft=$($session.DecoyBounds.Left)",
        "removed=$($changes.Removed -join ',')",
        "added=$($changes.Added -join ',')",
        "decoyStillOnDisk=$(Test-Path -LiteralPath (Join-Path $opened 'decoy-selected.txt'))",
        "selection=left-click, no context menu"
    )
    if (-not $gone -or -not $rowGone) { throw "Delete did not remove delete-me.txt from the fixture and its column. $($evidence -join '; ')" }
    if ($changes.Removed.Length -ne 1 -or $changes.Removed[0] -ne "delete-me.txt") {
        throw "Delete removed unexpected fixture files: $($changes.Removed -join ',')"
    }
    if ($changes.Added.Length -ne 0) { throw "Delete added fixture files: $($changes.Added -join ',')" }
    if (-not (Test-Path -LiteralPath (Join-Path $opened "decoy-selected.txt"))) { throw "Delete removed the child-column file decoy-selected.txt" }
    if (-not (Test-Path -LiteralPath (Join-Path $fixture "paste-me.txt"))) { throw "Delete removed paste-me.txt" }
    if (-not (Test-Path -LiteralPath (Join-Path $fixture "keep-root.txt"))) { throw "Delete removed keep-root.txt" }
    Add-Step "ancestor-delete" "PASS" $evidence
}

function Invoke-PasteIntoSelectedFolder {
    $session = Start-ColumnsSession
    $before = Get-FixtureRelativeFiles
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $source = Wait-ColumnRow $root "paste-me.txt" 4000
    $folder = Wait-ColumnRow $root "paste-dest" 4000
    if ($null -eq $source -or $null -eq $folder) { throw "paste source or paste-dest folder row missing" }
    $sourceLeft = $source.Current.BoundingRectangle.Left
    $folderBounds = $folder.Current.BoundingRectangle
    if ($sourceLeft -ge ($folderBounds.Left + $folderBounds.Width)) {
        throw "paste-me.txt is not in the ancestor column beside paste-dest"
    }
    $sourceName = $source.Current.Name
    Invoke-UitestClick -Element $source
    [void][RustExplorerUitest.Native]::SetForegroundWindow($script:Hwnd)
    Send-UitestKey -Key 0x43 -Modifiers @(0x11)
    Start-Sleep -Milliseconds 400
    # Selecting the source may horizontally reveal another column, so acquire
    # the destination row again before navigating. The outer watchdog bounds UIA.
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $folder = Wait-ColumnRow $root "paste-dest" 4000
    if ($null -eq $folder) { throw "paste-dest disappeared after copying source" }
    Invoke-UitestClick -Element $folder
    Start-Sleep -Milliseconds 1200
    Send-UitestKey -Key 0x56 -Modifiers @(0x11)
    $copied = Join-Path $pasteDest "paste-me.txt"
    $dialogReason = $null
    $landed = $false
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $dialog = Get-BlockingDialog $session.ProcessIds
        if ($dialog -ne [IntPtr]::Zero -and $null -eq $dialogReason) {
            $dialogReason = "external dialog class=#32770 title=$(Get-WindowTitle $dialog)"
            Send-Escape
        }
        if ((Test-Path -LiteralPath $copied) -and (Get-Content -LiteralPath $copied -Raw) -match "paste-token") {
            $landed = $true
            break
        }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    $changes = Get-FixtureChanges $before (Get-FixtureRelativeFiles)
    $evidence = @(
        "sourceRow=$sourceName left=$sourceLeft",
        "destinationRow=paste-dest left=$($folderBounds.Left)",
        "surface=$($session.Surface)",
        "added=$($changes.Added -join ',')",
        "removed=$($changes.Removed -join ',')",
        "selection=left-click paste-dest and navigate into it; no context menu"
    )
    if ($dialogReason -and -not $landed) { throw "SKIP $dialogReason; paste was not confirmed" }
    if (-not $landed) { throw "Ctrl+V did not copy paste-me.txt into the selected paste-dest folder" }
    if ($changes.Added.Length -ne 1 -or $changes.Added[0] -ne "paste-dest\paste-me.txt") {
        throw "paste changed unexpected fixture paths. added=$($changes.Added -join ',') removed=$($changes.Removed -join ',')"
    }
    if ($changes.Removed.Length -ne 0) { throw "paste removed fixture files: $($changes.Removed -join ',')" }
    if (Test-Path -LiteralPath (Join-Path $opened "paste-me.txt")) { throw "paste landed in the opened child folder" }
    if (Test-Path -LiteralPath (Join-Path $fixture "paste-me (2).txt")) { throw "paste created a duplicate in the fixture root" }
    if (Test-Path -LiteralPath (Join-Path $OutputDirectory "paste-me.txt")) { throw "paste landed outside the fixture in the case directory" }
    if (-not (Test-Path -LiteralPath (Join-Path $fixture "paste-me.txt"))) { throw "paste removed the source paste-me.txt" }
    Add-Step "paste-into-selected-folder" "PASS" $evidence
}

function Invoke-ColumnDrag(
    [Windows.Automation.AutomationElement] $Source,
    [Windows.Automation.AutomationElement] $Target,
    [IntPtr] $Hwnd
) {
    $start = Get-UitestPhysicalPoint -Element $Source -HorizontalOffset 80
    $targetPoint = Get-UitestPhysicalPoint -Element $Target -HorizontalOffset 70
    $point = [ColumnCommandAcceptance.Native+POINT]::new()
    $point.X = $start.X
    $point.Y = $start.Y
    $hit = [ColumnCommandAcceptance.Native]::WindowFromPoint($point)
    $owner = [ColumnCommandAcceptance.Native]::GetAncestor($hit, 2)
    if ($owner -ne $Hwnd) {
        throw "SKIP drag source is covered at $($start.X),$($start.Y); hit=$hit root=$owner expected=$Hwnd"
    }
    [void][RustExplorerUitest.Native]::SetForegroundWindow($Hwnd)
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware($start.X, $start.Y)) {
        throw "SKIP DPI-aware cursor positioning failed at the drag source"
    }
    Start-Sleep -Milliseconds 80
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    # SetCursorPos does not wait for the window procedure, so crossing into modal
    # DoDragDrop cannot deadlock this runner.
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($start.X + 28, $start.Y)
    Start-Sleep -Milliseconds 450
    for ($step = 1; $step -le 16; $step++) {
        $x = [int][Math]::Round($start.X + (($targetPoint.X - $start.X) * $step / 16))
        $y = [int][Math]::Round($start.Y + (($targetPoint.Y - $start.Y) * $step / 16))
        [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($x, $y)
        Start-Sleep -Milliseconds 30
    }
    $point.X = $targetPoint.X
    $point.Y = $targetPoint.Y
    $targetHit = [ColumnCommandAcceptance.Native]::WindowFromPoint($point)
    $targetRoot = [ColumnCommandAcceptance.Native]::GetAncestor($targetHit, 2)
    if ($targetRoot -ne $Hwnd) {
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        throw "SKIP drag destination is covered at $($targetPoint.X),$($targetPoint.Y); hit=$targetHit root=$targetRoot expected=$Hwnd"
    }
    Start-Sleep -Milliseconds 180
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    # Wake the OLE modal drag loop after button release, as in the existing
    # smoke_explorer_drag_interop driver.
    [RustExplorerUitest.Native]::mouse_event(0x0001, 1, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    $releaseDeadline = [DateTime]::UtcNow.AddSeconds(2)
    do {
        if (([ColumnCommandAcceptance.Native]::GetAsyncKeyState(0x01) -band 0x8000) -eq 0) { break }
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $releaseDeadline)
    return "source=$($start.X),$($start.Y) target=$($targetPoint.X),$($targetPoint.Y) targetName=$($Target.Current.Name)"
}

function Invoke-DragBetweenFolders {
    $session = Start-ColumnsSession
    $before = Get-FixtureRelativeFiles
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $dragSrcRow = Wait-ColumnRow $root "drag-src" 4000
    if ($null -eq $dragSrcRow) { throw "drag-src folder row missing" }
    Invoke-UitestClick -Element $dragSrcRow
    $root = [Windows.Automation.AutomationElement]::FromHandle($script:Hwnd)
    $dragFile = Wait-ColumnRow $root "drag-me.txt" 8000
    $dragDstRow = Wait-ColumnRow $root "drag-dst" 4000
    $notDragged = Wait-ColumnRow $root "not-dragged.txt" 2000
    if ($null -eq $dragFile -or $null -eq $dragDstRow) { throw "drag-me.txt or drag-dst was not visible" }
    $fileBounds = $dragFile.Current.BoundingRectangle
    $dstBounds = $dragDstRow.Current.BoundingRectangle
    if ($fileBounds.Left -le ($dstBounds.Left + 20)) {
        throw "drag-me.txt is not in a column to the right of drag-dst"
    }
    $sourceName = $dragFile.Current.Name
    $targetName = $dragDstRow.Current.Name
    $notDraggedName = if ($null -ne $notDragged) { $notDragged.Current.Name } else { "" }
    $dragTrace = Invoke-ColumnDrag -Source $dragFile -Target $dragDstRow -Hwnd $script:Hwnd
    $movedPath = Join-Path $dragDst "drag-me.txt"
    $sourcePath = Join-Path $dragSrc "drag-me.txt"
    $dialogReason = $null
    $moved = $false
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $dialog = Get-BlockingDialog $session.ProcessIds
        if ($dialog -ne [IntPtr]::Zero -and $null -eq $dialogReason) {
            $dialogReason = "external dialog class=#32770 title=$(Get-WindowTitle $dialog)"
            Send-Escape
        }
        $moved = (Test-Path -LiteralPath $movedPath) -and -not (Test-Path -LiteralPath $sourcePath) -and ((Get-Content -LiteralPath $movedPath -Raw) -match "drag-token")
        if ($moved) { break }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    $changes = Get-FixtureChanges $before (Get-FixtureRelativeFiles)
    $evidence = @(
        $dragTrace,
        "sourceRow=$sourceName left=$($fileBounds.Left)",
        "destinationRow=$targetName left=$($dstBounds.Left)",
        "notDraggedBeforeDrop=$notDraggedName",
        "added=$($changes.Added -join ',')",
        "removed=$($changes.Removed -join ',')",
        "surface=$($session.Surface)"
    )
    if ($dialogReason -and -not $moved) { throw "SKIP $dialogReason; drag choice was not confirmed" }
    if (-not $moved) { throw "drag did not move drag-me.txt from drag-src into drag-dst. $($evidence -join '; ')" }
    if ($changes.Added.Length -ne 1 -or $changes.Added[0] -ne "drag-dst\drag-me.txt") {
        throw "drag added unexpected files: $($changes.Added -join ',')"
    }
    if ($changes.Removed.Length -ne 1 -or $changes.Removed[0] -ne "drag-src\drag-me.txt") {
        throw "drag removed unexpected files: $($changes.Removed -join ',')"
    }
    if (Test-Path -LiteralPath (Join-Path $fixture "drag-me.txt")) { throw "drag moved the file to the fixture root instead of drag-dst" }
    if (-not (Test-Path -LiteralPath (Join-Path $dragSrc "not-dragged.txt"))) { throw "drag moved not-dragged.txt" }
    if (Test-Path -LiteralPath (Join-Path $opened "drag-me.txt")) { throw "drag moved the file into the opened child folder" }
    Add-Step "drag-between-folders" "PASS" $evidence
}

function Invoke-AncestorContextMenu {
    $session = Start-ColumnsSession
    $before = Get-FixtureRelativeFiles
    $ancestor = $session.Ancestor
    $decoy = $session.Decoy
    $ancestorBounds = $session.AncestorBounds
    $decoyBounds = $session.DecoyBounds
    $clickedName = $ancestor.Current.Name
    # Left-click the child first so a wrong menu target would be that file.
    # This UIA interaction finishes before the native menu opens.
    Invoke-UitestClick -Element $decoy
    Invoke-UitestClick -Element $ancestor -Right
    $popup = [IntPtr]::Zero
    $popupDeadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $dialog = Get-BlockingDialog $session.ProcessIds
        if ($dialog -ne [IntPtr]::Zero) {
            $title = Get-WindowTitle $dialog
            Send-Escape
            throw "SKIP external dialog appeared instead of the column menu: class=#32770 title=$title"
        }
        $found = Get-OwnedWindows $session.ProcessIds @("#32768", "SuperExplorer.ImmersivePopup.v1")
        if ($found.Length -gt 0) { $popup = $found[0]; break }
        $foreground = [RustExplorerUitest.Native]::GetForegroundWindow()
        if ($foreground -ne [IntPtr]::Zero) {
            $popupClass = [Text.StringBuilder]::new(128)
            [void][RustExplorerUitest.Native]::GetClassName($foreground, $popupClass, $popupClass.Capacity)
            if (@("#32768", "SuperExplorer.ImmersivePopup.v1") -contains $popupClass.ToString()) {
                $popup = $foreground
                break
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $popupDeadline)
    if ($popup -eq [IntPtr]::Zero) {
        Save-WindowShot $script:Hwnd "ancestor-menu-missing.png"
        $foreground = [RustExplorerUitest.Native]::GetForegroundWindow()
        if ($foreground -ne $script:Hwnd) {
            throw "SKIP context menu did not appear and the runner window was not foreground: foreground=$foreground expected=$($script:Hwnd)"
        }
        throw "right-click on ancestor-file.txt did not open a menu within 8s"
    }
    $popupRect = [ColumnCommandAcceptance.Native+RECT]::new()
    $hasPopupRect = [ColumnCommandAcceptance.Native]::GetWindowRect($popup, [ref]$popupRect)
    $labels = Get-MenuLabelsBounded $popup
    $joined = ($labels -join " | ")
    $hasCut = Test-MenuVerb $labels @("Cut", "剪下", "剪切")
    $hasDelete = Test-MenuVerb $labels @("Delete", "刪除", "删除")
    $hasRename = Test-MenuVerb $labels @("Rename", "重新命名", "重命名")
    Send-Escape
    Start-Sleep -Milliseconds 200
    $closed = -not [RustExplorerUitest.Native]::IsWindowVisible($popup)
    # Do not touch UIA after this point. The main-window provider can block.
    $changes = Get-FixtureChanges $before (Get-FixtureRelativeFiles)
    $ancestorCenterX = $ancestorBounds.Left + ($ancestorBounds.Width / 2)
    $decoyCenterX = $decoyBounds.Left + ($decoyBounds.Width / 2)
    $popupCenterX = if ($hasPopupRect) { ($popupRect.Left + $popupRect.Right) / 2 } else { 0 }
    $evidence = @(
        "menu=$joined",
        "clicked=$clickedName",
        "ancestorLeft=$($ancestorBounds.Left) decoyLeft=$($decoyBounds.Left) popupCenterX=$popupCenterX",
        "popupClosed=$closed",
        "surface=$($session.Surface)",
        $ContextMenuLimitation
    )
    if ($joined -match "decoy-selected\.txt") { throw "menu named decoy-selected.txt instead of the ancestor file: $joined" }
    if ($labels.Length -eq 0) {
        throw "native menu did not return item names within 800ms. $ContextMenuLimitation"
    }
    if (-not $hasCut -or -not $hasDelete -or -not $hasRename) {
        throw "ancestor file menu did not include cut, delete, and rename. labels=$joined"
    }
    if (-not $hasPopupRect) { throw "context menu popup did not expose a window rectangle" }
    if ([Math]::Abs($popupCenterX - $ancestorCenterX) -ge [Math]::Abs($popupCenterX - $decoyCenterX)) {
        throw "popup center $popupCenterX is closer to the child column ($decoyCenterX) than to ancestor-file.txt ($ancestorCenterX)"
    }
    if ($changes.Removed.Length -ne 0 -or $changes.Added.Length -ne 0) {
        throw "right-click changed fixture files. removed=$($changes.Removed -join ',') added=$($changes.Added -join ',')"
    }
    if (-not $closed) { throw "context menu was still open after Escape" }
    Add-Step "ancestor-context-menu" "PASS" $evidence
}

$caseStatus = "FAIL"
$caseReason = "case did not report a result"
try {
    switch ($Case) {
        "ancestor-delete" { Invoke-AncestorDelete }
        "paste-into-selected-folder" { Invoke-PasteIntoSelectedFolder }
        "drag-between-folders" { Invoke-DragBetweenFolders }
        "ancestor-context-menu" { Invoke-AncestorContextMenu }
        default { throw "unknown case $Case" }
    }
    $caseStatus = "PASS"
    $caseReason = "$Case passed"
    $exitCode = 0
} catch {
    $message = $_.Exception.Message
    Save-WindowShot $script:Hwnd "$Case-failed.png"
    if ($Case -eq "ancestor-context-menu" -and $script:OwnedProcess) {
        Close-Popups (Get-ProcessTreeIds $script:OwnedProcess.Id) | Out-Null
    } elseif ($script:Hwnd -ne [IntPtr]::Zero) {
        Send-Escape
    }
    if ($message -like "SKIP *") {
        Add-Step $Case "SKIP" @($message.Substring(5))
        $caseStatus = "SKIP"
        $caseReason = $message.Substring(5)
        $exitCode = 2
    } else {
        Add-Step $Case "FAIL" @($message)
        $caseStatus = "FAIL"
        $caseReason = $message
        $exitCode = 1
    }
} finally {
    Stop-OwnedExplorer
    if ($steps.Count -eq 0) {
        Add-Step $Case "FAIL" @("case ended without a step result")
        $caseStatus = "FAIL"
        $caseReason = "case ended without a step result"
        $exitCode = 1
    }
    Save-Report $caseStatus $caseReason
}
exit $exitCode
