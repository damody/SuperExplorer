Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-UitestOwnedPath {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$OwnedRoot)
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $root = [IO.Path]::GetFullPath($OwnedRoot).TrimEnd('\')
    if ($full -eq $root -or -not $full.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "path is outside the UITEST-owned root: $full (root $root)"
    }
    return $full
}

function New-UitestFilesystemCorpus {
    param(
        [Parameter(Mandatory)][string]$FixtureRoot,
        [Parameter(Mandatory)][string]$OwnedRoot,
        [ValidateSet('small','full','soak')][string]$Profile = 'small',
        [int]$BulkCount = 0
    )
    $fixture = Assert-UitestOwnedPath $FixtureRoot $OwnedRoot
    New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    $workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    $lua = Join-Path $workspace 'build\tools\lua\lua.exe'
    $generator = Join-Path $PSScriptRoot 'utit_filesystem_corpus.lua'
    if (-not (Test-Path -LiteralPath $lua -PathType Leaf)) { throw "Lua runtime not found: $lua" }
    if ($BulkCount -le 0) { $BulkCount = if ($Profile -eq 'soak') { 20000 } elseif ($Profile -eq 'full') { 2000 } else { 0 } }
    & $lua $generator $fixture $Profile $BulkCount
    if ($LASTEXITCODE -ne 0) { throw "corpus generator failed with exit code $LASTEXITCODE" }

    $readonly = Join-Path $fixture '05-mutation\readonly-source.txt'
    if (Test-Path -LiteralPath $readonly) { (Get-Item -LiteralPath $readonly).IsReadOnly = $true }
    $hidden = Join-Path $fixture '08-attributes\hidden-item.txt'
    New-Item -ItemType Directory -Force -Path (Split-Path $hidden) | Out-Null
    [IO.File]::WriteAllText($hidden, 'hidden fixture')
    (Get-Item -Force -LiteralPath $hidden).Attributes = [IO.FileAttributes]::Hidden -bor [IO.FileAttributes]::Archive
    $system = Join-Path $fixture '08-attributes\system-item.txt'
    [IO.File]::WriteAllText($system, 'system fixture')
    (Get-Item -Force -LiteralPath $system).Attributes = [IO.FileAttributes]::System -bor [IO.FileAttributes]::Archive
    return $fixture
}

function Get-UitestFilesystemSnapshot {
    param([Parameter(Mandatory)][string]$Root, [switch]$SkipHashes)
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($rootFull)
    $items = [Collections.Generic.List[object]]::new()
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        foreach ($entry in [IO.Directory]::EnumerateFileSystemEntries($directory)) {
            $info = Get-Item -Force -LiteralPath $entry
            $relative = $info.FullName.Substring($rootFull.Length).TrimStart('\').Replace('\','/')
            $isDirectory = $info -is [IO.DirectoryInfo]
            $isReparse = ($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
            $hash = $null
            $length = $null
            if (-not $isDirectory) {
                $length = $info.Length
                if (-not $SkipHashes) {
                    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $info.FullName).Hash.ToLowerInvariant()
                }
            }
            $items.Add([pscustomobject][ordered]@{
                relative_path = $relative
                kind = if ($isDirectory) { if ($isReparse) { 'reparse-directory' } else { 'directory' } } else { if ($isReparse) { 'reparse-file' } else { 'file' } }
                length = $length
                sha256 = $hash
                attributes = $info.Attributes.ToString()
                modified_utc = $info.LastWriteTimeUtc.ToString('o')
            })
            if ($isDirectory -and -not $isReparse) { $pending.Push($info.FullName) }
        }
    }
    return @($items | Sort-Object relative_path)
}

function Write-UitestCorpusManifest {
    param(
        [Parameter(Mandatory)][string]$FixtureRoot,
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Profile,
        [hashtable]$Capabilities = @{},
        [switch]$SkipHashes
    )
    $items = @(Get-UitestFilesystemSnapshot $FixtureRoot -SkipHashes:$SkipHashes)
    [ordered]@{
        schema_version = 1
        generated_utc = [DateTime]::UtcNow.ToString('o')
        profile = $Profile
        fixture_root = $FixtureRoot
        item_count = $items.Count
        capabilities = $Capabilities
        items = $items
    } | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $Path
    return $items
}

function Remove-UitestOwnedFixture {
    param([Parameter(Mandatory)][string]$FixtureRoot, [Parameter(Mandatory)][string]$OwnedRoot)
    if (-not (Test-Path -LiteralPath $FixtureRoot)) { return }
    $fixture = Assert-UitestOwnedPath $FixtureRoot $OwnedRoot
    Get-ChildItem -Force -LiteralPath $fixture -Recurse -ErrorAction SilentlyContinue | ForEach-Object {
        try { $_.Attributes = [IO.FileAttributes]::Normal } catch {}
    }
    Remove-Item -LiteralPath $fixture -Recurse -Force
}

Export-ModuleMember -Function Assert-UitestOwnedPath,New-UitestFilesystemCorpus,Get-UitestFilesystemSnapshot,Write-UitestCorpusManifest,Remove-UitestOwnedFixture
