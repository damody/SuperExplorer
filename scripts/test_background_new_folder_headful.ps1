param(
    [string]$Executable = 'D:\SuperExplorer\target\debug\SuperExplorer.exe',
    [string]$OutputDirectory = 'D:\SuperExplorer\build\background-new-folder'
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $OutputDirectory ('fixture-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $fixture -Force
$context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $OutputDirectory `
    -Executable $Executable -SkipBuild -AdditionalEnvironment @{ EXPLORER_AUTO_CLOSE_MS = '60000' }
try {
    $bounds = $context.Root.Current.BoundingRectangle
    [void][RustExplorerUitest.Native]::SetCursorPosDpiAware([int]($bounds.Left + 600), [int]($bounds.Top + 430))
    [RustExplorerUitest.Native]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
    [RustExplorerUitest.Native]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    Send-UitestKey -Key 0x57
    Start-Sleep -Milliseconds 400
    Send-UitestKey -Key 0x46
    $editor = Find-UitestElement -Root $context.Root -Description 'focused inline rename editor' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $element.Current.HasKeyboardFocus -and $element.Current.BoundingRectangle.Top -gt ($bounds.Top + 175)
    }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'rename-active.png')
    # Typing directly must replace the selected default name, without another F2 or Ctrl+A.
    [Windows.Forms.SendKeys]::SendWait('created-with-w-f')
    Send-UitestKey -Key 0x0D
    $created = Join-Path $fixture 'created-with-w-f'
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $created -PathType Container) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (-not (Test-Path -LiteralPath $created -PathType Container)) { throw 'Renamed folder was not created.' }
    $null = Find-UitestFileItem -Root $context.Root -Name 'created-with-w-f'
    if (@(Get-ChildItem -LiteralPath $fixture).Count -ne 1) { throw 'New command created duplicate folders.' }
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'created.png')
    [ordered]@{ status = 'PASS'; gesture = 'Background right-click, W, F'; focused_rename = $true;
        default_name_selected = $true; created_folder = $created; executable = $Executable } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding utf8
} catch {
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'failed.png')
    throw
} finally {
    Stop-UitestExplorer -Context $context
}
