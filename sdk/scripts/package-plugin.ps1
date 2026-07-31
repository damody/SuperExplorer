[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = (Resolve-Path -LiteralPath $PluginRoot).Path
$manifestPath = Join-Path $root 'plugin-project.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'plugin-project.json required' }

& powershell.exe -NoProfile -File (Join-Path $PSScriptRoot 'validate-plugin.ps1') -PluginRoot $root | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'plugin validation failed before packaging' }
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$buildRoot = Join-Path $root ("target\superexplorer\$($manifest.sdk.bundle_id)")
$buildReportPath = Join-Path $buildRoot 'reports\build.json'
$dllPath = Join-Path $buildRoot 'build\plugin.dll'
if (-not (Test-Path -LiteralPath $buildReportPath) -or -not (Test-Path -LiteralPath $dllPath)) {
    throw 'validated build report and plugin DLL are required; packaging never rebuilds automatically'
}
$buildReport = Get-Content -LiteralPath $buildReportPath -Raw | ConvertFrom-Json
$manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
$lockHash = (Get-FileHash -LiteralPath (Join-Path $root 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
$dllHash = (Get-FileHash -LiteralPath $dllPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($buildReport.bundle_id -ne $manifest.sdk.bundle_id -or $buildReport.inputs.manifest_sha256 -ne $manifestHash -or $buildReport.inputs.cargo_lock_sha256 -ne $lockHash -or $buildReport.plugin_dll.sha256 -ne $dllHash) {
    throw 'build inputs or DLL changed after the validated build'
}

Add-Type -AssemblyName System.IO.Compression
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
$baseName = "$($manifest.package.id)-$($manifest.package.version)-$($manifest.sdk.bundle_id)"
$finalPackage = Join-Path $dist "$baseName.sepack"
$finalHash = "$finalPackage.sha256"
$finalReport = Join-Path $dist "$baseName.package-report.json"
$stage = Join-Path $dist ('.stage-' + [guid]::NewGuid().ToString('N') + '.sepack')
$entries = [ordered]@{ 'manifest/plugin-project.json' = $manifestPath; 'plugin/plugin.dll' = $dllPath }
foreach ($payload in $manifest.payloads) { $entries["payload/$($payload.path)"] = Join-Path $root ([string]$payload.path) }
$orderedNames = @($entries.Keys | Sort-Object -CaseSensitive)
try {
    $stream = [IO.File]::Open($stage, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        $archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $true, [Text.Encoding]::UTF8)
        try {
            foreach ($name in $orderedNames) {
                $entry = $archive.CreateEntry($name, [IO.Compression.CompressionLevel]::NoCompression)
                $entry.LastWriteTime = [DateTimeOffset]::new(1980,1,1,0,0,0,[TimeSpan]::Zero)
                $input = [IO.File]::OpenRead($entries[$name])
                $output = $entry.Open()
                try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
            }
        } finally { $archive.Dispose() }
    } finally { $stream.Dispose() }

    $seen = @{}
    $readStream = [IO.File]::OpenRead($stage)
    try {
        $archive = [IO.Compression.ZipArchive]::new($readStream, [IO.Compression.ZipArchiveMode]::Read, $false, [Text.Encoding]::UTF8)
        try {
            foreach ($entry in $archive.Entries) {
                $folded = $entry.FullName.ToLowerInvariant()
                if ($seen.ContainsKey($folded) -or $entry.FullName -notin $orderedNames -or $entry.FullName.Contains('..') -or $entry.Length -gt 268435456) {
                    throw 'package archive verification rejected an entry'
                }
                $seen[$folded] = $true
                $sourceHash = (Get-FileHash -LiteralPath $entries[$entry.FullName] -Algorithm SHA256).Hash.ToLowerInvariant()
                $entryStream = $entry.Open()
                try {
                    $sha = [Security.Cryptography.SHA256]::Create()
                    try { $entryHash = ([BitConverter]::ToString($sha.ComputeHash($entryStream))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
                } finally { $entryStream.Dispose() }
                if ($entryHash -ne $sourceHash) { throw 'package archive payload hash mismatch' }
            }
            if ($seen.Count -ne $orderedNames.Count) { throw 'package archive entry count mismatch' }
        } finally { $archive.Dispose() }
    } finally { $readStream.Dispose() }

    $packageHash = (Get-FileHash -LiteralPath $stage -Algorithm SHA256).Hash.ToLowerInvariant()
    if (Test-Path -LiteralPath $finalPackage) {
        $existingHash = (Get-FileHash -LiteralPath $finalPackage -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($existingHash -ne $packageHash) { throw 'a different package already exists; refusing to overwrite it' }
        Remove-Item -LiteralPath $stage -Force
    } else {
        Move-Item -LiteralPath $stage -Destination $finalPackage
    }
    [IO.File]::WriteAllText($finalHash, "$packageHash  $baseName.sepack`n", [Text.UTF8Encoding]::new($false))
    $report = [ordered]@{
        schema_version = 1; package_id = [string]$manifest.package.id; version = [string]$manifest.package.version
        bundle_id = [string]$manifest.sdk.bundle_id; package = "$baseName.sepack"; sha256 = $packageHash
        entries = @($orderedNames | ForEach-Object { [ordered]@{ path = $_; size = (Get-Item -LiteralPath $entries[$_]).Length; sha256 = (Get-FileHash -LiteralPath $entries[$_] -Algorithm SHA256).Hash.ToLowerInvariant() } })
    }
    $reportJson = $report | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText($finalReport, "$reportJson`n", [Text.UTF8Encoding]::new($false))
    Write-Output $finalPackage
} finally {
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Force }
}
