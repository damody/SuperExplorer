param([string]$OutputDirectory='target\headful-evidence\final-bundle',[switch]$SkipBuild)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if(-not [IO.Path]::IsPathRooted($OutputDirectory)){$OutputDirectory=[IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))}
New-Item -ItemType Directory -Force -Path $OutputDirectory|Out-Null
if(-not $SkipBuild){& (Join-Path $PSScriptRoot 'finalize_windows_artifact.ps1') -Profile debug;if($LASTEXITCODE-ne 0){throw 'build failed'}}
$results=@()
function Run-Step([string]$Name,[scriptblock]$Command){
 $watch=[Diagnostics.Stopwatch]::StartNew();$global:LASTEXITCODE=0
 try{& $Command;if($LASTEXITCODE-ne 0){throw "exit $LASTEXITCODE"};$code=0}
 catch{$code=1;throw}
 finally{$watch.Stop();$script:results+=[ordered]@{name=$Name;exit_code=$code;elapsed_ms=$watch.ElapsedMilliseconds}}
}
Run-Step lifecycle {& (Join-Path $PSScriptRoot 'smoke_windows_lifecycle.ps1') -Profile debug -SkipBuild}
Run-Step repeated {& (Join-Path $PSScriptRoot 'smoke_windows_repeated.ps1') -Profile debug -Runs 3 -SkipBuild}
Run-Step keyboard {& (Join-Path $PSScriptRoot 'smoke_keyboard_navigation.ps1') -Profile debug -SkipBuild -OutputDirectory (Join-Path $OutputDirectory 'keyboard')}
Run-Step accessibility {& (Join-Path $PSScriptRoot 'smoke_accessibility.ps1') -Profile debug -SkipBuild -OutputDirectory (Join-Path $OutputDirectory 'accessibility')}
Run-Step mouse {& (Join-Path $PSScriptRoot 'smoke_mouse_controls.ps1') -Profile debug -SkipBuild -OutputDirectory (Join-Path $OutputDirectory 'mouse')}
Run-Step panic {& cargo test -p explorer-app --test panic_report --locked}
Run-Step visual {& (Join-Path $PSScriptRoot 'capture_visual_fixture.ps1') -Profile debug -SkipBuild -Theme light -ExpectedDpiPercent 175 -State populated -OutputDirectory (Join-Path $OutputDirectory 'visual')}
[ordered]@{schema_version=1;captured_utc=[DateTime]::UtcNow.ToString('o');results=$results;all_passed=($results.exit_code -notcontains 1)}|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'report.json')
Write-Output "Headful validation bundle passed: $OutputDirectory"
