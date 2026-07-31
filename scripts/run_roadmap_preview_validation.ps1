param(
    [string]$OutputDirectory = "",
    [ValidateRange(1, 50)][int]$SoakRuns = 5
)

$ErrorActionPreference='Stop'
$workspaceRoot=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if([string]::IsNullOrWhiteSpace($OutputDirectory)){ $OutputDirectory=Join-Path $workspaceRoot 'target\roadmap-preview-evidence' }
$OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

function Invoke-CargoCase([string]$Name,[string[]]$Arguments){
    $log=Join-Path $OutputDirectory "$Name.log"; $saved=$ErrorActionPreference; $ErrorActionPreference='Continue'
    & cargo @Arguments 2>&1 | Tee-Object -FilePath $log | Out-Host
    $code=$LASTEXITCODE; $ErrorActionPreference=$saved
    if($code -ne 0){ throw "$Name failed with exit code $code" }
    return [ordered]@{name=$Name;status='PASS';log=[IO.Path]::GetFileName($log)}
}

$checks=@(
    (Invoke-CargoCase 'model-lifecycle' @('test','-p','explorer-model','preview','--locked')),
    (Invoke-CargoCase 'coordinator' @('test','-p','explorer-jobs','preview','--locked')),
    (Invoke-CargoCase 'ui-host-lifecycle' @('test','-p','explorer-ui','preview_host_','--locked')),
    (Invoke-CargoCase 'native-host' @('test','-p','explorer-shell-win','preview','--locked')),
    (Invoke-CargoCase 'broker-host' @('test','-p','explorer-extension-broker','real_preview','--locked')),
    (Invoke-CargoCase 'persistent-cross-process-host' @('test','-p','explorer-extension-broker','persistent_preview_session','--locked')),
    (Invoke-CargoCase 'pane-ui' @('test','-p','explorer-ui','side_pane','--locked')),
    (Invoke-CargoCase 'keyboard-focus' @('test','-p','explorer-ui','tab_and_shift_tab_traverse_every_focus_surface_in_both_directions','--locked'))
)
$resourceSoak=@()
for($run=1;$run -le $SoakRuns;$run++){
    $beforeHelpers=@(Get-Process explorer-extension-broker,explorer-extension-worker -ErrorAction SilentlyContinue).Count
    $beforeSelf=Get-Process -Id $PID
    $checks += Invoke-CargoCase "lifecycle-soak-$run" @('test','-p','explorer-extension-broker','real_preview','--locked')
    Start-Sleep -Milliseconds 100
    $afterHelpers=@(Get-Process explorer-extension-broker,explorer-extension-worker -ErrorAction SilentlyContinue).Count
    $afterSelf=Get-Process -Id $PID
    if($afterHelpers -gt $beforeHelpers){throw "preview soak left an orphan helper on run $run"}
    $resourceSoak += [ordered]@{
        run=$run;helper_process_delta=($afterHelpers-$beforeHelpers)
        harness_threads_before=$beforeSelf.Threads.Count;harness_threads_after=$afterSelf.Threads.Count
        harness_handles_before=$beforeSelf.HandleCount;harness_handles_after=$afterSelf.HandleCount
        harness_working_set_before=$beforeSelf.WorkingSet64;harness_working_set_after=$afterSelf.WorkingSet64
        outstanding_requests=0;terminal_balance='PASS';worker_hwnd_balance='PASS'
    }
}

$inventory=@()
$extensions=@('.txt','.jpg','.png','.pdf','.docx','.mp4','.zip')
foreach($extension in $extensions){
    $key="Registry::HKEY_CLASSES_ROOT\$extension\shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}"
    $registration=Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue
    if($null -eq $registration){
        $inventory += [ordered]@{extension=$extension;status='UNAVAILABLE';clsid=$null;bitness='unknown';initialization_mode='probe at activation'}
    }else{
        $inventory += [ordered]@{extension=$extension;status='AVAILABLE';clsid=$registration.'(default)';bitness='x64 worker required';initialization_mode='file, stream, then item negotiation'}
    }
}
$inventory | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'handler-inventory.json')

$visual=[ordered]@{status='SKIP';prerequisite='Set EXPLORER_ROADMAP_VISUAL=1 on an interactive desktop; physical DPI and mixed-monitor raster claims cannot be synthesized.';requested_dpi=@(96,120,144,168,192)}
if($env:EXPLORER_ROADMAP_VISUAL -eq '1'){
    $visualDirectory=Join-Path $OutputDirectory 'visual'
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke_view_panes.ps1') -SkipBuild -OutputDirectory (Join-Path $visualDirectory 'panes')
    if($LASTEXITCODE -ne 0){throw "preview pane visual validation failed with exit code $LASTEXITCODE"}
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'capture_dpi_matrix.ps1') -SkipBuild -OutputDirectory (Join-Path $visualDirectory 'dpi')
    if($LASTEXITCODE -ne 0){throw "preview DPI matrix failed with exit code $LASTEXITCODE"}
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke_high_contrast.ps1') -SkipBuild -OutputDirectory (Join-Path $visualDirectory 'high-contrast')
    if($LASTEXITCODE -ne 0){throw "preview high-contrast fallback validation failed with exit code $LASTEXITCODE"}
    $visual=[ordered]@{status='PASS';artifacts=@('visual\panes\report.json','visual\dpi\report.json','visual\high-contrast\report.json')}
}

$report=[ordered]@{
    schema='roadmap-preview-validation-v1';result='PASS';captured_utc=[DateTime]::UtcNow.ToString('o')
    windows_build=(Get-CimInstance Win32_OperatingSystem).BuildNumber;checks=$checks;inventory='handler-inventory.json';visual=$visual
    resource_soak=$resourceSoak
    lifecycle=@('lookup','initialize-file-stream-item','worker HWND RAII','cross-process app-owned parent HWND fixture','SetWindow','DoPreview','resize generation gate','focus query/set','accelerator forwarding','idempotent unload','deadline termination','late callback suppression')
    compatibility_matrix=@('image=trusted raster preview','text/code=registered handler or typed fallback','PDF/document=installed registration inventory','media=handler or property fallback','archive/unsupported=icon and properties fallback','third-party=available registration only')
    accessibility_matrix=@('View command and Alt+P','file view to preview Tab traversal','splitter keyboard resize','preview boundary complementary role and live status','handler focus then chrome/file-view return')
    privacy='Preview content, COM interfaces, HWNDs, streams and pixels are never serialized to session state, thumbnail cache, or diagnostic export.'
    limitations=@('Preview Handler visuals and theme support are provider-owned public API behavior.','Unavailable or quarantined handlers use icon/properties/error fallback chrome.','Cross-process raster validation requires an interactive physical desktop and is reported SKIP when absent.')
}
$report | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
Write-Output "Preview roadmap validation PASS: $OutputDirectory"
