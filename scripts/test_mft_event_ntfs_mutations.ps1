[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\target\openspec-evidence\event-driven-mft-index-updates\4.2.ntfs-mutations'),
    [string]$FixtureRoot,
    [string]$DestructiveJournalVolumeRoot,
    [switch]$SkipServiceStop
)

$ErrorActionPreference = 'Stop'

function Wait-ServiceState {
    param([string]$Name, [string]$State, [int]$TimeoutSeconds = 15)
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $service = Get-Service -Name $Name -ErrorAction SilentlyContinue
        if ($null -ne $service -and [string]$service.Status -eq $State) { return $true }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    return $false
}

function Invoke-Mutation {
    param([string]$Name, [scriptblock]$Action)
    $started = [DateTime]::UtcNow
    & $Action
    [ordered]@{
        name = $Name
        started_utc = $started.ToString('o')
        completed_utc = [DateTime]::UtcNow.ToString('o')
        passed = $true
    }
}

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$reportPath = Join-Path $output 'report.json'
$createdFixture = $false
$serviceWasRunning = $false
$serviceStop = [ordered]@{ status = 'not-run'; reason = '' }
$discontinuity = [ordered]@{ status = 'skipped'; reason = 'An isolated disposable NTFS volume was not supplied.' }

try {
    if ([string]::IsNullOrWhiteSpace($FixtureRoot)) {
        $FixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("superexplorer-mft-events-{0}-{1}" -f $PID, [DateTime]::UtcNow.Ticks)
        $createdFixture = $true
    }
    $fixture = [IO.Path]::GetFullPath($FixtureRoot)
    $drive = Get-Volume -DriveLetter ([IO.Path]::GetPathRoot($fixture).Substring(0, 1)) -ErrorAction Stop
    if ($drive.FileSystemType -ne 'NTFS') {
        throw "NTFS prerequisite absent for fixture root: $fixture"
    }
    New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    $left = Join-Path $fixture 'left'
    $right = Join-Path $fixture 'right'
    New-Item -ItemType Directory -Force -Path $left, $right | Out-Null
    $file = Join-Path $left 'payload.bin'
    $renamed = Join-Path $left 'renamed.bin'
    $moved = Join-Path $right 'moved.bin'
    $link = Join-Path $right 'hard-link.bin'
    $mutations = @()
    $mutations += Invoke-Mutation 'create' { [IO.File]::WriteAllBytes($file, [byte[]](1..16)) }
    $mutations += Invoke-Mutation 'grow' { [IO.File]::WriteAllBytes($file, [byte[]](1..64)) }
    $mutations += Invoke-Mutation 'overwrite' { [IO.File]::WriteAllBytes($file, [byte[]](65..128)) }
    $mutations += Invoke-Mutation 'truncate' { [IO.File]::WriteAllBytes($file, [byte[]](1..8)) }
    $mutations += Invoke-Mutation 'rename' { Move-Item -LiteralPath $file -Destination $renamed }
    $mutations += Invoke-Mutation 'cross-parent-move' { Move-Item -LiteralPath $renamed -Destination $moved }
    $mutations += Invoke-Mutation 'hard-link' { New-Item -ItemType HardLink -Path $link -Target $moved | Out-Null }
    $mutations += Invoke-Mutation 'delete-hard-link' { Remove-Item -LiteralPath $link }
    $mutations += Invoke-Mutation 'delete' { Remove-Item -LiteralPath $moved }

    if (-not $SkipServiceStop) {
        $service = Get-Service -Name 'SuperExplorerMft' -ErrorAction SilentlyContinue
        if ($null -eq $service) {
            throw 'Installed SuperExplorerMft service prerequisite is absent.'
        }
        $serviceWasRunning = $service.Status -eq 'Running'
        if ($serviceWasRunning) {
            Stop-Service -Name 'SuperExplorerMft' -Force
            if (-not (Wait-ServiceState -Name 'SuperExplorerMft' -State 'Stopped')) {
                throw 'SuperExplorerMft did not stop within 15 seconds while its journal reader was blocked.'
            }
            Start-Service -Name 'SuperExplorerMft'
            if (-not (Wait-ServiceState -Name 'SuperExplorerMft' -State 'Running')) {
                throw 'SuperExplorerMft did not restart within 15 seconds.'
            }
            $serviceStop = [ordered]@{ status = 'passed'; reason = 'Blocked reader stopped and restarted within bounded waits.' }
        } else {
            $serviceStop = [ordered]@{ status = 'passed'; reason = 'Service was already stopped; no blocked reader existed.' }
        }
    } else {
        $serviceStop = [ordered]@{ status = 'skipped'; reason = 'Caller explicitly selected mutation-only mode.' }
    }

    if (-not [string]::IsNullOrWhiteSpace($DestructiveJournalVolumeRoot)) {
        $journalRoot = [IO.Path]::GetFullPath($DestructiveJournalVolumeRoot)
        $journalDrive = Get-Volume -DriveLetter ([IO.Path]::GetPathRoot($journalRoot).Substring(0, 1)) -ErrorAction Stop
        if ($journalDrive.FileSystemType -ne 'NTFS') {
            throw "Discontinuity volume is not NTFS: $journalRoot"
        }
        $discontinuity = [ordered]@{
            status = 'procedure-ready'
            reason = 'Disposable NTFS volume supplied. Run the service recovery test with its USN journal recreated; production volumes are never modified by this script.'
            exact_command = "fsutil usn deletejournal /d $($journalDrive.DriveLetter):"
        }
    }

    $report = [ordered]@{
        schema_version = 1
        status = 'passed'
        fixture_root = $fixture
        filesystem = $drive.FileSystemType
        mutations = $mutations
        blocked_service_stop = $serviceStop
        journal_discontinuity = $discontinuity
        completed_utc = [DateTime]::UtcNow.ToString('o')
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -LiteralPath $reportPath
    Write-Output "MFT NTFS mutation fixture PASS: $reportPath"
} catch {
    [ordered]@{
        schema_version = 1
        status = 'failed'
        error = $_.Exception.Message
        blocked_service_stop = $serviceStop
        journal_discontinuity = $discontinuity
        completed_utc = [DateTime]::UtcNow.ToString('o')
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -LiteralPath $reportPath
    throw
} finally {
    if ($serviceWasRunning -and -not (Get-Service -Name 'SuperExplorerMft' -ErrorAction SilentlyContinue).Status.Equals('Running')) {
        Start-Service -Name 'SuperExplorerMft' -ErrorAction SilentlyContinue
    }
    if ($createdFixture -and -not [string]::IsNullOrWhiteSpace($FixtureRoot) -and (Test-Path -LiteralPath $FixtureRoot)) {
        Remove-Item -LiteralPath $FixtureRoot -Recurse -Force
    }
}
