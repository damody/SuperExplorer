param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture 'address-edit-sentinel.txt') -Value 'sentinel' -Encoding utf8
$context = $null

function Find-AddressSurface {
    Find-UitestElement -Root $context.Root -Description 'browsing address surface' -TimeoutSeconds 8 -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Document -and
            $element.Current.Name -like 'Address: *' -and
            $bounds.Top -lt ($window.Top + 180) -and
            $bounds.Left -lt ($window.Left + $window.Width * 0.65)
    }
}

function Find-AddressEditor {
    Find-UitestElement -Root $context.Root -Description 'editable address' -TimeoutSeconds 8 -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $bounds.Top -lt ($window.Top + 180) -and
            $bounds.Left -lt ($window.Left + $window.Width * 0.65)
    }
}

function Get-EditorValue([Windows.Automation.AutomationElement]$Editor) {
    $pattern = $null
    if ($Editor.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        return ([Windows.Automation.ValuePattern]$pattern).Current.Value
    }
    if ($Editor.TryGetCurrentPattern([Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
        return ([Windows.Automation.TextPattern]$pattern).DocumentRange.GetText(-1)
    }
    if ($Editor.Current.Name -match '^[^:]+:\s*(.*)$') { return $Matches[1] }
    $Editor.SetFocus()
    Send-UitestKey -Key 0x41 -Modifiers @(0x11) -DelayMilliseconds 80
    Send-UitestKey -Key 0x43 -Modifiers @(0x11) -DelayMilliseconds 120
    $value = Get-Clipboard -Raw
    if ($null -ne $value) { return $value.TrimEnd("`r", "`n") }
    throw 'address editor exposes no readable value'
}

function Click-Physical([double]$X, [double]$Y) {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    if (-not [RustExplorerUitest.Native]::SetCursorPosDpiAware([int]$X, [int]$Y)) {
        throw 'DPI-aware pointer positioning failed'
    }
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
}

function Assert-KeyboardEntry([byte]$Key, [byte[]]$Modifiers, [string]$Label) {
    Send-UitestKey -Key 0x1B -DelayMilliseconds 100
    Send-UitestKey -Key $Key -Modifiers $Modifiers -DelayMilliseconds 220
    $editor = Find-AddressEditor
    Send-UitestKey -Key 0x58 -Modifiers @(0x10) -DelayMilliseconds 150
    $value = Get-EditorValue $editor
    if ($value -ine 'X') { throw "$Label did not focus and select the complete path: '$value'" }
    Send-UitestKey -Key 0x1B -DelayMilliseconds 180
    [void](Find-AddressSurface)
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 20, 20, 1440, 880, 0x0040)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $window = $context.Root.Current.BoundingRectangle

    $surface = Find-AddressSurface
    $bounds = $surface.Current.BoundingRectangle
    Click-Physical -X ($bounds.Right - 16) -Y ($bounds.Top + $bounds.Height / 2)
    $editor = Find-AddressEditor
    $initialValue = Get-EditorValue $editor
    if ([string]::IsNullOrWhiteSpace($initialValue) -or -not $initialValue.Contains('fixture')) {
        throw "pointer entry did not expose the complete fixture path: '$initialValue'"
    }
    Send-UitestKey -Key 0x58 -Modifiers @(0x10) -DelayMilliseconds 150
    $pointerValue = Get-EditorValue (Find-AddressEditor)
    if ($pointerValue -ine 'X') {
        throw "address editor did not survive the pointer release and receive selected text input: '$pointerValue'"
    }
    Send-UitestKey -Key 0x1B -DelayMilliseconds 180
    [void](Find-AddressSurface)

    Assert-KeyboardEntry -Key 0x4C -Modifiers @(0x11) -Label 'Ctrl+L'
    Assert-KeyboardEntry -Key 0x44 -Modifiers @(0x12) -Label 'Alt+D'

    Send-UitestKey -Key 0x4C -Modifiers @(0x11) -DelayMilliseconds 220
    $editor = Find-AddressEditor
    $validPath = Get-EditorValue $editor
    Send-UitestKey -Key 0x0D -DelayMilliseconds 700
    [void](Find-AddressSurface)

    $dateHeader = Find-UitestElement -Root $context.Root -Description 'Date modified header' -Predicate {
        param($element) $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $element.Current.Name -like 'Sort by Date modified*'
    }
    $typeHeader = Find-UitestElement -Root $context.Root -Description 'Type header' -Predicate {
        param($element) $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $element.Current.Name -like 'Sort by Type*'
    }
    $dateBefore = $dateHeader.Current.BoundingRectangle.Left
    $typeBefore = $typeHeader.Current.BoundingRectangle.Left
    $from = $dateHeader.Current.BoundingRectangle
    $to = $typeHeader.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware([int]($from.Left + $from.Width / 2), [int]($from.Top + $from.Height / 2))
    [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($step in 1..10) {
        $x = $from.Left + $from.Width / 2 + (($to.Right - ($from.Left + $from.Width / 2)) * $step / 10.0)
        [void][RustExplorerUitest.Native]::SetCursorPosDpiAware([int]$x, [int]($to.Top + $to.Height / 2))
        Start-Sleep -Milliseconds 35
    }
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware([int]($window.Right + 24), [int]($window.Bottom + 24))
    [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 450
    $dateAfter = (Find-UitestElement -Root $context.Root -Description 'Date header after outside release' -Predicate {
        param($element) $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $element.Current.Name -like 'Sort by Date modified*'
    }).Current.BoundingRectangle.Left
    $typeAfter = (Find-UitestElement -Root $context.Root -Description 'Type header after outside release' -Predicate {
        param($element) $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $element.Current.Name -like 'Sort by Type*'
    }).Current.BoundingRectangle.Left
    if ([Math]::Abs($dateAfter - $dateBefore) -gt 2 -or [Math]::Abs($typeAfter - $typeBefore) -gt 2) {
        throw "outside release committed preview order: date=$dateBefore->$dateAfter type=$typeBefore->$typeAfter"
    }

    $screenshot = Join-Path $output 'address-edit-lifecycle.png'
    Save-UitestScreenshot -Root $context.Root -Path $screenshot
    [ordered]@{
        schema = 'superexplorer.address-edit-lifecycle.v1'
        status = 'passed'
        genuine_pointer_entry_survived_release = $true
        initial_complete_path = $initialValue
        escape_restored_breadcrumb = $true
        ctrl_l_selected_complete_path = $true
        alt_d_selected_complete_path = $true
        enter_submitted_valid_path = $validPath
        outside_drag_release_canceled_preview = $true
        screenshot = 'address-edit-lifecycle.png'
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}
