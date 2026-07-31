param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$workspaceRoot = [IO.Path]::GetPathRoot($workspace)
# Keep evidence/log writes outside the watched drive. The Windows watcher is recursive, so placing
# UTIT output below the active drive would continuously generate unrelated refreshes.
$driveRoot = @(Get-PSDrive -PSProvider FileSystem | ForEach-Object { $_.Root } | Where-Object {
    $_ -and ([IO.Path]::GetFullPath($_) -ne $workspaceRoot)
} | Sort-Object -Descending | Select-Object -First 1)
if ($driveRoot.Count -ne 1) {
    throw "navigation delete refresh UTIT requires a writable drive other than $workspaceRoot"
}
$driveRoot = [IO.Path]::GetFullPath($driveRoot[0])
$runId = [Guid]::NewGuid().ToString('N').Substring(0, 8)
$externalName = "000-uitest-external-delete-$runId"
$appName = "000-uitest-app-delete-$runId"
$survivorName = "000-uitest-survivor-$runId"
$externalTarget = Join-Path $driveRoot $externalName
$appTarget = Join-Path $driveRoot $appName
$survivor = Join-Path $driveRoot $survivorName
$context = $null
$passed = $false

function Get-NavigationRows([string]$Name) {
    $window = $context.Root.Current.BoundingRectangle
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button
        )
    ) | Where-Object {
        try {
            $candidateName = $_.Current.Name
            $bounds = $_.Current.BoundingRectangle
            $candidateName -and
                ($candidateName -eq $Name -or $candidateName.StartsWith($Name + ' (', [StringComparison]::OrdinalIgnoreCase)) -and
                $bounds.Width -gt 0 -and
                $bounds.Top -gt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + 500)
        } catch { $false }
    })
}

function Wait-NavigationRow([string]$Name, [bool]$Exists, [int]$TimeoutSeconds = 20) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $found = @(Get-NavigationRows -Name $Name).Count -gt 0
        if ($found -eq $Exists) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $window = $context.Root.Current.BoundingRectangle
    $available = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button
        )
    ) | Where-Object {
        try { $_.Current.BoundingRectangle.Left -lt ($window.Left + 500) } catch { $false }
    } | ForEach-Object {
        try { $_.Current.Name } catch { '<stale automation element>' }
    } | Sort-Object -Unique)
    throw "navigation row '$Name' existence did not become $Exists; available: $($available -join ', ')"
}

function Expand-NavigationRow([string]$Name) {
    $row = @(Get-NavigationRows -Name $Name | Select-Object -First 1)
    if ($row.Count -ne 1) { throw "navigation row was not found: $Name" }
    $chevrons = @($row[0].FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button
        )
    ) | Where-Object {
        try {
            $_.Current.Name -in @('Expand', 'Collapse') -and
                $_.Current.BoundingRectangle.Width -gt 0 -and
                $_.Current.BoundingRectangle.Height -gt 0
        } catch { $false }
    })
    if ($chevrons.Count -eq 0) { throw "expected an expand/collapse control for $Name" }
    $expand = @($chevrons | Where-Object { $_.Current.Name -eq 'Expand' } | Select-Object -First 1)
    if ($expand.Count -eq 0) { return }
    Invoke-UitestClick -Element $expand[0]
    Start-Sleep -Milliseconds 500
}

function Select-FileItem([string]$Name) {
    $item = Find-UitestFileItem -Root $context.Root -Name $Name
    $selection = $null
    if (-not $item.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
        throw "file item does not expose SelectionItemPattern: $Name"
    }
    ([Windows.Automation.SelectionItemPattern]$selection).Select()
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 200
}

function Confirm-ShiftDelete {
    [Windows.Forms.SendKeys]::SendWait('+{DELETE}')
    Start-Sleep -Milliseconds 250
    Find-UitestElement -Root $context.Root -Description 'permanent delete dialog' -Predicate {
        param($element)
        $element.Current.Name -like 'Permanently delete 1 item*' -and
            $element.Current.BoundingRectangle.Width -gt 0
    } | Out-Null
    Send-UitestKey -Key 0x0D -DelayMilliseconds 300
}

try {
    foreach ($ownedPath in @($externalTarget, $appTarget, $survivor)) {
        if (Test-Path -LiteralPath $ownedPath) {
            throw "refusing to reuse pre-existing navigation fixture: $ownedPath"
        }
    }
    New-Item -ItemType Directory -Force -Path $externalTarget, $appTarget, $survivor | Out-Null
    $context = Start-UitestExplorer -InitialPath $driveRoot -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    $window = $context.Root.Current.BoundingRectangle
    $driveLetter = $driveRoot.Substring(0, 1).ToUpperInvariant()
    $driveRows = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Button
        )
    ) | Where-Object {
        try {
            $bounds = $_.Current.BoundingRectangle
            $_.Current.Name -match "^(?:.*\($driveLetter`:\)|$driveLetter`:)$" -and
                $bounds.Top -gt ($window.Top + 180) -and
                $bounds.Left -lt ($window.Left + 500)
        } catch { $false }
    })
    if ($driveRows.Count -ne 1) { throw "expected one navigation drive row for $driveLetter`:" }
    Expand-NavigationRow -Name $driveRows[0].Current.Name
    Find-UitestFileItem -Root $context.Root -Name $externalName | Out-Null
    Wait-NavigationRow -Name $externalName -Exists $true
    Wait-NavigationRow -Name $appName -Exists $true
    Wait-NavigationRow -Name $survivorName -Exists $true

    # External deletion proves the active filesystem watcher invalidates the separately cached
    # navigation enumeration without F5, collapsing the tree, or leaving the directory.
    Remove-Item -LiteralPath $externalTarget -Recurse -Force
    Wait-NavigationRow -Name $externalName -Exists $false
    Wait-NavigationRow -Name $survivorName -Exists $true

    # App-owned Shift+Delete proves the successful operation terminal performs the same invalidation
    # immediately, independently of watcher timing.
    Select-FileItem -Name $appName
    Confirm-ShiftDelete
    Wait-UitestPath -Path $appTarget -Exists $false -TimeoutSeconds 20
    Wait-NavigationRow -Name $appName -Exists $false
    Wait-NavigationRow -Name $survivorName -Exists $true

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'navigation-delete-refresh.png')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        external_delete_removed_navigation_row = $true
        app_shift_delete_removed_navigation_row = $true
        expanded_parent_preserved = $true
        survivor_preserved = $true
        manual_refresh_used = $false
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
    $passed = $true
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if ($passed) {
        foreach ($ownedPath in @($externalTarget, $appTarget, $survivor)) {
            if (-not (Test-Path -LiteralPath $ownedPath)) { continue }
            $resolvedOwned = [IO.Path]::GetFullPath($ownedPath)
            if ([IO.Path]::GetPathRoot($resolvedOwned) -ne $driveRoot -or
                -not (Split-Path -Leaf $resolvedOwned).StartsWith('000-uitest-', [StringComparison]::Ordinal)) {
                throw "refusing to remove path outside the exact drive-root test scope: $resolvedOwned"
            }
            Remove-Item -LiteralPath $resolvedOwned -Recurse -Force
        }
    }
}

if (-not $passed) { throw 'Navigation delete refresh smoke did not reach PASS' }
Write-Output "Navigation delete refresh smoke passed: $output"
