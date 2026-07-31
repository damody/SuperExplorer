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
Set-Content -Encoding ascii -LiteralPath (Join-Path $fixture 'keyboard-selection-sentinel.txt') -Value 'sentinel'
$context = $null
$results = [Collections.Generic.List[object]]::new()

function Send-ShiftKey([byte]$Key, [int]$DelayMilliseconds = 90) {
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $sequence = switch ($Key) {
        0x23 { '+{END}' }
        0x24 { '+{HOME}' }
        0x25 { '+{LEFT}' }
        0x27 { '+{RIGHT}' }
        default { throw "unsupported shifted navigation key: $Key" }
    }
    [Windows.Forms.SendKeys]::SendWait($sequence)
    Start-Sleep -Milliseconds $DelayMilliseconds
}

function Get-EditorValue([Windows.Automation.AutomationElement]$Editor) {
    $pattern = $null
    if ($Editor.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        return ([Windows.Automation.ValuePattern]$pattern).Current.Value
    }
    if ($Editor.TryGetCurrentPattern([Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
        return ([Windows.Automation.TextPattern]$pattern).DocumentRange.GetText(-1)
    }
    $accessibleName = $Editor.Current.Name
    if ($accessibleName -match '^[^:]+:\s*(.*)$') { return $Matches[1] }
    foreach ($attempt in 1..8) {
        $sentinel = "__uitest_clipboard_sentinel_$attempt"
        Set-UitestClipboardText -Text $sentinel
        $Editor.SetFocus()
        Send-UitestKey -Key 0x41 -Modifiers @(0x11) -DelayMilliseconds 60
        Send-UitestKey -Key 0x43 -Modifiers @(0x11) -DelayMilliseconds 120
        $copied = Get-Clipboard -Raw
        if ($null -ne $copied) {
            $copied = $copied.TrimEnd("`r", "`n")
            if ($copied -cne $sentinel) { return $copied }
        }
        Start-Sleep -Milliseconds 80
    }
    throw "editable field exposes no readable value: name=$accessibleName"
}

function Set-EditorText(
    [Windows.Automation.AutomationElement]$Editor,
    [string]$Text,
    [string]$Label
) {
    foreach ($attempt in 1..8) {
        $Editor.SetFocus()
        Send-UitestKey -Key 0x41 -Modifiers @(0x11) -DelayMilliseconds 60
        Set-UitestClipboardText -Text $Text
        Send-UitestKey -Key 0x56 -Modifiers @(0x11) -DelayMilliseconds 140
        $actual = Get-EditorValue $Editor
        if ($actual -ceq $Text) { return }
        Start-Sleep -Milliseconds 80
    }
    throw "$Label text setup failed: expected='$Text' actual='$actual'"
}

function Assert-SelectionReplacement(
    [Windows.Automation.AutomationElement]$Editor,
    [string]$ExpectedValue,
    [string]$Label,
    [string]$Sequence
) {
    $Editor.SetFocus()
    [Windows.Forms.SendKeys]::SendWait('z') # Typed text replaces exactly the selected range.
    Start-Sleep -Milliseconds 120
    $actual = Get-EditorValue $Editor
    if ($actual -cne $ExpectedValue) {
        throw "$Label $Sequence replaced wrong range: expected='$ExpectedValue' actual='$actual'"
    }
    $results.Add([pscustomobject][ordered]@{
        editor = $Label
        sequence = $Sequence
        replacement_value = $actual
    })
}

function Test-ShiftSelection(
    [Windows.Automation.AutomationElement]$Editor,
    [string]$Text,
    [string]$Label
) {
    Set-EditorText -Editor $Editor -Text $Text -Label $Label
    Send-UitestKey -Key 0x24 -DelayMilliseconds 70 # Home
    Send-ShiftKey -Key 0x23 # Shift+End
    Assert-SelectionReplacement -Editor $Editor -ExpectedValue 'Z' -Label $Label -Sequence 'Home,Shift+End'

    Set-EditorText -Editor $Editor -Text $Text -Label $Label
    Send-UitestKey -Key 0x24 -DelayMilliseconds 70
    Send-ShiftKey -Key 0x27 # Shift+Right
    Assert-SelectionReplacement -Editor $Editor -ExpectedValue ("Z" + $Text.Substring(1)) -Label $Label -Sequence 'Home,Shift+Right'

    Set-EditorText -Editor $Editor -Text $Text -Label $Label
    Send-UitestKey -Key 0x24 -DelayMilliseconds 70
    Send-ShiftKey -Key 0x27 -DelayMilliseconds 70
    Send-ShiftKey -Key 0x27 -DelayMilliseconds 70
    Send-ShiftKey -Key 0x25 # Shift+Left contracts
    Assert-SelectionReplacement -Editor $Editor -ExpectedValue ("Z" + $Text.Substring(1)) -Label $Label -Sequence 'Shift+Right,Shift+Right,Shift+Left'

    Set-EditorText -Editor $Editor -Text $Text -Label $Label
    Send-UitestKey -Key 0x23 -DelayMilliseconds 70 # End
    Send-ShiftKey -Key 0x25 # Shift+Left
    Assert-SelectionReplacement -Editor $Editor -ExpectedValue ($Text.Substring(0, $Text.Length - 1) + 'Z') -Label $Label -Sequence 'End,Shift+Left'

    Set-EditorText -Editor $Editor -Text $Text -Label $Label
    Send-UitestKey -Key 0x23 -DelayMilliseconds 70
    Send-ShiftKey -Key 0x24 # Shift+Home
    Assert-SelectionReplacement -Editor $Editor -ExpectedValue 'Z' -Label $Label -Sequence 'End,Shift+Home'
}

function Find-TopEditor([scriptblock]$Predicate, [string]$Description) {
    Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds 8 -Predicate {
        param($element)
        if ($element.Current.ControlType -ne [Windows.Automation.ControlType]::Edit) { return $false }
        & $Predicate $element
    }
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $window = $context.Root.Current.BoundingRectangle

    $address = $null
    foreach ($attempt in 1..5) {
        Send-UitestKey -Key 0x1B -DelayMilliseconds 80
        Send-UitestKey -Key 0x4C -Modifiers @(0x11) -DelayMilliseconds 220 # Ctrl+L
        try {
            $address = Find-UitestElement -Root $context.Root -Description 'address editor after Ctrl+L' -TimeoutSeconds 2 -Predicate {
                param($element)
                $bounds = $element.Current.BoundingRectangle
                $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
                    $bounds.Top -lt ($window.Top + 180) -and $bounds.Left -lt ($window.Left + $window.Width * 0.58)
            }
            break
        } catch {
            Start-Sleep -Milliseconds 120
        }
    }
    if ($null -eq $address) { throw 'address editor did not appear after bounded Ctrl+L retries' }
    Test-ShiftSelection -Editor $address -Text 'addresskeyboard' -Label 'address'
    Send-UitestKey -Key 0x1B -DelayMilliseconds 120

    Send-UitestKey -Key 0x46 -Modifiers @(0x11) -DelayMilliseconds 220 # Ctrl+F
    $search = Find-TopEditor -Description 'search editor after Ctrl+F' -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $bounds.Top -lt ($window.Top + 180) -and $bounds.Left -gt ($window.Left + $window.Width * 0.58)
    }
    Test-ShiftSelection -Editor $search -Text 'searchkeyboard' -Label 'search'
    Send-UitestKey -Key 0x1B -DelayMilliseconds 180

    $fileItem = Find-UitestFileItem -Root $context.Root -Name 'keyboard-selection-sentinel.txt'
    Invoke-UitestClick -Element $fileItem
    Send-UitestKey -Key 0x71 -DelayMilliseconds 220 # F2
    $rename = Find-UitestElement -Root $context.Root -Description 'inline rename editor' -TimeoutSeconds 8 -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and $element.Current.Name -like 'Rename*'
    }
    Test-ShiftSelection -Editor $rename -Text 'renamekeyboard' -Label 'rename'
    Send-UitestKey -Key 0x1B -DelayMilliseconds 120

    [ordered]@{
        schema = 'superexplorer.editable-keyboard-selection.v1'
        genuine_keyboard_input = $true
        exact_replacement_oracle = $true
        editors = @('address', 'search', 'rename')
        shortcuts = @('Shift+Home', 'Shift+End', 'Shift+Left', 'Shift+Right')
        assertions = @($results)
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Editable keyboard selection smoke passed: $OutputDirectory"
