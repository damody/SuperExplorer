Set-StrictMode -Version Latest

$script:ZipUtf8Flag = [uint16]0x0800
$script:ZipStoredMethod = [uint16]0
$script:ZipVersion = [uint16]20
$script:ZipDosTime = [uint16]0
$script:ZipDosDate = [uint16]33 # 1980-01-01

if ($null -eq ('SuperExplorerCanonicalCrc32' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
public static class SuperExplorerCanonicalCrc32 {
    public static uint Compute(byte[] bytes) {
        uint crc = 0xffffffffu;
        foreach (byte value in bytes) {
            crc ^= value;
            for (var bit = 0; bit < 8; bit++) {
                crc = (crc & 1u) != 0 ? (crc >> 1) ^ 0xedb88320u : crc >> 1;
            }
        }
        return ~crc;
    }
}
'@
}

function Write-UInt16Le([IO.BinaryWriter]$Writer, [uint32]$Value) {
    if ($Value -gt [uint16]::MaxValue) { throw 'canonical ZIP field exceeds u16' }
    $Writer.Write([uint16]$Value)
}

function Write-UInt32Le([IO.BinaryWriter]$Writer, [uint64]$Value) {
    if ($Value -gt [uint32]::MaxValue) { throw 'canonical ZIP field exceeds u32 / ZIP64 is forbidden' }
    $Writer.Write([uint32]$Value)
}

function Get-Crc32([byte[]]$Bytes) {
    return [SuperExplorerCanonicalCrc32]::Compute($Bytes)
}

function Get-CanonicalZipCrc32([byte[]]$Bytes) {
    return Get-Crc32 $Bytes
}

function Read-UInt16Le([byte[]]$Bytes, [int]$Offset) {
    if ($Offset -lt 0 -or $Offset + 2 -gt $Bytes.Length) { throw 'canonical ZIP is truncated' }
    return [uint16](([uint16]$Bytes[$Offset]) -bor (([uint16]$Bytes[($Offset + 1)]) -shl 8))
}

function Read-UInt32Le([byte[]]$Bytes, [int]$Offset) {
    if ($Offset -lt 0 -or $Offset + 4 -gt $Bytes.Length) { throw 'canonical ZIP is truncated' }
    return [uint32]( ([uint64]$Bytes[$Offset]) -bor (([uint64]$Bytes[($Offset + 1)]) -shl 8) -bor (([uint64]$Bytes[($Offset + 2)]) -shl 16) -bor (([uint64]$Bytes[($Offset + 3)]) -shl 24) )
}

function Assert-CanonicalStoreOnlyZip([string]$Path, [string[]]$ExpectedNames) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($ExpectedNames.Count -eq 0 -or $ExpectedNames.Count -gt [uint16]::MaxValue) { throw 'canonical ZIP has an invalid entry count' }
    $encoding = [Text.UTF8Encoding]::new($false, $true)
    $locals = @{}
    $offset = 0
    foreach ($expectedName in $ExpectedNames) {
        if ((Read-UInt32Le $bytes $offset) -ne 0x04034b50) { throw 'canonical ZIP local header is missing' }
        if ((Read-UInt16Le $bytes ($offset + 4)) -ne $script:ZipVersion -or
            (Read-UInt16Le $bytes ($offset + 6)) -ne $script:ZipUtf8Flag -or
            (Read-UInt16Le $bytes ($offset + 8)) -ne $script:ZipStoredMethod -or
            (Read-UInt16Le $bytes ($offset + 10)) -ne $script:ZipDosTime -or
            (Read-UInt16Le $bytes ($offset + 12)) -ne $script:ZipDosDate) { throw 'canonical ZIP local header is not fixed store-only UTF-8 form' }
        $crc = Read-UInt32Le $bytes ($offset + 14)
        $compressedSize = Read-UInt32Le $bytes ($offset + 18)
        $uncompressedSize = Read-UInt32Le $bytes ($offset + 22)
        $nameLength = [int](Read-UInt16Le $bytes ($offset + 26))
        $extraLength = [int](Read-UInt16Le $bytes ($offset + 28))
        if ($compressedSize -ne $uncompressedSize -or $extraLength -ne 0) { throw 'canonical ZIP local entry has compression or extra data' }
        $nameStart = $offset + 30
        $dataStart = $nameStart + $nameLength
        $dataEnd = $dataStart + [int64]$uncompressedSize
        if ($dataEnd -gt $bytes.Length) { throw 'canonical ZIP local entry data is truncated' }
        $name = $encoding.GetString($bytes, $nameStart, $nameLength)
        if ($name -cne $expectedName) {
            throw "canonical ZIP local entry order or name is invalid (expected '$expectedName', found '$name')"
        }
        $payload = [byte[]]::new([int]$uncompressedSize)
        [Array]::Copy($bytes, $dataStart, $payload, 0, $payload.Length)
        if ((Get-Crc32 $payload) -ne $crc) { throw 'canonical ZIP local entry CRC32 is invalid' }
        $locals[$name] = [pscustomobject]@{ Offset = [uint32]$offset; Crc32 = $crc; Size = $uncompressedSize }
        $offset = [int]$dataEnd
    }
    $centralOffset = $offset
    foreach ($expectedName in $ExpectedNames) {
        if ((Read-UInt32Le $bytes $offset) -ne 0x02014b50) { throw 'canonical ZIP central directory is missing' }
        if ((Read-UInt16Le $bytes ($offset + 4)) -ne $script:ZipVersion -or
            (Read-UInt16Le $bytes ($offset + 6)) -ne $script:ZipVersion -or
            (Read-UInt16Le $bytes ($offset + 8)) -ne $script:ZipUtf8Flag -or
            (Read-UInt16Le $bytes ($offset + 10)) -ne $script:ZipStoredMethod -or
            (Read-UInt16Le $bytes ($offset + 12)) -ne $script:ZipDosTime -or
            (Read-UInt16Le $bytes ($offset + 14)) -ne $script:ZipDosDate) { throw 'canonical ZIP central entry is not fixed store-only UTF-8 form' }
        $nameLength = [int](Read-UInt16Le $bytes ($offset + 28))
        $extraLength = [int](Read-UInt16Le $bytes ($offset + 30))
        $commentLength = [int](Read-UInt16Le $bytes ($offset + 32))
        if ($extraLength -ne 0 -or $commentLength -ne 0 -or (Read-UInt16Le $bytes ($offset + 34)) -ne 0 -or (Read-UInt16Le $bytes ($offset + 36)) -ne 0 -or (Read-UInt32Le $bytes ($offset + 38)) -ne 0) { throw 'canonical ZIP central entry has noncanonical metadata' }
        $name = $encoding.GetString($bytes, $offset + 46, $nameLength)
        $local = $locals[$name]
        if ($name -ne $expectedName -or $null -eq $local -or
            (Read-UInt32Le $bytes ($offset + 16)) -ne $local.Crc32 -or
            (Read-UInt32Le $bytes ($offset + 20)) -ne $local.Size -or
            (Read-UInt32Le $bytes ($offset + 24)) -ne $local.Size -or
            (Read-UInt32Le $bytes ($offset + 42)) -ne $local.Offset) { throw 'canonical ZIP central entry order, name, sizes, CRC32, or offset is invalid' }
        $offset += 46 + $nameLength
    }
    $centralSize = $offset - $centralOffset
    if ((Read-UInt32Le $bytes $offset) -ne 0x06054b50 -or
        (Read-UInt16Le $bytes ($offset + 4)) -ne 0 -or (Read-UInt16Le $bytes ($offset + 6)) -ne 0 -or
        (Read-UInt16Le $bytes ($offset + 8)) -ne $ExpectedNames.Count -or (Read-UInt16Le $bytes ($offset + 10)) -ne $ExpectedNames.Count -or
        (Read-UInt32Le $bytes ($offset + 12)) -ne $centralSize -or (Read-UInt32Le $bytes ($offset + 16)) -ne $centralOffset -or
        (Read-UInt16Le $bytes ($offset + 20)) -ne 0 -or $offset + 22 -ne $bytes.Length) { throw 'canonical ZIP EOCD is invalid, extended, or has a comment' }
}

function Write-CanonicalStoreOnlyZip([string]$Path, [System.Collections.IDictionary]$Entries) {
    if ($Entries.Count -eq 0 -or $Entries.Count -gt [uint16]::MaxValue) { throw 'canonical ZIP has an invalid entry count' }
    $encoding = [Text.UTF8Encoding]::new($false, $true)
    $prepared = [Collections.Generic.List[object]]::new()
    $preparedNames = [Collections.Generic.List[string]]::new()
    [uint64]$totalPayloadBytes = 0
    foreach ($name in @($Entries.Keys | Sort-Object -CaseSensitive)) {
        if ($name -notmatch '^[A-Za-z0-9][A-Za-z0-9._/-]*$' -or $name.Contains('//') -or $name.Contains('..') -or $name.EndsWith('/')) { throw 'canonical ZIP entry name is unsafe' }
        $nameBytes = $encoding.GetBytes([string]$name)
        if ($nameBytes.Length -eq 0 -or $nameBytes.Length -gt [uint16]::MaxValue) { throw 'canonical ZIP entry name exceeds u16' }
        $source = [string]$Entries[$name]
        $info = Get-Item -LiteralPath $source -Force
        if ($info.PSIsContainer -or ($info.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $info.Length -gt (512MB)) { throw 'canonical ZIP source is not a bounded regular file' }
        $totalPayloadBytes += [uint64]$info.Length
        if ($totalPayloadBytes -gt (512MB)) { throw 'canonical ZIP exceeds the production importer byte limit' }
        $payload = [IO.File]::ReadAllBytes($source)
        if ($payload.LongLength -ne $info.Length) { throw 'canonical ZIP source changed while being read' }
        $entryName = [string]$name
        $prepared.Add([pscustomobject]@{ Name = $entryName; NameBytes = $nameBytes; Payload = $payload; Crc32 = (Get-Crc32 $payload); Offset = [uint32]0 })
        $preparedNames.Add($entryName)
    }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $writer = [IO.BinaryWriter]::new($stream, [Text.Encoding]::UTF8, $true)
        try {
            foreach ($entry in $prepared) {
                $entry.Offset = [uint32]$stream.Position
                Write-UInt32Le $writer 0x04034b50; Write-UInt16Le $writer $script:ZipVersion; Write-UInt16Le $writer $script:ZipUtf8Flag; Write-UInt16Le $writer $script:ZipStoredMethod; Write-UInt16Le $writer $script:ZipDosTime; Write-UInt16Le $writer $script:ZipDosDate
                Write-UInt32Le $writer $entry.Crc32; Write-UInt32Le $writer $entry.Payload.Length; Write-UInt32Le $writer $entry.Payload.Length; Write-UInt16Le $writer $entry.NameBytes.Length; Write-UInt16Le $writer 0
                $writer.Write($entry.NameBytes); $writer.Write($entry.Payload)
            }
            $centralOffset = [uint64]$stream.Position
            foreach ($entry in $prepared) {
                Write-UInt32Le $writer 0x02014b50; Write-UInt16Le $writer $script:ZipVersion; Write-UInt16Le $writer $script:ZipVersion; Write-UInt16Le $writer $script:ZipUtf8Flag; Write-UInt16Le $writer $script:ZipStoredMethod; Write-UInt16Le $writer $script:ZipDosTime; Write-UInt16Le $writer $script:ZipDosDate
                Write-UInt32Le $writer $entry.Crc32; Write-UInt32Le $writer $entry.Payload.Length; Write-UInt32Le $writer $entry.Payload.Length; Write-UInt16Le $writer $entry.NameBytes.Length; Write-UInt16Le $writer 0; Write-UInt16Le $writer 0; Write-UInt16Le $writer 0; Write-UInt16Le $writer 0; Write-UInt32Le $writer 0; Write-UInt32Le $writer $entry.Offset
                $writer.Write($entry.NameBytes)
            }
            $centralSize = [uint64]$stream.Position - $centralOffset
            Write-UInt32Le $writer 0x06054b50; Write-UInt16Le $writer 0; Write-UInt16Le $writer 0; Write-UInt16Le $writer $prepared.Count; Write-UInt16Le $writer $prepared.Count; Write-UInt32Le $writer $centralSize; Write-UInt32Le $writer $centralOffset; Write-UInt16Le $writer 0
        } finally { $writer.Dispose() }
    } finally { $stream.Dispose() }
    Assert-CanonicalStoreOnlyZip $Path $preparedNames.ToArray()
}

Export-ModuleMember -Function Write-CanonicalStoreOnlyZip, Assert-CanonicalStoreOnlyZip, Get-CanonicalZipCrc32
