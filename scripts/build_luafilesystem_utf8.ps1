param(
    [switch]$SkipInstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$source = Join-Path $workspace 'vendor\luafilesystem'
$luaRoot = Join-Path $workspace 'build\tools\lua'
$include = Join-Path $source 'vendor\lua-5.4.8\include'
$staging = Join-Path $workspace 'target\luafilesystem-utf8-build'
$testRoot = Join-Path $staging 'unicode-smoke-fixture'
$stagedDll = Join-Path $staging 'lfs.dll'
$gcc = (Get-Command gcc.exe -ErrorAction Stop).Source

New-Item -ItemType Directory -Force -Path $staging | Out-Null
if (Test-Path -LiteralPath $testRoot) {
    Get-ChildItem -Force -LiteralPath $testRoot -Recurse -ErrorAction SilentlyContinue | ForEach-Object {
        try { $_.Attributes = [IO.FileAttributes]::Normal } catch { }
    }
    Remove-Item -LiteralPath $testRoot -Recurse -Force
}

$arguments = @(
    '-O2', '-Wall', '-Wextra', '-shared',
    '-I', $include,
    (Join-Path $source 'src\lfs.c'),
    (Join-Path $source 'src\lfs_win32_utf8.c'),
    (Join-Path $luaRoot 'lua54.dll'),
    '-o', $stagedDll
)
& $gcc @arguments
if ($LASTEXITCODE -ne 0) { throw "LuaFileSystem compilation failed: $LASTEXITCODE" }

$previousCPath = $env:LUA_CPATH
try {
    $env:LUA_CPATH = (Join-Path $staging '?.dll')
    & (Join-Path $luaRoot 'lua.exe') (Join-Path $PSScriptRoot 'test_luafilesystem_utf8.lua') $testRoot
    if ($LASTEXITCODE -ne 0) { throw "LuaFileSystem UTF-8 smoke failed: $LASTEXITCODE" }
} finally {
    $env:LUA_CPATH = $previousCPath
}

if (-not $SkipInstall) {
    Copy-Item -Force -LiteralPath $stagedDll -Destination (Join-Path $luaRoot 'lfs.dll')
}

$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $stagedDll).Hash
Write-Output "LuaFileSystem UTF-8 build passed: $stagedDll"
Write-Output "SHA-256: $hash"
