$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$fixture = Join-Path $repo 'sdk\fixtures\extension-dll-loader-contract'
$temp = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-dll-loader-' + [Guid]::NewGuid().ToString('N'))
$savedHome = $env:CARGO_HOME; $savedTarget = $env:CARGO_TARGET_DIR
function Assert-EmptyCallMarkerDirectory([string] $StateDirectory) {
    $markerDirectory = Join-Path $StateDirectory 'native-call-markers-v1'
    if (-not (Test-Path -LiteralPath $markerDirectory -PathType Container)) { throw "missing host call-marker directory: $markerDirectory" }
    $launches = @(Get-ChildItem -LiteralPath $markerDirectory -Force)
    if ($launches.Count -eq 0) { return }
    if ($launches.Count -ne 1 -or -not $launches[0].PSIsContainer -or -not $launches[0].Name.StartsWith('launch-')) { throw "unexpected host call-marker namespace: $markerDirectory" }
    $launchContents = @(Get-ChildItem -LiteralPath $launches[0].FullName -Force)
    if ($launchContents.Count -ne 1 -or $launchContents[0].Name -ne 'owner.lease') { throw "host call-marker residue remains: $($launches[0].FullName)" }
}
try {
    $workspace = Join-Path $temp 'workspace'; $cargoHome = Join-Path $temp 'cargo-home'; $target = Join-Path $temp 'target'; $runtime = Join-Path $temp 'runtime'
    New-Item -ItemType Directory -Path $workspace, $cargoHome, $target, $runtime -Force | Out-Null
    Copy-Item -LiteralPath $fixture -Destination $workspace -Recurse
    $workspace = Join-Path $workspace 'extension-dll-loader-contract'
    New-Item -ItemType Directory -Path (Join-Path $workspace 'sdk') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repo 'sdk\ui-abi-fingerprint.json') -Destination (Join-Path $workspace 'sdk\ui-abi-fingerprint.json')
    New-Item -ItemType Directory -Path (Join-Path $workspace 'crates') -Force | Out-Null
    foreach ($crate in @('explorer-extension-api', 'explorer-extension-ui-api', 'explorer-extension-host')) { Copy-Item -LiteralPath (Join-Path $repo "crates\$crate") -Destination (Join-Path $workspace "crates\$crate") -Recurse }
    Copy-Item -LiteralPath (Join-Path $repo 'sdk\fixtures\extension-api-contract\old-v1-api') -Destination (Join-Path $temp 'old-v1-api') -Recurse
    Copy-Item -LiteralPath (Join-Path $repo 'sdk\fixtures\extension-api-contract\old-v1-plugin') -Destination (Join-Path $temp 'old-v1-plugin') -Recurse
    $vendor = (Join-Path $repo 'sdk\vendor\cargo-sources').Replace('\', '/')
    [IO.File]::WriteAllText((Join-Path $cargoHome 'config.toml'), "[build]`ntarget = 'x86_64-pc-windows-msvc'`n`n[net]`noffline = true`n`n[source.crates-io]`nreplace-with = 'cargo-sources'`n`n[source.cargo-sources]`ndirectory = '$vendor'`n", [Text.UTF8Encoding]::new($false))
    $env:CARGO_HOME = $cargoHome; $env:CARGO_TARGET_DIR = $target
    $artifact = Get-Content -LiteralPath (Join-Path $repo 'sdk\ui-abi-fingerprint.json') -Raw | ConvertFrom-Json
    $env:SUPEREXPLORER_UI_ABI_FINGERPRINT = $artifact.fingerprint
    $plugin = Join-Path $target 'x86_64-pc-windows-msvc\debug\extension_dll_loader_contract_plugin.dll'
    $variants = @{ data=@(); gpui=@('gpui'); missing=@(); wrong=@('gpui','wrong-fingerprint'); alternate=@('alternate'); foreign=@('foreign-root') }
    foreach ($name in $variants.Keys) {
        $features = $variants[$name] -join ','
        if ([string]::IsNullOrEmpty($features)) {
            & cargo.exe build --manifest-path (Join-Path $workspace 'Cargo.toml') --locked --offline -p extension-dll-loader-contract-plugin
        } else {
            & cargo.exe build --manifest-path (Join-Path $workspace 'Cargo.toml') --locked --offline -p extension-dll-loader-contract-plugin --features $features
        }
        if ($LASTEXITCODE -ne 0) { throw "plugin build failed: $name" }
        Copy-Item -LiteralPath $plugin -Destination (Join-Path $runtime "$name.dll") -Force
    }
    & cargo.exe build --manifest-path (Join-Path $workspace 'Cargo.toml') --locked --offline -p extension-dll-loader-contract-runner
    if ($LASTEXITCODE -ne 0) { throw 'runner build failed' }
    & cargo.exe build --manifest-path (Join-Path $workspace 'Cargo.toml') --locked --offline -p extension-dll-loader-contract-old-runner
    if ($LASTEXITCODE -ne 0) { throw 'old v1 runner build failed' }
    $env:EXTENSION_API_CONTRACT_MODE = 'compatible'
    & cargo.exe build --manifest-path (Join-Path $temp 'old-v1-plugin\Cargo.toml') --locked --offline
    if ($LASTEXITCODE -ne 0) { throw 'old v1 plugin build failed' }
    $runner = Join-Path $target 'x86_64-pc-windows-msvc\debug\extension-dll-loader-contract-runner.exe'
    $oldRunner = Join-Path $target 'x86_64-pc-windows-msvc\debug\extension-dll-loader-contract-old-runner.exe'
    $oldPlugin = Join-Path $target 'x86_64-pc-windows-msvc\debug\extension_api_contract_old_v1_plugin.dll'
    Copy-Item -LiteralPath $oldPlugin -Destination (Join-Path $runtime 'old-data.dll') -Force
    function Invoke-RunnerScenario([string] $Name, [string[]] $ScenarioArguments, [string] $OldPluginMode = 'compatible') {
        $callbackMarker = Join-Path $runtime ($Name + '.callback')
        $stateDirectory = Join-Path $runtime ($Name + '.state')
        Remove-Item -LiteralPath $callbackMarker, $stateDirectory -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $stateDirectory -Force | Out-Null
        $env:EXTENSION_DLL_LOADER_CONTRACT_MARKER = $callbackMarker
        $env:EXTENSION_API_CONTRACT_MARKER = $callbackMarker
        $env:EXTENSION_DLL_LOADER_CONTRACT_STATE_DIR = $stateDirectory
        $env:EXTENSION_API_CONTRACT_MODE = $OldPluginMode
        & $runner @ScenarioArguments
        if ($LASTEXITCODE -ne 0) { throw "contract scenario failed: $Name" }
        Assert-EmptyCallMarkerDirectory $stateDirectory
    }
    Invoke-RunnerScenario 'data' @('data',(Join-Path $runtime 'data.dll'))
    Invoke-RunnerScenario 'gpui-exact' @('gpui-exact',(Join-Path $runtime 'gpui.dll'))
    Invoke-RunnerScenario 'gpui-missing-binary' @('gpui-missing-binary',(Join-Path $runtime 'missing.dll'))
    Invoke-RunnerScenario 'gpui-wrong-binary' @('gpui-wrong-binary',(Join-Path $runtime 'wrong.dll'))
    Invoke-RunnerScenario 'gpui-wrong-manifest' @('gpui-wrong-manifest',(Join-Path $runtime 'gpui.dll'))
    Invoke-RunnerScenario 'two-roots' @('two-roots',(Join-Path $runtime 'data.dll'),(Join-Path $runtime 'alternate.dll'))
    Invoke-RunnerScenario 'batch-invalid' @('batch-invalid',(Join-Path $runtime 'data.dll'),(Join-Path $runtime 'foreign.dll'))
    Invoke-RunnerScenario 'old-data' @('old-data',(Join-Path $runtime 'old-data.dll'))
    Invoke-RunnerScenario 'old-panic' @('old-panic',(Join-Path $runtime 'old-data.dll')) 'panic'
    Invoke-RunnerScenario 'old-schema-mismatch' @('old-schema-mismatch',(Join-Path $runtime 'old-data.dll')) 'schema-mismatch'
    Invoke-RunnerScenario 'old-root-contract-mismatch' @('old-root-contract-mismatch',(Join-Path $runtime 'old-data.dll')) 'root-contract-mismatch'
    Invoke-RunnerScenario 'old-sdk-major-mismatch' @('old-sdk-major-mismatch',(Join-Path $runtime 'old-data.dll')) 'sdk-major-mismatch'
    foreach ($oldPlugin in @('data.dll', 'gpui.dll')) {
        $oldMarker = Join-Path $runtime ("old-host-" + $oldPlugin + '.marker')
        Remove-Item -LiteralPath $oldMarker -Force -ErrorAction SilentlyContinue
        $env:EXTENSION_DLL_LOADER_CONTRACT_MARKER = $oldMarker
        & $oldRunner (Join-Path $runtime $oldPlugin) $oldMarker
        if ($LASTEXITCODE -ne 0) { throw "old v1 host compatibility failed: $oldPlugin" }
    }
    Write-Output 'extension DLL loader contract: PASS'
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    if ($null -eq $savedHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedHome }
    if ($null -eq $savedTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedTarget }
}
