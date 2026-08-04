$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repo = (Resolve-Path (Join-Path $sdk '..')).Path
$scripts = Join-Path $sdk 'scripts'
$fixture = Join-Path $sdk 'fixtures\rust-folder-size-visual-column'
Import-Module (Join-Path $scripts 'canonical-store-zip.psm1') -Force
Import-Module (Join-Path $scripts 'sealed-cargo-authority.psm1') -Force
if ((Get-CanonicalZipCrc32 ([Text.Encoding]::UTF8.GetBytes('{}'))) -ne [Convert]::ToUInt32('a3a6bf43', 16)) {
    throw 'canonical ZIP CRC32 cannot represent a high-bit result'
}
foreach ($script in @('build-plugin.ps1','validate-plugin.ps1','package-plugin.ps1')) {
    [scriptblock]::Create((Get-Content (Join-Path $scripts $script) -Raw)) | Out-Null
}
$toolSources = @{
    build = Get-Content -LiteralPath (Join-Path $scripts 'build-plugin.ps1') -Raw
    validate = Get-Content -LiteralPath (Join-Path $scripts 'validate-plugin.ps1') -Raw
    package = Get-Content -LiteralPath (Join-Path $scripts 'package-plugin.ps1') -Raw
}
$readme = Get-Content -LiteralPath (Join-Path $sdk 'README.md') -Raw -Encoding UTF8
foreach ($required in @('ExtensionRegistrarImplementationV1', 'ExtensionRootModuleV1::new', 'abi_stable', 'Rust-first author')) {
    if (-not $readme.Contains($required)) { throw "README lost the Rust-first plugin author contract: $required" }
}
if ($readme.Contains('RegistrarCallbackV1::new')) { throw 'README advertises the removed author-facing RegistrarCallbackV1 API' }
$toolingDoc = Get-Content -LiteralPath (Join-Path $sdk 'PLUGIN_TOOLING.md') -Raw -Encoding UTF8
if (-not $toolingDoc.Contains('script_produced_sepack_reaches_production_native_lifecycle')) { throw 'PLUGIN_TOOLING.md lost the production native-lifecycle package gate' }
foreach ($required in @('zh-TW-p0-tooling','build.complete.json','plugin-tooling-wrapper-contract')) {
    if (-not $toolingDoc.Contains($required)) { throw "PLUGIN_TOOLING.md lost required zh-TW guidance: $required" }
}
$diagnosticsDoc = Get-Content -LiteralPath (Join-Path $sdk 'PLUGIN_DIAGNOSTICS.md') -Raw -Encoding UTF8
foreach ($required in @('zh-TW-p0-diagnostics','SESDK-PRIVATE','Safe Mode')) {
    if (-not $diagnosticsDoc.Contains($required)) { throw "PLUGIN_DIAGNOSTICS.md lost required zh-TW guidance: $required" }
}
foreach ($required in @('Get-ConsumerTreeDigest', 'Copy-BoundedConsumerSnapshot', 'ReparsePoint', 'private no-reparse build snapshot', 'CARGO_HOME', 'CARGO_BUILD_RUSTC', 'RUSTC_BOOTSTRAP', 'CARGO_INCREMENTAL', 'SUPEREXPLORER_TRUSTED_CARGO_SHA256', 'SUPEREXPLORER_TRUSTED_RUSTC_SHA256', '$PSHOME', '--manifest-path', 'publishedThisAttempt', 'build.complete.json', 'Repair-IncompleteBuildPublication', 'consumer_tree_sha256', '--locked --offline')) {
    if (-not $toolSources.build.Contains($required)) { throw "build wrapper lost Cargo authority or no-reparse control: $required" }
}
if ($toolSources.build.Contains('& powershell.exe')) { throw 'build wrapper may resolve PowerShell through caller PATH' }
if ($toolSources.build.Contains('manifest.payloads') -or $toolSources.build.Contains('$inputPaths')) { throw 'build wrapper may derive or open live author payload paths before the bounded core validation snapshot' }
foreach ($required in @('Copy-BoundedConsumerSnapshot', 'validation report escaped', 'ReparsePoint', 'CARGO_BUILD_RUSTC', 'RUSTC_BOOTSTRAP', 'CARGO_INCREMENTAL', 'SUPEREXPLORER_TRUSTED_CARGO_SHA256', 'SUPEREXPLORER_TRUSTED_RUSTC_SHA256', '--manifest-path', '--locked --offline')) {
    if (-not $toolSources.validate.Contains($required)) { throw "validate wrapper lost containment or Cargo authority control: $required" }
}
foreach ($required in @('private snapshot', 'Copy-BoundedConsumerSnapshot', 'stage-package', 'exact runtime manifest inventory', 'RUSTC_BOOTSTRAP', 'CARGO_INCREMENTAL', 'SUPEREXPLORER_TRUSTED_CARGO_SHA256', 'SUPEREXPLORER_TRUSTED_RUSTC_SHA256', '$PSHOME', "'manifest.json'", "'plugin/plugin.dll'", 'package input is a symlink or reparse point', 'Assert-NoReparseAncestors', "'package publication directory'", "'package staging directory'", 'Write-CanonicalStoreOnlyZip', 'complete-publication marker', 'Repair-StalePackageAttempts', 'build.complete.json', 'live bounded consumer tree does not match the build snapshot', 'injected package publication failure')) {
    if (-not $toolSources.package.Contains($required)) { throw "package wrapper lost snapshot/publication control: $required" }
}
if ($toolSources.package.Contains('& powershell.exe')) { throw 'package wrapper may resolve PowerShell through caller PATH' }
$pluginGates = Get-Content -LiteralPath (Join-Path $sdk 'ci\plugin-gates.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$uitest = Get-Content -LiteralPath (Join-Path $repo 'uitest\manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$diagnosticSchemaPath = Join-Path $sdk 'schemas\p0-diagnostics.schema.json'
$diagnosticSchema = Get-Content -LiteralPath $diagnosticSchemaPath -Raw -Encoding UTF8 | ConvertFrom-Json
$manifestSchemaPath = Join-Path $sdk 'schemas\p0-manifest.schema.json'
$manifestSchema = Get-Content -LiteralPath $manifestSchemaPath -Raw -Encoding UTF8 | ConvertFrom-Json
$privateDependencyPathPattern = [regex]::new([string]$manifestSchema.'$defs'.private_dependency.properties.path.pattern)
$diagnosticCodePattern = [regex]::new([string]$diagnosticSchema.properties.diagnostics.items.properties.code.pattern)
foreach ($actualCode in @(Select-String -LiteralPath (Join-Path $sdk 'tools\plugin-tooling\src\lib.rs') -Pattern 'SESDK-[A-Z-]+-[0-9]{3}' -AllMatches | ForEach-Object { $_.Matches.Value } | Sort-Object -Unique)) {
    if (-not $diagnosticCodePattern.IsMatch($actualCode)) { throw "P0 diagnostics schema rejects a core diagnostic code: $actualCode" }
}
function Get-OfflineWorkflowSteps([string]$Path) {
    $steps = @()
    $current = $null
    $collectRun = $false
    foreach ($line in @(Get-Content -LiteralPath $Path -Encoding UTF8)) {
        if ($line -match '^\s*- name:\s*(.+?)\s*$') {
            if ($null -ne $current) { $steps += [pscustomobject]$current }
            $current = [ordered]@{ name = $matches[1].Trim('"', "'"); run = '' }
            $collectRun = $false
            continue
        }
        if ($null -eq $current) { continue }
        if ($line -match '^\s*run:\s*(.*)$') {
            $value = $matches[1]
            $collectRun = $value -eq '|' -or $value -eq '>-'
            if (-not $collectRun) { $current.run = $value.Trim('"', "'") }
            continue
        }
        if ($collectRun) {
            if ($line -match '^\s{10,}(.+)$') { $current.run += "$($matches[1])`n" } else { $collectRun = $false }
        }
    }
    if ($null -ne $current) { $steps += [pscustomobject]$current }
    return @($steps)
}

$offlineSteps = Get-OfflineWorkflowSteps (Join-Path $repo '.github\workflows\sdk-offline-windows.yml')
foreach ($requirement in @($pluginGates.requirements)) {
    foreach ($kind in @('unit','integration','uitest','security','docs')) {
        foreach ($id in @($requirement.evidence.$kind)) {
            $cases = @($uitest.cases | Where-Object { [string]$_.id -eq $id })
            if ($kind -eq 'uitest' -and ($cases.Count -ne 1 -or $requirement.requirement_id -notin @($cases[0].covers))) { throw "trusted P0 UITEST evidence must have one case covering its exact requirement: $id" }
            $steps = @($offlineSteps | Where-Object { $_.name -eq $id })
            if ($steps.Count -ne 1 -or [string]::IsNullOrWhiteSpace($steps[0].run)) { throw "trusted P0 gate must have one nonempty named offline CI step: $id" }
            if ($cases.Count -eq 1) {
                $case = $cases[0]
                if ($case.program -eq 'powershell.exe') {
                    $fileIndex = [array]::IndexOf([string[]]$case.arguments, '-File')
                    if ($fileIndex -lt 0 -or $fileIndex + 1 -ge $case.arguments.Count -or -not $steps[0].run.Contains([IO.Path]::GetFileName([string]$case.arguments[$fileIndex + 1]))) { throw "offline CI step does not run the expected PowerShell contract: $id" }
                } elseif ($case.program -eq 'cargo.exe' -and -not $steps[0].run.Contains('cargo')) { throw "offline CI step does not run the expected Cargo contract: $id" }
            }
        }
    }
}

function Assert-Fails([scriptblock]$Action, [string]$Case) {
    $failed = $false
    try { & $Action } catch { $failed = $true }
    if (-not $failed) { throw "$Case unexpectedly succeeded" }
}

function Get-ToolingTemporaryArtifacts {
    return @(
        Get-ChildItem -LiteralPath ([IO.Path]::GetTempPath()) -Directory -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^superexplorer-(sealed-cargo|plugin-target|plugin-cargo|plugin-source|plugin-validate-cargo|plugin-validate-target|plugin-validate-inputs|package-inputs|package-synthesis-cargo|package-synthesis-target)-' } |
            ForEach-Object FullName
    )
}

function Assert-NoNewToolingTemporaryArtifacts([string[]]$Before, [string]$Case) {
    $leaks = @(Get-ToolingTemporaryArtifacts | Where-Object { $_ -notin $Before })
    if ($leaks.Count -ne 0) { throw "$Case leaked private tooling temporary artifacts: $($leaks -join ', ')" }
}

function Invoke-FailingReport([scriptblock]$Action, [string]$Case, [string]$PluginRoot) {
    $output = @()
    $failed = $false
    try { $output = @(& $Action) } catch { $failed = $true }
    if (-not $failed) { throw "$Case unexpectedly succeeded" }
    foreach ($line in @($output | ForEach-Object { [string]$_ } | Where-Object { $_.TrimStart().StartsWith('{') } | Select-Object -Last 1)) {
        try { return ($line | ConvertFrom-Json) } catch {}
    }
    # A terminating error from an in-process PowerShell script discards prior
    # pipeline output while its atomically published diagnostics report remains
    # available. Prefer that canonical report over weakening the failure check.
    $reportRoot = Join-Path $PluginRoot 'target\superexplorer'
    $publishedReport = @(Get-ChildItem -LiteralPath $reportRoot -Filter 'validation.json' -File -Recurse -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1)
    if ($publishedReport.Count -eq 1) {
        try { return (Get-Content -LiteralPath $publishedReport[0].FullName -Raw -Encoding UTF8 | ConvertFrom-Json) } catch {}
    }
    throw "$Case did not emit a serialized diagnostics report"
}

function Invoke-WithFakePowerShellPath([scriptblock]$Action) {
    $fakePath = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fake-powershell-' + [guid]::NewGuid().ToString('N'))
    $savedPath = [Environment]::GetEnvironmentVariable('PATH', 'Process')
    try {
        New-Item -ItemType Directory -Path $fakePath -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\cmd.exe') -Destination (Join-Path $fakePath 'powershell.exe')
        [Environment]::SetEnvironmentVariable('PATH', "$fakePath;$savedPath", 'Process')
        & $Action
    } finally {
        [Environment]::SetEnvironmentVariable('PATH', $savedPath, 'Process')
        if (Test-Path -LiteralPath $fakePath) { Remove-Item -LiteralPath $fakePath -Recurse -Force }
    }
}

function Start-WrapperPausedBeforePublication([string]$ScriptPath, [string]$PluginRoot, [string]$PauseEnvironmentName, [string]$SignalPath) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $PSHOME 'powershell.exe'
    $start.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptPath`" -PluginRoot `"$PluginRoot`""
    $start.UseShellExecute = $false
    $start.EnvironmentVariables[$PauseEnvironmentName] = $SignalPath
    $process = [Diagnostics.Process]::Start($start)
    for ($attempt = 0; $attempt -lt 600; $attempt++) {
        if (Test-Path -LiteralPath $SignalPath) { return $process }
        if ($process.HasExited) { throw "paused wrapper exited before its publication barrier ($PauseEnvironmentName)" }
        Start-Sleep -Milliseconds 100
    }
    if (-not $process.HasExited) { $process.Kill() }
    throw "timed out waiting for wrapper publication barrier ($PauseEnvironmentName)"
}

function Stop-InterruptedWrapper([Diagnostics.Process]$Process) {
    if (-not $Process.HasExited) { $Process.Kill() }
    $Process.WaitForExit()
    $Process.Dispose()
}

Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot (Join-Path $fixture 'missing') } 'missing root'

$temp = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-p0-plugin-' + [guid]::NewGuid().ToString('N'))
Copy-Item -LiteralPath $fixture -Destination $temp -Recurse
if (Test-Path -LiteralPath (Join-Path $temp 'target')) { Remove-Item -LiteralPath (Join-Path $temp 'target') -Recurse -Force }
try {
    $lock = Get-Content (Join-Path $sdk 'sdk-lock.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $fingerprint = Get-Content (Join-Path $sdk 'ui-abi-fingerprint.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($field in @('cargo_sha256','rustc_sha256')) {
        if ([string]$lock.toolchain.$field -notmatch '^[0-9a-f]{64}$') { throw "sdk-lock toolchain.$field is not an exact SHA-256" }
    }
    $fakeToolchainPath = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-fake-rustup-' + [guid]::NewGuid().ToString('N'))
    $savedFakePath = [Environment]::GetEnvironmentVariable('PATH', 'Process')
    $savedFakeUserProfile = [Environment]::GetEnvironmentVariable('USERPROFILE', 'Process')
    $savedFakeRustupHome = [Environment]::GetEnvironmentVariable('RUSTUP_HOME', 'Process')
    try {
        New-Item -ItemType Directory -Path $fakeToolchainPath -Force | Out-Null
        # These are executable-looking PATH shims. The authority resolver must
        # not consult any of them, including rustup.exe.
        foreach ($name in @('cargo.exe','rustc.exe','rustup.exe')) { Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\cmd.exe') -Destination (Join-Path $fakeToolchainPath $name) }
        [Environment]::SetEnvironmentVariable('PATH', "$fakeToolchainPath;$savedFakePath", 'Process')
        [Environment]::SetEnvironmentVariable('USERPROFILE', $fakeToolchainPath, 'Process')
        [Environment]::SetEnvironmentVariable('RUSTUP_HOME', $fakeToolchainPath, 'Process')
        $rustcReplacementDenied = $false
        $pathAuthority = New-SealedCargoAuthority $lock.toolchain {
            param($lockedRustc)
            try {
                $replacement = [IO.File]::Open($lockedRustc, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::None)
                $replacement.Dispose()
            } catch [IO.IOException] {
                $script:rustcReplacementDenied = $true
            }
        }
        try {
            if (-not $rustcReplacementDenied) { throw 'rustc replacement was not denied during sealed authority validation' }
            if ($pathAuthority.RustcSha256 -ne $lock.toolchain.rustc_sha256 -or $pathAuthority.Sha256 -ne $lock.toolchain.cargo_sha256) { throw 'PATH-prepended fake Rust shims replaced the pinned toolchain authority' }
            if ([IO.Path]::GetDirectoryName($pathAuthority.RustcPath) -notlike '*\.rustup\toolchains\1.97.1-x86_64-pc-windows-msvc\bin') { throw 'authority did not use the canonical installed 1.97.1 toolchain root' }
        } finally { Remove-SealedCargoAuthority $pathAuthority }
    } finally {
        [Environment]::SetEnvironmentVariable('PATH', $savedFakePath, 'Process')
        [Environment]::SetEnvironmentVariable('USERPROFILE', $savedFakeUserProfile, 'Process')
        [Environment]::SetEnvironmentVariable('RUSTUP_HOME', $savedFakeRustupHome, 'Process')
        if (Test-Path -LiteralPath $fakeToolchainPath) { Remove-Item -LiteralPath $fakeToolchainPath -Recurse -Force }
    }
    foreach ($mutation in @(
        @{ name = 'cargo hash'; field = 'cargo_sha256'; value = ('0' * 64) },
        @{ name = 'rustc hash'; field = 'rustc_sha256'; value = ('0' * 64) },
        @{ name = 'cargo commit'; field = 'cargo_commit_hash'; value = ('0' * 40) },
        @{ name = 'rustc commit'; field = 'rustc_commit_hash'; value = ('0' * 40) }
    )) {
        $tampered = ($lock.toolchain | ConvertTo-Json -Depth 4 | ConvertFrom-Json)
        $tampered.($mutation.field) = $mutation.value
        Assert-Fails { New-SealedCargoAuthority $tampered } "tampered pinned $($mutation.name)"
    }
    $source = Join-Path $temp 'src\lib.rs'
    $manifestPath = Join-Path $temp 'plugin-project.json'
    $boundedSource = Join-Path $temp 'bounded-snapshot-fixture'
    $boundedOutput = Join-Path $temp 'bounded-snapshot-output'
    try {
        New-Item -ItemType Directory -Path (Join-Path $boundedSource 'deep\child') -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $boundedSource 'one.txt'), '12', [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText((Join-Path $boundedSource 'two.txt'), '34', [Text.UTF8Encoding]::new($false))
        Assert-Fails { Copy-BoundedConsumerSnapshot $boundedSource $boundedOutput -MaxFileBytes 1 } 'bounded snapshot oversized file'
        if (Test-Path -LiteralPath $boundedOutput) { throw 'failed bounded snapshot created an output directory' }
        Assert-Fails { Copy-BoundedConsumerSnapshot $boundedSource $boundedOutput -MaxFiles 1 } 'bounded snapshot file count'
        if (Test-Path -LiteralPath $boundedOutput) { throw 'file-count rejection created a snapshot output directory' }
        Assert-Fails { Copy-BoundedConsumerSnapshot $boundedSource $boundedOutput -MaxDepth 1 } 'bounded snapshot depth'
        if (Test-Path -LiteralPath $boundedOutput) { throw 'depth rejection created a snapshot output directory' }
    } finally {
        foreach ($path in @($boundedOutput, $boundedSource)) { if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force } }
    }
    $testJson = Get-Command Test-Json -ErrorAction SilentlyContinue
    $positive = (Get-Content $manifestPath -Raw -Encoding UTF8).
        Replace('@SDK_BUNDLE_ID@', [string]$lock.bundle_id).
        Replace('@UI_ABI_FINGERPRINT@', [string]$fingerprint.fingerprint).
        Replace('@ABI_SCHEMA@', [string]$lock.build_policy.abi_schema_version).
        Replace('@SOURCE_SIZE@', [string](Get-Item $source).Length).
        Replace('@SOURCE_SHA256@', (Get-FileHash $source -Algorithm SHA256).Hash.ToLowerInvariant())
    [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))
    $positiveManifest = $positive | ConvertFrom-Json
    if (@($positiveManifest.private_dependencies).Count -ne 1 -or -not $privateDependencyPathPattern.IsMatch([string]$positiveManifest.private_dependencies[0].path)) {
        throw 'positive private dependency manifest does not satisfy the schema-derived vendor path contract'
    }
    if ($testJson -and -not (Test-Json -Json $positive -SchemaFile $manifestSchemaPath)) { throw 'positive private dependency manifest failed p0-manifest.schema.json' }

    & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp
    $validationReportPath = Join-Path $temp ("target\superexplorer\$($lock.bundle_id)\reports\validation.json")
    $validationReport = Get-Content -LiteralPath $validationReportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($validationReport.schema_version -ne 1 -or $validationReport.valid -isnot [bool] -or $validationReport.diagnostics -isnot [array] -or [string]$validationReport.inputs.consumer_tree_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'actual validation report does not satisfy the P0 diagnostics envelope and snapshot identity' }
    foreach ($diagnostic in @($validationReport.diagnostics)) {
        if (@($diagnostic.PSObject.Properties.Name | Sort-Object) -join ',' -ne 'code,message,path,phase,severity' -or -not $diagnosticCodePattern.IsMatch([string]$diagnostic.code) -or $diagnostic.severity -ne 'error') { throw 'actual validation report does not satisfy the P0 typed diagnostics schema' }
    }
    if ($testJson -and -not (Test-Json -Json (Get-Content -LiteralPath $validationReportPath -Raw -Encoding UTF8) -SchemaFile $diagnosticSchemaPath)) { throw 'actual validation report failed p0-diagnostics.schema.json' }

    $missingPayloadManifest = $positive | ConvertFrom-Json
    $missingPayloadManifest.payloads[0].path = 'src/missing payload.rs'
    $missingPayloadManifest.payloads[0].size = 1
    $missingPayloadManifest.payloads[0].sha256 = ('0' * 64)
    [IO.File]::WriteAllText($manifestPath, ($missingPayloadManifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    $failureReport = Invoke-FailingReport { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'missing payload diagnostics' $temp
    $failureReportJson = $failureReport | ConvertTo-Json -Depth 20 -Compress
    if ($failureReport.schema_version -ne 1 -or $failureReport.valid -ne $false -or @($failureReport.diagnostics).Count -eq 0) { throw 'missing payload did not produce a failing typed diagnostics report' }
    foreach ($diagnostic in @($failureReport.diagnostics)) {
        if (@($diagnostic.PSObject.Properties.Name | Sort-Object) -join ',' -ne 'code,message,path,phase,severity' -or -not $diagnosticCodePattern.IsMatch([string]$diagnostic.code) -or $diagnostic.severity -ne 'error') { throw 'failing validation report does not satisfy the P0 typed diagnostics schema' }
    }
    if ($failureReportJson.Contains($temp) -or $failureReportJson -match '[A-Za-z]:[\\/]' -or $failureReportJson -match '\\\\') { throw 'serialized diagnostics leaked an absolute or UNC plugin-root path' }
    if ($testJson -and -not (Test-Json -Json $failureReportJson -SchemaFile $diagnosticSchemaPath)) { throw 'failing validation report failed p0-diagnostics.schema.json' }
    $publishedFailureReport = Get-Content -LiteralPath $validationReportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($publishedFailureReport.valid -ne $false -or @($publishedFailureReport.diagnostics).Count -eq 0) { throw 'failed validation left a stale valid report' }
    [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))
    & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp | Out-Null
    $oldValidationPublicationFailure = $env:SUPEREXPLORER_VALIDATE_TEST_FAIL_PUBLICATION
    $validationTempsBefore = Get-ToolingTemporaryArtifacts
    try {
        $env:SUPEREXPLORER_VALIDATE_TEST_FAIL_PUBLICATION = '1'
        Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'injected validation report publication failure'
    } finally {
        if ($null -eq $oldValidationPublicationFailure) { Remove-Item Env:SUPEREXPLORER_VALIDATE_TEST_FAIL_PUBLICATION -ErrorAction SilentlyContinue } else { $env:SUPEREXPLORER_VALIDATE_TEST_FAIL_PUBLICATION = $oldValidationPublicationFailure }
    }
    if (Test-Path -LiteralPath $validationReportPath) { throw 'failed validation report publication left a stale or partial report' }
    Assert-NoNewToolingTemporaryArtifacts $validationTempsBefore 'failed validation report publication'
    & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp | Out-Null
    $oldValidationSnapshotMutation = $env:SUPEREXPLORER_VALIDATE_TEST_MUTATE_AFTER_SNAPSHOT
    $validationSnapshotTempsBefore = Get-ToolingTemporaryArtifacts
    try {
        $env:SUPEREXPLORER_VALIDATE_TEST_MUTATE_AFTER_SNAPSHOT = '1'
        Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'validation mutation after bounded snapshot'
    } finally {
        if ($null -eq $oldValidationSnapshotMutation) { Remove-Item Env:SUPEREXPLORER_VALIDATE_TEST_MUTATE_AFTER_SNAPSHOT -ErrorAction SilentlyContinue } else { $env:SUPEREXPLORER_VALIDATE_TEST_MUTATE_AFTER_SNAPSHOT = $oldValidationSnapshotMutation }
        [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))
    }
    if (Test-Path -LiteralPath $validationReportPath) { throw 'validation snapshot race left a stale identity report' }
    Assert-NoNewToolingTemporaryArtifacts $validationSnapshotTempsBefore 'validation snapshot race rejection'
    & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp | Out-Null

    [IO.File]::WriteAllText((Join-Path $temp 'rust-toolchain.toml'), "[toolchain]`nchannel = '1.0.0'`n", [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'consumer Rustup override'
    Remove-Item -LiteralPath (Join-Path $temp 'rust-toolchain.toml') -Force

    $manifest = $positive | ConvertFrom-Json
    $manifest | Add-Member -NotePropertyName program -NotePropertyValue 'cmd.exe'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'unknown command injection field'

    $manifest = $positive | ConvertFrom-Json
    $manifest.payloads[0].path = '../escape.dll'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'unsafe payload path'

    $outsideSentinel = Join-Path ([IO.Path]::GetDirectoryName($temp)) ('superexplorer-outside-sentinel-' + [guid]::NewGuid().ToString('N') + '.txt')
    try {
        [IO.File]::WriteAllText($outsideSentinel, 'must-never-be-opened-by-build', [Text.UTF8Encoding]::new($false))
        $manifest = $positive | ConvertFrom-Json
        $manifest.payloads[0].path = '../' + [IO.Path]::GetFileName($outsideSentinel)
        [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
        Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp } 'build rejects traversal payload without opening an outside sentinel'
        if ([IO.File]::ReadAllText($outsideSentinel, [Text.UTF8Encoding]::new($false)) -ne 'must-never-be-opened-by-build') { throw 'build modified the outside traversal sentinel' }
    } finally { Remove-Item -LiteralPath $outsideSentinel -Force -ErrorAction SilentlyContinue }

    $largeManifest = $positive + (' ' * ((1MB + 1) - [Text.Encoding]::UTF8.GetByteCount($positive)))
    [IO.File]::WriteAllText($manifestPath, $largeManifest, [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp } 'build rejects oversized manifest before JSON parsing'

    $manifest = $positive | ConvertFrom-Json
    $manifest.sdk.bundle_id = 'wrong-bundle'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'wrong SDK bundle'

    $manifest = $positive | ConvertFrom-Json
    $manifest.payloads[0].sha256 = '0' * 64
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'payload hash drift'

    $manifest = $positive | ConvertFrom-Json
    $manifest.verification.requirements[0].requirement_id = 'unknown/requirement'
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } 'unknown trusted gate mapping'

    $originalSource = Get-Content -LiteralPath $source -Raw
    $missingRootSource = $originalSource.Replace('#[export_root_module]', '')
    if ($missingRootSource -eq $originalSource) { throw 'P0 fixture no longer contains the expected root export attribute' }
    [IO.File]::WriteAllText($source, $missingRootSource, [Text.UTF8Encoding]::new($false))
    $missingRootManifest = $positive | ConvertFrom-Json
    $missingRootManifest.payloads[0].size = (Get-Item -LiteralPath $source).Length
    $missingRootManifest.payloads[0].sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($manifestPath, ($missingRootManifest | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp } 'cdylib without abi_stable loader export'
    $unexpectedBuild = Join-Path $temp ("target\superexplorer\$($lock.bundle_id)\reports\build.json")
    if (Test-Path -LiteralPath $unexpectedBuild) { throw 'failed ABI inspection published a build report' }

    [IO.File]::WriteAllText($source, $originalSource, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))

    foreach ($override in @('RUSTC_BOOTSTRAP','CARGO_INCREMENTAL')) {
        $oldOverride = [Environment]::GetEnvironmentVariable($override, 'Process')
        try {
            [Environment]::SetEnvironmentVariable($override, '1', 'Process')
            Assert-Fails { & (Join-Path $scripts 'validate-plugin.ps1') -PluginRoot $temp } "ambient $override"
            Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp } "ambient build $override"
            Assert-Fails { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp } "ambient package $override"
        } finally {
            [Environment]::SetEnvironmentVariable($override, $oldOverride, 'Process')
        }
    }
    $oldBuildMutation = $env:SUPEREXPLORER_BUILD_TEST_MUTATE_AFTER_SNAPSHOT
    $buildTempsBefore = Get-ToolingTemporaryArtifacts
    try {
        $env:SUPEREXPLORER_BUILD_TEST_MUTATE_AFTER_SNAPSHOT = '1'
        Assert-Fails { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp } 'mutation after build snapshot'
    } finally {
        if ($null -eq $oldBuildMutation) { Remove-Item Env:SUPEREXPLORER_BUILD_TEST_MUTATE_AFTER_SNAPSHOT -ErrorAction SilentlyContinue } else { $env:SUPEREXPLORER_BUILD_TEST_MUTATE_AFTER_SNAPSHOT = $oldBuildMutation }
        [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))
    }
    if (Test-Path -LiteralPath (Join-Path $temp ("target\superexplorer\$($lock.bundle_id)\build"))) { throw 'build snapshot race published a DLL generation' }
    Assert-NoNewToolingTemporaryArtifacts $buildTempsBefore 'build snapshot race rejection'
    # A killed producer can expose files before its completion marker. The
    # observer must reject that incomplete generation, and the next build must
    # boundedly repair it before producing a fresh marked generation.
    $buildRoot = Join-Path $temp ("target\superexplorer\$($lock.bundle_id)")
    $buildMarker = Join-Path $buildRoot 'reports\build.complete.json'
    $buildPauseSignal = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-build-publication-' + [guid]::NewGuid().ToString('N') + '.ready')
    $buildChildTempsBefore = Get-ToolingTemporaryArtifacts
    $interruptedBuild = $null
    try {
        $interruptedBuild = Start-WrapperPausedBeforePublication (Join-Path $scripts 'build-plugin.ps1') $temp 'SUPEREXPLORER_BUILD_TEST_WAIT_BEFORE_COMPLETE_MARKER' $buildPauseSignal
        if (-not (Test-Path -LiteralPath (Join-Path $buildRoot 'build\plugin.dll')) -or -not (Test-Path -LiteralPath (Join-Path $buildRoot 'reports\build.json')) -or (Test-Path -LiteralPath $buildMarker)) {
            throw 'build observer did not see the expected incomplete unmarked generation'
        }
        Assert-Fails { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp } 'package observer rejects an unmarked killed build generation'
    } finally {
        if ($interruptedBuild) { Stop-InterruptedWrapper $interruptedBuild }
        Remove-Item -LiteralPath $buildPauseSignal -Force -ErrorAction SilentlyContinue
        # A hard process kill bypasses PowerShell finally blocks. The recovery
        # contract is for output attempts; remove the test child's private temp
        # directories here so this test does not pollute subsequent gates.
        foreach ($leak in @(Get-ToolingTemporaryArtifacts | Where-Object { $_ -notin $buildChildTempsBefore })) { Remove-Item -LiteralPath $leak -Recurse -Force }
    }
    if (Test-Path -LiteralPath $buildMarker) { throw 'killed build producer unexpectedly published a completion marker' }
    Invoke-WithFakePowerShellPath { & (Join-Path $scripts 'build-plugin.ps1') -PluginRoot $temp | Out-Null }
    $buildReport = Join-Path $temp ("target\superexplorer\$($lock.bundle_id)\reports\build.json")
    if (-not (Test-Path -LiteralPath $buildReport)) { throw 'build report was not retained' }
    if (-not (Test-Path -LiteralPath $buildMarker)) { throw 'recovered build generation omitted its final completion marker' }
    $buildJson = Get-Content -LiteralPath $buildReport -Raw -Encoding UTF8 | ConvertFrom-Json
    $buildComplete = Get-Content -LiteralPath $buildMarker -Raw -Encoding UTF8 | ConvertFrom-Json
    if ([string]$buildJson.inputs.consumer_tree_sha256 -notmatch '^[0-9a-f]{64}$' -or $buildComplete.consumer_tree_sha256 -ne $buildJson.inputs.consumer_tree_sha256) { throw 'build marker does not bind the private consumer snapshot digest' }
    if (-not (Test-Path -LiteralPath (Join-Path $temp ("target\superexplorer\$($lock.bundle_id)\reports\validation.json")))) { throw 'validate-to-build publication did not retain the validation report' }
    [IO.File]::AppendAllText($source, "`n// ordinary source drift after build`n", [Text.UTF8Encoding]::new($false))
    Assert-Fails { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp } 'ordinary source mutation after build snapshot'
    [IO.File]::WriteAllText($source, $originalSource, [Text.UTF8Encoding]::new($false))
    $dist = Join-Path $temp 'dist'
    $oldInjectedFailure = $env:SUPEREXPLORER_PACKAGE_TEST_FAIL_AFTER_SIDECAR
    $packagePublicationTempsBefore = Get-ToolingTemporaryArtifacts
    try {
        $env:SUPEREXPLORER_PACKAGE_TEST_FAIL_AFTER_SIDECAR = '1'
        Assert-Fails { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp } 'injected sidecar publication failure'
    } finally {
        if ($null -eq $oldInjectedFailure) { Remove-Item Env:SUPEREXPLORER_PACKAGE_TEST_FAIL_AFTER_SIDECAR -ErrorAction SilentlyContinue } else { $env:SUPEREXPLORER_PACKAGE_TEST_FAIL_AFTER_SIDECAR = $oldInjectedFailure }
    }
    if ((Test-Path -LiteralPath $dist) -and @(Get-ChildItem -LiteralPath $dist -Force).Count -ne 0) {
        throw 'injected publication failure left a partial package output'
    }
    Assert-NoNewToolingTemporaryArtifacts $packagePublicationTempsBefore 'package publication failure'
    $packagePauseSignal = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-package-publication-' + [guid]::NewGuid().ToString('N') + '.ready')
    $packageChildTempsBefore = Get-ToolingTemporaryArtifacts
    $interruptedPackage = $null
    try {
        $interruptedPackage = Start-WrapperPausedBeforePublication (Join-Path $scripts 'package-plugin.ps1') $temp 'SUPEREXPLORER_PACKAGE_TEST_WAIT_AFTER_SIDECAR' $packagePauseSignal
        $partialSidecars = @(Get-ChildItem -LiteralPath $dist -File -Force -ErrorAction Stop)
        if ($partialSidecars.Count -ne 1 -or $partialSidecars[0].Name -notlike '*.sepack.sha256' -or @(Get-ChildItem -LiteralPath $dist -Filter '*.sepack' -File -Force).Count -ne 0) {
            throw 'package observer did not see the expected sidecar-only unmarked publication'
        }
    } finally {
        if ($interruptedPackage) { Stop-InterruptedWrapper $interruptedPackage }
        Remove-Item -LiteralPath $packagePauseSignal -Force -ErrorAction SilentlyContinue
        foreach ($leak in @(Get-ToolingTemporaryArtifacts | Where-Object { $_ -notin $packageChildTempsBefore })) { Remove-Item -LiteralPath $leak -Recurse -Force }
    }
    $package = Invoke-WithFakePowerShellPath { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp }
    $firstPackageHash = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash
    $packageBytes = [IO.File]::ReadAllBytes($package)
    if ($packageBytes.Length -lt 30 -or [BitConverter]::ToUInt32($packageBytes, 0) -ne 0x04034b50 -or [BitConverter]::ToUInt16($packageBytes, 8) -ne 0) {
        throw 'package is not a store-only ZIP: local-header compression method at offset 8 must be zero'
    }
    Add-Type -AssemblyName System.IO.Compression
    $archiveStream = [IO.File]::OpenRead($package)
    try {
        $archive = [IO.Compression.ZipArchive]::new($archiveStream, [IO.Compression.ZipArchiveMode]::Read, $false, [Text.Encoding]::UTF8)
        try {
            $archiveNames = @($archive.Entries | ForEach-Object FullName)
            if ($archiveNames -contains 'manifest/plugin-project.json' -or $archiveNames -notcontains 'manifest.json' -or $archiveNames -notcontains 'plugin/plugin.dll') { throw 'package did not expose the production runtime manifest and DLL payload at the required archive paths' }
            $manifestEntry = @($archive.Entries | Where-Object FullName -eq 'manifest.json')
            if ($manifestEntry.Count -ne 1) { throw 'package runtime manifest entry is missing or duplicated' }
            $reader = [IO.StreamReader]::new($manifestEntry[0].Open(), [Text.UTF8Encoding]::new($false), $true)
            try { $runtimeManifest = ($reader.ReadToEnd() | ConvertFrom-Json) } finally { $reader.Dispose() }
            $expectedArchiveNames = @((@('manifest.json') + @($runtimeManifest.payloads | ForEach-Object { [string]$_.path })) | Sort-Object -CaseSensitive)
            $actualArchiveInventory = (@($archiveNames | Sort-Object -CaseSensitive) -join "`n")
            $expectedArchiveInventory = ($expectedArchiveNames -join "`n")
            if ($actualArchiveInventory -ne $expectedArchiveInventory) { throw 'package archive differs from the exact core runtime manifest inventory' }
            Assert-CanonicalStoreOnlyZip $package ([string[]]$expectedArchiveNames)
            $dllEntry = @($archive.Entries | Where-Object FullName -eq 'plugin/plugin.dll')
            if ($runtimeManifest.manifest_version -ne 1 -or $runtimeManifest.signature.kind -ne 'unsigned' -or $runtimeManifest.rust.Count -ne 1 -or $runtimeManifest.rust[0].entrypoint -ne 'plugin/plugin.dll' -or @($runtimeManifest.payloads | Where-Object { $_.path -eq 'plugin/plugin.dll' -and $_.kind -eq 'rust_dll' -and $_.size -eq $dllEntry[0].Length }).Count -ne 1) { throw 'runtime PackageManifestV1 does not bind the packaged DLL or local-developer provenance correctly' }
            $dllEntryStream = $dllEntry[0].Open()
            try {
                $sha = [Security.Cryptography.SHA256]::Create()
                try { $runtimeDllHash = ([BitConverter]::ToString($sha.ComputeHash($dllEntryStream))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
            } finally { $dllEntryStream.Dispose() }
            $dllPayload = @($runtimeManifest.payloads | Where-Object { $_.path -eq 'plugin/plugin.dll' })[0]
            if ($dllPayload.sha256 -ne $runtimeDllHash) { throw 'runtime PackageManifestV1 DLL hash does not match the archive payload' }
            if (@($runtimeManifest.publisher.contacts | Where-Object { $_.kind -eq 'email' -and $_.purposes -contains 'support' }).Count -eq 0) { throw 'runtime PackageManifestV1 lost publisher support contact semantics' }
            $licensePayloads = @($runtimeManifest.payloads | Where-Object { $_.kind -eq 'license' })
            if ($licensePayloads.Count -ne 2 -or @($licensePayloads | Where-Object { $_.path -notmatch '^licenses/private/exif-lite-0\.1\.0/LICENSE-(APACHE|MIT)$' }).Count -ne 0) { throw 'private dependency licenses were not staged into the runtime package inventory' }
            $noticePayload = @($runtimeManifest.payloads | Where-Object { $_.path -eq 'notices/private-dependencies.json' -and $_.kind -eq 'notice' })
            if ($noticePayload.Count -ne 1) { throw 'private dependency provenance notice was not staged into the runtime package inventory' }
            foreach ($payload in @($runtimeManifest.payloads)) {
                $entry = @($archive.Entries | Where-Object FullName -eq $payload.path)
                if ($entry.Count -ne 1 -or $entry[0].Length -ne $payload.size) { throw 'runtime package inventory entry size differs from manifest' }
                $payloadStream = $entry[0].Open()
                try {
                    $sha = [Security.Cryptography.SHA256]::Create()
                    try { $payloadHash = ([BitConverter]::ToString($sha.ComputeHash($payloadStream))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
                } finally { $payloadStream.Dispose() }
                if ($payloadHash -ne $payload.sha256) { throw 'runtime package inventory entry hash differs from manifest' }
            }
            $noticeReader = [IO.StreamReader]::new((@($archive.Entries | Where-Object FullName -eq 'notices/private-dependencies.json')[0].Open()), [Text.UTF8Encoding]::new($false), $true)
            try { $privateNotice = $noticeReader.ReadToEnd() | ConvertFrom-Json } finally { $noticeReader.Dispose() }
            if ($privateNotice.schema_version -ne 1 -or @($privateNotice.private_dependencies | Where-Object { $_.name -eq 'exif-lite' -and $_.version -eq '0.1.0' -and $_.vendor_path -eq 'vendor/private/exif-lite-0.1.0' }).Count -ne 1) { throw 'private dependency provenance notice does not bind the vendored fixture' }
        } finally { $archive.Dispose() }
    } finally { $archiveStream.Dispose() }
    # The script producer and the host importer must agree on the exact archive
    # bytes; this ignored host test drives LocalDeveloperPackageStoreV1,
    # SePackImporterV1, PackageValidatorV1, and NativeExtensionLifecycleV1
    # rather than a test-only reader.
    $savedSePackPath = [Environment]::GetEnvironmentVariable('SUPEREXPLORER_TEST_SEPACK_PATH', 'Process')
    $savedHostRustc = [Environment]::GetEnvironmentVariable('RUSTC', 'Process')
    $savedHostCargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME', 'Process')
    $hostCargoHome = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-host-gate-cargo-' + [guid]::NewGuid().ToString('N'))
    $hostTestAuthority = $null
    $hostTestPushed = $false
    try {
        $env:SUPEREXPLORER_TEST_SEPACK_PATH = [IO.Path]::GetFullPath($package)
        $hostTestAuthority = New-SealedCargoAuthority $lock.toolchain
        New-Item -ItemType Directory -Path $hostCargoHome -Force | Out-Null
        $configPath = & powershell.exe -NoProfile -File (Join-Path $repo 'sdk\scripts\prepare-local-cargo-source.ps1') -PluginRoot (Join-Path $sdk 'fixtures\host-gate')
        Copy-Item -LiteralPath $configPath -Destination (Join-Path $hostCargoHome 'config.toml') -Force
        $env:CARGO_HOME = $hostCargoHome
        $env:RUSTC = $hostTestAuthority.RustcPath
        # Run through a standalone fixture workspace.  Invoking the root
        # workspace here makes Cargo resolve unrelated application members
        # (including reqwest) that are intentionally absent from the SDK
        # vendor snapshot.  The fixture depends only on the production host
        # crate and carries its own minimal lockfile.
        $hostFixtureManifest = Join-Path $sdk 'fixtures\host-gate\Cargo.toml'
        if (-not (Test-Path -LiteralPath $hostFixtureManifest -PathType Leaf)) { throw 'isolated host-gate fixture manifest is missing' }
        Push-Location $repo
        $hostTestPushed = $true
        & $hostTestAuthority.Path test --manifest-path $hostFixtureManifest --locked --offline script_produced_sepack_reaches_production_native_lifecycle -- --exact
        if ($LASTEXITCODE -ne 0) { throw 'script-produced .sepack failed the production local-developer importer/validator/resolver/native-lifecycle gate' }
    } finally {
        if ($hostTestPushed) { Pop-Location }
        [Environment]::SetEnvironmentVariable('SUPEREXPLORER_TEST_SEPACK_PATH', $savedSePackPath, 'Process')
        [Environment]::SetEnvironmentVariable('RUSTC', $savedHostRustc, 'Process')
        [Environment]::SetEnvironmentVariable('CARGO_HOME', $savedHostCargoHome, 'Process')
        Remove-SealedCargoAuthority $hostTestAuthority
        if (Test-Path -LiteralPath $hostCargoHome) { Remove-Item -LiteralPath $hostCargoHome -Recurse -Force }
    }
    $secondPackage = & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp
    if ($package -ne $secondPackage -or $firstPackageHash -ne (Get-FileHash -LiteralPath $secondPackage -Algorithm SHA256).Hash) {
        throw 'repeated packaging was not byte-identical'
    }
    foreach ($sidecar in @("$package.sha256", ($package -replace '\.sepack$', '.package-report.json'))) {
        if (-not (Test-Path -LiteralPath $sidecar)) { throw "complete package publication omitted sidecar: $sidecar" }
    }
    $oldMutation = $env:SUPEREXPLORER_PACKAGE_TEST_MUTATE_AFTER_SNAPSHOT
    $packageMutationTempsBefore = Get-ToolingTemporaryArtifacts
    try {
        $env:SUPEREXPLORER_PACKAGE_TEST_MUTATE_AFTER_SNAPSHOT = '1'
        Assert-Fails { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp } 'mutation after package snapshot'
    } finally {
        if ($null -eq $oldMutation) { Remove-Item Env:SUPEREXPLORER_PACKAGE_TEST_MUTATE_AFTER_SNAPSHOT -ErrorAction SilentlyContinue } else { $env:SUPEREXPLORER_PACKAGE_TEST_MUTATE_AFTER_SNAPSHOT = $oldMutation }
    }
    if ($firstPackageHash -ne (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash) { throw 'race rejection changed the published package' }
    Assert-NoNewToolingTemporaryArtifacts $packageMutationTempsBefore 'package snapshot race rejection'
    [IO.File]::WriteAllText($manifestPath, $positive, [Text.UTF8Encoding]::new($false))
    [IO.File]::AppendAllText((Join-Path $temp "target\superexplorer\$($lock.bundle_id)\build\plugin.dll"), 'tamper')
    Assert-Fails { & (Join-Path $scripts 'package-plugin.ps1') -PluginRoot $temp } 'changed DLL after build'
    if ($firstPackageHash -ne (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash) {
        throw 'failed packaging changed the existing package'
    }
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}

Write-Output 'plugin tooling wrapper self-test passed'
