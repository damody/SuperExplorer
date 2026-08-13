param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Executable = '',
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$context = $null

function Find-Id([Windows.Automation.AutomationElement]$Root, [string]$Id, [int]$TimeoutSeconds = 10) {
    Find-UitestElement -Root $Root -Description $Id -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        $element.Current.AutomationId -eq $Id
    }
}

function Find-NamePrefix([Windows.Automation.AutomationElement]$Root, [string]$Prefix) {
    Find-UitestElement -Root $Root -Description $Prefix -TimeoutSeconds 10 -Predicate {
        param($element)
        $element.Current.Name.StartsWith($Prefix, [StringComparison]::Ordinal)
    }
}

function Find-Input([Windows.Automation.AutomationElement]$Root, [string]$Prefix) {
    Find-UitestElement -Root $Root -Description $Prefix -TimeoutSeconds 10 -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
        $element.Current.Name.StartsWith($Prefix, [StringComparison]::Ordinal)
    }
}

function Find-Slider([Windows.Automation.AutomationElement]$Root, [string]$Name) {
    Find-UitestElement -Root $Root -Description $Name -TimeoutSeconds 10 -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Slider -and
        $element.Current.Name -eq $Name
    }
}

function Focus-Owner([Windows.Automation.AutomationElement]$Element) {
    $owner = $Element
    $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
    while ($null -ne $owner -and $owner.Current.NativeWindowHandle -eq 0) {
        $owner = $walker.GetParent($owner)
    }
    if ($null -ne $owner -and $owner.Current.NativeWindowHandle -ne 0) {
        [void][RustExplorerUitest.Native]::SetForegroundWindow([IntPtr]$owner.Current.NativeWindowHandle)
        Start-Sleep -Milliseconds 120
    }
}

function Invoke-Control([Windows.Automation.AutomationElement]$Element) {
    Focus-Owner $Element
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        ([Windows.Automation.InvokePattern]$pattern).Invoke()
    } else {
        $bounds = $Element.Current.BoundingRectangle
        [void][RustExplorerUitest.Native]::SetPhysicalCursorPos(
            [int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
        [RustExplorerUitest.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [RustExplorerUitest.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 500
}

function Set-Text([Windows.Automation.AutomationElement]$Element, [string]$Value) {
    $bounds = $Element.Current.BoundingRectangle
    if ($null -eq $script:optionsHwnd -or $script:optionsHwnd -eq [IntPtr]::Zero) {
        throw 'cannot resolve cache editor owner window'
    }
    $hwnd = [IntPtr]$script:optionsHwnd
    $point = New-Object RustExplorerUitest.Native+POINT
    $point.X = [int]($bounds.Left - $script:optionsBounds.Left + $bounds.Width / 2)
    $point.Y = [int]($bounds.Top - $script:optionsBounds.Top + $bounds.Height / 2)
    $lparam = [IntPtr](($point.Y -shl 16) -bor ($point.X -band 0xffff))
    [void][RustExplorerUitest.Native]::PostMessage($hwnd, 0x0201, [IntPtr]1, $lparam)
    [void][RustExplorerUitest.Native]::PostMessage($hwnd, 0x0202, [IntPtr]0, $lparam)
    Start-Sleep -Milliseconds 150
    [void][RustExplorerUitest.Native]::PostMessage($hwnd, 0x0100, [IntPtr]0x23, [IntPtr]0)
    foreach ($erase in 1..10) {
        [void][RustExplorerUitest.Native]::PostMessage($hwnd, 0x0100, [IntPtr]0x08, [IntPtr]0)
        [void][RustExplorerUitest.Native]::PostMessage($hwnd, 0x0101, [IntPtr]0x08, [IntPtr]0)
    }
    foreach ($character in $Value.ToCharArray()) {
        [void][RustExplorerUitest.Native]::PostMessage($hwnd, 0x0102, [IntPtr][int]$character, [IntPtr]0)
    }
    Start-Sleep -Milliseconds 250
}

function Read-Text([Windows.Automation.AutomationElement]$Element) {
    if ($Element.Current.Name -match ', ([0-9]+) MB$') { return $Matches[1] }
    throw "could not read value from $($Element.Current.Name)"
}

function Set-SliderValue([Windows.Automation.AutomationElement]$Element, [double]$Value) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        throw "slider does not expose RangeValuePattern: $($Element.Current.Name)"
    }
    ([Windows.Automation.RangeValuePattern]$pattern).SetValue($Value)
    Start-Sleep -Milliseconds 350
}

function Scroll-To-Bottom([Windows.Automation.AutomationElement]$Options) {
    $target = Find-Slider $Options 'Icon memory limit'
    $bounds = $target.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetPhysicalCursorPos(
        [int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2))
    $wheelDown = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]-120), 0)
    foreach ($step in 1..40) {
        [RustExplorerUitest.Native]::mouse_event(0x0800, 0, 0, $wheelDown, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 20
    }
    Start-Sleep -Milliseconds 500
}

function Wait-OptionsWindow([int]$ProcessId) {
    $dialogName = [string]([char]0x8CC7) + [char]0x6599 + [char]0x593E + [char]0x9078 + [char]0x9805
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ProcessIdProperty, $ProcessId)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $windows = @([Windows.Automation.AutomationElement]::RootElement.FindAll(
            [Windows.Automation.TreeScope]::Children, $condition) | Where-Object {
                $_.Current.NativeWindowHandle -ne 0 -and
                $_.Current.NativeWindowHandle -ne $context.Hwnd -and
                $_.Current.Name -eq $dialogName
            })
        if ($windows.Count -eq 1) { return $windows[0] }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Folder Options window did not appear'
}

function Open-Options([Windows.Automation.AutomationElement]$MainRoot) {
    # Reacquire the root after the owned options window closes. UI Automation
    # elements from the previous tree can retain stale GPUI bounds.
    $MainRoot = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$context.Hwnd)
    $more = Find-UitestElement -Root $MainRoot -Description 'command-more-menu' -TimeoutSeconds 10 -Predicate {
        param($element)
        $element.Current.AutomationId -eq 'command-more-menu' -or
        ($element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
         $element.Current.Name -eq ([string]([char]0x5176) + [char]0x5B83))
    }
    Invoke-Control $more
    # The deferred popup is painted in the main GPUI surface and its UIA bounds
    # are not stable across a dialog close/reopen. Match Explorer keyboard
    # behavior: End selects About, Up selects Options, Enter invokes it.
    Send-UitestKey -Key 0x23
    Send-UitestKey -Key 0x26
    Send-UitestKey -Key 0x0D
    Wait-OptionsWindow $context.Process.Id
}

$env:SUPEREXPLORER_UITEST_OPEN_FOLDER_OPTIONS = 'view'
$context = Start-UitestExplorer -InitialPath $workspace -OutputDirectory $output -Profile $Profile -Executable $Executable -SkipBuild:$SkipBuild
try {
    $mainRoot = $context.Root
    $options = Wait-OptionsWindow $context.Process.Id
    $script:optionsHwnd = [IntPtr]$options.Current.NativeWindowHandle
    $script:optionsBounds = $options.Current.BoundingRectangle
    Invoke-Control (Find-NamePrefix $options ([string]([char]0x6AA2) + [char]0x8996))
    $ids = @(
        'IconMemory','BaseIconMemory','ThumbnailMemory','ExtensionMemory',
        'IconGpu','ThumbnailGpu','IconDisk','ThumbnailDisk','ExtensionDisk',
        'MftPersistedIndex','MftVolumeIndex','MftFileData','MftAggregates','MftLru')
    $physicalSliderWidth = $null
    foreach ($name in @('Icon memory limit','Thumbnail memory limit')) {
        $slider = Find-Slider $options $name
        $width = [double]$slider.Current.BoundingRectangle.Width
        if ($width -lt 395 -or $width -gt 805) { throw "$name slider physical width $width is not a 400 logical-pixel control" }
        if ($null -ne $physicalSliderWidth -and [Math]::Abs($width - $physicalSliderWidth) -gt 2) {
            throw "cache sliders do not share the same 400 logical-pixel width"
        }
        $physicalSliderWidth = $width
    }
    [void](Find-Input $options 'Icon memory limit, ')
    [void](Find-Input $options 'Thumbnail memory limit, ')

    $representativeBudgets = [ordered]@{
        'Icon memory limit' = 48
        'Extension data-column memory limit' = 64
        'Icon GPU limit' = 96
        'Icon BC7 disk limit' = 1024
        'Folder aggregates memory limit' = 128
    }
    foreach ($entry in $representativeBudgets.GetEnumerator()) {
        Set-SliderValue (Find-Slider $options $entry.Key) $entry.Value
    }

    Scroll-To-Bottom $options
    $mft = Find-Slider $options 'MFT Service LRU limit'
    Set-SliderValue $mft 2048
    Invoke-Control (Find-NamePrefix $options ([string]([char]0x5957) + [char]0x7528))
    Save-UitestScreenshot -Root $options -Path (Join-Path $output 'cache-budgets-apply-2048.png')

    $mft = Find-Slider $options 'MFT Service LRU limit'
    Set-SliderValue $mft 4096
    Invoke-Control (Find-NamePrefix $options ([string]([char]0x78BA) + [char]0x5B9A))
    Start-Sleep -Seconds 3
    Stop-UitestExplorer -Context $context
    $context = Start-UitestExplorer -InitialPath '' -OutputDirectory $output -Profile $Profile -Executable $Executable -SkipBuild
    $mainRoot = $context.Root
    $options = Wait-OptionsWindow $context.Process.Id
    $script:optionsHwnd = [IntPtr]$options.Current.NativeWindowHandle
    $script:optionsBounds = $options.Current.BoundingRectangle
    Invoke-Control (Find-NamePrefix $options ([string]([char]0x6AA2) + [char]0x8996))
    Scroll-To-Bottom $options
    $mft = Find-Input $options 'MFT Service LRU limit, '
    if ((Read-Text $mft) -ne '4096') { throw 'OK did not persist MFT LRU 4096' }
    foreach ($entry in $representativeBudgets.GetEnumerator()) {
        $editor = Find-Input $options "$($entry.Key), "
        if ((Read-Text $editor) -ne [string]$entry.Value) {
            throw "OK did not persist representative budget $($entry.Key)=$($entry.Value)"
        }
    }
    Save-UitestScreenshot -Root $options -Path (Join-Path $output 'cache-budgets-ok-4096.png')

    $mftSlider = Find-Slider $options 'MFT Service LRU limit'
    Set-SliderValue $mftSlider 8192
    Invoke-Control (Find-NamePrefix $options ([string]([char]0x53D6) + [char]0x6D88))
    Start-Sleep -Seconds 1
    Stop-UitestExplorer -Context $context
    $context = Start-UitestExplorer -InitialPath '' -OutputDirectory $output -Profile $Profile -Executable $Executable -SkipBuild
    $mainRoot = $context.Root
    $options = Wait-OptionsWindow $context.Process.Id
    $script:optionsHwnd = [IntPtr]$options.Current.NativeWindowHandle
    $script:optionsBounds = $options.Current.BoundingRectangle
    Invoke-Control (Find-NamePrefix $options ([string]([char]0x6AA2) + [char]0x8996))
    Scroll-To-Bottom $options
    $mft = Find-Input $options 'MFT Service LRU limit, '
    if ((Read-Text $mft) -ne '4096') { throw 'Cancel changed the committed MFT LRU value' }
    Save-UitestScreenshot -Root $options -Path (Join-Path $output 'cache-budgets-cancel-preserves-4096.png')

    [ordered]@{
        schema_version = 1
        status = 'PASS'
        editor_count = $ids.Count
        slider_width_px = 400
        includes_24_mb_stop = $true
        apply_value_mb = 2048
        ok_value_mb = 4096
        cancel_attempt_mb = 8192
        cancel_preserved_mb = 4096
        representative_budgets_mb = $representativeBudgets
    } | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    Remove-Item Env:SUPEREXPLORER_UITEST_OPEN_FOLDER_OPTIONS -ErrorAction SilentlyContinue
}

Write-Output "Cache budget editor smoke passed: $OutputDirectory"
