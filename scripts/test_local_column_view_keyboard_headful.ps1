param(
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $Executable = "",
    [ValidateSet("all", "view-menu-keyboard", "arrow-address", "shift-ctrl-selection", "accessible-names", "focus-return")]
    [string] $Case = "all",
    [switch] $Worker
)

# Keyboard and UIA acceptance for the local Columns view.
# Files are created only under this invocation's -OutputDirectory.
# Each subcase runs in its own process. The parent kills that process when
# its wall clock expires, because a UIA tree walk can block. This script does
# not open a native menu. The View menu is the in-process GPUI menu.
# Checked state is asserted only when that menu button exposes TogglePattern.
$ErrorActionPreference = "Stop"

$CaseOrder = @(
    "view-menu-keyboard",
    "arrow-address",
    "shift-ctrl-selection",
    "accessible-names",
    "focus-return"
)
$CaseWallClockSeconds = @{
    "view-menu-keyboard" = 110
    "arrow-address" = 110
    "shift-ctrl-selection" = 110
    "accessible-names" = 110
    "focus-return" = 110
}

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
            reason = "SuperExplorer.exe was not built, so no Columns keyboard case was started."
            fixture = $OutputDirectory
            executable = $Executable
            steps = $aggregateSteps.ToArray()
        }
        $skippedReport | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory "report.json") -Encoding utf8
        exit 0
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
            & "$env:SystemRoot\System32\taskkill.exe" /PID $workerProcess.Id /T /F 2>$null | Out-Null
            try { $workerProcess.WaitForExit(5000) | Out-Null } catch { }
            [void]$aggregateSteps.Add([ordered]@{
                id = $caseName
                status = "FAIL"
                evidence = @(
                    "case exceeded the ${watchSeconds}s wall clock and the owned worker was stopped",
                    "workerPid=$($workerProcess.Id)",
                    "elapsedMs=$elapsedMs",
                    "caseDirectory=$caseDirectory"
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
    $passed = New-Object System.Collections.Generic.List[string]
    $skipped = New-Object System.Collections.Generic.List[string]
    foreach ($step in $aggregateSteps) {
        if ($step.status -eq "FAIL") { [void]$failed.Add($step.id) }
        elseif ($step.status -eq "PASS") { [void]$passed.Add($step.id) }
        elseif ($step.status -eq "SKIP") { [void]$skipped.Add($step.id) }
    }
    $overall = "PASS"
    $reason = "Columns keyboard cases passed: $($passed.ToArray() -join ', ')."
    $exitCode = 0
    if ($failed.Count -gt 0) {
        $overall = "FAIL"
        $reason = "failed: " + ($failed.ToArray() -join ", ")
        $exitCode = 1
    } elseif ($passed.Count -eq 0) {
        $overall = "SKIP"
        $reason = "skipped: " + ($skipped.ToArray() -join ", ")
        $exitCode = 0
    } elseif ($skipped.Count -gt 0) {
        $reason = "passed: $($passed.ToArray() -join ', '); skipped: $($skipped.ToArray() -join ', ')"
    }
    $aggregate = [ordered]@{
        status = $overall
        reason = $reason
        fixture = $OutputDirectory
        executable = $Executable
        steps = $aggregateSteps.ToArray()
    }
    $aggregate | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory "report.json") -Encoding utf8
    exit $exitCode
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ("ColumnKeyboardSmoke.Native" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace ColumnKeyboardSmoke {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    }
}
"@
}

$script:App = $null
$script:Shot = Join-Path $OutputDirectory "window.png"
$script:Deadline = [DateTime]::UtcNow.AddSeconds(90)
$script:Fixture = Join-Path $OutputDirectory "fixture"
$script:Nest = Join-Path $script:Fixture "nest"
$script:ColumnListCache = @{}

function Assert-Budget {
    if ([DateTime]::UtcNow -gt $script:Deadline) {
        throw "worker exceeded its 90s budget before the process watchdog"
    }
}

function Stop-App {
    if ($script:App -and -not $script:App.HasExited) {
        Stop-Process -Id $script:App.Id -Force -ErrorAction SilentlyContinue
        try { $script:App.WaitForExit(3000) | Out-Null } catch { }
    }
}

function Save-Shot {
    if ($null -eq $script:App -or $script:App.HasExited -or $script:App.MainWindowHandle -eq [IntPtr]::Zero) {
        return
    }
    try {
        $root = [Windows.Automation.AutomationElement]::FromHandle($script:App.MainWindowHandle)
        $bounds = $root.Current.BoundingRectangle
        if ($bounds.Width -lt 20 -or $bounds.Height -lt 20) { return }
        $bitmap = [Drawing.Bitmap]::new([int]$bounds.Width, [int]$bounds.Height)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen([int]$bounds.Left, [int]$bounds.Top, 0, 0, $bitmap.Size)
            $bitmap.Save($script:Shot, [Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $graphics.Dispose()
            $bitmap.Dispose()
        }
    } catch { }
}

function Get-Root {
    Assert-Budget
    if ($null -eq $script:App -or $script:App.HasExited) { throw "SuperExplorer exited" }
    $script:App.Refresh()
    if ($script:App.MainWindowHandle -eq [IntPtr]::Zero) { throw "SuperExplorer has no window" }
    return [Windows.Automation.AutomationElement]::FromHandle($script:App.MainWindowHandle)
}

function Send-Key([byte] $Key, [byte] $Modifier = 0) {
    Assert-Budget
    if ($null -ne $script:App -and -not $script:App.HasExited) {
        [void][ColumnKeyboardSmoke.Native]::SetForegroundWindow($script:App.MainWindowHandle)
    }
    if ($Modifier -ne 0) {
        $keyName = switch ($Key) {
            0x26 { "UP" }
            0x28 { "DOWN" }
            0x25 { "LEFT" }
            0x27 { "RIGHT" }
            0x20 { "SPACE" }
            0x09 { "TAB" }
            default { throw "unsupported modified key $Key" }
        }
        $prefix = if ($Modifier -eq 0x10) { "+" } elseif ($Modifier -eq 0x11) { "^" } else { throw "unsupported modifier $Modifier" }
        $chord = if ($keyName -eq "SPACE") { "$prefix " } else { "$prefix{$keyName}" }
        [System.Windows.Forms.SendKeys]::SendWait($chord)
        Start-Sleep -Milliseconds 160
        return
    }
    [ColumnKeyboardSmoke.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [ColumnKeyboardSmoke.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 160
}

function Find-ByName([Windows.Automation.AutomationElement] $Root, [string] $Name) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    return $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Wait-ByName([string] $Name, [int] $TimeoutMs = 4000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        Assert-Budget
        $match = Find-ByName (Get-Root) $Name
        if ($null -ne $match) { return $match }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Find-Control([string] $Name, [Windows.Automation.ControlType] $Type) {
    $nameCondition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    $typeCondition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, $Type)
    $both = New-Object Windows.Automation.AndCondition($nameCondition, $typeCondition)
    return (Get-Root).FindFirst([Windows.Automation.TreeScope]::Descendants, $both)
}

function Wait-Control([string] $Name, [Windows.Automation.ControlType] $Type, [int] $TimeoutMs = 4000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        Assert-Budget
        $match = Find-Control $Name $Type
        if ($null -ne $match) { return $match }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Get-ListItems([Windows.Automation.AutomationElement] $Root) {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::ListItem)
    return @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition))
}

function Split-RowLeaf([string] $Name) {
    foreach ($prefix in @("項目 ", "Item ")) {
        if ($Name.StartsWith($prefix)) { return $Name.Substring($prefix.Length) }
    }
    return $null
}

function Test-ItemSelected([Windows.Automation.AutomationElement] $Item) {
    $pattern = $null
    $supported = $false
    try {
        $supported = $Item.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)
    } catch {
        return $null
    }
    if (-not $supported -or $null -eq $pattern) { return $null }
    $selected = [bool]([Windows.Automation.SelectionItemPattern]$pattern).Current.IsSelected
    return $selected
}

function Get-ColumnRows([string] $Title) {
    Assert-Budget
    $list = $script:ColumnListCache[$Title]
    if ($null -eq $list) {
        $lists = (Get-Root).FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::List)))
        foreach ($candidate in $lists) {
            $name = [string]$candidate.Current.Name
            if ($name.Contains($Title)) {
                $list = $candidate
                $script:ColumnListCache[$Title] = $candidate
                break
            }
        }
    }
    if ($null -eq $list) { return @() }
    $rows = New-Object System.Collections.Generic.List[object]
    try {
        $items = @(Get-ListItems $list)
    } catch {
        $script:ColumnListCache.Remove($Title)
        return @()
    }
    foreach ($item in $items) {
        $leaf = Split-RowLeaf ([string]$item.Current.Name)
        if ($null -eq $leaf) { continue }
        $selected = Test-ItemSelected $item
        [void]$rows.Add([pscustomobject]@{
            Leaf = $leaf
            Name = [string]$item.Current.Name
            Role = [string]$item.Current.ControlType.ProgrammaticName
            Selected = $selected
        })
    }
    return @($rows.ToArray())
}

function Get-AddressText {
    $condition = New-Object Windows.Automation.PropertyCondition(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Document)
    foreach ($document in @((Get-Root).FindAll([Windows.Automation.TreeScope]::Descendants, $condition))) {
        $name = [string]$document.Current.Name
        if ($name.StartsWith("Address:")) {
            return ($name.Substring(8).Trim() -replace '[\u2066-\u2069]', '')
        }
    }
    return $null
}

function Wait-AddressEnding([string] $Suffix, [int] $TimeoutMs = 8000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        Assert-Budget
        $address = Get-AddressText
        if ($address -and $address.TrimEnd("\").EndsWith($Suffix, [StringComparison]::OrdinalIgnoreCase)) {
            return $address
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Wait-SelectedLeaves([string] $Title, [string[]] $Present, [string[]] $Absent, [int] $TimeoutMs = 2500) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    $selected = @()
    do {
        Assert-Budget
        $selected = @((Get-ColumnRows $Title) | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf })
        $ready = $true
        foreach ($leaf in @($Present)) {
            if ($selected -notcontains $leaf) { $ready = $false }
        }
        foreach ($leaf in @($Absent)) {
            if ($selected -contains $leaf) { $ready = $false }
        }
        if ($ready) { return ,$selected }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Wait-ColumnRows([string] $Title, [string[]] $Required, [int] $TimeoutMs = 8000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    do {
        Assert-Budget
        $rows = @(Get-ColumnRows $Title)
        $leaves = @($rows | ForEach-Object { $_.Leaf })
        $missing = @($Required | Where-Object { $leaves -notcontains $_ })
        if ($rows.Count -gt 0 -and $missing.Count -eq 0) { return $rows }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    return @()
}

function Click-Element([Windows.Automation.AutomationElement] $Element) {
    try {
        $pattern = $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
        $pattern.Invoke()
        return "invoke"
    } catch {
        $bounds = $Element.Current.BoundingRectangle
        if ($bounds.Width -lt 2 -or $bounds.Height -lt 2) { throw "element has no clickable bounds" }
        $x = [int]($bounds.Left + ($bounds.Width / 2))
        $y = [int]($bounds.Top + ($bounds.Height / 2))
        [void][ColumnKeyboardSmoke.Native]::SetCursorPos($x, $y)
        [ColumnKeyboardSmoke.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ColumnKeyboardSmoke.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        return "pointer"
    }
}

function New-KeyboardFixture {
    $nested = $script:Nest
    New-Item -ItemType Directory -Force -Path $nested | Out-Null
    Set-Content -LiteralPath (Join-Path $script:Fixture "a.txt") -Value "a" -Encoding utf8
    Set-Content -LiteralPath (Join-Path $script:Fixture "b.txt") -Value "b" -Encoding utf8
    Set-Content -LiteralPath (Join-Path $script:Fixture "c.txt") -Value "c" -Encoding utf8
    Set-Content -LiteralPath (Join-Path $nested "leaf.txt") -Value "leaf" -Encoding utf8
    Set-Content -LiteralPath (Join-Path $nested "m.txt") -Value "m" -Encoding utf8
}

function Start-ColumnApp {
    $profile = Join-Path $OutputDirectory "localappdata"
    New-Item -ItemType Directory -Force -Path $profile | Out-Null
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $Executable
    $start.WorkingDirectory = "D:\SuperExplorer"
    $start.UseShellExecute = $false
    $start.Environment["EXPLORER_INITIAL_PATH"] = $script:Fixture
    $start.Environment["EXPLORER_LOG_DIR"] = $OutputDirectory
    $start.Environment["LOCALAPPDATA"] = $profile
    $start.Environment["SUPEREXPLORER_DISABLE_REPEATED_LAUNCH_DETECTION"] = "1"
    $start.Environment["SUPEREXPLORER_LOCALE"] = "zh-TW"
    $script:App = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        Assert-Budget
        if ($script:App.HasExited) { throw "application exited early: $($script:App.ExitCode)" }
        $script:App.Refresh()
        if ($script:App.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($script:App.MainWindowHandle -eq [IntPtr]::Zero) { throw "application window did not appear" }
    [void][ColumnKeyboardSmoke.Native]::SetWindowPos($script:App.MainWindowHandle, [IntPtr]::Zero, 40, 40, 1440, 900, 0x0040)
    [void][ColumnKeyboardSmoke.Native]::SetForegroundWindow($script:App.MainWindowHandle)
    Start-Sleep -Milliseconds 400
    $address = Wait-AddressEnding "\fixture" 5000
    if ($null -eq $address) {
        $documents = @((Get-Root).FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Document))) |
            ForEach-Object { [string]$_.Current.Name })
        throw "address did not settle on the runner-owned fixture; documents=$($documents -join ' | ')"
    }
    $detailsDeadline = [DateTime]::UtcNow.AddSeconds(8)
    $detailsVisible = $false
    do {
        Assert-Budget
        $names = @(Get-ListItems (Get-Root) | ForEach-Object { [string]$_.Current.Name })
        $detailsVisible = @($names | Where-Object { $_ -match '(^|\s)a\.txt(\s|$)' }).Count -gt 0 -and
            @($names | Where-Object { $_ -match '(^|\s)b\.txt(\s|$)' }).Count -gt 0
        if ($detailsVisible) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $detailsDeadline)
    if (-not $detailsVisible) { throw "initial Details view did not show the runner-owned fixture files" }
    if ($address.TrimEnd("\").EndsWith("\nest", [StringComparison]::OrdinalIgnoreCase)) {
        throw "address was already inside nest before keyboard navigation: $address"
    }
    return $address
}

function Open-ColumnsByKeyboard([System.Collections.Generic.List[string]] $Evidence) {
    $root = Get-Root
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-ByName $root "View" }
    if ($null -eq $view) { throw "view menu button was not found" }
    [void]$Evidence.Add("view button: $($view.Current.Name) enabled=$($view.Current.IsEnabled)")
    if (-not $view.Current.IsEnabled) { throw "view menu button is disabled" }
    $opened = Click-Element $view
    [void][ColumnKeyboardSmoke.Native]::SetCursorPos(1, 1)
    [void]$Evidence.Add("view open: $opened")
    $columnsName = [string]::new(@([char]0x5206, [char]0x6B04))
    $columns = Wait-ByName $columnsName 4000
    if ($null -eq $columns) { $columns = Wait-ByName "Columns" 1500 }
    if ($null -eq $columns) { throw "columns menu item was not found" }
    [void]$Evidence.Add("columns item: $($columns.Current.Name) enabled=$($columns.Current.IsEnabled) role=$($columns.Current.ControlType.ProgrammaticName)")
    if (-not $columns.Current.IsEnabled) { throw "columns menu item is disabled" }
    # Home clears a hover-moved index. Eight Downs land on VIEW_MENU_COLUMNS.
    Send-Key 0x24
    foreach ($unused in 1..8) { Send-Key 0x28 }
    Send-Key 0x0D
    $surfaceName = [string]::new(@([char]0x5206, [char]0x6B04, [char]0x6AA2, [char]0x8996))
    $surface = Wait-Control $surfaceName ([Windows.Automation.ControlType]::Pane) 8000
    if ($null -eq $surface) { $surface = Wait-Control "Column view" ([Windows.Automation.ControlType]::Pane) 1500 }
    if ($null -eq $surface) { throw "keyboard activation did not expose the Columns pane" }
    Start-Sleep -Milliseconds 1800
    [void]$Evidence.Add("column pane: $($surface.Current.Name) role=$($surface.Current.ControlType.ProgrammaticName)")
    return $surface
}

function Measure-CheckedState([System.Collections.Generic.List[string]] $Evidence) {
    $root = Get-Root
    $view = Find-ByName $root ([string]::new(@([char]0x6AA2, [char]0x8996)))
    if ($null -eq $view) { $view = Find-ByName $root "View" }
    if ($null -eq $view) {
        [void]$Evidence.Add("checked reopen did not find the view button")
        return "SKIP"
    }
    [void](Click-Element $view)
    [void][ColumnKeyboardSmoke.Native]::SetCursorPos(1, 1)
    $columns = Wait-ByName ([string]::new(@([char]0x5206, [char]0x6B04))) 3000
    if ($null -eq $columns) { $columns = Wait-ByName "Columns" 1000 }
    if ($null -eq $columns) {
        Send-Key 0x1B
        [void]$Evidence.Add("checked probe could not reopen the Columns item")
        return "SKIP"
    }
    $toggle = $null
    $supported = $false
    try {
        $supported = $columns.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern, [ref]$toggle)
    } catch {
        $supported = $false
    }
    $name = [string]$columns.Current.Name
    Send-Key 0x1B
    if (-not $supported -or $null -eq $toggle) {
        [void]$Evidence.Add("checked state is not exposed: name=$name has no TogglePattern")
        return "SKIP"
    }
    $state = [string]([Windows.Automation.TogglePattern]$toggle).Current.ToggleState
    [void]$Evidence.Add("checked toggle=$state name=$name")
    if ($state -eq "On") { return "PASS" }
    throw "Columns toggle state was $state after keyboard activation"
}

function Select-FixtureLeaf([string] $Leaf) {
    foreach ($unused in 1..6) { Send-Key 0x26 }
    for ($step = 0; $step -lt 8; $step++) {
        $rows = @(Get-ColumnRows "fixture")
        $selected = @($rows | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf })
        if ($selected.Count -eq 1 -and $selected[0] -eq $Leaf) { return $rows }
        Send-Key 0x28
    }
    $seen = @((Get-ColumnRows "fixture") | ForEach-Object { "$($_.Leaf)=$($_.Selected)" }) -join ", "
    throw "could not keyboard-select ${Leaf}; rows=$seen"
}

function New-Step([string] $Id, [string] $Status, $Evidence) {
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($line in @($Evidence)) {
        if ($null -ne $line -and "$line" -ne "") { [void]$lines.Add([string]$line) }
    }
    if (Test-Path -LiteralPath $script:Shot) { [void]$lines.Add("screenshot=$script:Shot") }
    return [ordered]@{ id = $Id; status = $Status; evidence = $lines.ToArray() }
}

function Invoke-ViewMenuCase {
    $evidence = New-Object System.Collections.Generic.List[string]
    try {
        New-KeyboardFixture
        $address = Start-ColumnApp
        [void]$evidence.Add("initial address=$address")
        [void]$evidence.Add("fixture=$script:Fixture")
        [void](Open-ColumnsByKeyboard $evidence)
        Save-Shot
        $checkedEvidence = New-Object System.Collections.Generic.List[string]
        $checked = "FAIL"
        try {
            $checked = Measure-CheckedState $checkedEvidence
        } catch {
            [void]$checkedEvidence.Add($_.Exception.Message)
        }
        Save-Shot
        return @(
            (New-Step "view-menu-keyboard" "PASS" $evidence.ToArray()),
            (New-Step "view-menu-checked-state" $checked $checkedEvidence.ToArray())
        )
    } catch {
        [void]$evidence.Add($_.Exception.Message)
        Save-Shot
        return @(New-Step "view-menu-keyboard" "FAIL" $evidence.ToArray())
    } finally {
        Stop-App
    }
}

function Invoke-ArrowCase {
    $evidence = New-Object System.Collections.Generic.List[string]
    try {
        New-KeyboardFixture
        $address = Start-ColumnApp
        [void]$evidence.Add("initial address=$address")
        [void](Open-ColumnsByKeyboard $evidence)
        [void](Select-FixtureLeaf "nest")
        Send-Key 0x28
        Send-Key 0x26
        $back = @(Get-ColumnRows "fixture" | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf })
        if (@($back) -notcontains "nest") { throw "Down then Up did not return to nest; selected=$($back -join ',')" }
        [void]$evidence.Add("down/up returned to nest")
        Send-Key 0x27
        $child = @(Wait-ColumnRows "nest" @("leaf.txt", "m.txt") 8000)
        if ($child.Count -eq 0) { throw "Right did not reveal nest's child rows" }
        $entered = Wait-AddressEnding "\fixture\nest" 8000
        if ($null -eq $entered) { throw "Right did not move the address to fixture\nest; address=$(Get-AddressText)" }
        [void]$evidence.Add("address after Right=$entered")
        Send-Key 0x28
        $childHit = @()
        $childDeadline = [DateTime]::UtcNow.AddMilliseconds(2500)
        do {
            Assert-Budget
            $childHit = @((Get-ColumnRows "nest") | Where-Object { $_.Selected -eq $true -and ($_.Leaf -eq "leaf.txt" -or $_.Leaf -eq "m.txt") } | ForEach-Object { $_.Leaf })
            if ($childHit.Count -ge 1) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $childDeadline)
        if ($childHit.Count -lt 1) {
            $seen = @((Get-ColumnRows "nest") | ForEach-Object { "$($_.Leaf)=$($_.Selected)" }) -join ", "
            throw "Down in the child column did not select leaf.txt or m.txt; rows=$seen"
        }
        [void]$evidence.Add("child selection after Down=$($childHit -join ',')")
        Send-Key 0x25
        $kept = Wait-AddressEnding "\fixture\nest" 4000
        if ($null -eq $kept) { throw "Left dropped the nest path; address=$(Get-AddressText)" }
        $childAfter = Wait-SelectedLeaves "nest" @() @("leaf.txt", "m.txt") 2500
        if ($null -eq $childAfter) {
            $seen = @((Get-ColumnRows "nest") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf }) -join ", "
            throw "Left left a child file selected: $seen"
        }
        $parentSelected = @((Get-ColumnRows "fixture") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf })
        if (@($parentSelected) -notcontains "nest") { throw "Left did not return selection to nest; selected=$($parentSelected -join ',')" }
        [void]$evidence.Add("address after Left=$kept")
        [void]$evidence.Add("parent selection after Left=$($parentSelected -join ',')")
        Save-Shot
        return @(New-Step "arrow-address" "PASS" $evidence.ToArray())
    } catch {
        [void]$evidence.Add($_.Exception.Message)
        Save-Shot
        return @(New-Step "arrow-address" "FAIL" $evidence.ToArray())
    } finally {
        Stop-App
    }
}

function Invoke-SelectionCase {
    $evidence = New-Object System.Collections.Generic.List[string]
    try {
        New-KeyboardFixture
        [void]$evidence.Add("initial address=$(Start-ColumnApp)")
        [void](Open-ColumnsByKeyboard $evidence)
        [void](Select-FixtureLeaf "nest")
        Send-Key 0x27
        if (@(Wait-ColumnRows "nest" @("leaf.txt", "m.txt") 8000).Count -eq 0) {
            throw "Right did not open nest before the selection check"
        }
        Send-Key 0x25
        if ($null -eq (Wait-AddressEnding "\fixture\nest" 4000)) {
            throw "active column is not the fixture column under nest; address=$(Get-AddressText)"
        }
        $rows = @(Get-ColumnRows "fixture")
        $index = -1
        for ($i = 0; $i -lt $rows.Count; $i++) {
            if ($rows[$i].Leaf -eq "nest") { $index = $i; break }
        }
        if ($index -lt 0) { throw "fixture column lost the nest row" }
        $sign = 0
        if (($rows.Count - $index - 1) -ge 2) { $sign = 1 }
        elseif ($index -ge 2) { $sign = -1 }
        else { throw "fixture column has no two-row side around nest" }
        $near = $rows[$index + $sign].Leaf
        $far = $rows[$index + (2 * $sign)].Leaf
        $key = if ($sign -gt 0) { [byte]0x28 } else { [byte]0x26 }
        Send-Key $key 0x10
        Send-Key $key 0x10
        $selected = Wait-SelectedLeaves "fixture" @("nest", $near, $far) @()
        if ($null -eq $selected) {
            $seen = @((Get-ColumnRows "fixture") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf }) -join ", "
            throw "Shift selection missed nest/$near/$far; selected=$seen"
        }
        $childShift = Wait-SelectedLeaves "nest" @() @("leaf.txt", "m.txt") 1200
        if ($null -eq $childShift) { throw "Shift selection entered the child column" }
        [void]$evidence.Add("shift selected=$($selected -join ',') direction=$sign near=$near far=$far")
        Send-Key 0x20 0x11
        $afterToggle = Wait-SelectedLeaves "fixture" @("nest", $near) @($far)
        if ($null -eq $afterToggle) {
            $seen = @((Get-ColumnRows "fixture") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf }) -join ", "
            throw "Ctrl+Space did not keep $near and clear $far in the fixture column; selected=$seen"
        }
        $childToggle = Wait-SelectedLeaves "nest" @() @("leaf.txt", "m.txt") 1200
        if ($null -eq $childToggle) { throw "Ctrl+Space selected a child-column file" }
        [void]$evidence.Add("ctrl+space selected=$($afterToggle -join ',')")
        Send-Key 0x28 0x11
        $afterCtrl = Wait-SelectedLeaves "fixture" @($near) @()
        if ($null -eq $afterCtrl) {
            $seen = @((Get-ColumnRows "fixture") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf }) -join ", "
            throw "Ctrl+Down left the active column; selected=$seen"
        }
        $childCtrl = Wait-SelectedLeaves "nest" @() @("leaf.txt", "m.txt") 1200
        if ($null -eq $childCtrl) { throw "Ctrl+Down selected a child-column file" }
        [void]$evidence.Add("ctrl+down selected=$($afterCtrl -join ',')")
        Save-Shot
        return @(New-Step "shift-ctrl-selection" "PASS" $evidence.ToArray())
    } catch {
        [void]$evidence.Add($_.Exception.Message)
        Save-Shot
        return @(New-Step "shift-ctrl-selection" "FAIL" $evidence.ToArray())
    } finally {
        Stop-App
    }
}

function Invoke-NamesCase {
    $evidence = New-Object System.Collections.Generic.List[string]
    try {
        New-KeyboardFixture
        [void]$evidence.Add("initial address=$(Start-ColumnApp)")
        $surface = Open-ColumnsByKeyboard $evidence
        $paneName = [string]$surface.Current.Name
        $paneRole = [string]$surface.Current.ControlType.ProgrammaticName
        [void](Select-FixtureLeaf "nest")
        $folderStatus = [string]::new(@([char]0x8CC7, [char]0x6599, [char]0x593E))
        $previewName = [string]::new(@([char]0x6574, [char]0x5408, [char]0x9810, [char]0x89BD))
        $status = Wait-Control $folderStatus ([Windows.Automation.ControlType]::StatusBar) 5000
        if ($null -eq $status) { $status = Wait-Control "Folder" ([Windows.Automation.ControlType]::StatusBar) 1500 }
        if ($null -eq $status) { throw "folder preview did not expose a status named $folderStatus" }
        $preview = Wait-Control $previewName ([Windows.Automation.ControlType]::Group) 4000
        if ($null -eq $preview) { $preview = Wait-Control "Integrated preview" ([Windows.Automation.ControlType]::Group) 1500 }
        if ($null -eq $preview) { throw "integrated preview group was not exposed" }
        $rows = @(Get-ColumnRows "fixture")
        $nestRow = $rows | Where-Object { $_.Leaf -eq "nest" } | Select-Object -First 1
        if ($null -eq $nestRow) { throw "fixture column row nest was not exposed" }
        $columnList = $null
        $lists = (Get-Root).FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::List)))
        foreach ($candidate in $lists) {
            if ([string]$candidate.Current.Name -match "(欄|Column)\s+\d+\s+fixture(\s|$)") {
                $columnList = $candidate
                break
            }
        }
        if ($null -eq $columnList) { throw "fixture column list was not exposed" }
        $columnName = [string]$columnList.Current.Name
        $columnRole = [string]$columnList.Current.ControlType.ProgrammaticName
        $previewLabel = [string]$preview.Current.Name
        $statusLabel = [string]$status.Current.Name
        $first = [ordered]@{
            pane = "$paneRole|$paneName"
            column = "$columnRole|$columnName"
            row = "$($nestRow.Role)|$($nestRow.Name)"
            preview = "$($preview.Current.ControlType.ProgrammaticName)|$previewLabel"
            status = "$($status.Current.ControlType.ProgrammaticName)|$statusLabel"
        }
        Start-Sleep -Milliseconds 250
        $rowsAgain = @(Get-ColumnRows "fixture")
        $nestAgain = $rowsAgain | Where-Object { $_.Leaf -eq "nest" } | Select-Object -First 1
        $previewAgain = Find-Control $previewLabel ([Windows.Automation.ControlType]::Group)
        $statusAgain = Find-Control $statusLabel ([Windows.Automation.ControlType]::StatusBar)
        $surfaceAgain = Find-Control $paneName ([Windows.Automation.ControlType]::Pane)
        $columnAgain = $null
        $listsAgain = (Get-Root).FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            (New-Object Windows.Automation.PropertyCondition(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::List)))
        foreach ($candidate in $listsAgain) {
            if ([string]$candidate.Current.Name -eq $columnName) {
                $columnAgain = $candidate
                break
            }
        }
        if ($null -eq $nestAgain -or $null -eq $previewAgain -or $null -eq $statusAgain -or $null -eq $surfaceAgain -or $null -eq $columnAgain) {
            throw "accessible name disappeared on the second read"
        }
        $second = [ordered]@{
            pane = "$($surfaceAgain.Current.ControlType.ProgrammaticName)|$($surfaceAgain.Current.Name)"
            column = "$($columnAgain.Current.ControlType.ProgrammaticName)|$($columnAgain.Current.Name)"
            row = "$($nestAgain.Role)|$($nestAgain.Name)"
            preview = "$($previewAgain.Current.ControlType.ProgrammaticName)|$($previewAgain.Current.Name)"
            status = "$($statusAgain.Current.ControlType.ProgrammaticName)|$($statusAgain.Current.Name)"
        }
        foreach ($key in @("pane", "column", "row", "preview", "status")) {
            if ($first[$key] -ne $second[$key]) { throw "unstable $key : $($first[$key]) -> $($second[$key])" }
            [void]$evidence.Add("$key=$($first[$key])")
        }
        if ($first["column"] -notlike "ControlType.List|*") { throw "column role was $($first['column'])" }
        if ($nestRow.Role -ne "ControlType.ListItem") { throw "row role was $($nestRow.Role)" }
        if ($paneRole -ne "ControlType.Pane") { throw "column surface role was $paneRole" }
        if ($first["preview"] -notlike "ControlType.Group|*") { throw "preview role was $($first['preview'])" }
        if ($first["status"] -notlike "ControlType.StatusBar|*") { throw "status role was $($first['status'])" }
        Save-Shot
        return @(New-Step "accessible-names" "PASS" $evidence.ToArray())
    } catch {
        [void]$evidence.Add($_.Exception.Message)
        Save-Shot
        return @(New-Step "accessible-names" "FAIL" $evidence.ToArray())
    } finally {
        Stop-App
    }
}

function Invoke-FocusCase {
    $evidence = New-Object System.Collections.Generic.List[string]
    try {
        New-KeyboardFixture
        [void]$evidence.Add("initial address=$(Start-ColumnApp)")
        [void](Open-ColumnsByKeyboard $evidence)
        [void](Select-FixtureLeaf "nest")
        Send-Key 0x27
        if (@(Wait-ColumnRows "nest" @("leaf.txt", "m.txt") 8000).Count -eq 0) { throw "Right did not open nest" }
        Send-Key 0x28
        Send-Key 0x25
        if ($null -eq (Wait-AddressEnding "\fixture\nest" 4000)) { throw "Left lost the nest address; address=$(Get-AddressText)" }
        $rows = @(Get-ColumnRows "fixture")
        $index = -1
        for ($i = 0; $i -lt $rows.Count; $i++) {
            if ($rows[$i].Leaf -eq "nest") { $index = $i; break }
        }
        if ($index -lt 0) { throw "nest row was gone after Left" }
        $key = [byte]0x28
        $neighbor = $null
        if ($index + 1 -lt $rows.Count) { $neighbor = $rows[$index + 1].Leaf }
        elseif ($index -gt 0) { $key = [byte]0x26; $neighbor = $rows[$index - 1].Leaf }
        else { throw "fixture column has no neighbor to prove focus return" }
        $before = @((Get-ColumnRows "fixture") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf })
        if ($before -contains $neighbor) { throw "neighbor $neighbor was already selected before the focus probe" }
        Send-Key 0x09
        Send-Key $key
        Start-Sleep -Milliseconds 400
        foreach ($probe in 1..2) {
            $during = @((Get-ColumnRows "fixture") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf })
            if ($during -contains $neighbor) { throw "Tab did not leave the file view; the follow-up arrow selected $neighbor" }
            if ($probe -eq 1) { Start-Sleep -Milliseconds 300 }
        }
        [void]$evidence.Add("after Tab, neighbor $neighbor stayed unselected")
        Send-Key 0x09 0x10
        Send-Key $key
        $after = Wait-SelectedLeaves "fixture" @($neighbor) @() 2500
        if ($null -eq $after) {
            $seen = @((Get-ColumnRows "fixture") | Where-Object { $_.Selected -eq $true } | ForEach-Object { $_.Leaf }) -join ", "
            throw "Shift+Tab did not return file-view focus; $neighbor stayed unselected; selected=$seen"
        }
        $child = Wait-SelectedLeaves "nest" @() @("leaf.txt", "m.txt") 1200
        if ($null -eq $child) { throw "focus return selected a child file" }
        [void]$evidence.Add("focus returned to fixture column via $neighbor; selected=$($after -join ',')")
        Save-Shot
        return @(New-Step "focus-return" "PASS" $evidence.ToArray())
    } catch {
        [void]$evidence.Add($_.Exception.Message)
        Save-Shot
        return @(New-Step "focus-return" "FAIL" $evidence.ToArray())
    } finally {
        Stop-App
    }
}

$produced = @()
switch ($Case) {
    "view-menu-keyboard" { $produced = @(Invoke-ViewMenuCase) }
    "arrow-address" { $produced = @(Invoke-ArrowCase) }
    "shift-ctrl-selection" { $produced = @(Invoke-SelectionCase) }
    "accessible-names" { $produced = @(Invoke-NamesCase) }
    "focus-return" { $produced = @(Invoke-FocusCase) }
    default { throw "unknown case $Case" }
}
$report = [ordered]@{
    status = $(if (@($produced | Where-Object { $_.status -eq "FAIL" }).Count -gt 0) { "FAIL" } else { "PASS" })
    fixture = $script:Fixture
    executable = $Executable
    steps = @($produced)
}
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory "report.json") -Encoding utf8
exit 0
