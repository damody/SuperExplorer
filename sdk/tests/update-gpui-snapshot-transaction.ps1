[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Import-Module (Join-Path $repo 'sdk\scripts\update-gpui-snapshot-support.psm1') -Force

function Invoke-GitLocal([string]$Directory, [string[]]$Arguments) {
    $output = & git -C $Directory @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git $($Arguments -join ' ') failed: $($output -join "`n")" }
    return @($output)
}

$temp = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-gpui-transaction-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    $files = @((Join-Path $temp 'approved.json'), (Join-Path $temp 'sdk-lock.json'), (Join-Path $temp 'bundle.json'), (Join-Path $temp 'fingerprint.json'))
    $before = @{}
    for ($index = 0; $index -lt $files.Count; $index++) { $bytes = [byte[]](0, $index, 127, 255); [IO.File]::WriteAllBytes($files[$index], $bytes); $before[$files[$index]] = [Convert]::ToBase64String($bytes) }
    $created = Join-Path $temp 'candidate-attestation.json'
    $failed = $false
    try {
        Invoke-WithFileTransaction @($files + $created) {
            foreach ($file in $files) { [IO.File]::WriteAllBytes($file, [byte[]](9, 8, 7)) }
            [IO.File]::WriteAllText($created, 'candidate', [Text.UTF8Encoding]::new($false))
            throw 'injected transaction failure'
        }
    } catch { $failed = $true }
    if (-not $failed) { throw 'transaction failure was swallowed' }
    foreach ($file in $files) { if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($file)) -ne $before[$file]) { throw "rollback was not byte-exact for $file" } }
    if (Test-Path -LiteralPath $created) { throw 'rollback retained created candidate file' }

    $git = Join-Path $temp 'git'
    New-Item -ItemType Directory -Path $git | Out-Null
    Invoke-GitLocal $git @('init') | Out-Null
    Invoke-GitLocal $git @('config', 'user.email', 'transaction@example.invalid') | Out-Null
    Invoke-GitLocal $git @('config', 'user.name', 'transaction') | Out-Null
    [IO.File]::WriteAllText((Join-Path $git 'tracked.txt'), 'one', [Text.UTF8Encoding]::new($false))
    Invoke-GitLocal $git @('add', '.') | Out-Null
    Invoke-GitLocal $git @('commit', '-m', 'one') | Out-Null
    Invoke-GitLocal $git @('branch', '-M', 'main') | Out-Null
    $state = Get-GpuiCheckoutState $git
    [IO.File]::WriteAllText((Join-Path $git 'tracked.txt'), 'two', [Text.UTF8Encoding]::new($false))
    Invoke-GitLocal $git @('commit', '-am', 'two') | Out-Null
    Restore-GpuiCheckoutState $git $state
    $restored = Get-GpuiCheckoutState $git
    if ($restored.head -ne $state.head -or $restored.branch -ne $state.branch -or (Get-Content -LiteralPath (Join-Path $git 'tracked.txt') -Raw) -ne 'one') { throw 'git checkout rollback was not exact' }
    Assert-GpuiRepositoryComplete $git @($state.head)
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}

$production = Get-Content -LiteralPath (Join-Path $repo 'sdk\scripts\update-gpui-snapshot.ps1') -Raw
foreach ($required in @('Assert-GpuiRepositoryComplete', 'Get-GpuiCheckoutState', 'Restore-GpuiCheckoutState', 'candidate-only pending separate protected promotion')) {
    if ($production -notlike "*$required*") { throw "production update script lost transaction guard: $required" }
}
$workflow = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\update-gpui-snapshot.yml') -Raw
foreach ($required in @('fetch --no-tags --unshallow origin main', 'GPUI repository history is incomplete', 'PROMOTION-REQUIRED.txt')) {
    if ($workflow -notlike "*$required*") { throw "snapshot workflow lost required guard: $required" }
}
$promotionWorkflow = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\promote-gpui-snapshot.yml') -Raw
if ($promotionWorkflow -notlike '*compare-and-swap*' -or $promotionWorkflow -notlike '*GPUI_SNAPSHOT_APPROVAL_HMAC_KEY*') { throw 'separate protected promotion workflow is missing' }
Write-Output 'gpui snapshot transaction contract passed'
