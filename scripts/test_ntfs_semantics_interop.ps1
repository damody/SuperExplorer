param(
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'UitestFilesystemCorpus.psm1') -Force

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$fixture = Join-Path $output 'fixture'
$createdReparsePoints = [Collections.Generic.List[string]]::new()
$checks = [ordered]@{}
$skips = [Collections.Generic.List[string]]::new()

function Try-Capability([string]$Name, [scriptblock]$Action) {
    try {
        & $Action
        $checks[$Name] = 'PASS'
        return $true
    } catch {
        $checks[$Name] = 'SKIP'
        $skips.Add("${Name}: $($_.Exception.Message)")
        return $false
    }
}

try {
    New-UitestFilesystemCorpus -FixtureRoot $fixture -OwnedRoot $output -Profile small | Out-Null
    $driveRoot = [IO.Path]::GetPathRoot($fixture)
    $volume = Get-Volume -DriveLetter $driveRoot.Substring(0,1) -ErrorAction Stop
    $checks['volume_filesystem'] = $volume.FileSystem
    if ($volume.FileSystem -ne 'NTFS') {
        $skips.Add("NTFS-only scenarios skipped on $($volume.FileSystem)")
    } else {
        $ntfs = Join-Path $fixture '09-ntfs'
        [IO.Directory]::CreateDirectory($ntfs) | Out-Null
        $source = Join-Path $ntfs 'hardlink-source.txt'
        [IO.File]::WriteAllText($source, 'hardlink payload')

        [void](Try-Capability 'hard_link' {
            $link = Join-Path $ntfs 'hardlink-peer.txt'
            New-Item -ItemType HardLink -Path $link -Target $source -ErrorAction Stop | Out-Null
            if ((Get-FileHash $link).Hash -ne (Get-FileHash $source).Hash) { throw 'hard-link content differs' }
            [IO.File]::AppendAllText($link, '!')
            if (-not [IO.File]::ReadAllText($source).EndsWith('!')) { throw 'hard-link mutation did not reach source' }
        })

        [void](Try-Capability 'alternate_data_stream' {
            Set-Content -LiteralPath $source -Stream 'uitest-metadata' -Value 'alternate stream payload' -Encoding Ascii -NoNewline -ErrorAction Stop
            $adsValue = Get-Content -LiteralPath $source -Stream 'uitest-metadata' -Raw -ErrorAction Stop
            if ($adsValue -ne 'alternate stream payload') { throw 'ADS round trip failed' }
            if (@([IO.Directory]::EnumerateFiles($ntfs, '*')).Count -ne 2) { throw 'ADS appeared as a normal directory entry' }
        })

        [void](Try-Capability 'junction_and_cycle_guard' {
            $target = Join-Path $ntfs 'junction-target'
            [IO.Directory]::CreateDirectory($target) | Out-Null
            [IO.File]::WriteAllText((Join-Path $target 'inside.txt'), 'junction payload')
            $junction = Join-Path $ntfs 'junction-link'
            New-Item -ItemType Junction -Path $junction -Target $target -ErrorAction Stop | Out-Null
            $createdReparsePoints.Add($junction)
            $cycle = Join-Path $target 'cycle-to-parent'
            New-Item -ItemType Junction -Path $cycle -Target $ntfs -ErrorAction Stop | Out-Null
            $createdReparsePoints.Add($cycle)
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $snapshot = @(Get-UitestFilesystemSnapshot -Root $fixture)
            $timer.Stop()
            if ($timer.Elapsed.TotalSeconds -gt 5) { throw "snapshot cycle guard took $($timer.Elapsed.TotalSeconds) seconds" }
            if (@($snapshot | Where-Object kind -eq 'reparse-directory').Count -lt 2) { throw 'reparse directories were not identified' }
        })

        [void](Try-Capability 'symbolic_link' {
            $symlink = Join-Path $ntfs 'symbolic-file.txt'
            New-Item -ItemType SymbolicLink -Path $symlink -Target $source -ErrorAction Stop | Out-Null
            $createdReparsePoints.Add($symlink)
            if ([IO.File]::ReadAllText($symlink) -ne [IO.File]::ReadAllText($source)) { throw 'symbolic-link content differs' }
        })

        [void](Try-Capability 'broken_symbolic_link' {
            $broken = Join-Path $ntfs 'broken-symbolic-file.txt'
            $missing = Join-Path $ntfs 'missing-target.txt'
            New-Item -ItemType SymbolicLink -Path $broken -Target $missing -ErrorAction Stop | Out-Null
            $createdReparsePoints.Add($broken)
            if (Test-Path -LiteralPath $missing) { throw 'broken-link target unexpectedly exists' }
        })
    }

    $items = @(Write-UitestCorpusManifest -FixtureRoot $fixture -Path (Join-Path $output 'fixture-manifest.json') -Profile 'small-ntfs' -Capabilities $checks)
    [ordered]@{
        schema_version = 1
        status = 'PASS'
        filesystem = $volume.FileSystem
        item_count = $items.Count
        checks = $checks
        skipped_subscenarios = @($skips)
        safety = [ordered]@{
            snapshot_does_not_follow_reparse_directories = $true
            cleanup_is_limited_to_fixture = $true
        }
    } | ConvertTo-Json -Depth 7 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $output 'report.json')
} finally {
    foreach ($path in @($createdReparsePoints | Sort-Object Length -Descending)) {
        try {
            if ([IO.Directory]::Exists($path)) { [IO.Directory]::Delete($path) }
            elseif ([IO.File]::Exists($path) -or (Test-Path -LiteralPath $path)) { [IO.File]::Delete($path) }
        } catch { }
    }
    if (Test-Path -LiteralPath $fixture) {
        Remove-UitestOwnedFixture -FixtureRoot $fixture -OwnedRoot $output
    }
}

Write-Output "NTFS semantics interop passed: $OutputDirectory"
