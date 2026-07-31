param(
    [string]$OutputDirectory = "",
    [ValidateRange(1, 50)][int]$SoakRuns = 5
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { $OutputDirectory = Join-Path $workspaceRoot 'target\roadmap-broker-evidence' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

function Invoke-Logged([string]$Name, [scriptblock]$Body) {
    $log = Join-Path $OutputDirectory "$Name.log"; $watch=[Diagnostics.Stopwatch]::StartNew()
    $saved=$ErrorActionPreference; $ErrorActionPreference='Continue'
    & $Body 2>&1 | Tee-Object -FilePath $log | Out-Host
    $code=$LASTEXITCODE; $ErrorActionPreference=$saved; $watch.Stop()
    if ($code -ne 0) { throw "$Name failed with exit code $code" }
    return [ordered]@{ name=$Name; status='PASS'; duration_ms=[math]::Round($watch.Elapsed.TotalMilliseconds,3); log=[IO.Path]::GetFileName($log) }
}

$checks=@()
$checks += Invoke-Logged 'protocol' { cargo test -p explorer-extension-protocol --locked }
$checks += Invoke-Logged 'process-boundary' { cargo test -p explorer-extension-broker --all-targets --locked }
$checks += Invoke-Logged 'context-menu-latency' { cargo test -p explorer-extension-broker --test process_boundary cold_and_warm_context_menu_queries_record_one_persistent_broker_launch --locked -- --nocapture }
$checks += Invoke-Logged 'architecture' { powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'check_architecture.ps1') }
$checks += Invoke-Logged 'binary-finalization' { powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile debug }

$latencyText = Get-Content -Raw -LiteralPath (Join-Path $OutputDirectory 'context-menu-latency.log')
$latencyMatch = [regex]::Match($latencyText, 'broker-context-latency cold_ms=(\d+) warm_ms=(\d+) broker_pid=(\d+) broker_launches=(\d+) worker_pid=(\d+)')
if (-not $latencyMatch.Success) { throw 'context-menu latency evidence was not emitted' }
$lifecycleEvidence = [ordered]@{
    cold_ms = [int64]$latencyMatch.Groups[1].Value
    warm_ms = [int64]$latencyMatch.Groups[2].Value
    broker_pid = [uint32]$latencyMatch.Groups[3].Value
    broker_launches = [uint64]$latencyMatch.Groups[4].Value
    last_worker_pid = [uint32]$latencyMatch.Groups[5].Value
    persistent_session = ($latencyMatch.Groups[4].Value -eq '1')
    helper_console_windows = 0
    evidence = 'context-menu-latency.log plus persistent_broker_and_active_worker_never_create_visible_top_level_windows'
}
if (-not $lifecycleEvidence.persistent_session) { throw 'warm context menu launched an additional broker' }

$samples=@()
for($run=1;$run -le $SoakRuns;$run++) {
    $before=@(Get-Process explorer-extension-broker,explorer-extension-worker -ErrorAction SilentlyContinue).Count
    $result=Invoke-Logged "mixed-soak-$run" { cargo test -p explorer-extension-broker --test process_boundary --locked }
    Start-Sleep -Milliseconds 100
    $after=@(Get-Process explorer-extension-broker,explorer-extension-worker -ErrorAction SilentlyContinue).Count
    if($after -gt $before) { throw "broker soak left an orphan process on run $run" }
    $samples += [ordered]@{ run=$run; duration_ms=$result.duration_ms; residual_process_delta=($after-$before); terminal_balance='PASS' }
}

$sevenZip = Get-ItemProperty 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\7-Zip' -ErrorAction SilentlyContinue
$interop = if($null -eq $sevenZip) {
    [ordered]@{ status='SKIP'; prerequisite='Installed 7-Zip registry registration was not found.' }
} else {
    $checks += Invoke-Logged 'installed-7zip-interop' { cargo test -p explorer-shell-win installed_7zip_extension_queries_submenu_and_invokes_owned_archive_command --locked -- --ignored --nocapture }
    [ordered]@{ status='PASS'; installation_path=$sevenZip.Path; evidence='Installed 7-Zip submenu was enumerated and its owned safe archive command produced a non-empty archive; broker differential query separately verifies the same public Shell menu route.' }
}

$installer = if ($env:EXPLORER_INSTALLER_PATH -and (Test-Path -LiteralPath $env:EXPLORER_INSTALLER_PATH -PathType Leaf)) {
    $installerEvidence = Join-Path $OutputDirectory 'installer'
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke_roadmap_installer.ps1') -InstallerPath $env:EXPLORER_INSTALLER_PATH -OutputDirectory $installerEvidence | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "installed-path broker validation failed with exit code $LASTEXITCODE" }
    [ordered]@{ status='PASS'; report='installer/report.json' }
} else {
    [ordered]@{ status='SKIP'; prerequisite='Set EXPLORER_INSTALLER_PATH to a built NSIS installer for fresh-install/upgrade/uninstall validation.' }
}

$report=[ordered]@{
    schema='roadmap-broker-validation-v1'; result='PASS'; captured_utc=[DateTime]::UtcNow.ToString('o')
    windows_build=(Get-CimInstance Win32_OperatingSystem).BuildNumber; checks=$checks; broker_lifecycle=$lifecycleEvidence; mixed_soak=$samples; extension_interop=$interop; installer=$installer
    security_review=[ordered]@{
        authentication='Per-generation 128-bit nonce and Hello/HelloAck compatibility handshake; every request remains nonce-authenticated and monotonically correlated.'
        least_authority='Explicit operation class, bounded descriptors, validated authority flags, and DuplicateHandle ownership helper.'
        containment='Privilege-stripped restricted thread tokens plus a kill-on-close one-process Job Object with memory/CPU limits. Read-only thumbnail, preview, and namespace workers use Low Integrity; context-menu workers retain the caller integrity level because explicitly invoked Shell verbs require user-authorized filesystem writes.'
        privacy='Diagnostics retain correlation/protocol/opaque handler digest and never accept path/content/secret fields.'
    }
    fault_matrix=@('missing','wrong-version','malformed','oversized','startup-crash','worker-crash','hang','disconnect','late-terminal','child-process')
    rollback='Remove broker binaries or fail version verification: app records typed unavailable and retains safe built-in fallback surfaces.'
}
$report | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
Write-Output "Broker roadmap validation PASS: $OutputDirectory"
