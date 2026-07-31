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
New-Item -ItemType Directory -Force -Path $output | Out-Null
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureParent = [IO.Path]::GetPathRoot($workspaceRoot)
$fixtureRoot = Join-Path $fixtureParent ('bgs-' + [guid]::NewGuid().ToString('N').Substring(0, 10))
$fixture = Join-Path $fixtureRoot 'git-overlay-fixture'
$plain = Join-Path $fixtureRoot 'plain-folder'
$context = $null

function Find-SearchHint([string]$FolderName) {
    Find-UitestElement -Root $context.Root -Description "search hint for $FolderName" -TimeoutSeconds 10 -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Edit -and
            $element.Current.Name -like "* $FolderName;*"
    }
}

function Find-BreadcrumbSegment([string]$FolderName) {
    Find-UitestElement -Root $context.Root -Description "breadcrumb segment for $FolderName" -TimeoutSeconds 10 -Predicate {
        param($element)
        $element.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and
            $element.Current.Name -eq "Go to $FolderName"
    }
}

function Get-BreadcrumbIconHash(
    [Windows.Automation.AutomationElement]$Segment,
    [string]$ScreenshotPath
) {
    Save-UitestScreenshot -Root $context.Root -Path $ScreenshotPath
    $window = $context.Root.Current.BoundingRectangle
    $bounds = $Segment.Current.BoundingRectangle
    $bitmap = [Drawing.Bitmap]::FromFile($ScreenshotPath)
    try {
        $left = [Math]::Max(0, [int][Math]::Round($bounds.Left - $window.Left + 2))
        $top = [Math]::Max(0, [int][Math]::Round($bounds.Top - $window.Top + 2))
        $width = [Math]::Min(32, $bitmap.Width - $left)
        $height = [Math]::Min([int][Math]::Round($bounds.Height - 4), $bitmap.Height - $top)
        if ($width -le 0 -or $height -le 0) { throw "invalid breadcrumb icon crop: $bounds" }
        $bytes = [Collections.Generic.List[byte]]::new($width * $height * 3)
        for ($y = $top; $y -lt ($top + $height); $y++) {
            for ($x = $left; $x -lt ($left + $width); $x++) {
                $color = $bitmap.GetPixel($x, $y)
                $bytes.Add($color.R)
                $bytes.Add($color.G)
                $bytes.Add($color.B)
            }
        }
        $sha = [Security.Cryptography.SHA256]::Create()
        try { return (([BitConverter]::ToString($sha.ComputeHash($bytes.ToArray()))) -replace '-', '').ToLowerInvariant() }
        finally { $sha.Dispose() }
    } finally {
        $bitmap.Dispose()
    }
}

try {
    $auditDirectory = Join-Path $output 'overlay-audit'
    & (Join-Path $PSScriptRoot 'audit_shell_icon_overlays.ps1') -OutputDirectory $auditDirectory | Out-Null
    $audit = Get-Content -LiteralPath (Join-Path $auditDirectory 'report.json') -Encoding utf8 -Raw | ConvertFrom-Json
    if (-not ($audit.tortoise_status_availability.normal -and $audit.tortoise_status_availability.modified)) {
        throw 'TortoiseGit normal and modified handlers must both occupy active Windows overlay slots'
    }
    if (-not (Test-Path -LiteralPath 'C:\Program Files\TortoiseGit\bin\TortoiseGitProc.exe' -PathType Leaf)) {
        throw 'TortoiseGit is not installed'
    }

    New-Item -ItemType Directory -Force -Path $fixture,$plain | Out-Null
    & git -C $fixture init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'git init failed' }
    & git -C $fixture config user.email 'uitest@example.invalid'
    & git -C $fixture config user.name 'SuperExplorer UITest'
    Set-Content -LiteralPath (Join-Path $fixture 'tracked.txt') -Encoding utf8 -Value 'clean'
    & git -C $fixture add tracked.txt
    & git -C $fixture commit --quiet -m fixture
    if ($LASTEXITCODE -ne 0) { throw 'git fixture commit failed' }
    Add-Content -LiteralPath (Join-Path $fixture 'tracked.txt') -Encoding utf8 -Value 'modified'

    $context = Start-UitestExplorer -InitialPath $fixtureRoot -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    Start-Sleep -Seconds 6

    $gitName = Split-Path -Leaf $fixture
    $plainName = Split-Path -Leaf $plain
    $fixtureRootName = Split-Path -Leaf $fixtureRoot
    [void](Find-SearchHint $fixtureRootName)
    Set-UitestAddress -Context $context -Path $fixture
    [void](Find-SearchHint $gitName)
    $gitSegment = Find-BreadcrumbSegment $gitName
    $gitHash = Get-BreadcrumbIconHash $gitSegment (Join-Path $output 'git-before.png')

    Set-UitestAddress -Context $context -Path $plain
    [void](Find-SearchHint $plainName)
    $plainSegment = Find-BreadcrumbSegment $plainName
    $plainHash = Get-BreadcrumbIconHash $plainSegment (Join-Path $output 'plain-folder.png')
    if ($gitHash -eq $plainHash) {
        throw "Git breadcrumb did not expose a distinct TortoiseGit-composited icon: hash=$gitHash"
    }

    Set-UitestAddress -Context $context -Path 'C:\'
    [void](Find-SearchHint 'C:')
    Set-UitestAddress -Context $context -Path $fixture
    [void](Find-SearchHint $gitName)
    $returnedSegment = Find-BreadcrumbSegment $gitName

    $returnedHash = $null
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $returnedHash = Get-BreadcrumbIconHash $returnedSegment (Join-Path $output 'git-after.png')
        if ($returnedHash -ne $gitHash) { Start-Sleep -Milliseconds 250 }
    } while ($returnedHash -ne $gitHash -and [DateTime]::UtcNow -lt $deadline)
    if ($returnedHash -ne $gitHash) {
        throw "TortoiseGit breadcrumb overlay changed or disappeared after C:\ round-trip: before=$gitHash after=$returnedHash"
    }

    [ordered]@{
        schema_version = 1
        captured_utc = [DateTime]::UtcNow.ToString('o')
        git_fixture = $fixture
        plain_fixture = $plain
        search_hint_folders = @($gitName, $plainName, 'C:', $gitName)
        tortoise_handlers = $audit.tortoise_status_availability
        git_icon_hash_before = $gitHash
        plain_folder_icon_hash = $plainHash
        git_icon_hash_after = $returnedHash
        overlay_distinct_from_plain = $gitHash -ne $plainHash
        overlay_survived_round_trip = $returnedHash -eq $gitHash
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding utf8
} catch {
    $_ | Out-String | Set-Content -LiteralPath (Join-Path $output 'failure.txt') -Encoding utf8
    throw
} finally {
    if ($null -ne $context) { Stop-UitestExplorer -Context $context }
    if (Test-Path -LiteralPath $fixtureRoot) {
        $resolved = [IO.Path]::GetFullPath($fixtureRoot)
        $parentPrefix = [IO.Path]::GetFullPath($fixtureParent).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
        if (-not $resolved.StartsWith($parentPrefix, [StringComparison]::OrdinalIgnoreCase) -or
            -not ([IO.Path]::GetFileName($resolved)).StartsWith('bgs-', [StringComparison]::Ordinal)) {
            throw "refusing to remove unexpected breadcrumb Git fixture: $resolved"
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
