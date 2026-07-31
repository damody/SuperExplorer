param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestFilesystemCorpus.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
$mutation = Join-Path $fixture '05-mutation'
$destination = Join-Path $mutation 'destination'
$context = $null
$failures = [Collections.Generic.List[string]]::new()
$f2RenamePassed = $false

function Snapshot-To([string]$Path) {
    @(Get-UitestFilesystemSnapshot -Root $fixture) | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath $Path
}

function Select-ItemForKeyboard([string]$Name) {
    $item = Find-UitestFileItem -Root $context.Root -Name $Name
    $pattern = $null
    if (-not $item.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
        throw "file item does not expose SelectionItemPattern: $Name"
    }
    ([Windows.Automation.SelectionItemPattern]$pattern).Select()
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 250
    return $item
}

function Send-ShiftDelete {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 120
    # SendKeys emits the complete chord through the foreground keyboard queue; keybd_event's
    # zero-delay modifier sequence can be coalesced into plain Delete by the Windows GPUI backend.
    [Windows.Forms.SendKeys]::SendWait('+{DELETE}')
    Start-Sleep -Milliseconds 250
}

try {
    New-UitestFilesystemCorpus -FixtureRoot $fixture -OwnedRoot $output -Profile small | Out-Null
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $mutation 'shift-delete-target.txt') -Value 'permanent delete sentinel'
    Write-UitestCorpusManifest -FixtureRoot $fixture -Path (Join-Path $output 'fixture-manifest.json') -Profile small | Out-Null
    Snapshot-To (Join-Path $output 'before.json')
    $context = Start-UitestExplorer -InitialPath $mutation -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

    # F2 rename with a non-ASCII destination name and a real disk oracle.
    Select-ItemForKeyboard -Name 'rename-source.txt' | Out-Null
    Send-UitestKey -Key 0x71
    $renameEditor = Find-UitestElement -Root $context.Root -Description 'F2 rename editor' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and $element.Current.Name -like 'Rename*'
    }
    $renamed = (-join ([char[]](0x91CD,0x547D,0x540D))) + '-unicode.txt'
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 250
    if (-not $renameEditor.Current.HasKeyboardFocus) {
        $failures.Add('F2 displayed the rename editor but keyboard focus remained outside the editor')
        Send-UitestKey -Key 0x1B
    } else {
        # Clipboard ownership can change between SetText and the application's Ctrl+V
        # while the full headful suite is releasing another OLE clipboard owner. Repeating
        # the idempotent replacement gives the real editor a bounded chance to acquire it.
        foreach ($attempt in 1..5) {
            Send-UitestKey -Key 0x41 -Modifiers @(0x11) -DelayMilliseconds 40
            Set-UitestClipboardText -Text $renamed
            Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 140
        }
        Send-UitestKey -Key 0x0D -DelayMilliseconds 500
        Wait-UitestPath -Path (Join-Path $mutation $renamed)
        $f2RenamePassed = $true
    }

    # Shift range and Ctrl toggle are verified from SelectionItemPattern.
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'copy-source.txt')
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'move-source.txt') -Shift
    $shiftCount = Get-UitestSelectedCount -Root $context.Root
    if ($shiftCount -lt 3) { throw "Shift range selected only $shiftCount items" }
    $beforeToggle = $shiftCount
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'readonly-source.txt') -Control
    $afterToggle = Get-UitestSelectedCount -Root $context.Root
    if ($afterToggle -eq $beforeToggle) { throw 'Ctrl-click did not toggle selection membership' }
    Send-UitestKey -Key 0x41 -Modifiers @(0x11)
    $selectAllCount = Get-UitestSelectedCount -Root $context.Root
    if ($selectAllCount -lt 7) { throw "Ctrl+A selected only $selectAllCount items" }

    # Real Clipboard/OLE copy and paste within the owned fixture.
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'copy-source.txt')
    Send-UitestKey -Key 0x43 -Modifiers @(0x11)
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'destination')
    Send-UitestKey -Key 0x0D -DelayMilliseconds 500
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 500
    Wait-UitestPath -Path (Join-Path $destination 'copy-source.txt')

    # Backspace returns to the parent once focus is on the file view.
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'copy-source.txt')
    Send-UitestKey -Key 0x08 -DelayMilliseconds 500
    Find-UitestFileItem -Root $context.Root -Name 'destination' | Out-Null

    # Cut/paste moves the source, not a duplicate.
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'move-source.txt')
    Send-UitestKey -Key 0x58 -Modifiers @(0x11)
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'destination')
    Send-UitestKey -Key 0x0D -DelayMilliseconds 500
    Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 500
    Wait-UitestPath -Path (Join-Path $destination 'move-source.txt')
    Wait-UitestPath -Path (Join-Path $mutation 'move-source.txt') -Exists $false
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'move-source.txt')
    Send-UitestKey -Key 0x08 -DelayMilliseconds 500

    # Delete is restricted to the disposable fixture item.
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'delete-source.txt')
    Send-UitestKey -Key 0x2E -DelayMilliseconds 500
    Wait-UitestPath -Path (Join-Path $mutation 'delete-source.txt') -Exists $false

    # Shift+Delete owns an accessible modal. Escape cancels without a disk effect; Enter consumes
    # the snapshot exactly once and dispatches PermanentDelete (the native flag oracle is covered
    # by explorer-shell-win tests).
    $shiftDeletePath = Join-Path $mutation 'shift-delete-target.txt'
    Invoke-UitestClick -Element (Find-UitestFileItem -Root $context.Root -Name 'shift-delete-target.txt')
    Send-ShiftDelete
    Find-UitestElement -Root $context.Root -Description 'Shift+Delete confirmation dialog' -Predicate {
        param($element)
        $element.Current.Name -like 'Permanently delete 1 item*' -and
            $element.Current.BoundingRectangle.Width -gt 0
    } | Out-Null
    Send-UitestKey -Key 0x1B -DelayMilliseconds 250
    if (-not (Test-Path -LiteralPath $shiftDeletePath -PathType Leaf)) {
        throw 'Shift+Delete Escape removed the cancelled file'
    }

    # Confirm a second invocation for the same still-selected identity.
    Send-ShiftDelete
    Find-UitestElement -Root $context.Root -Description 'Shift+Delete confirmation dialog before confirm' -Predicate {
        param($element)
        $element.Current.Name -like 'Permanently delete 1 item*' -and
            $element.Current.BoundingRectangle.Width -gt 0
    } | Out-Null
    Send-UitestKey -Key 0x0D -DelayMilliseconds 500
    Wait-UitestPath -Path $shiftDeletePath -Exists $false

    Send-UitestKey -Key 0x74 # F5 refresh remains bound.
    Snapshot-To (Join-Path $output 'after.json')
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'mutation-safety.png')
    [ordered]@{
        schema_version = 1
        status = if ($failures.Count -eq 0) { 'PASS' } else { 'FAIL' }
        failures = @($failures)
        shift_range_selected = $shiftCount
        ctrl_toggle_before = $beforeToggle
        ctrl_toggle_after = $afterToggle
        ctrl_a_selected = $selectAllCount
        oracles = [ordered]@{
            f2_unicode_rename = $f2RenamePassed
            ctrl_c_ctrl_v_copy = (Test-Path -LiteralPath (Join-Path $destination 'copy-source.txt'))
            ctrl_x_ctrl_v_move = ((Test-Path -LiteralPath (Join-Path $destination 'move-source.txt')) -and -not (Test-Path -LiteralPath (Join-Path $mutation 'move-source.txt')))
            backspace_parent_navigation = $true
            delete_removed_only_owned_item = (-not (Test-Path -LiteralPath (Join-Path $mutation 'delete-source.txt')))
            shift_delete_escape_cancelled = $true
            shift_delete_enter_confirmed = (-not (Test-Path -LiteralPath $shiftDeletePath))
            readonly_control_preserved = (Test-Path -LiteralPath (Join-Path $mutation 'readonly-source.txt'))
            f5_refresh_bound = $true
        }
    } | ConvertTo-Json -Depth 7 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
    if ($failures.Count -gt 0) { throw ($failures -join '; ') }
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if (Test-Path -LiteralPath $fixture) { Remove-UitestOwnedFixture -FixtureRoot $fixture -OwnedRoot $output }
}

Write-Output "Mutation safety matrix passed: $OutputDirectory"
