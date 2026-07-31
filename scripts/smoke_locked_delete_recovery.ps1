param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'owned-lock-recovery-fixture'
$cancelPath = Join-Path $fixture 'locked-cancel.txt'
$closeFirstPath = Join-Path $fixture 'locked-close-a.txt'
$closeSecondPath = Join-Path $fixture 'locked-close-b.txt'
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$helperBinary = Join-Path $workspace "target\$Profile\explorer-lock-holder.exe"
$context = $null
$helpers = [Collections.Generic.List[Diagnostics.Process]]::new()
$passed = $false

function Start-LockHolder([string]$Path) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $helperBinary
    $start.Arguments = '"' + $Path + '"'
    $start.WorkingDirectory = $workspace
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($start)
    $ready = $process.StandardOutput.ReadLine()
    if ($ready -notlike 'READY *') {
        $errorText = $process.StandardError.ReadToEnd()
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
        throw "lock holder did not become ready: output=$ready error=$errorText"
    }
    $process
}

function Select-FileItem([string]$Name) {
    $item = Find-UitestFileItem -Root $context.Root -Name $Name
    $selection = $null
    if (-not $item.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
        throw "file item does not expose SelectionItemPattern: $Name"
    }
    ([Windows.Automation.SelectionItemPattern]$selection).Select()
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 250
}

function Add-FileItemToSelection([string]$Name) {
    $item = Find-UitestFileItem -Root $context.Root -Name $Name
    Invoke-UitestClick -Element $item -Control
}

function Find-LockDialog([int]$TimeoutSeconds = 12) {
    Find-UitestElement -Root $context.Root -Description 'locked-file recovery dialog' -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        if ($element.Current.ControlType -ne [Windows.Automation.ControlType]::Window -or
            $element.Current.BoundingRectangle.Width -le 0) { return $false }
        @($element.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition
        ) | Where-Object {
            try {
                $_.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
                    $_.Current.Name -like 'explorer-lock-holder.exe*'
            } catch { $false }
        }).Count -gt 0
    }
}

function Find-LockButton([ValidateRange(0,2)][int]$Index) {
    $dialog = Find-LockDialog
    $buttons = @($dialog.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    ) | Where-Object {
        try {
            $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
                $_.Current.BoundingRectangle.Width -gt 0
        } catch { $false }
    } | Sort-Object { $_.Current.BoundingRectangle.Left })
    if ($buttons.Count -ne 3) { throw "expected three lock-recovery buttons, found $($buttons.Count)" }
    $buttons[$Index]
}

function Wait-LockDialogClosed([int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $visible = @($context.Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition
        ) | Where-Object {
            try {
                if ($_.Current.ControlType -ne [Windows.Automation.ControlType]::Window -or
                    $_.Current.BoundingRectangle.Width -le 0) { return $false }
                @($_.FindAll(
                    [Windows.Automation.TreeScope]::Descendants,
                    [Windows.Automation.Condition]::TrueCondition
                ) | Where-Object {
                    try { $_.Current.Name -like 'explorer-lock-holder.exe*' } catch { $false }
                }).Count -gt 0
            } catch { $false }
        }).Count -gt 0
        if (-not $visible) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'locked-file recovery dialog did not close'
}

function Assert-HelperAlive([Diagnostics.Process]$Process, [string]$Operation) {
    $Process.Refresh()
    if ($Process.HasExited) { throw "$Operation unexpectedly closed lock holder $($Process.Id)" }
}

try {
    if (-not $SkipBuild) {
        $profileArgs = if ($Profile -eq 'release') { @('--release') } else { @() }
        & cargo.exe build -p explorer-shell-win --bin explorer-lock-holder @profileArgs --locked
        if ($LASTEXITCODE -ne 0) { throw "lock-holder build failed: $LASTEXITCODE" }
    }
    if (-not (Test-Path -LiteralPath $helperBinary -PathType Leaf)) {
        throw "missing lock-holder helper: $helperBinary"
    }
    New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    Set-Content -Encoding utf8 -LiteralPath $cancelPath -Value 'cancel sentinel'
    $cancelHelper = Start-LockHolder -Path $cancelPath
    $helpers.Add($cancelHelper)
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

    # Plain Retry must retry exactly once without closing the owner. Since the helper intentionally
    # keeps the lock, the Explorer-like dialog must return and remain cancellable.
    Select-FileItem -Name 'locked-cancel.txt'
    Send-UitestKey -Key 0x2E -DelayMilliseconds 3000
    Find-LockDialog | Out-Null
    $owner = Find-UitestElement -Root $context.Root -Description 'eligible lock owner' -Predicate {
        param($element)
            $element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
            $element.Current.Name -like 'explorer-lock-holder.exe*'
    }
    Find-LockButton -Index 0 | Out-Null
    Find-LockButton -Index 1 | Out-Null
    Find-LockButton -Index 2 | Out-Null
    Send-UitestKey -Key 0x09
    Send-UitestKey -Key 0x0D -DelayMilliseconds 2500
    Find-LockDialog | Out-Null
    Assert-HelperAlive -Process $cancelHelper -Operation 'plain Retry'
    if (-not (Test-Path -LiteralPath $cancelPath -PathType Leaf)) {
        throw 'plain Retry removed a still-locked file'
    }

    # Escape must dismiss the modal without touching either the process or the file.
    Send-UitestKey -Key 0x1B -DelayMilliseconds 350
    Wait-LockDialogClosed
    Assert-HelperAlive -Process $cancelHelper -Operation 'Escape'
    if (-not (Test-Path -LiteralPath $cancelPath -PathType Leaf)) {
        throw 'Escape removed the locked file'
    }

    # Pointer Cancel has the same non-destructive contract.
    Select-FileItem -Name 'locked-cancel.txt'
    Send-UitestKey -Key 0x2E -DelayMilliseconds 2500
    Find-LockDialog | Out-Null
    Invoke-UitestClick -Element (Find-LockButton -Index 2)
    Wait-LockDialogClosed
    Assert-HelperAlive -Process $cancelHelper -Operation 'Cancel'
    if (-not (Test-Path -LiteralPath $cancelPath -PathType Leaf)) {
        throw 'Cancel removed the locked file'
    }

    # Two selected locked files prove the list, explicit pointer action, graceful close, and one
    # recycle-delete retry across multiple independent Restart Manager owners.
    Set-Content -Encoding utf8 -LiteralPath $closeFirstPath -Value 'close sentinel A'
    Set-Content -Encoding utf8 -LiteralPath $closeSecondPath -Value 'close sentinel B'
    $closeFirstHelper = Start-LockHolder -Path $closeFirstPath
    $helpers.Add($closeFirstHelper)
    $closeSecondHelper = Start-LockHolder -Path $closeSecondPath
    $helpers.Add($closeSecondHelper)
    Send-UitestKey -Key 0x74 -DelayMilliseconds 800
    Select-FileItem -Name 'locked-close-a.txt'
    Add-FileItemToSelection -Name 'locked-close-b.txt'
    if ((Get-UitestSelectedCount -Root $context.Root) -ne 2) {
        throw 'multiple locked-file selection was not preserved'
    }
    Send-UitestKey -Key 0x2E -DelayMilliseconds 3000
    Find-LockDialog | Out-Null
    $ownerItems = @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    ) | Where-Object {
        try {
            $_.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
                $_.Current.Name -like 'explorer-lock-holder.exe*'
        } catch { $false }
    })
    if ($ownerItems.Count -ne 2) {
        throw "expected two eligible lock owners, found $($ownerItems.Count)"
    }

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'locked-delete-recovery.png')

    $snapshot = foreach ($element in $context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    )) {
        try {
            $bounds = $element.Current.BoundingRectangle
            if ($element.Current.Name -or $element.Current.AutomationId) {
                '{0}`tid={1}`tname={2}`tbounds={3},{4},{5},{6}' -f
                    $element.Current.ControlType.ProgrammaticName,
                    $element.Current.AutomationId,
                    $element.Current.Name,
                    [int]$bounds.Left,
                    [int]$bounds.Top,
                    [int]$bounds.Width,
                    [int]$bounds.Height
            }
        } catch { }
    }
    $snapshot | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'lock-recovery-uia.txt')

    Invoke-UitestClick -Element (Find-LockButton -Index 0)
    Wait-UitestPath -Path $closeFirstPath -Exists $false -TimeoutSeconds 20
    Wait-UitestPath -Path $closeSecondPath -Exists $false -TimeoutSeconds 20
    if (-not $closeFirstHelper.WaitForExit(10000) -or -not $closeSecondHelper.WaitForExit(10000)) {
        throw 'graceful Close programs and retry left an owned helper running'
    }

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        eligible_owner_uia = $owner.Current.Name
        keyboard_retry_preserved_owner_and_file = $true
        escape_preserved_owner_and_file = $true
        pointer_cancel_preserved_owner_and_file = $true
        multiple_owner_count = $ownerItems.Count
        close_and_retry_closed_all_helpers = $true
        close_and_retry_removed_both_sources = $true
        denied_and_stale_identity_contracts = 'explorer-ui::locked_delete_partial_close_duplicate_event_and_navigation_are_safe; explorer-shell-win::locked_delete_protected_process_classes_are_never_eligible'
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
    $passed = $true
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    foreach ($ownedHelper in $helpers) {
        if (-not $ownedHelper.HasExited) { $ownedHelper.Kill(); $ownedHelper.WaitForExit() }
        $ownedHelper.Dispose()
    }
    if ($passed -and (Test-Path -LiteralPath $fixture)) {
        $resolvedFixture = [IO.Path]::GetFullPath($fixture)
        $ownedPrefix = $output.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
        if (-not $resolvedFixture.StartsWith($ownedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing to remove fixture outside evidence directory: $resolvedFixture"
        }
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}

if (-not $passed) { throw 'Locked-delete recovery smoke did not reach PASS' }
Write-Output "Locked-delete recovery smoke passed: $OutputDirectory"
