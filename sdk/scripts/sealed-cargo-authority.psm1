Set-StrictMode -Version Latest

$script:PinnedToolchainName = '1.97.1-x86_64-pc-windows-msvc'

function Get-SealedFileSha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
        $stream.Dispose()
    }
}

function Assert-NoCargoReparsePath([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($true) {
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw 'resolved SDK toolchain authority contains a symlink, junction, or reparse point'
            }
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if (-not $parent -or $parent -eq $cursor) { break }
        $cursor = $parent
    }
}

function Assert-ToolVersion([string]$Path, [string]$Label, [string]$Release, [string]$Commit, [string]$Target) {
    $lines = @(& $Path -Vv | ForEach-Object { [string]$_ })
    if ($LASTEXITCODE -ne 0 -or "release: $Release" -notin $lines -or "commit-hash: $Commit" -notin $lines) {
        throw "$Label version or commit differs from sdk-lock"
    }
    if ($Label -eq 'rustc' -and "host: $Target" -notin $lines) {
        throw 'rustc host differs from sdk-lock'
    }
}

function New-SealedCargoAuthority($Toolchain, [scriptblock] $AfterRustcLock) {
    foreach ($field in @('rustc_release','rustc_commit_hash','rustc_sha256','cargo_release','cargo_commit_hash','cargo_sha256','target')) {
        if ([string]::IsNullOrWhiteSpace([string]$Toolchain.$field)) { throw "sdk-lock toolchain.$field is required" }
    }
    if ($Toolchain.rustc_release -ne '1.97.1' -or $Toolchain.cargo_release -ne '1.97.1' -or $Toolchain.target -ne 'x86_64-pc-windows-msvc') {
        throw 'SDK toolchain policy requires Rust 1.97.1 for x86_64-pc-windows-msvc'
    }
    if ($Toolchain.rustc_sha256 -notmatch '^[0-9a-f]{64}$' -or $Toolchain.cargo_sha256 -notmatch '^[0-9a-f]{64}$') {
        throw 'sdk-lock tool binary hashes must be lowercase SHA-256 values'
    }

    # This is an OS-profile installation path, not a caller-controlled PATH,
    # RUSTUP_HOME, RUSTUP_TOOLCHAIN, or rustup.exe/shim resolution.
    $profile = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
    if ([string]::IsNullOrWhiteSpace($profile)) { throw 'Windows user profile is unavailable for SDK toolchain resolution' }
    $toolchainRoot = Join-Path $profile ".rustup\toolchains\$script:PinnedToolchainName"
    $bin = Join-Path $toolchainRoot 'bin'
    $cargo = Join-Path $bin 'cargo.exe'
    $rustc = Join-Path $bin 'rustc.exe'
    foreach ($path in @($toolchainRoot,$bin,$cargo,$rustc)) { Assert-NoCargoReparsePath $path }
    if (-not (Test-Path -LiteralPath $cargo -PathType Leaf) -or -not (Test-Path -LiteralPath $rustc -PathType Leaf)) {
        throw 'SDK-owned pinned cargo.exe and rustc.exe must both exist'
    }
    $cargo = (Resolve-Path -LiteralPath $cargo).Path
    $rustc = (Resolve-Path -LiteralPath $rustc).Path
    $cargoBin = [IO.Path]::GetDirectoryName($cargo)
    $rustcBin = [IO.Path]::GetDirectoryName($rustc)
    if (-not [string]::Equals($cargoBin, $rustcBin, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'pinned cargo.exe and rustc.exe must share one bin directory'
    }
    $directory = $null
    $rustcHandle = $null
    try {
        # Acquire deny-write/delete authority before hashing or executing rustc.
        # The same handle remains live for the complete Cargo invocation.
        $rustcHandle = [IO.File]::Open($rustc, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        if ($AfterRustcLock) { & $AfterRustcLock $rustc }
        $cargoHash = Get-SealedFileSha256 $cargo
        $rustcHash = Get-SealedFileSha256 $rustc
        if ($cargoHash -cne $Toolchain.cargo_sha256 -or $rustcHash -cne $Toolchain.rustc_sha256) {
            throw 'actual pinned cargo.exe or rustc.exe SHA-256 differs from sdk-lock'
        }
        Assert-ToolVersion $cargo 'cargo' $Toolchain.cargo_release $Toolchain.cargo_commit_hash $Toolchain.target
        Assert-ToolVersion $rustc 'rustc' $Toolchain.rustc_release $Toolchain.rustc_commit_hash $Toolchain.target

        $directory = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-sealed-cargo-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
        $sealed = Join-Path $directory 'cargo.exe'
        $source = [IO.File]::Open($cargo, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        try {
            $destination = [IO.File]::Open($sealed, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
            try { $source.CopyTo($destination) } finally { $destination.Dispose() }
        } finally { $source.Dispose() }
        $sealedHash = Get-SealedFileSha256 $sealed
        if ($sealedHash -cne $cargoHash) { throw 'sealed Cargo copy hash differs from the verified actual cargo.exe' }
        Assert-ToolVersion $sealed 'sealed Cargo' $Toolchain.cargo_release $Toolchain.cargo_commit_hash $Toolchain.target
        return [pscustomobject]@{
            Path = $sealed; Sha256 = $sealedHash; Directory = $directory
            RustcPath = $rustc; RustcSha256 = $rustcHash; RustcHandle = $rustcHandle
        }
    } catch {
        if ($rustcHandle) { $rustcHandle.Dispose() }
        if ($directory -and (Test-Path -LiteralPath $directory)) { Remove-Item -LiteralPath $directory -Recurse -Force }
        throw
    }
}

function Remove-SealedCargoAuthority($Authority) {
    if (-not $Authority) { return }
    if ($Authority.RustcHandle) { $Authority.RustcHandle.Dispose() }
    if (Test-Path -LiteralPath $Authority.Directory) { Remove-Item -LiteralPath $Authority.Directory -Recurse -Force }
}

Export-ModuleMember -Function New-SealedCargoAuthority,Remove-SealedCargoAuthority
