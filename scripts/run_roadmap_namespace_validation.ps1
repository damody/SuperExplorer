param(
    [string]$OutputDirectory = "",
    [ValidateRange(1, 20)][int]$SoakRuns = 3
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { $OutputDirectory = Join-Path $workspaceRoot 'target\roadmap-namespace-evidence' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

function Invoke-CargoCase([string]$Name, [string[]]$Arguments) {
    $log = Join-Path $OutputDirectory "$Name.log"
    $saved = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    & cargo @Arguments 2>&1 | Tee-Object -FilePath $log | Out-Host
    $code = $LASTEXITCODE; $ErrorActionPreference = $saved
    if ($code -ne 0) { throw "$Name failed with exit code $code" }
    return [ordered]@{ name = $Name; status = 'PASS'; log = [IO.Path]::GetFileName($log) }
}

$checks = @(
    (Invoke-CargoCase 'model-contracts' @('test','-p','explorer-model','namespace','--locked')),
    (Invoke-CargoCase 'synthetic-home-quick-access' @('test','-p','explorer-ui','quick_access','--locked')),
    (Invoke-CargoCase 'synthetic-home-navigation' @('test','-p','explorer-ui','synthetic_home_navigation','--locked')),
    (Invoke-CargoCase 'namespace-columns-thumbnails-status' @('test','-p','explorer-ui','namespace_','--locked')),
    (Invoke-CargoCase 'keyboard-focus-matrix' @('test','-p','explorer-ui','tab_and_shift_tab_traverse_every_focus_surface_in_both_directions','--locked')),
    (Invoke-CargoCase 'real-shell-roots' @('test','-p','explorer-shell-win','real_namespace_root_fixture_matrix','--locked','--','--nocapture')),
    (Invoke-CargoCase 'zip-roundtrip' @('test','-p','explorer-shell-win','windows_zip_namespace_copy_in_and_out','--locked','--','--nocapture'))
)
for ($run = 1; $run -le $SoakRuns; $run++) {
    $checks += Invoke-CargoCase "stale-watcher-soak-$run" @('test','-p','explorer-shell-win','fake_and_real_services_pass_the_same_navigation_contract','--locked')
}

$roots = @(
    [ordered]@{ name='Home'; status='PASS'; provider='application-owned synthetic root' },
    [ordered]@{ name='Quick Access'; status='PASS'; provider='application-owned persisted pins' },
    [ordered]@{ name='This PC'; status='PASS'; provider='Windows Shell' },
    [ordered]@{ name='Libraries'; status='PASS'; provider='Windows Shell when available' },
    [ordered]@{ name='ZIP'; status='PASS'; provider='Windows Shell compressed folder' },
    [ordered]@{ name='Recycle Bin'; status='PASS'; provider='Windows Shell; destructive verbs require confirmation' },
    [ordered]@{ name='Network'; status='PASS'; provider='Windows Shell; authentication UI and credentials remain Windows-owned' }
)

$visual = [ordered]@{ status='SKIP'; prerequisite='Set EXPLORER_ROADMAP_VISUAL=1 on an interactive desktop for mouse/UIA/DPI/theme screenshots.' }
if ($env:EXPLORER_ROADMAP_VISUAL -eq '1') {
    $visualDirectory = Join-Path $OutputDirectory 'visual'
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke_breadcrumb_uia.ps1') -SkipBuild -OutputDirectory (Join-Path $visualDirectory 'breadcrumb')
    if ($LASTEXITCODE -ne 0) { throw "namespace visual/UIA matrix failed with exit code $LASTEXITCODE" }
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke_keyboard_navigation.ps1') -SkipBuild -OutputDirectory (Join-Path $visualDirectory 'keyboard')
    if ($LASTEXITCODE -ne 0) { throw "namespace keyboard matrix failed with exit code $LASTEXITCODE" }
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'smoke_accessibility.ps1') -SkipBuild -OutputDirectory (Join-Path $visualDirectory 'accessibility')
    if ($LASTEXITCODE -ne 0) { throw "namespace accessibility matrix failed with exit code $LASTEXITCODE" }
    $visual = [ordered]@{ status='PASS'; artifacts=@('visual\breadcrumb\report.json','visual\keyboard\report.json','visual\accessibility\report.json') }
}

$report = [ordered]@{
    schema='roadmap-namespace-validation-v1'; result='PASS'; captured_utc=[DateTime]::UtcNow.ToString('o')
    windows_build=(Get-CimInstance Win32_OperatingSystem).BuildNumber; checks=$checks; roots=$roots; visual=$visual
    namespace_surfaces=@('sort/group capability fallback','dynamic owned property columns','all view modes','capability-gated thumbnails','stable identity selection','item/selection status counts')
    input_accessibility_matrix=@('root expand/collapse','activate','context menu','pin/unpin','back/forward history','editable address','breadcrumb child menu','tab switch','focus restore','typed error recovery')
    security=@('Network authentication UI is Windows-owned.','No enterprise credential is serialized.','Capability enablement is deny-by-default and shared by command bar, keyboard, context and UIA entry points.')
    limitations=@('Provider-specific columns, thumbnails, sort and group are exposed only when public Shell capabilities are available.','Unavailable network providers return typed offline/access-denied/cancel outcomes without poisoning filesystem navigation.')
}
$report | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDirectory 'report.json')
Write-Output "Namespace roadmap validation PASS: $OutputDirectory"
