$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$fixture = Join-Path $repo 'sdk\fixtures\extension-dll-loader-contract'
$temp = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-dll-loader-' + [Guid]::NewGuid().ToString('N'))
$savedHome = $env:CARGO_HOME; $savedTarget = $env:CARGO_TARGET_DIR
function Assert-EmptyCallMarkerDirectory([string] $StateDirectory) {
    $markerDirectory = Join-Path $StateDirectory 'native-call-markers-v1'
    if (-not (Test-Path -LiteralPath $markerDirectory -PathType Container)) { throw 'missing host call-marker directory' }
    $launches = @(Get-ChildItem -LiteralPath $markerDirectory -Force)
    if ($launches.Count -eq 0) { return }
    if ($launches.Count -ne 1 -or -not $launches[0].PSIsContainer -or -not $launches[0].Name.StartsWith('launch-')) { throw 'unexpected host call-marker namespace' }
    $launchContents = @(Get-ChildItem -LiteralPath $launches[0].FullName -Force)
    if ($launchContents.Count -ne 1 -or $launchContents[0].Name -ne 'owner.lease') { throw 'host call-marker residue remains' }
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
    # The isolated runner builds explorer-extension-host only as a normal
    # dependency. Remove repository test-only path dependencies from the copied
    # manifest so Cargo does not resolve unrelated workspace crates/vendor data.
    $copiedHostManifest = Join-Path $workspace 'crates\explorer-extension-host\Cargo.toml'
    $copiedHostText = Get-Content -LiteralPath $copiedHostManifest -Raw -Encoding UTF8
    $copiedHostText = [regex]::Replace($copiedHostText, '(?ms)^\[dev-dependencies\]\r?\n.*?(?=^\[lints\])', '')
    [IO.File]::WriteAllText($copiedHostManifest, $copiedHostText, [Text.UTF8Encoding]::new($false))
    # ABI v1 is still unpublished. This fixture intentionally exercises only
    # the current stateful registrar object; legacy raw-callback roots are not
    # a compatibility promise during this migration.
    $env:CARGO_HOME = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) '.cargo'; $env:CARGO_TARGET_DIR = $target
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
    $runner = Join-Path $target 'x86_64-pc-windows-msvc\debug\extension-dll-loader-contract-runner.exe'
    function Invoke-RunnerScenario([string] $Name, [string[]] $ScenarioArguments) {
        $callbackMarker = Join-Path $runtime ($Name + '.callback')
        $stateDirectory = Join-Path $runtime ($Name + '.state')
        Remove-Item -LiteralPath $callbackMarker, $stateDirectory -Recurse -Force -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $stateDirectory -Force | Out-Null
        $env:EXTENSION_DLL_LOADER_CONTRACT_MARKER = $callbackMarker
        $env:EXTENSION_API_CONTRACT_MARKER = $callbackMarker
        $env:EXTENSION_DLL_LOADER_CONTRACT_STATE_DIR = $stateDirectory
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
    $rawAbortCallback = Join-Path $runtime 'raw-abort.callback'
    $rawAbortState = Join-Path $runtime 'raw-abort.state'
    Remove-Item -LiteralPath $rawAbortCallback, $rawAbortState -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $rawAbortState -Force | Out-Null
    $env:EXTENSION_DLL_LOADER_CONTRACT_MARKER = $rawAbortCallback
    $env:EXTENSION_DLL_LOADER_CONTRACT_STATE_DIR = $rawAbortState
    $env:EXTENSION_DLL_LOADER_CONTRACT_RAW_ABORT = '1'
    & $runner 'raw-abort' (Join-Path $runtime 'data.dll')
    if ($LASTEXITCODE -eq 0) { throw 'raw-abort fixture unexpectedly returned successfully' }
    Remove-Item Env:EXTENSION_DLL_LOADER_CONTRACT_RAW_ABORT -ErrorAction SilentlyContinue
    if (-not (Test-Path -LiteralPath $rawAbortCallback -PathType Leaf)) { throw 'raw-abort fixture did not enter the registrar callback' }
    Remove-Item -LiteralPath $rawAbortCallback -Force
    & $runner 'safe-mode-blocked' (Join-Path $runtime 'data.dll')
    if ($LASTEXITCODE -ne 0) { throw 'raw-abort residue was not denied by the next helper process' }
    if (Test-Path -LiteralPath $rawAbortCallback -PathType Leaf) { throw 'Safe Mode dispatched the blocked callback' }
    & $runner 'safe-mode-confirm' (Join-Path $runtime 'data.dll')
    if ($LASTEXITCODE -ne 0) { throw 'scoped Safe Mode confirmation did not re-enable the callback' }
    Assert-EmptyCallMarkerDirectory $rawAbortState

    $slowCallback = Join-Path $runtime 'slow.callback'
    $slowState = Join-Path $runtime 'slow.state'
    Remove-Item -LiteralPath $slowCallback, $slowState -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $slowState -Force | Out-Null
    $env:EXTENSION_DLL_LOADER_CONTRACT_MARKER = $slowCallback
    $env:EXTENSION_DLL_LOADER_CONTRACT_STATE_DIR = $slowState
    $env:EXTENSION_DLL_LOADER_CONTRACT_SLOW_MS = '75'
    & $runner 'slow' (Join-Path $runtime 'data.dll')
    if ($LASTEXITCODE -ne 0) { throw 'slow callback timing contract failed' }
    Remove-Item Env:EXTENSION_DLL_LOADER_CONTRACT_SLOW_MS -ErrorAction SilentlyContinue
    Assert-EmptyCallMarkerDirectory $slowState
    Invoke-RunnerScenario 'drain-timeout' @('drain-timeout',(Join-Path $runtime 'data.dll'))
    Write-Output 'extension DLL loader contract: PASS'
} finally {
    Remove-Item Env:EXTENSION_DLL_LOADER_CONTRACT_RAW_ABORT -ErrorAction SilentlyContinue
    Remove-Item Env:EXTENSION_DLL_LOADER_CONTRACT_SLOW_MS -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    if ($null -eq $savedHome) { Remove-Item Env:CARGO_HOME -ErrorAction SilentlyContinue } else { $env:CARGO_HOME = $savedHome }
    if ($null -eq $savedTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue } else { $env:CARGO_TARGET_DIR = $savedTarget }
}
