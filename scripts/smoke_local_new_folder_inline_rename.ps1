param([string]$OutputDirectory, [switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$target = Join-Path $workspace 'target'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $target ('local-new-folder-rename-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$fixture = Join-Path $target ('local-new-folder-rename-fixture-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $OutputDirectory, $fixture | Out-Null
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
$context = $null
try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $OutputDirectory -SkipBuild:$SkipBuild
    $newButton = Find-UitestElement -Root $context.Root -Description 'New command button' -Predicate {
        param($element)
        $element.Current.Name -eq 'Create a new item' -and $element.Current.BoundingRectangle.Width -gt 0
    }
    Invoke-UitestClick -Element $newButton
    $folder = Find-UitestElement -Root $context.Root -Description 'Folder new item' -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::MenuItem -and
            $element.Current.Name -eq 'Folder' -and $element.Current.BoundingRectangle.Width -gt 0
    }
    Invoke-UitestClick -Element $folder
    $rename = Find-UitestElement -Root $context.Root -Description 'new folder inline rename' -TimeoutSeconds 15 -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $element.Current.Name -eq 'Rename New folder' -and $element.Current.BoundingRectangle.Width -gt 0
    }
    $createdPath = Join-Path $fixture 'New folder'
    if (Test-Path -LiteralPath $createdPath) {
        throw "folder was created before rename commit: $createdPath"
    }
    $renameName = $rename.Current.Name
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $OutputDirectory 'local-new-folder-inline-rename.png')
    Send-UitestKey -Key 0x0D -DelayMilliseconds 700
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path -LiteralPath $createdPath) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (-not (Test-Path -LiteralPath $createdPath -PathType Container)) {
        throw "folder was not created after rename commit: $createdPath"
    }
    [ordered]@{ status='passed'; path=$fixture; inline_rename=$renameName; absent_before_commit=$true; created_after_commit=$true } |
        ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Local new-folder inline rename smoke passed: $OutputDirectory"
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    $resolved = [IO.Path]::GetFullPath($fixture)
    $allowed = [IO.Path]::GetFullPath($target).TrimEnd('\') + '\local-new-folder-rename-fixture-'
    if (-not $resolved.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) {
        throw "refusing unsafe fixture cleanup: $resolved"
    }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
