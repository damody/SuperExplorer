param([string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $workspaceRoot ('target\tortoise-overlay-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

$auditDirectory = Join-Path $OutputDirectory 'registry-audit'
& (Join-Path $PSScriptRoot 'audit_shell_icon_overlays.ps1') -OutputDirectory $auditDirectory
$audit = Get-Content -Raw -Encoding utf8 -LiteralPath (Join-Path $auditDirectory 'report.json') | ConvertFrom-Json
if (-not $audit.tortoise_status_availability.normal -or -not $audit.tortoise_status_availability.modified -or -not $audit.tortoise_status_availability.added) {
    throw 'TortoiseGit normal/modified/added handlers are not all inside the active Windows overlay slots'
}
if (-not (Get-Command git.exe -ErrorAction SilentlyContinue)) { throw 'git.exe is required' }
if (-not (Test-Path -LiteralPath 'C:\Program Files\TortoiseGit\bin\TortoiseGitProc.exe')) { throw 'TortoiseGit is not installed' }

$cargoLog = Join-Path $OutputDirectory 'cargo-test.log'
$previousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$output = & cargo test -p explorer-shell-win real_tortoise_git_clean_modified_and_added_overlays_are_distinct -- --ignored --nocapture 2>&1
$exitCode = $LASTEXITCODE
$ErrorActionPreference = $previousErrorActionPreference
$output | Set-Content -Encoding utf8 -LiteralPath $cargoLog
if ($exitCode -ne 0) { throw "real TortoiseGit Shell pixel test failed: $exitCode" }
$hashLine = @($output | Select-String -Pattern 'TortoiseGit Shell hashes clean=([0-9a-f]+) modified=([0-9a-f]+) added=([0-9a-f]+) unversioned=([0-9a-f]+)')[-1]
if ($null -eq $hashLine) { throw 'Shell pixel hashes were not emitted' }
$match = $hashLine.Matches[0]
$hashes = [ordered]@{
    clean = $match.Groups[1].Value
    modified = $match.Groups[2].Value
    added = $match.Groups[3].Value
    unversioned = $match.Groups[4].Value
}
if ($hashes.clean -eq $hashes.modified -or $hashes.clean -eq $hashes.added) {
    throw 'TortoiseGit overlay pixels did not differ from the clean Shell icon'
}

$cacheRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'RustGpuiExplorer\icon-cache\v1'
$cacheEntries = if (Test-Path -LiteralPath $cacheRoot) { @(Get-ChildItem -LiteralPath $cacheRoot -File -Recurse).Count } else { 0 }
if ($cacheEntries -lt 1) { throw "Shell icon disk cache was not populated: $cacheRoot" }

[ordered]@{
    schema_version = 1
    captured_utc = [DateTime]::UtcNow.ToString('o')
    tortoisegit_executable = 'C:\Program Files\TortoiseGit\bin\TortoiseGitProc.exe'
    active_handlers = $audit.tortoise_handlers
    shell_pixel_hashes = $hashes
    disk_cache_root = $cacheRoot
    disk_cache_entries = $cacheEntries
    assertions = [ordered]@{
        handlers_active = $true
        clean_modified_distinct = $true
        clean_added_distinct = $true
        persistent_cache_populated = $true
    }
} | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
Write-Output "TortoiseGit overlay smoke passed: $OutputDirectory"
