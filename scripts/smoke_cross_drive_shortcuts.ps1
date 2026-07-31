param(
    [switch]$SkipBuild,
    [string]$OutputDirectory,
    [int]$TimeoutSeconds = 45
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$targetRoot = Join-Path $workspaceRoot 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $targetRoot ('shortcut-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

if (-not $SkipBuild) {
    cargo build -p explorer-app --locked
    if ($LASTEXITCODE -ne 0) { throw "build failed: $LASTEXITCODE" }
}
$executable = Join-Path $targetRoot 'debug\SuperExplorer.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "missing app: $executable" }

$runId = 'explorer-uitest-' + [guid]::NewGuid().ToString('N')
$cFixtureParent = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Temp\RustGpuiExplorerUITest'
$dFixtureParent = Join-Path $workspaceRoot 'target\uitest-drive-fixtures'
$cFixture = Join-Path $cFixtureParent $runId
$dFixture = Join-Path $dFixtureParent $runId

function Assert-OwnedPath([string]$Path, [string]$Parent) {
    $fullPath = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $fullParent = [IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
    if (-not $fullPath.StartsWith($fullParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing non-owned fixture path: $fullPath"
    }
}
Assert-OwnedPath $cFixture $cFixtureParent
Assert-OwnedPath $dFixture $dFixtureParent

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
if (-not ('ExplorerShortcut.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace ExplorerShortcut {
    public static class Native {
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
        [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
        [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);
        [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    }
}
'@
}

function Send-Key([byte]$Key, [byte[]]$Modifiers = @()) {
    foreach ($modifier in $Modifiers) { [ExplorerShortcut.Native]::keybd_event($modifier, 0, 0, [UIntPtr]::Zero) }
    [ExplorerShortcut.Native]::keybd_event($Key, 0, 0, [UIntPtr]::Zero)
    [ExplorerShortcut.Native]::keybd_event($Key, 0, 2, [UIntPtr]::Zero)
    for ($index = $Modifiers.Count - 1; $index -ge 0; $index--) {
        [ExplorerShortcut.Native]::keybd_event($Modifiers[$index], 0, 2, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 180
}

function Send-Text([string]$Text) {
    foreach ($character in $Text.ToCharArray()) {
        $shift = $false
        if ([char]::IsLetter($character)) {
            $key = [byte][char]::ToUpperInvariant($character)
            $shift = [char]::IsUpper($character)
        } elseif ([char]::IsDigit($character)) {
            $key = [byte]$character
        } else {
            switch ($character) {
                ':' { $key = 0xBA; $shift = $true }
                '\' { $key = 0xDC }
                '-' { $key = 0xBD }
                '.' { $key = 0xBE }
                '_' { $key = 0xBD; $shift = $true }
                default { throw "unsupported synthetic text character: $character" }
            }
        }
        if ($shift) { [ExplorerShortcut.Native]::keybd_event(0x10, 0, 0, [UIntPtr]::Zero) }
        [ExplorerShortcut.Native]::keybd_event($key, 0, 0, [UIntPtr]::Zero)
        [ExplorerShortcut.Native]::keybd_event($key, 0, 2, [UIntPtr]::Zero)
        if ($shift) { [ExplorerShortcut.Native]::keybd_event(0x10, 0, 2, [UIntPtr]::Zero) }
        Start-Sleep -Milliseconds 12
    }
}

function Paste-Text([string]$Text) {
    $lastError = $null
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        try {
            [Windows.Forms.Clipboard]::SetText($Text)
            $lastError = $null
            break
        } catch {
            $lastError = $_
            Start-Sleep -Milliseconds 50
        }
    }
    if ($null -ne $lastError) { throw $lastError }
    Start-Sleep -Milliseconds 250
    Send-Key 0x56 @(0x11)
}

function Find-Element([Windows.Automation.AutomationElement]$Root, [scriptblock]$Predicate, [string]$Description, [int]$Seconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
            try { if (& $Predicate $element) { return $element } } catch { }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UIA element not found: $Description"
}

function Find-Row([Windows.Automation.AutomationElement]$Root, [string]$Name, [int]$Seconds = 10) {
    Find-Element $Root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem -and
            $element.Current.Name -like "*$Name*" -and
            $element.Current.BoundingRectangle.Left -gt 300
    } "file row '$Name'" $Seconds
}

function Click-Element([Windows.Automation.AutomationElement]$Element, [switch]$Double, [switch]$Shift) {
    $bounds = $Element.Current.BoundingRectangle
    [void][ExplorerShortcut.Native]::SetCursorPos([int]($bounds.Left + [Math]::Min(120, $bounds.Width / 2)), [int]($bounds.Top + $bounds.Height / 2))
    if ($Shift) { [ExplorerShortcut.Native]::keybd_event(0x10, 0, 0, [UIntPtr]::Zero) }
    $count = if ($Double) { 2 } else { 1 }
    for ($index = 0; $index -lt $count; $index++) {
        [ExplorerShortcut.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ExplorerShortcut.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 70
    }
    if ($Shift) { [ExplorerShortcut.Native]::keybd_event(0x10, 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 220
}

function Open-AddressEditorByClick([Windows.Automation.AutomationElement]$Root) {
    $address = Find-Element $Root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Document -and
            $element.Current.Name -like 'Address: *' -and
            $element.Current.BoundingRectangle.Top -lt 180
    } 'browsing address field'
    $bounds = $address.Current.BoundingRectangle
    [void][ExplorerShortcut.Native]::SetCursorPos(
        [int]($bounds.Right - 12),
        [int]($bounds.Top + $bounds.Height / 2)
    )
    [ExplorerShortcut.Native]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    [ExplorerShortcut.Native]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 250
    Find-Element $Root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $element.Current.BoundingRectangle.Top -lt 180 -and
            $element.Current.BoundingRectangle.Left -lt 1400
    } 'clicked address editor'
}

function Read-EditorValue([Windows.Automation.AutomationElement]$Editor) {
    [void][ExplorerShortcut.Native]::SetForegroundWindow($process.MainWindowHandle)
    $Editor.SetFocus()
    $sentinel = 'uitest-address-' + [guid]::NewGuid().ToString('N')
    $clipboardReady = $false
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        try {
            [Windows.Forms.Clipboard]::SetText($sentinel)
            $clipboardReady = $true
            break
        } catch {
            Start-Sleep -Milliseconds 50
        }
    }
    if (-not $clipboardReady) { throw 'clipboard stayed busy before address copy' }
    Send-Key 0x41 @(0x11)
    Send-Key 0x43 @(0x11)
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        try { $value = [Windows.Forms.Clipboard]::GetText() } catch { $value = '' }
        if ($value -and $value -ne $sentinel) { return $value }
        Start-Sleep -Milliseconds 50
    }
    throw 'address editor did not copy its selected text to the clipboard'
}

function Wait-Path([string]$Path, [bool]$Exists = $true, [int]$Seconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if ((Test-Path -LiteralPath $Path) -eq $Exists) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $siblings = if (Test-Path -LiteralPath (Split-Path -Parent $Path)) {
        @(Get-ChildItem -LiteralPath (Split-Path -Parent $Path) -Force | Select-Object -ExpandProperty Name) -join ', '
    } else { '<parent missing>' }
    throw "filesystem oracle failed: expected exists=$Exists path=$Path siblings=[$siblings]"
}

function Set-Address([Windows.Automation.AutomationElement]$Root, [string]$Path, [string]$ExpectedRow) {
    $lastError = $null
    for ($attempt = 0; $attempt -lt 3; $attempt++) {
        try {
            [void][ExplorerShortcut.Native]::SetForegroundWindow($process.MainWindowHandle)
            Send-Key 0x1B
            Send-Key 0x4C @(0x11)
            $editor = Find-Element $Root {
                param($element)
                $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
                    $element.Current.BoundingRectangle.Top -lt 180 -and
                    $element.Current.BoundingRectangle.Left -lt 1400
            } 'address editor' 3
            Send-Key 0x41 @(0x11)
            Paste-Text $Path
            Send-Key 0x0D
            Start-Sleep -Milliseconds 700
            if ($ExpectedRow) { Find-Row $Root $ExpectedRow 3 | Out-Null }
            return
        } catch {
            $lastError = $_
            Start-Sleep -Milliseconds 250
        }
    }
    throw $lastError
}

function Selected-RowCount([Windows.Automation.AutomationElement]$Root) {
    $count = 0
    foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
        if ($element.Current.ControlType -ne [Windows.Automation.ControlType]::ListItem -or $element.Current.BoundingRectangle.Left -le 300) { continue }
        $pattern = $null
        if ($element.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
            if (([Windows.Automation.SelectionItemPattern]$pattern).Current.IsSelected) { $count++ }
        }
    }
    return $count
}

$process = $null
$recycledPath = $null
try {
    New-Item -ItemType Directory -Force -Path $cFixture, $dFixture | Out-Null
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $cFixture 'copy-source.txt') -Value 'clipboard-cross-drive'
    Set-Content -Encoding utf8 -LiteralPath (Join-Path $cFixture 'rename-source.txt') -Value 'rename'
    foreach ($name in @('range-01.txt','range-02.txt','range-03.txt','delete-me.txt')) {
        Set-Content -Encoding utf8 -LiteralPath (Join-Path $cFixture $name) -Value $name
    }
    New-Item -ItemType Directory -Path (Join-Path $dFixture 'backspace-child') | Out-Null

    $start = [Diagnostics.ProcessStartInfo]::new($executable)
    $start.WorkingDirectory = $workspaceRoot
    $start.UseShellExecute = $false
    $start.Environment['LOCALAPPDATA'] = (Join-Path $OutputDirectory 'localappdata')
    $start.Environment['EXPLORER_INITIAL_PATH'] = $cFixture
    $start.Environment['EXPLORER_LOG_DIR'] = $OutputDirectory
    $process = [Diagnostics.Process]::Start($start)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do { $process.Refresh(); Start-Sleep -Milliseconds 100 } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'application window did not appear' }
    [void][ExplorerShortcut.Native]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 20, 20, 1400, 860, 0x0040)
    [void][ExplorerShortcut.Native]::SetForegroundWindow($process.MainWindowHandle)
    Start-Sleep -Milliseconds 900
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)

    # A filesystem-backed Shell shortcut must become a portable full path when the address field
    # is clicked. Submitting that copied value must navigate through the ordinary path pipeline.
    $expectedDocuments = [Environment]::GetFolderPath('MyDocuments')
    $navigationButtons = @($root.FindAll(
        [Windows.Automation.TreeScope]::Descendants,
        [Windows.Automation.Condition]::TrueCondition
    ) | Where-Object {
        $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
        $_.Current.BoundingRectangle.Left -lt 320
    })
    $librariesNavigation = $navigationButtons | Where-Object {
        $_.Current.Name -eq 'Libraries'
    } | Select-Object -First 1
    $documentsNavigation = if ($null -eq $librariesNavigation) {
        $null
    } else {
        $navigationButtons | Where-Object {
            $_.Current.BoundingRectangle.Top -lt $librariesNavigation.Current.BoundingRectangle.Top
        } | Sort-Object { $_.Current.BoundingRectangle.Top } | Select-Object -Last 4 | Select-Object -First 1
    }
    if ($null -eq $documentsNavigation) {
        $candidates = @($navigationButtons | ForEach-Object {
            "name='$($_.Current.Name)' id='$($_.Current.AutomationId)' top=$([int]$_.Current.BoundingRectangle.Top)"
        }) -join '; '
        throw "Documents navigation item not found; candidates: $candidates"
    }
    Click-Element $documentsNavigation
    Find-Element $root {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Document -and
            $element.Current.Name.Equals("Address: $expectedDocuments", [StringComparison]::OrdinalIgnoreCase)
    } 'canonical Documents browsing address' | Out-Null
    $documentsEditor = Open-AddressEditorByClick $root
    $documentsAddress = Read-EditorValue $documentsEditor
    if ($documentsAddress.StartsWith('shell:', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Documents address leaked its Shell alias: $documentsAddress"
    }
    if (-not [IO.Path]::IsPathRooted($documentsAddress) -or -not (Test-Path -LiteralPath $documentsAddress -PathType Container)) {
        throw "Documents address is not a complete filesystem path: $documentsAddress"
    }
    if ($expectedDocuments -and -not $documentsAddress.Equals($expectedDocuments, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Documents address mismatch: actual=$documentsAddress expected=$expectedDocuments"
    }
    Send-Key 0x0D
    Start-Sleep -Milliseconds 700
    $resubmittedDocumentsAddress = Read-EditorValue (Open-AddressEditorByClick $root)
    if (-not $resubmittedDocumentsAddress.Equals($documentsAddress, [StringComparison]::OrdinalIgnoreCase)) {
        throw "resubmitted Documents path changed: before=$documentsAddress after=$resubmittedDocumentsAddress"
    }
    Set-Address $root $cFixture 'copy-source.txt'

    # F2 edits a real C: item and Enter commits it to disk.
    Click-Element (Find-Row $root 'rename-source.txt')
    Send-Key 0x71
    $renameEditor = Find-Element $root {
        param($element) $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and $element.Current.Name -like 'Rename*'
    } 'F2 rename editor'
    [void][ExplorerShortcut.Native]::SetForegroundWindow($process.MainWindowHandle)
    Send-Key 0x41 @(0x11)
    Paste-Text 'renamed-on-c.txt'
    Send-Key 0x0D
    Wait-Path (Join-Path $cFixture 'renamed-on-c.txt')

    # Shift-click follows visible order; Ctrl+A selects every visible item.
    Click-Element (Find-Row $root 'range-01.txt')
    Click-Element (Find-Row $root 'range-03.txt') -Shift
    $shiftCount = Selected-RowCount $root
    if ($shiftCount -lt 3) { throw "Shift range selected only $shiftCount rows" }
    Send-Key 0x41 @(0x11)
    $ctrlACount = Selected-RowCount $root
    if ($ctrlACount -lt 6) { throw "Ctrl+A selected only $ctrlACount rows" }

    # Build C: -> D: history before placing files on the clipboard, so address text never
    # overwrites the real CF_HDROP/OLE data object under test.
    Set-Address $root $dFixture 'backspace-child'
    Click-Element (Find-Row $root 'backspace-child')
    Send-Key 0x25 @(0x12) # Alt+Left returns to C:.
    Start-Sleep -Milliseconds 700
    Click-Element (Find-Row $root 'copy-source.txt')
    Send-Key 0x43 @(0x11)
    Send-Key 0x27 @(0x12) # Alt+Right returns to D: without touching the clipboard.
    Start-Sleep -Milliseconds 700
    Click-Element (Find-Row $root 'backspace-child')
    Send-Key 0x56 @(0x11)
    Wait-Path (Join-Path $dFixture 'copy-source.txt') $true 15

    # Enter opens a child; Backspace returns to its parent.
    Click-Element (Find-Row $root 'backspace-child')
    Send-Key 0x0D
    Start-Sleep -Milliseconds 500
    Send-Key 0x08
    Find-Row $root 'backspace-child' 10 | Out-Null

    # F2 must still work after changing drives.
    Click-Element (Find-Row $root 'copy-source.txt')
    Send-Key 0x71
    $secondEditor = Find-Element $root {
        param($element) $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and $element.Current.Name -like 'Rename*'
    } 'D drive F2 rename editor'
    [void][ExplorerShortcut.Native]::SetForegroundWindow($process.MainWindowHandle)
    Send-Key 0x41 @(0x11)
    Paste-Text 'renamed-on-d.txt'
    Send-Key 0x0D
    Wait-Path (Join-Path $dFixture 'renamed-on-d.txt')

    # Delete uses the app's Recycle Delete command and must remove only the owned fixture item.
    Click-Element (Find-Row $root 'renamed-on-d.txt')
    Set-Address $root $cFixture 'delete-me.txt'
    Click-Element (Find-Row $root 'delete-me.txt')
    $recycledPath = Join-Path $cFixture 'delete-me.txt'
    Send-Key 0x2E
    Wait-Path $recycledPath $false 15

    # Remaining Explorer bindings must be accepted without losing file-view operation.
    Send-Key 0x74 # F5
    Send-Key 0x54 @(0x11) # Ctrl+T
    Send-Key 0x57 @(0x11) # Ctrl+W
    Send-Key 0x46 @(0x11) # Ctrl+F
    $searchEditor = Find-Element $root {
        param($element) $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and $element.Current.BoundingRectangle.Left -gt 800
    } 'Ctrl+F search editor'

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        c_fixture = $cFixture
        d_fixture = $dFixture
        oracles = [ordered]@{
            f2_rename_c = $true
            shift_range_selected = $shiftCount
            ctrl_a_selected = $ctrlACount
            ctrl_c_v_cross_drive = (Test-Path -LiteralPath (Join-Path $dFixture 'renamed-on-d.txt'))
            enter_and_backspace = $true
            f2_rename_d_after_switch = $true
            delete_removed_owned_item = (-not (Test-Path -LiteralPath $recycledPath))
            f5_ctrl_t_ctrl_w_ctrl_f = $true
            documents_click_full_path = $documentsAddress
            documents_full_path_resubmitted = $resubmittedDocumentsAddress
        }
        cleanup_scope = @($cFixture, $dFixture)
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Cross-drive shortcut smoke passed: $OutputDirectory"
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        [void][ExplorerShortcut.Native]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    if ($null -ne $process) { $process.Dispose() }
    foreach ($fixture in @($cFixture, $dFixture)) {
        $parent = if ($fixture -eq $cFixture) { $cFixtureParent } else { $dFixtureParent }
        Assert-OwnedPath $fixture $parent
        if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
    }
}
