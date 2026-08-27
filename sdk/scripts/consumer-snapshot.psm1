Set-StrictMode -Version Latest

function Get-ConsumerFileSha256 {
    param([Parameter(Mandatory)][string]$Path)
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
        $stream.Dispose()
    }
}

# Windows PowerShell can run with module auto-loading disabled by the installer
# process boundary. Provide the narrow Get-FileHash shape used by the SDK
# wrappers so their integrity checks do not depend on that ambient setting.
function Get-FileHash {
    param(
        [Parameter(Mandatory)][string]$LiteralPath,
        [ValidateSet('SHA256')][string]$Algorithm = 'SHA256'
    )
    [pscustomobject]@{ Hash = (Get-ConsumerFileSha256 $LiteralPath); Algorithm = $Algorithm; Path = $LiteralPath }
}

function Get-BoundedConsumerFiles {
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [int]$MaxDepth = 32,
        [int]$MaxFiles = 10000,
        [Int64]$MaxTotalBytes = 536870912,
        [Int64]$MaxFileBytes = 67108864,
        [switch]$IncludeBuildOutputs
    )
    if (-not (Test-Path -LiteralPath $SourceRoot -PathType Container)) { throw 'consumer snapshot root is not a directory' }
    $rootInfo = Get-Item -LiteralPath $SourceRoot -Force
    if ($rootInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'consumer snapshot root is a symlink, junction, or reparse point' }
    $source = [IO.Path]::GetFullPath($rootInfo.FullName).TrimEnd('\', '/')
    $pending = [System.Collections.Generic.Queue[object]]::new()
    $pending.Enqueue([pscustomobject]@{ Path = $source; Relative = ''; Depth = 0 })
    $files = [System.Collections.Generic.List[object]]::new()
    [Int64]$totalBytes = 0

    while ($pending.Count -gt 0) {
        $next = $pending.Dequeue()
        foreach ($item in @(Get-ChildItem -LiteralPath $next.Path -Force)) {
            $relative = if ([string]::IsNullOrEmpty($next.Relative)) { [string]$item.Name } else { "$($next.Relative)/$($item.Name)" }
            # Consumer outputs are deliberately not source inputs. Only root
            # target/dist are excluded; nested source directories remain visible.
            if (-not $IncludeBuildOutputs -and $relative -match '^(target|dist)(/|$)') { continue }
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'consumer snapshot contains a symlink, junction, or reparse point' }
            if ($item.PSIsContainer) {
                if ($next.Depth -ge $MaxDepth) { throw 'consumer snapshot exceeds maximum depth' }
                $pending.Enqueue([pscustomobject]@{ Path = $item.FullName; Relative = $relative; Depth = $next.Depth + 1 })
                continue
            }
            if ($item.Length -gt $MaxFileBytes) { throw 'consumer snapshot contains a file exceeding the byte limit' }
            $totalBytes += [Int64]$item.Length
            if ($totalBytes -gt $MaxTotalBytes) { throw 'consumer snapshot exceeds the total byte limit' }
            if ($files.Count -ge $MaxFiles) { throw 'consumer snapshot exceeds the file count limit' }
            $files.Add([pscustomobject]@{ FullName = $item.FullName; Relative = $relative.Replace('\', '/'); Length = [Int64]$item.Length })
        }
    }
    return @($files | Sort-Object -Property Relative -CaseSensitive)
}

function Get-BoundedConsumerTreeDigest {
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [int]$MaxDepth = 32,
        [int]$MaxFiles = 10000,
        [Int64]$MaxTotalBytes = 536870912,
        [Int64]$MaxFileBytes = 67108864,
        [switch]$IncludeBuildOutputs
    )
    $files = @(Get-BoundedConsumerFiles @PSBoundParameters)
    $lines = foreach ($file in $files) {
        $hash = Get-ConsumerFileSha256 $file.FullName
        "$($file.Relative):$($file.Length):$hash"
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($lines -join "`n"))))).Replace('-', '').ToLowerInvariant()
    } finally { $sha.Dispose() }
}

function Copy-BoundedConsumerSnapshot {
    param(
        [Parameter(Mandatory)][string]$SourceRoot,
        [Parameter(Mandatory)][string]$DestinationRoot,
        [int]$MaxDepth = 32,
        [int]$MaxFiles = 10000,
        [Int64]$MaxTotalBytes = 536870912,
        [Int64]$MaxFileBytes = 67108864,
        [switch]$IncludeBuildOutputs
    )
    $copyParameters = @{
        SourceRoot = $SourceRoot; MaxDepth = $MaxDepth; MaxFiles = $MaxFiles
        MaxTotalBytes = $MaxTotalBytes; MaxFileBytes = $MaxFileBytes; IncludeBuildOutputs = $IncludeBuildOutputs
    }
    $files = @(Get-BoundedConsumerFiles @copyParameters)
    if (Test-Path -LiteralPath $DestinationRoot) { throw 'consumer snapshot destination already exists' }
    New-Item -ItemType Directory -Path $DestinationRoot -ErrorAction Stop | Out-Null
    foreach ($file in $files) {
        $sourceInfo = Get-Item -LiteralPath $file.FullName -Force
        if (($sourceInfo.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $sourceInfo.PSIsContainer -or $sourceInfo.Length -ne $file.Length) { throw 'consumer source changed while creating the bounded snapshot' }
        $destination = Join-Path $DestinationRoot $file.Relative.Replace('/', '\')
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($destination))) -Force | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $destination -ErrorAction Stop
        $sourceHash = Get-ConsumerFileSha256 $file.FullName
        $destinationHash = Get-ConsumerFileSha256 $destination
        if ($sourceHash -ne $destinationHash) { throw 'consumer source changed while creating the bounded snapshot' }
    }
    return $files.Count
}

Export-ModuleMember -Function Copy-BoundedConsumerSnapshot, Get-BoundedConsumerTreeDigest, Get-FileHash
