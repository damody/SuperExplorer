$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$helper = Join-Path $workspace 'installer\quiesce-superexplorer.ps1'
$installerSource = Join-Path $workspace 'installer\SuperExplorer.nsi'
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("superexplorer-quiesce-" + [guid]::NewGuid().ToString('N'))
$targetRoot = Join-Path $fixtureRoot 'installed'
$outsideRoot = Join-Path $fixtureRoot 'outside'
$targetProcess = $null
$outsideProcess = $null
try {
    New-Item -ItemType Directory -Path $targetRoot, $outsideRoot | Out-Null
    Copy-Item -LiteralPath "$env:SystemRoot\System32\cmd.exe" -Destination (Join-Path $targetRoot 'SuperExplorer.exe')
    Copy-Item -LiteralPath "$env:SystemRoot\System32\cmd.exe" -Destination (Join-Path $outsideRoot 'SuperExplorer.exe')
    & $helper -InstallDirectory $targetRoot -GracefulTimeoutMilliseconds 0 -ForceTimeoutMilliseconds 1000
    if ($LASTEXITCODE -ne 0) { throw 'no-process quiescence failed' }
    $targetProcess = Start-Process (Join-Path $targetRoot 'SuperExplorer.exe') -ArgumentList '/d','/c','ping.exe -t 127.0.0.1' -WindowStyle Hidden -PassThru
    $outsideProcess = Start-Process (Join-Path $outsideRoot 'SuperExplorer.exe') -ArgumentList '/d','/c','ping.exe -t 127.0.0.1' -WindowStyle Hidden -PassThru
    Start-Sleep -Milliseconds 500
    & $helper -InstallDirectory $targetRoot -GracefulTimeoutMilliseconds 0 -ForceTimeoutMilliseconds 2000
    if ($LASTEXITCODE -ne 0) { throw 'exact-path quiescence failed' }
    $targetProcess.Refresh(); $outsideProcess.Refresh()
    if (-not $targetProcess.HasExited) { throw 'target process remained alive' }
    if ($outsideProcess.HasExited) { throw 'outside process was terminated' }
    $nsi = Get-Content -Raw -LiteralPath $installerSource
    $invokeIndex = $nsi.IndexOf('quiesce-superexplorer.ps1')
    $initIndex = $nsi.IndexOf('InitPluginsDir')
    $fileIndex = $nsi.IndexOf('File "${APP_EXE}"')
    if ($initIndex -lt 0 -or $invokeIndex -lt 0 -or $initIndex -ge $invokeIndex -or $fileIndex -lt 0 -or $invokeIndex -ge $fileIndex) { throw 'NSIS install quiescence ordering is invalid' }
    if ([regex]::Matches($nsi, 'quiesce-superexplorer\.ps1').Count -lt 4) { throw 'NSIS install and uninstall do not both package and invoke quiescence' }
    $uninstallIndex = $nsi.IndexOf('Section "Uninstall"')
    $deleteIndex = $nsi.IndexOf('Delete "$INSTDIR\SuperExplorer.exe"')
    $uninstallQuiesceIndex = $nsi.IndexOf('quiesce-superexplorer.ps1', $uninstallIndex)
    if ($uninstallIndex -lt 0 -or $uninstallQuiesceIndex -lt $uninstallIndex -or $deleteIndex -lt 0 -or $uninstallQuiesceIndex -ge $deleteIndex) { throw 'NSIS uninstall quiescence ordering is invalid' }
    if ([regex]::Matches($nsi, '/SD IDOK').Count -lt 2) { throw 'NSIS silent quiescence failures can block on a dialog' }
    if (-not $nsi.Contains('SetErrorLevel 1603') -or -not $nsi.Contains('Abort')) { throw 'NSIS fail-closed contract is missing' }
    Write-Output 'Installer quiescence behavior and source contract PASS'
} finally {
    foreach ($process in @($targetProcess, $outsideProcess)) {
        if ($null -ne $process) {
            $process.Refresh()
            if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue; $process.WaitForExit() }
            $process.Dispose()
        }
    }
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}
