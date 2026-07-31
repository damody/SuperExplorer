$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$sourceRoot = Join-Path $workspaceRoot 'crates\explorer-ui\src'
$tokenDefinitions = @(
    (Join-Path $sourceRoot 'theme.rs'),
    (Join-Path $sourceRoot 'layout.rs')
)
$patterns = @(
    [regex]::new('\b(?:rgb|rgba)\s*\('),
    [regex]::new('\bRgba\s*\{'),
    [regex]::new('\b0x[0-9A-Fa-f]{6,8}\b'),
    [regex]::new('\.(?:w|h|min_w|min_h|max_w|max_h|p|px|py|gap)\s*\(\s*px\s*\(\s*\d')
)

$violations = foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -Recurse -Filter '*.rs' -File) {
    if ($tokenDefinitions.Contains($file.FullName)) {
        continue
    }
    $lineNumber = 0
    foreach ($line in Get-Content -Encoding utf8 -LiteralPath $file.FullName) {
        $lineNumber++
        if ($line.Contains('token-lint: allow')) {
            continue
        }
        foreach ($pattern in $patterns) {
            if ($pattern.IsMatch($line)) {
                [pscustomobject]@{
                    File = $file.FullName.Substring($workspaceRoot.Length + 1)
                    Line = $lineNumber
                    Text = $line.Trim()
                }
                break
            }
        }
    }
}

if ($violations) {
    $violations | Format-Table -AutoSize | Out-String | Write-Error
    throw 'Explorer feature UI contains raw color or primary layout literals; use theme/layout tokens.'
}

Write-Output 'UI token check passed: feature UI contains no raw color or primary layout literals.'
