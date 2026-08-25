param(
    [Parameter(Mandatory = $true)]
    [string]$Path
)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$bytes = [IO.File]::ReadAllBytes($resolved)
if ($bytes.Length -lt 256) {
    throw "PE image is too small: $resolved"
}
$peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
if ($peOffset -lt 0 -or $peOffset + 94 -gt $bytes.Length) {
    throw "PE header offset is invalid: $resolved"
}
if ([BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x00004550) {
    throw "PE signature is missing: $resolved"
}
$optionalHeader = $peOffset + 24
$magic = [BitConverter]::ToUInt16($bytes, $optionalHeader)
if ($magic -notin @(0x010b, 0x020b)) {
    throw "PE optional header is unsupported: 0x$($magic.ToString('x4'))"
}
$subsystem = [BitConverter]::ToUInt16($bytes, $optionalHeader + 68)
if ($subsystem -ne 3) {
    throw "Expected IMAGE_SUBSYSTEM_WINDOWS_CUI (3), found $subsystem in $resolved"
}
Write-Output "console subsystem verified: $resolved"
