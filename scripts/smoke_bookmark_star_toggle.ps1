param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestHeadful.psm1') -Force
Initialize-UitestHeadful

$output = [IO.Path]::GetFullPath($OutputDirectory)
$fixture = Join-Path $output 'bookmark-star-fixture'
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $fixture 'Folder bookmark') | Out-Null
New-Item -ItemType File -Force -Path (Join-Path $fixture 'File bookmark.txt') | Out-Null
$context = $null

function Find-ByName([string]$Name) {
    Find-UitestElement -Root $context.Root -Description $Name -Predicate {
        param($element)
        $element.Current.Name -eq $Name
    }
}

function Save-BookmarkScreenshot([string]$Name) {
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-1), 20, 20, 1440, 880, 0x0040)
    [void][RustExplorerUitest.Native]::SetForegroundWindow($context.Hwnd)
    $context.Root.SetFocus()
    Start-Sleep -Milliseconds 200
    Save-UitestScreenshot -Root $context.Root -Path (Join-Path $output $Name)
    [void][RustExplorerUitest.Native]::SetWindowPos($context.Hwnd, [IntPtr](-2), 20, 20, 1440, 880, 0x0040)
}

function Assert-StarToggle {
    $add = Find-ByName 'Add current folder to bookmarks'
    Save-BookmarkScreenshot 'bookmark-star-off.png'

    Invoke-UitestClick -Element $add
    $remove = Find-ByName 'Remove current folder from bookmarks'
    if ($remove.Current.BoundingRectangle.Height -lt 24) {
        throw "Bookmark star is not visually prominent: height=$($remove.Current.BoundingRectangle.Height)"
    }
    Save-BookmarkScreenshot 'bookmark-star-on.png'

    $item = Find-UitestFileItem -Root $context.Root -Name 'File bookmark.txt'
    Invoke-UitestClick -Element $item
    [void](Find-ByName 'Remove current folder from bookmarks')
    Save-BookmarkScreenshot 'bookmark-star-selected.png'

    Invoke-UitestClick -Element $remove
    [void](Find-ByName 'Add current folder to bookmarks')
    Invoke-UitestClick -Element (Find-ByName 'Add current folder to bookmarks')
    [void](Find-ByName 'Remove current folder from bookmarks')
}

try {
    $context = Start-UitestExplorer -InitialPath $fixture -OutputDirectory $output -Profile $Profile -SkipBuild:$SkipBuild
    Assert-StarToggle
    [ordered]@{
        schema = 'bookmark-star-toggle-uitest-v1'
        status = 'PASS'
        verified_target = $fixture
        selection_independent = $true
        sequence = 'add-remove-add'
        minimum_star_height = 24
        artifacts = @('bookmark-star-off.png', 'bookmark-star-on.png')
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
}
finally {
    if ($null -ne $context) {
        Stop-UitestExplorer -Context $context
    }
}

Write-Output "Bookmark star toggle UITEST passed: $OutputDirectory"
