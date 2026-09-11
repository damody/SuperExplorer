$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$script = Join-Path $workspace 'scripts\prepare_test_installer_windows.ps1'
$lua = Join-Path $workspace 'build\build_install.lua'
if (-not (Test-Path -LiteralPath $script)) { throw 'prepare_test_installer_windows.ps1 missing' }
$source = Get-Content -Raw -LiteralPath $lua
if ($source.IndexOf('prepare_test_installer_windows.ps1') -lt 0) {
    throw 'build_install.lua does not invoke Windows test-installer prepare'
}
if ($source.IndexOf('publish.apk(temporary_output, output)') -gt $source.IndexOf('prepare_test_installer_windows.ps1')) {
    throw 'Windows prepare must run after the installer is published'
}
& powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script -InstallerPath $script -SelfTest
if ($LASTEXITCODE -ne 0) { throw "self-test exit $LASTEXITCODE" }
Write-Output 'prepare_test_installer_windows contract PASS'
