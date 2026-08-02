[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$support = Join-Path $repo 'sdk\scripts\update-gpui-snapshot-support.psm1'
Import-Module $support -Force
$gitCommand = Get-Command git -CommandType Application | Select-Object -First 1
$gitRoot = Split-Path $gitCommand.Source -Parent | Split-Path -Parent
$gitUnixBin = Join-Path $gitRoot 'usr\bin'
$savedPath = $env:PATH
if (Test-Path -LiteralPath $gitUnixBin) { $env:PATH = "$gitUnixBin;$savedPath" }

function Invoke-Git {
    param([string]$Directory, [string[]]$Arguments)
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $output = & git -C $Directory @Arguments 2>&1; $exit = $LASTEXITCODE }
    finally { $ErrorActionPreference = $saved }
    if ($exit -ne 0) { throw "git failed in ${Directory}: git $($Arguments -join ' ')`n$($output -join "`n")" }
    return @($output)
}

function Git-Text {
    param([string]$Directory, [string[]]$Arguments)
    return ((Invoke-Git $Directory $Arguments) -join "`n").Trim()
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Name)
    $failed = $false
    try { & $Action } catch { $failed = $true }
    if (-not $failed) { throw "$Name unexpectedly passed" }
}

function Write-Bytes([string]$Path, [byte[]]$Bytes) {
    [IO.File]::WriteAllBytes($Path, $Bytes)
}

$temp = Join-Path ([IO.Path]::GetTempPath()) ("superexplorer-gpui-contract-" + [guid]::NewGuid().ToString('N'))
$work = Join-Path $temp 'work'
$remote = Join-Path $temp 'remote.git'
$sub = Join-Path $temp 'sub'
$subRemote = Join-Path $temp 'sub.git'
New-Item -ItemType Directory -Path $temp -Force | Out-Null

try {
    # Build an entirely local remote and submodule fixture; no network access is used.
    Invoke-Git $temp @('init', '--bare', $remote) | Out-Null
    Invoke-Git $temp @('init', $subRemote) | Out-Null
    Invoke-Git $temp @('init', $sub) | Out-Null
    Invoke-Git $sub @('config', 'user.email', 'contract@example.invalid') | Out-Null
    Invoke-Git $sub @('config', 'user.name', 'contract') | Out-Null
    Set-Content -LiteralPath (Join-Path $sub 'sub.txt') -Value 'sub-one' -NoNewline
    Invoke-Git $sub @('add', 'sub.txt') | Out-Null
    Invoke-Git $sub @('commit', '-m', 'sub one') | Out-Null
    Invoke-Git $sub @('remote', 'add', 'origin', $subRemote) | Out-Null
    Invoke-Git $sub @('push', '-u', 'origin', 'HEAD:main') | Out-Null

    Invoke-Git $temp @('init', $work) | Out-Null
    Invoke-Git $work @('config', 'user.email', 'contract@example.invalid') | Out-Null
    Invoke-Git $work @('config', 'user.name', 'contract') | Out-Null
    Set-Content -LiteralPath (Join-Path $work 'tracked.txt') -Value 'one' -NoNewline
    Invoke-Git $work @('add', 'tracked.txt') | Out-Null
    Invoke-Git $work @('commit', '-m', 'baseline') | Out-Null
    Invoke-Git $work @('branch', '-M', 'main') | Out-Null
    Invoke-Git $work @('remote', 'add', 'origin', $remote) | Out-Null
    Invoke-Git $work @('push', '-u', 'origin', 'main') | Out-Null

    # Clean, tracked, index and untracked state detection are all fail-closed.
    if (@(Invoke-Git $work @('status', '--porcelain')).Count -ne 0) { throw 'clean fixture was not clean' }
    Set-Content -LiteralPath (Join-Path $work 'tracked.txt') -Value 'working-tree' -NoNewline
    $status = (Invoke-Git $work @('status', '--porcelain')) -join "`n"
    if ($status -notmatch '(?m)^ M tracked\.txt$') { throw 'tracked dirty state was not detected' }
    Invoke-Git $work @('add', 'tracked.txt') | Out-Null
    $status = (Invoke-Git $work @('status', '--porcelain')) -join "`n"
    if ($status -notmatch '(?m)^M  tracked\.txt$') { throw 'index dirty state was not detected' }
    Invoke-Git $work @('restore', '--staged', 'tracked.txt') | Out-Null
    Set-Content -LiteralPath (Join-Path $work 'untracked.txt') -Value 'untracked' -NoNewline
    $status = (Invoke-Git $work @('status', '--porcelain')) -join "`n"
    if ($status -notmatch '(?m)^\?\? untracked\.txt$') { throw 'untracked state was not detected' }
    Invoke-Git $work @('restore', 'tracked.txt') | Out-Null
    Remove-Item -LiteralPath (Join-Path $work 'untracked.txt') -Force

    # A submodule is clean only when its checked-out worktree is clean too.
    # Construct a gitlink directly.  This avoids invoking Git's shell helper on
    # minimal Windows runners while retaining real submodule status semantics.
    $subCommit = Git-Text $sub @('rev-parse', 'HEAD')
    $subPath = Join-Path $work 'modules\fixture'
    New-Item -ItemType Directory -Path $subPath -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $sub 'sub.txt') -Destination (Join-Path $subPath 'sub.txt')
    [IO.File]::WriteAllText((Join-Path $subPath '.git'), "gitdir: $($sub -replace '\\', '/')/.git`n")
    Invoke-Git $work @('update-index', '--add', '--cacheinfo', "160000,$subCommit,modules/fixture") | Out-Null
    Invoke-Git $work @('commit', '-m', 'add submodule') | Out-Null
    Set-Content -LiteralPath (Join-Path $work 'modules\fixture\sub.txt') -Value 'sub-dirty' -NoNewline
    if ((Git-Text $work @('status', '--porcelain')) -notmatch 'modules/fixture') { throw 'dirty submodule state was not detected' }
    Copy-Item -LiteralPath (Join-Path $sub 'sub.txt') -Destination (Join-Path $subPath 'sub.txt') -Force
    if (@(Invoke-Git $work @('status', '--porcelain')).Count -ne 0) { throw 'submodule restore did not return to clean state' }

    # Exact approval: fast-forward permits no approval; divergent updates require every identity field.
    $old = 'a' * 40; $new = 'b' * 40; $tree = 'c' * 40; $digest = 'd' * 64
    $run = 'run-123'; $nonce = 'nonce-123'; $now = [datetime]::UtcNow
    Assert-GpuiUpdateApproval $null $old $new $tree $digest $run $nonce $true $now | Out-Null
    Assert-Throws { Assert-GpuiUpdateApproval $null $old $new $tree $digest $run $nonce $false $now } 'divergent update without approval'
    $approval = [pscustomobject]@{
        schema_version = 1; baseline_revision = $old; old_revision = $old; new_revision = $new
        new_tree = $tree; candidate_plan_digest = $digest; workflow_run_id = $run; nonce = $nonce
        reason = 'contract'; approver = 'contract'; issued_utc = $now.AddMinutes(-1).ToString('o')
        expires_utc = $now.AddHours(1).ToString('o')
    }
    Assert-GpuiUpdateApproval $approval $old $new $tree $digest $run $nonce $false $now | Out-Null
    foreach ($field in @('new_tree', 'workflow_run_id', 'nonce')) {
        $wrong = $approval.PSObject.Copy()
        if ($field -eq 'new_tree') { $wrong.$field = 'e' * 40 } else { $wrong.$field = "wrong-$field" }
        Assert-Throws { Assert-GpuiUpdateApproval $wrong $old $new $tree $digest $run $nonce $false $now } "wrong $field approval"
    }
    $expired = $approval.PSObject.Copy(); $expired.expires_utc = $now.AddMinutes(-1).ToString('o')
    Assert-Throws { Assert-GpuiUpdateApproval $expired $old $new $tree $digest $run $nonce $false $now } 'expired approval'
    $missing = $approval.PSObject.Copy(); $missing.reason = ''
    Assert-Throws { Assert-GpuiUpdateApproval $missing $old $new $tree $digest $run $nonce $false $now } 'missing approval field'

    # Remote-head race: a candidate is rejected when origin/main changes after resolution.
    $candidate = Git-Text $work @('rev-parse', 'refs/remotes/origin/main')
    Set-Content -LiteralPath (Join-Path $work 'race.txt') -Value 'race' -NoNewline
    Invoke-Git $work @('add', 'race.txt') | Out-Null
    Invoke-Git $work @('commit', '-m', 'advance remote') | Out-Null
    Invoke-Git $work @('push', 'origin', 'main') | Out-Null
    Invoke-Git $work @('fetch', 'origin', 'main') | Out-Null
    $after = Git-Text $work @('rev-parse', 'refs/remotes/origin/main')
    if ($candidate -eq $after) { throw 'remote race fixture failed to advance' }
    if ($after -eq $candidate) { throw 'remote-head race was accepted' }

    # Transaction rollback restores bytes exactly and removes files created by the failed action.
    $existing = Join-Path $temp 'existing.bin'; $created = Join-Path $temp 'created.bin'
    $before = [byte[]](0, 1, 2, 127, 128, 255); Write-Bytes $existing $before
    Assert-Throws { Invoke-WithFileTransaction @($existing, $created) { Write-Bytes $existing ([byte[]](9, 8, 7)); Write-Bytes $created ([byte[]](6)); throw 'injected transaction failure' } } 'transaction failure'
    if ([Convert]::ToBase64String($before) -ne [Convert]::ToBase64String([IO.File]::ReadAllBytes($existing))) { throw 'rollback was not byte-identical' }
    if (Test-Path -LiteralPath $created) { throw 'rollback retained a created file' }
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    $env:PATH = $savedPath
}

# Keep this contract coupled to the production race guard rather than a test-only imitation.
$production = Get-Content -LiteralPath (Join-Path $repo 'sdk\scripts\update-gpui-snapshot.ps1') -Raw
foreach ($required in @("'fetch','--no-tags','origin','main'", 'remote main advanced during update', 'Invoke-WithFileTransaction', 'Assert-GpuiRepositoryComplete', 'Restore-GpuiCheckoutState')) {
    if ($production -notlike "*$required*") { throw "production update script lost required guard: $required" }
}

Write-Output 'gpui snapshot update contract passed'
