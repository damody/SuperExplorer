param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'safe-mode-fixture'
$stateRoot = Join-Path $output 'localappdata\RustGpuiExplorer\extensions\v1'
$markerRoot = Join-Path $stateRoot 'state\native-call-markers-v1'
$probePath = Join-Path $stateRoot 'safe-mode-probe-v1.json'
$context = $null

function Write-RecoveredMarker([string]$Namespace, [string]$PackageId, [string]$MarkerId) {
    $directory = Join-Path $markerRoot $Namespace
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    [IO.File]::WriteAllText(
        (Join-Path $directory 'owner.lease'),
        'v1',
        [Text.UTF8Encoding]::new($false))
    $marker = [ordered]@{
        schema_version = 1
        package_id = $PackageId
        sealed_manifest_digest = ('a' * 64)
        entrypoint_id = 'uitest.entrypoint'
        root_module_id = 'root-contract-v1'
        primary_interface_namespace = 1397811201
        primary_interface_value = 41
        operation = 'registrar'
    } | ConvertTo-Json -Compress
    $path = Join-Path $directory $MarkerId
    [IO.File]::WriteAllText($path, $marker, [Text.UTF8Encoding]::new($false))
    return $path
}

function Find-SafeModeDialog {
    Find-UitestElement -Root $context.Root -Description 'Safe Mode confirmation dialog' -Predicate {
        param($element)
        $element.Current.Name -like 'Safe Mode confirmation required; Suspect package: safe.mode.*' -and
            $element.Current.BoundingRectangle.Width -gt 0 -and
            $element.Current.BoundingRectangle.Height -gt 0
    }
}

function Find-SafeModeConfirmButton {
    Find-UitestElement -Root $context.Root -Description 'Safe Mode UIA confirmation button' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq 'Confirm and re-enable' -and
            $element.Current.BoundingRectangle.Width -gt 0
    }
}

function Get-VisibleSafeModePackage {
    foreach ($element in $context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition)) {
        if ($element.Current.BoundingRectangle.Width -gt 0 -and
            $element.Current.Name -like 'Safe Mode confirmation required; Suspect package: safe.mode.*') {
            return $element.Current.Name.Substring('Safe Mode confirmation required; Suspect package: '.Length)
        }
    }
    throw 'Safe Mode dialog did not expose its suspect package identity through UIA'
}

function Wait-SafeModePackage([string]$ExpectedPackage) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        foreach ($element in $context.Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition)) {
            if ($element.Current.BoundingRectangle.Width -gt 0 -and
                $element.Current.Name -eq "Safe Mode confirmation required; Suspect package: $ExpectedPackage") {
                return
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Safe Mode dialog did not converge to exact package identity: $ExpectedPackage"
}

function Wait-PathState([string]$Path, [bool]$Exists, [int]$TimeoutSeconds = 8) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ((Test-Path -LiteralPath $Path -PathType Leaf) -eq $Exists) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "path state did not converge ($Exists): $Path"
}

function Wait-DialogHidden {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $visible = $false
        foreach ($element in $context.Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition)) {
            if ($element.Current.Name -like 'Safe Mode confirmation required; Suspect package: safe.mode.*' -and
                $element.Current.BoundingRectangle.Width -gt 0) {
                $visible = $true
                break
            }
        }
        if (-not $visible) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Safe Mode dialog remained visible after exact UIA confirmation'
}

function Invoke-Uia([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        throw 'Safe Mode confirmation button does not expose InvokePattern'
    }
    ([Windows.Automation.InvokePattern]$pattern).Invoke()
}

try {
    New-Item -ItemType Directory -Force -Path $fixture, $markerRoot | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'safe-mode.txt'), 'headful fixture', [Text.UTF8Encoding]::new($false))
    $alphaMarker = Write-RecoveredMarker `
        -Namespace 'launch-0123456789abcdef0123456789abcdef' `
        -PackageId 'safe.mode.alpha' `
        -MarkerId 'marker-0000000000000001.json'
    $betaMarker = Write-RecoveredMarker `
        -Namespace 'launch-fedcba9876543210fedcba9876543210' `
        -PackageId 'safe.mode.beta' `
        -MarkerId 'marker-0000000000000002.json'

    $context = Start-UitestExplorer `
        -InitialPath $fixture `
        -OutputDirectory $output `
        -Profile $Profile `
        -SkipBuild:$SkipBuild `
        -CargoFeatures @('uitest-support') `
        -AdditionalEnvironment @{ EXPLORER_UITEST_EXTENSION_STATE_ROOT = $stateRoot }

    Wait-PathState -Path $probePath -Exists $true
    $probe = Get-Content -LiteralPath $probePath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($probe.schema_version -ne 1 -or -not $probe.recovered_callback_denied) {
        throw 'startup probe proves native callbacks were not denied before confirmation'
    }

    Find-SafeModeDialog | Out-Null
    @($context.Root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
            "type=$($_.Current.ControlType.ProgrammaticName) id=$($_.Current.AutomationId) name=$($_.Current.Name)"
        }) | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'safe-mode-uia-before.txt')
    $firstPackage = Get-VisibleSafeModePackage
    if ($firstPackage -notin @('safe.mode.alpha', 'safe.mode.beta')) {
        throw "unexpected Safe Mode suspect identity: $firstPackage"
    }
    $firstMarker = if ($firstPackage -eq 'safe.mode.alpha') { $alphaMarker } else { $betaMarker }
    $secondPackage = if ($firstPackage -eq 'safe.mode.alpha') { 'safe.mode.beta' } else { 'safe.mode.alpha' }
    $secondMarker = if ($firstPackage -eq 'safe.mode.alpha') { $betaMarker } else { $alphaMarker }
    if (-not (Test-Path -LiteralPath $firstMarker) -or -not (Test-Path -LiteralPath $secondMarker)) {
        throw 'a recovered marker was removed before explicit confirmation'
    }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'safe-mode-before-confirm.png')

    # Keyboard confirmation must remove only the UI-presented incident. The
    # second recovered marker remains denied and is immediately presented next.
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Send-UitestKey -Key 0x0D -DelayMilliseconds 400
    Wait-PathState -Path $firstMarker -Exists $false
    Wait-PathState -Path $secondMarker -Exists $true
    Wait-SafeModePackage -ExpectedPackage $secondPackage
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'safe-mode-after-keyboard-confirm.png')

    # The replacement offer is confirmed via the actual UIA InvokePattern.
    Invoke-Uia (Find-SafeModeConfirmButton)
    Wait-PathState -Path $secondMarker -Exists $false
    Wait-DialogHidden
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'safe-mode-after-uia-confirm.png')

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        localappdata_isolated = $true
        recovered_packages = @('safe.mode.alpha', 'safe.mode.beta')
        first_presented_package = $firstPackage
        keyboard_confirmed_exact_incident = $true
        uia_confirmed_exact_incident = $true
        native_callbacks_denied_before_confirmation = $true
        second_incident_remained_after_first_confirmation = $true
        visible_path_free_suspect_identity = $true
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
}

Write-Output "Extension Safe Mode headful smoke passed: $output"
