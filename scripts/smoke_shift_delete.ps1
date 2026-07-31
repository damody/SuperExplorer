param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'owned-shift-delete-fixture'
$targetPath = Join-Path $fixture 'shift-delete-target.txt'
$context = $null
$passed = $false

function Send-ShiftDelete {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 150
    [Windows.Forms.SendKeys]::SendWait('+{DELETE}')
    Start-Sleep -Milliseconds 300
}

function Find-PermanentDeleteDialog {
    Find-UitestElement -Root $context.Root -Description 'permanent delete dialog' -Predicate {
        param($element)
        $element.Current.Name -like 'Permanently delete 1 item*' -and
            $element.Current.BoundingRectangle.Width -gt 0
    }
}

function Find-DialogButton([string]$Name) {
    Find-UitestElement -Root $context.Root -Description "permanent delete $Name button" -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq $Name -and
            $element.Current.BoundingRectangle.Width -gt 0
    }
}

function Get-CapturedButtonPixel(
    [string]$Path,
    [Windows.Automation.AutomationElement]$Button
) {
    $window = $context.Root.Current.BoundingRectangle
    $bounds = $Button.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::FromFile($Path)
    try {
        $color = $bitmap.GetPixel(
            [int]($bounds.Left - $window.Left + 12),
            [int]($bounds.Top - $window.Top + $bounds.Height / 2))
        [ordered]@{ r=[int]$color.R; g=[int]$color.G; b=[int]$color.B }
    } finally {
        $bitmap.Dispose()
    }
}

function Get-ColorDistance($Left, $Right) {
    [Math]::Max(
        [Math]::Abs([int]$Left.r - [int]$Right.r),
        [Math]::Max(
            [Math]::Abs([int]$Left.g - [int]$Right.g),
            [Math]::Abs([int]$Left.b - [int]$Right.b)))
}

function Select-FileItem([string]$Name) {
    $item = Find-UitestFileItem -Root $context.Root -Name $Name
    $selection = $null
    if (-not $item.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
        throw "file item does not expose SelectionItemPattern: $Name"
    }
    ([Windows.Automation.SelectionItemPattern]$selection).Select()
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Milliseconds 300
    return $item
}

try {
    New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    Set-Content -Encoding utf8 -LiteralPath $targetPath -Value 'permanent delete sentinel'
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild

    Select-FileItem -Name 'shift-delete-target.txt' | Out-Null
    Send-ShiftDelete
    Find-PermanentDeleteDialog | Out-Null
    $cancel = Find-DialogButton 'Cancel'
    $delete = Find-DialogButton 'Delete'
    $defaultCapture = Join-Path $output 'shift-delete-focus-default.png'
    Save-UitestScreenshot -Root $context.Root -Path $defaultCapture
    $defaultCancel = Get-CapturedButtonPixel $defaultCapture $cancel
    $defaultDelete = Get-CapturedButtonPixel $defaultCapture $delete

    Send-UitestKey -Key 0x09 -DelayMilliseconds 300
    $cancel = Find-DialogButton 'Cancel'
    $delete = Find-DialogButton 'Delete'
    $tabCapture = Join-Path $output 'shift-delete-focus-tab.png'
    Save-UitestScreenshot -Root $context.Root -Path $tabCapture
    $tabCancel = Get-CapturedButtonPixel $tabCapture $cancel
    $tabDelete = Get-CapturedButtonPixel $tabCapture $delete
    $focusSwapDistance = Get-ColorDistance $defaultDelete $tabCancel
    $idleSwapDistance = Get-ColorDistance $defaultCancel $tabDelete
    $focusContrast = Get-ColorDistance $defaultDelete $defaultCancel
    if ($focusSwapDistance -gt 3 -or $idleSwapDistance -gt 3) {
        throw "Shift+Delete focus gray did not follow Tab: focus=$focusSwapDistance idle=$idleSwapDistance"
    }
    if ($focusContrast -lt 5) { throw "Shift+Delete focused button is not visibly distinct: distance=$focusContrast" }
    if ($defaultDelete.r -lt 200 -or $defaultDelete.r -gt 250 -or
        $defaultDelete.g -lt 200 -or $defaultDelete.g -gt 250 -or
        $defaultDelete.b -lt 200 -or $defaultDelete.b -gt 250) {
        throw "Shift+Delete focus is not neutral gray: rgb=$($defaultDelete.r),$($defaultDelete.g),$($defaultDelete.b)"
    }
    Send-UitestKey -Key 0x0D -DelayMilliseconds 300
    if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
        throw 'Tab then Enter did not invoke focused Cancel'
    }

    Send-ShiftDelete
    Find-PermanentDeleteDialog | Out-Null
    $cancel = Find-DialogButton 'Cancel'
    $point = Get-UitestPhysicalPoint -Element $cancel
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware($point.X, $point.Y)
    Start-Sleep -Milliseconds 300
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'shift-delete-hover-cancel.png')
    Invoke-UitestClick -Element (Find-DialogButton 'Cancel')
    if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
        throw 'pointer click did not invoke Cancel'
    }

    # The same selected identity must remain valid after both cancel routes; the default focused
    # Delete action must still submit exactly once.
    Send-ShiftDelete
    Find-PermanentDeleteDialog | Out-Null
    Send-UitestKey -Key 0x0D -DelayMilliseconds 500
    Wait-UitestPath -Path $targetPath -Exists $false -TimeoutSeconds 20

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'shift-delete.png')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        cancel_preserved_file = $true
        tab_enter_cancel_preserved_file = $true
        pointer_cancel_preserved_file = $true
        confirm_removed_file = $true
        no_repeat_confirmation = $true
        focus_gray_followed_tab = $true
        focus_colors = [ordered]@{
            default_cancel=$defaultCancel
            default_delete=$defaultDelete
            tab_cancel=$tabCancel
            tab_delete=$tabDelete
        }
        focus_distance = [ordered]@{
            focused_swap=$focusSwapDistance
            idle_swap=$idleSwapDistance
            contrast=$focusContrast
        }
        native_no_recycle_flag_test = 'explorer-shell-win::permanent_delete_never_sets_recycle_or_undo_flags_even_for_permissive_callers'
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
    $passed = $true
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if ($passed -and (Test-Path -LiteralPath $fixture)) {
        $resolvedFixture = [IO.Path]::GetFullPath($fixture)
        $resolvedOutput = [IO.Path]::GetFullPath($output).TrimEnd([IO.Path]::DirectorySeparatorChar)
        if (-not $resolvedFixture.StartsWith($resolvedOutput + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing to remove fixture outside evidence directory: $resolvedFixture"
        }
        Remove-Item -LiteralPath $fixture -Recurse -Force
    }
}

if (-not $passed) { throw 'Shift+Delete smoke did not reach PASS' }
Write-Output "Shift+Delete smoke passed: $output"
