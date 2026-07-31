param(
    [ValidateSet('debug','release')][string]$Profile='debug',
    [string]$OutputDirectory='target\dpi-evidence\matrix-final',
    [switch]$SkipBuild
)
$ErrorActionPreference='Stop'
$workspaceRoot=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if(-not [IO.Path]::IsPathRooted($OutputDirectory)){$OutputDirectory=[IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))}
New-Item -ItemType Directory -Force -Path $OutputDirectory|Out-Null
if(-not $SkipBuild){& (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile $Profile;if($LASTEXITCODE-ne 0){throw 'build failed'}}
$cases=@()
foreach($percent in 100,125,150,200){
    $case=Join-Path $OutputDirectory "$percent-percent"
    & (Join-Path $PSScriptRoot 'capture_visual_fixture.ps1') -Profile $Profile -SkipBuild `
        -ExpectedDpiPercent $percent -AllowDpiMismatch -State focused -Width 1120 -Height 720 -OutputDirectory $case
    $metadata=Get-Content -Raw -Encoding utf8 (Join-Path $case 'metadata.json')|ConvertFrom-Json
    $diagnostics=Get-Content -Raw -Encoding utf8 (Join-Path $case 'diagnostics.json')|ConvertFrom-Json
    $cases+=[ordered]@{
        requested_percent=$percent;actual_dpi=$metadata.dpi.actual_window_dpi
        actual_scale_factor=$metadata.dpi.actual_scale_factor;matches_expectation=$metadata.dpi.matches_expectation
        logical_width=$diagnostics.fixture.width_logical;logical_height=$diagnostics.fixture.height_logical
        screenshot_sha256=$metadata.screenshot_sha256;diagnostics_sha256=$metadata.diagnostics_sha256
    }
}
$layoutJson=@($cases|ForEach-Object{(Get-Content -Raw -Encoding utf8 (Join-Path $OutputDirectory "$($_.requested_percent)-percent\diagnostics.json")|ConvertFrom-Json).layout|ConvertTo-Json -Compress})
if(($layoutJson|Select-Object -Unique).Count-ne 1){throw 'logical layout changed across declared DPI cases'}
& cargo test -p explorer-ui layout::tests::logical_values_scale_once_at_supported_windows_percentages --locked -- --exact
if($LASTEXITCODE-ne 0){throw 'logical DPI unit matrix failed'}
[ordered]@{
 schema_version=1;captured_utc=[DateTime]::UtcNow.ToString('o');cases=$cases
 active_monitor_count=1;actual_session_percent=175
 actual_result='All four requested sessions executed on the only available 175% monitor; mismatch is explicit and none is accepted as a 100/125/150/200 visual baseline.'
 logical_result='Typed 100/125/150/200 scale matrix passed and diagnostics retained identical logical geometry without double scaling.'
 manual_limitation='Changing the sole interactive display scale requires mutating the user desktop/session; no second monitor or isolated session was available.'
}|ConvertTo-Json -Depth 6|Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
Write-Output "DPI matrix captured with explicit single-monitor limitation: $OutputDirectory"
