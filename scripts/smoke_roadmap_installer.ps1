param(
    [Parameter(Mandatory=$true)][string]$InstallerPath,
    [string]$OutputDirectory = ""
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$InstallerPath = (Resolve-Path -LiteralPath $InstallerPath).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $workspaceRoot 'target\roadmap-installer-evidence'
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$ownedRoot = Join-Path ([IO.Path]::GetTempPath()) ("SuperExplorer-UTIT-" + [guid]::NewGuid().ToString('N'))
$localAppData = Join-Path $OutputDirectory 'isolated-local-app-data'
New-Item -ItemType Directory -Path $localAppData -Force | Out-Null

function Invoke-Installer([string]$Name) {
    $process = Start-Process -FilePath $InstallerPath -ArgumentList @('/S', "/D=$ownedRoot") -PassThru -Wait -WindowStyle Hidden
    if ($process.ExitCode -ne 0) { throw "$Name exited with code $($process.ExitCode)" }
}

function Assert-InstalledBinaries {
    foreach ($name in @('SuperExplorer.exe','explorer-extension-broker.exe','explorer-extension-worker.exe','Uninstall.exe')) {
        $path = Join-Path $ownedRoot $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "installed binary is missing: $name" }
    }
}

try {
    Invoke-Installer 'fresh install'
    Assert-InstalledBinaries
    # Release helpers use the Windows subsystem, so PowerShell's direct native invocation can
    # discard stdout even though an inherited console pipe exists. `cmd /d /c` preserves the
    # redirected standard handle and makes the installed marker observable without a window.
    $brokerPath = Join-Path $ownedRoot 'explorer-extension-broker.exe'
    $marker = (& $env:ComSpec /d /c "`"$brokerPath`" --version-json") -join "`n"
    if ($LASTEXITCODE -ne 0 -or -not $marker.Contains('"role":"supervisor"')) {
        throw 'installed broker did not publish a compatible supervisor marker'
    }

    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $ownedRoot 'SuperExplorer.exe'
    $start.WorkingDirectory = $ownedRoot
    $start.UseShellExecute = $false
    $start.Environment['LOCALAPPDATA'] = $localAppData
    $start.Environment['EXPLORER_LOG_DIR'] = Join-Path $OutputDirectory 'installed-run'
    $start.Environment['EXPLORER_AUTO_CLOSE_MS'] = '900'
    $app = [Diagnostics.Process]::Start($start)
    if (-not $app.WaitForExit(30000)) { $app.Kill(); throw 'installed app timed out' }
    if ($app.ExitCode -ne 0) { throw "installed app exited with code $($app.ExitCode)" }
    $appLog = Join-Path $OutputDirectory 'installed-run\explorer.log'
    $appText = Get-Content -Raw -Encoding UTF8 -LiteralPath $appLog
    if (-not $appText.Contains('event="extension_broker_ready"') -or -not $appText.Contains('event="clean_shutdown"')) {
        throw 'installed-path app did not verify its broker and shut down cleanly'
    }

    Invoke-Installer 'in-place upgrade'
    Assert-InstalledBinaries
    $uninstaller = Join-Path $ownedRoot 'Uninstall.exe'
    # Do not use NSIS `_?=` here: that suppresses the normal temporary self-copy and deliberately
    # leaves Uninstall.exe behind. A normal silent invocation validates real Explorer-user cleanup.
    $uninstall = Start-Process -FilePath $uninstaller -ArgumentList @('/S') -PassThru -Wait -WindowStyle Hidden
    if ($uninstall.ExitCode -ne 0) { throw "uninstall exited with code $($uninstall.ExitCode)" }
    $cleanupDeadline = (Get-Date).AddSeconds(5)
    while ((Get-Date) -lt $cleanupDeadline -and (Test-Path -LiteralPath $ownedRoot)) {
        Start-Sleep -Milliseconds 100
    }
    $residual = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        try { $_.Path -and [IO.Path]::GetFullPath($_.Path).StartsWith($ownedRoot, [StringComparison]::OrdinalIgnoreCase) } catch { $false }
    })
    if ($residual.Count -ne 0) { throw 'uninstall left a process launched from the owned install root' }
    if (Test-Path -LiteralPath $ownedRoot) {
        $leftovers = @(Get-ChildItem -LiteralPath $ownedRoot -Force -ErrorAction SilentlyContinue)
        if ($leftovers.Count -ne 0) { throw 'uninstall left files in the owned install root' }
    }

    [ordered]@{
        schema='roadmap-installer-validation-v1'; result='PASS'; captured_utc=[DateTime]::UtcNow.ToString('o')
        installed_binaries=@('SuperExplorer.exe','explorer-extension-broker.exe','explorer-extension-worker.exe')
        broker_marker=$marker; fresh_install='PASS'; installed_path_e2e='PASS'; upgrade='PASS'; uninstall='PASS'
        user_data_policy='The uninstaller removes program files only; isolated LOCALAPPDATA remains recoverable by design.'
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
    Write-Output "Roadmap installer validation PASS: $OutputDirectory"
} finally {
    # Never recursively delete here. The NSIS uninstaller owns cleanup and any residue is evidence.
}
