param()

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $workspaceRoot
try {
    # Scope admission to the application inputs; companion and test evidence are separate.
    $dirtyInputs = @(git status --porcelain=v1 --untracked-files=all -- `
        Cargo.toml Cargo.lock rust-toolchain.toml crates installer/SuperExplorer.nsi `
        scripts/finalize_windows_artifact.ps1 scripts/package_superexplorer_release.ps1)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read application source status.' }
    if ($dirtyInputs.Count -gt 0) {
        throw ("Commit application inputs before packaging:`n" + ($dirtyInputs -join "`n"))
    }
    $sourceCommit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read source commit.' }
    $commitTime = [DateTimeOffset]::Parse((git show -s --format=%cI HEAD).Trim())
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read commit timestamp.' }
    $version = '1.{0}.{1}.{2}' -f $commitTime.Year, $commitTime.Month, $commitTime.Day
    $dist = Join-Path $workspaceRoot 'dist'
    $installerName = "SuperExplorer-Setup-$version-x64.exe"
    $installerPath = Join-Path $dist $installerName
    if (Test-Path -LiteralPath $installerPath) {
        throw "Output already exists; preserve it before packaging: $installerPath"
    }
    $packagingDirectory = Join-Path $dist ('.release-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $packagingDirectory -Force | Out-Null

    $nsisCandidates = @(
        (Join-Path ${env:ProgramFiles(x86)} 'NSIS/makensis.exe'),
        (Join-Path $env:ProgramFiles 'NSIS/makensis.exe')
    )
    $makensis = $nsisCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
    if (-not $makensis) { throw 'NSIS makensis.exe was not found.' }

    & (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile release
    if ($LASTEXITCODE -ne 0) { throw 'Release artifact finalization failed.' }
    $targetRoot = if ($env:CARGO_TARGET_DIR) {
        [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    } else { Join-Path $workspaceRoot 'target' }
    $releaseDirectory = Join-Path $targetRoot 'release'

    $inputs = [ordered]@{
        APP_EXE = 'SuperExplorer.exe'
        BROKER_EXE = 'explorer-extension-broker.exe'
        MFT_HELPER_EXE = 'superexplorer-mft-helper.exe'
        MFT_SERVICE_EXE = 'superexplorer-mft-service.exe'
        WORKER_EXE = 'explorer-extension-worker.exe'
        QUIESCE_EXE = 'superexplorer-quiesce.exe'
        EVERYTHING_DLL = 'Everything64.dll'
    }
    $defines = [ordered]@{
        APP_VERSION = $version
        OUTPUT_FILE = (Join-Path $packagingDirectory $installerName)
        OUTPUT_BASENAME = $installerName
    }
    $payload = @()
    foreach ($inputName in $inputs.Keys) {
        $binary = Join-Path $releaseDirectory $inputs[$inputName]
        if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Missing input: $binary" }
        $bytes = [System.IO.File]::ReadAllBytes($binary)
        if ($bytes.Length -lt 1024) { throw "Invalid PE input: $binary" }
        $offset = [BitConverter]::ToInt32($bytes, 0x3c)
        if ($offset -lt 64 -or $offset + 6 -gt $bytes.Length -or
            [BitConverter]::ToUInt32($bytes, $offset) -ne 0x00004550 -or
            [BitConverter]::ToUInt16($bytes, $offset + 4) -ne 0x8664) {
            throw "Input is not an x64 PE: $binary"
        }
        $defines[$inputName] = $binary
        $payload += [ordered]@{
            name = $inputs[$inputName]
            size = (Get-Item -LiteralPath $binary).Length
            sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }

    $sdkLock = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot 'sdk/sdk-lock.json') |
        ConvertFrom-Json
    $plugins = [ordered]@{
        PLUGIN_FOLDER_SIZE = 'rust-folder-size-visual-column'
        PLUGIN_SIZE_MAP = 'rust-folder-size-map-view'
        PLUGIN_RUST_TOKEI = 'rust-tokei-code-lines-column'
        PLUGIN_LUA_TOKEI = 'lua-tokei-code-lines-column'
        PLUGIN_LOCK_OWNER = 'rust-lock-owner-column'
        PLUGIN_EXIF_RENAME = 'rust-exif-rename-command'
        PLUGIN_7Z = 'rust-7z-virtual-folder'
        PLUGIN_BULK_FOLDER = 'lua-bulk-folder-generator'
    }
    foreach ($defineName in $plugins.Keys) {
        $pluginName = $plugins[$defineName]
        $pluginDirectory = Join-Path $workspaceRoot "sdk/fixtures/$pluginName/dist"
        $plugin = Get-ChildItem -LiteralPath $pluginDirectory -File |
            Where-Object { $_.Name.EndsWith('-' + $sdkLock.bundle_id + '.sepack') } |
            Sort-Object LastWriteTimeUtc, FullName -Descending | Select-Object -First 1
        if (-not $plugin) { throw "Missing plugin for SDK $($sdkLock.bundle_id): $pluginName" }
        $defines[$defineName] = $plugin.FullName
        $payload += [ordered]@{
            name = "plugins/$pluginName.sepack"
            size = $plugin.Length
            sha256 = (Get-FileHash -LiteralPath $plugin.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $defineLines = foreach ($key in $defines.Keys) {
        $value = [string]$defines[$key]
        if ($value -match '["\r\n]') { throw "Invalid NSIS define: $key" }
        '!define {0} "{1}"' -f $key, $value
    }
    $definesPath = Join-Path $packagingDirectory 'installer-defines.nsh'
    [System.IO.File]::WriteAllLines($definesPath, [string[]]$defineLines,
        [System.Text.UTF8Encoding]::new($false))
    & $makensis '/V3' '/WX' '/INPUTCHARSET' 'UTF8' '/OUTPUTCHARSET' 'UTF8' `
        "/DGENERATED_DEFINES=$definesPath" (Join-Path $workspaceRoot 'installer/SuperExplorer.nsi')
    if ($LASTEXITCODE -ne 0) { throw "NSIS packaging failed: $LASTEXITCODE" }
    $stagedInstaller = $defines['OUTPUT_FILE']
    $installerInfo = Get-Item -LiteralPath $stagedInstaller
    if ($installerInfo.Length -lt 1MB -or $installerInfo.VersionInfo.ProductVersion -ne $version) {
        throw 'Installer size or VERSIONINFO validation failed.'
    }
    # -LiteralPath and a checked, fixed dist output avoid interpreting wildcard names.
    Move-Item -LiteralPath $stagedInstaller -Destination $installerPath
    $installerHash = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifestName = "SuperExplorer-$version-release-manifest.json"
    $manifestPath = Join-Path $dist $manifestName
    $manifest = [ordered]@{
        schema_version = 1
        version = $version
        source_commit = $sourceCommit
        platform = 'windows-x64'
        sdk_bundle = $sdkLock.bundle_id
        installer = [ordered]@{ name = $installerName; size = $installerInfo.Length; sha256 = $installerHash }
        payload = $payload
    }
    [System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 5) + "`n",
        [System.Text.UTF8Encoding]::new($false))
    $manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $checksumsPath = Join-Path $dist "SuperExplorer-$version-SHA256SUMS.txt"
    [System.IO.File]::WriteAllText($checksumsPath,
        "$installerHash  $installerName`n$manifestHash  $manifestName`n",
        [System.Text.UTF8Encoding]::new($false))
    Write-Output "Installer: $installerPath"
    Write-Output "Source commit: $sourceCommit"
    Write-Output "SHA256: $installerHash"
    Write-Output "Manifest: $manifestPath"
    Write-Output "Checksums: $checksumsPath"
} finally {
    Pop-Location
}
