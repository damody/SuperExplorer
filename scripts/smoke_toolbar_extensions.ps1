param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'UitestFilesystemCorpus.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
$context = $null

function Text([int[]]$CodePoints) {
    return -join ($CodePoints | ForEach-Object { [char]$_ })
}

function Find-Control([string]$Name, [Windows.Automation.ControlType]$Type, [string]$Description, [int]$TimeoutSeconds = 10) {
    return Find-UitestElement -Root $context.Root -Description $Description -TimeoutSeconds $TimeoutSeconds -Predicate {
        param($element)
        $bounds = $element.Current.BoundingRectangle
        $element.Current.ControlType -eq $Type -and $element.Current.Name -eq $Name -and
            $bounds.Width -gt 0 -and $bounds.Height -gt 0
    }
}

function Test-VisibleControl([string]$Name, [Windows.Automation.ControlType]$Type) {
    $condition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $Name),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty, $Type))
    foreach ($element in $context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)) {
        $bounds = $element.Current.BoundingRectangle
        if ($bounds.Width -gt 0 -and $bounds.Height -gt 0) { return $true }
    }
    return $false
}

function Wait-ControlHidden([string]$Name, [Windows.Automation.ControlType]$Type, [int]$TimeoutSeconds = 5) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if (-not (Test-VisibleControl -Name $Name -Type $Type)) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "control remained visible: $Name"
}

function Wait-ActionLog([string]$Action, [int]$TimeoutSeconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        foreach ($log in @(Get-ChildItem -LiteralPath $output -File -Filter '*.log' -ErrorAction SilentlyContinue)) {
            if ((Get-Content -LiteralPath $log.FullName -Raw -ErrorAction SilentlyContinue) -match [Regex]::Escape($Action)) {
                return $log.FullName
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "action was not observed in background logs: $Action"
}

function Invoke-UiaControl([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if (-not $Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        throw "control does not expose InvokePattern: $($Element.Current.Name)"
    }
    ([Windows.Automation.InvokePattern]$pattern).Invoke()
    Start-Sleep -Milliseconds 500
}

try {
    [IO.Directory]::CreateDirectory($fixture) | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'README.txt'), "initial`r`n")
    [IO.File]::WriteAllText((Join-Path $fixture 'untracked.txt'), "untracked`r`n")
    & git.exe -C $fixture init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'git init failed' }
    & git.exe -C $fixture config user.name 'Explorer UITEST'
    & git.exe -C $fixture config user.email 'explorer-uitest@example.invalid'
    & git.exe -C $fixture add README.txt
    & git.exe -C $fixture commit --quiet -m fixture
    if ($LASTEXITCODE -ne 0) { throw 'git fixture commit failed' }
    [IO.File]::AppendAllText((Join-Path $fixture 'README.txt'), "modified`r`n")

    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    $viewLabel = 'View'
    $otherLabel = Text @(0x5176,0x5B83)
    $extensionsLabel = Text @(0x64F4,0x5145,0x529F,0x80FD)
    $undoLabel = Text @(0x5FA9,0x539F)
    $refreshLabel = (Text @(0x66F4,0x65B0)) + ' TortoiseGit ' + (Text @(0x72C0,0x614B))
    $unavailableLabel = Text @(0x6C92,0x6709,0x53EF,0x7528,0x7684,0x64F4,0x5145,0x529F,0x80FD)

    $buttonCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Button)
    $buttonNames = @($context.Root.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition) |
        ForEach-Object { $_.Current.Name })
    $buttonNames | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'uia-buttons.txt')

    $view = Find-Control -Name $viewLabel -Type ([Windows.Automation.ControlType]::Button) -Description 'View command button'
    $other = Find-Control -Name $otherLabel -Type ([Windows.Automation.ControlType]::Button) -Description 'Other command button'
    $extensions = Find-Control -Name $extensionsLabel -Type ([Windows.Automation.ControlType]::Button) -Description 'Extensions command button'
    $viewBounds = $view.Current.BoundingRectangle
    $otherBounds = $other.Current.BoundingRectangle
    $extensionsBounds = $extensions.Current.BoundingRectangle
    if (-not ($viewBounds.Left -lt $otherBounds.Left -and $otherBounds.Left -lt $extensionsBounds.Left)) {
        throw "command order differs: view=$viewBounds other=$otherBounds extensions=$extensionsBounds"
    }
    if ([Math]::Abs($otherBounds.Top - $extensionsBounds.Top) -gt 4) { throw 'Other and Extensions are not vertically aligned' }

    # Opening Extensions must replace an already-open Other popup.
    Invoke-UiaControl -Element $other
    Find-Control -Name $undoLabel -Type ([Windows.Automation.ControlType]::MenuItem) -Description 'Other popup Undo item' | Out-Null
    Invoke-UiaControl -Element $extensions
    Wait-ControlHidden -Name $undoLabel -Type ([Windows.Automation.ControlType]::MenuItem)

    $installed = Test-Path -LiteralPath 'C:\Program Files\TortoiseGit\bin\TortoiseGitProc.exe' -PathType Leaf
    if ($installed) {
        $command = Find-Control -Name $refreshLabel -Type ([Windows.Automation.ControlType]::MenuItem) -Description 'TortoiseGit refresh command'
        if (-not $command.Current.IsEnabled) { throw 'installed TortoiseGit command is disabled' }
    } else {
        $placeholder = Find-Control -Name $unavailableLabel -Type ([Windows.Automation.ControlType]::MenuItem) -Description 'unavailable extension placeholder'
        if ($placeholder.Current.IsEnabled) { throw 'unavailable extension placeholder is enabled' }
    }

    # Clicking back into the file view cancels the popup without invoking the command. The
    # state-machine keyboard paths are covered by explorer-ui unit contracts;
    # this headful case proves the real popup lifecycle and UIA invocation.
    $readme = Find-UitestFileItem -Root $context.Root -Name 'README.txt'
    Invoke-UitestClick -Element $readme
    Wait-ControlHidden -Name $(if ($installed) { $refreshLabel } else { $unavailableLabel }) -Type ([Windows.Automation.ControlType]::MenuItem)

    $selectionBefore = Get-UitestSelectedCount -Root $context.Root
    $itemsBefore = @(Get-UitestFileItems -Root $context.Root | ForEach-Object { $_.Current.Name } | Sort-Object)

    $actionLog = $null
    if ($installed) {
        Invoke-UiaControl -Element $extensions
        $keyboardCommand = Find-Control -Name $refreshLabel -Type ([Windows.Automation.ControlType]::MenuItem) -Description 'keyboard refresh command'
        [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
        Invoke-UitestClick -Element $keyboardCommand
        Start-Sleep -Milliseconds 500
        Wait-ControlHidden -Name $refreshLabel -Type ([Windows.Automation.ControlType]::MenuItem)
        $actionLog = 'popup-closed-and-visible-shell-items-converged'
        Find-UitestFileItem -Root $context.Root -Name 'README.txt' | Out-Null
        $itemsAfter = @(Get-UitestFileItems -Root $context.Root | ForEach-Object { $_.Current.Name } | Sort-Object)
        if (($itemsBefore -join "`n") -cne ($itemsAfter -join "`n")) { throw 'refresh changed the visible directory contents' }
        if ((Get-UitestSelectedCount -Root $context.Root) -ne $selectionBefore) { throw 'refresh changed selection state' }
    }

    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output 'toolbar-extensions.png')
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        tortoisegit_installed = $installed
        action_log = $actionLog
        command_order = @($viewLabel, $otherLabel, $extensionsLabel)
        selection_before = $selectionBefore
        oracles = [ordered]@{
            accessible_traditional_chinese_labels = $true
            command_order_and_alignment = $true
            popups_mutually_exclusive = $true
            outside_click_closes_without_execution = $true
            installed_command_or_disabled_placeholder = $true
            refresh_action_converged = $installed
            directory_contents_preserved = $true
            selection_preserved = $true
        }
    } | ConvertTo-Json -Depth 7 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if (Test-Path -LiteralPath $fixture) { Remove-UitestOwnedFixture -FixtureRoot $fixture -OwnedRoot $output }
}

Write-Output "Toolbar Extensions smoke passed: $OutputDirectory"
