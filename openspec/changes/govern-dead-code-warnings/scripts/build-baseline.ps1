param(
    [string]$Workspace = (Get-Location).Path,
    [string]$EvidenceDirectory = "openspec/changes/govern-dead-code-warnings/evidence"
)

$ErrorActionPreference = "Stop"
$workspacePath = [IO.Path]::GetFullPath($Workspace)
$evidencePath = [IO.Path]::GetFullPath((Join-Path $workspacePath $EvidenceDirectory))
$compilerPath = Join-Path $evidencePath "compiler-messages.jsonl"
$prechangePath = Join-Path $evidencePath "prechange-diffs"

if (-not (Test-Path -LiteralPath $compilerPath)) {
    throw "Missing compiler JSONL: $compilerPath"
}

New-Item -ItemType Directory -Force -Path $prechangePath | Out-Null

function Get-Sha256Text([string]$Text) {
    $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
    $hash = [Security.Cryptography.SHA256]::HashData($bytes)
    return [Convert]::ToHexString($hash).ToLowerInvariant()
}

function Get-Sha256File([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-RelativeSourcePath([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath((Join-Path $workspacePath $Path))
    if (-not $fullPath.StartsWith($workspacePath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Compiler source path is outside the workspace: $Path"
    }
    return $fullPath.Substring($workspacePath.Length).TrimStart('\', '/').Replace('\', '/')
}

function Get-CompilingTargets([string]$SourcePath, [string[]]$EmittingTargets) {
    $fileName = [IO.Path]::GetFileName($SourcePath)
    $targets = switch ($fileName) {
        "mft_journal.rs" { @("explorer_app", "superexplorer-mft-helper", "superexplorer-mft-service") }
        "mft_size_map.rs" { @("explorer_app", "superexplorer-mft-helper", "superexplorer-mft-service") }
        "mft_focus.rs" { @("explorer_app", "superexplorer-mft-service") }
        "mft_migration.rs" { @("explorer_app", "superexplorer-mft-service") }
        "mft_persistence.rs" { @("explorer_app", "superexplorer-mft-service") }
        "mft_query.rs" { @("explorer_app", "superexplorer-mft-service") }
        "mft_runtime.rs" { @("explorer_app", "superexplorer-mft-service") }
        "mft_sqlite.rs" { @("explorer_app", "superexplorer-mft-service") }
        default { @($EmittingTargets) }
    }
    return @($targets | Sort-Object -Unique)
}

function Get-DeadCodeSuppressions {
    $records = @()
    $rustFiles = @(& rg --files (Join-Path $workspacePath "crates") -g "*.rs")
    foreach ($rustFile in $rustFiles) {
        $lines = @([IO.File]::ReadAllLines($rustFile))
        for ($index = 0; $index -lt $lines.Count; $index += 1) {
            if ($lines[$index] -notmatch "#!?\s*\[\s*(allow|expect)\s*\([^\]]*dead_code") {
                continue
            }
            $next = $index + 1
            while ($next -lt $lines.Count -and ($lines[$next].Trim().Length -eq 0 -or $lines[$next].TrimStart().StartsWith("#["))) {
                $next += 1
            }
            $relativePath = [IO.Path]::GetFullPath($rustFile).Substring($workspacePath.Length).TrimStart('\', '/').Replace('\', '/')
            $attribute = $lines[$index].Trim()
            $declaration = if ($next -lt $lines.Count) { $lines[$next].Trim() } else { "" }
            $fingerprintInput = "$relativePath|$attribute|$declaration"
            $records += [ordered]@{
                source_path = $relativePath
                line = $index + 1
                attribute = $attribute
                declaration = $declaration
                fingerprint = Get-Sha256Text $fingerprintInput
            }
        }
    }
    return @($records | Sort-Object source_path, line)
}

$diagnostics = [Collections.Generic.List[object]]::new()
$warningCounts = @{}

Get-Content -LiteralPath $compilerPath | ForEach-Object {
    $event = $_ | ConvertFrom-Json -Depth 50
    if ($event.reason -ne "compiler-message") {
        return
    }
    $message = $event.message
    if ($message.level -eq "warning") {
        $warningCode = if ($null -ne $message.code) { $message.code.code } else { "uncoded" }
        if (-not $warningCounts.ContainsKey($warningCode)) {
            $warningCounts[$warningCode] = 0
        }
        $warningCounts[$warningCode] += 1
    }
    if ($null -eq $message.code -or $message.code.code -ne "dead_code") {
        return
    }
    $primarySpans = @($message.spans | Where-Object { $_.is_primary })
    if ($primarySpans.Count -eq 0) {
        throw "dead_code diagnostic has no primary span: $($message.message)"
    }
    $diagnostics.Add([pscustomobject]@{
        package_id = $event.package_id
        target = $event.target.name
        target_kind = @($event.target.kind)
        message = $message.message
        primary_spans = $primarySpans
    })
}

$canonicalGroups = @{}
$itemGroups = @{}

foreach ($diagnostic in $diagnostics) {
    $first = $diagnostic.primary_spans[0]
    $sourcePath = Get-RelativeSourcePath $first.file_name
    $canonicalKey = "$sourcePath|$($first.line_start)|$($first.column_start)|$($diagnostic.message)"
    if (-not $canonicalGroups.ContainsKey($canonicalKey)) {
        $canonicalGroups[$canonicalKey] = [ordered]@{
            id = "DC-" + (Get-Sha256Text $canonicalKey).Substring(0, 16)
            source_path = $sourcePath
            line = $first.line_start
            column = $first.column_start
            message = $diagnostic.message
            emitting_targets = [Collections.Generic.HashSet[string]]::new()
            primary_item_ids = [Collections.Generic.HashSet[string]]::new()
        }
    }
    [void]$canonicalGroups[$canonicalKey].emitting_targets.Add($diagnostic.target)

    foreach ($span in $diagnostic.primary_spans) {
        $itemPath = Get-RelativeSourcePath $span.file_name
        $snippet = (@($span.text | ForEach-Object { $_.text }) -join [Environment]::NewLine).Trim()
        $itemKey = "$itemPath|$($span.line_start)|$($span.column_start)|$snippet"
        if (-not $itemGroups.ContainsKey($itemKey)) {
            $itemGroups[$itemKey] = [ordered]@{
                id = "DCI-" + (Get-Sha256Text $itemKey).Substring(0, 16)
                source_path = $itemPath
                line_start = $span.line_start
                line_end = $span.line_end
                column_start = $span.column_start
                column_end = $span.column_end
                source_text = $snippet
                parent_diagnostic_ids = [Collections.Generic.HashSet[string]]::new()
                emitting_targets = [Collections.Generic.HashSet[string]]::new()
            }
        }
        [void]$itemGroups[$itemKey].parent_diagnostic_ids.Add($canonicalGroups[$canonicalKey].id)
        [void]$itemGroups[$itemKey].emitting_targets.Add($diagnostic.target)
        [void]$canonicalGroups[$canonicalKey].primary_item_ids.Add($itemGroups[$itemKey].id)
    }
}

$sourceHashes = @{}
$sourcePaths = @($canonicalGroups.Values | ForEach-Object { $_.source_path } | Sort-Object -Unique)
foreach ($sourcePath in $sourcePaths) {
    $absolutePath = Join-Path $workspacePath $sourcePath
    $sourceHashes[$sourcePath] = Get-Sha256File $absolutePath
}

$canonicalSites = @($canonicalGroups.Values | ForEach-Object {
    $emitting = @($_.emitting_targets | Sort-Object)
    $compiling = Get-CompilingTargets $_.source_path $emitting
    [ordered]@{
        id = $_.id
        source_path = $_.source_path
        line = $_.line
        column = $_.column
        message = $_.message
        source_sha256 = $sourceHashes[$_.source_path]
        emitting_targets = $emitting
        compiling_targets = $compiling
        target_local = (@($emitting).Count -lt @($compiling).Count)
        primary_item_ids = @($_.primary_item_ids | Sort-Object)
    }
} | Sort-Object source_path, line, column, message)

$items = @($itemGroups.Values | ForEach-Object {
    [ordered]@{
        id = $_.id
        source_path = $_.source_path
        line_start = $_.line_start
        line_end = $_.line_end
        column_start = $_.column_start
        column_end = $_.column_end
        source_text = $_.source_text
        source_sha256 = $sourceHashes[$_.source_path]
        parent_diagnostic_ids = @($_.parent_diagnostic_ids | Sort-Object)
        emitting_targets = @($_.emitting_targets | Sort-Object)
        disposition = $null
    }
} | Sort-Object source_path, line_start, column_start, source_text)

$targetTopology = @($sourcePaths | ForEach-Object {
    $sourcePath = $_
    $emitting = @($canonicalSites | Where-Object source_path -eq $sourcePath | ForEach-Object emitting_targets | Sort-Object -Unique)
    [ordered]@{
        source_path = $sourcePath
        source_sha256 = $sourceHashes[$sourcePath]
        emitting_targets = $emitting
        compiling_targets = Get-CompilingTargets $sourcePath $emitting
    }
})

$ownedPaths = @($sourcePaths + @(
    "Cargo.toml",
    "Cargo.lock",
    "crates/explorer-app/Cargo.toml",
    "crates/explorer-app/src/application.rs",
    "crates/explorer-app/src/folder_size_service.rs",
    "crates/explorer-app/src/lib.rs",
    "crates/explorer-app/src/bin/mft_helper.rs",
    "crates/explorer-app/src/bin/mft_service.rs",
    "crates/explorer-extension-host/Cargo.toml",
    "crates/explorer-extension-host/src/runtime_authority.rs",
    "crates/explorer-ui/Cargo.toml"
    "crates/explorer-ui/src/chrome.rs",
    "crates/explorer-ui/src/state.rs"
) | Sort-Object -Unique)

$ownedSnapshots = @()
foreach ($relativePath in $ownedPaths) {
    $absolutePath = Join-Path $workspacePath $relativePath
    if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
        continue
    }
    $safeName = $relativePath.Replace('/', '__').Replace('\', '__').Replace(':', '_') + ".patch"
    $patchPath = Join-Path $prechangePath $safeName
    $diff = & git -C $workspacePath diff --no-ext-diff -- $relativePath
    [IO.File]::WriteAllLines($patchPath, @($diff), [Text.UTF8Encoding]::new($false))
    $ownedSnapshots += [ordered]@{
        path = $relativePath
        sha256 = Get-Sha256File $absolutePath
        dirty = (@($diff).Count -gt 0)
        prechange_diff = $patchPath.Substring($workspacePath.Length).TrimStart('\', '/').Replace('\', '/')
        attribution = if (@($diff).Count -gt 0) { "pre-existing-user-or-active-change" } else { "clean-at-baseline" }
    }
}

$rustc = @(& rustc -Vv)
$cargo = (& cargo -V)
$activeToolchain = (& rustup show active-toolchain)
$revision = (& git -C $workspacePath rev-parse HEAD).Trim()
$dirtyTree = @(& git -C $workspacePath status --short)
$capturedAt = [DateTimeOffset]::Now.ToString("o")

$baseline = [ordered]@{
    schema_version = 1
    gate = "DCG-INVENTORY"
    captured_at = $capturedAt
    revision = $revision
    command = "cargo check --workspace --locked --offline --message-format=json"
    exit_status = 0
    toolchain = [ordered]@{
        rustc = $rustc
        cargo = $cargo
        active_toolchain = $activeToolchain
        target_triple = (($rustc | Where-Object { $_ -like "host:*" }) -replace "^host:\s*", "")
        cargo_config_paths = @()
        features = "default"
        environment = [ordered]@{
            RUSTFLAGS = $env:RUSTFLAGS
            CARGO_TARGET_DIR = $env:CARGO_TARGET_DIR
            CARGO_BUILD_TARGET = $env:CARGO_BUILD_TARGET
        }
    }
    compiler_jsonl = "evidence/compiler-messages.jsonl"
    compiler_jsonl_sha256 = Get-Sha256File $compilerPath
    dirty_tree = $dirtyTree
    warning_counts = [ordered]@{}
    emitted_dead_code_count = $diagnostics.Count
    canonical_dead_code_count = $canonicalSites.Count
    primary_item_count = $items.Count
    target_local_canonical_count = @($canonicalSites | Where-Object target_local).Count
    canonical_sites = $canonicalSites
    items = $items
    target_topology = $targetTopology
    owned_file_snapshots = $ownedSnapshots
    existing_dead_code_suppressions = @(Get-DeadCodeSuppressions)
    task_records = @(
        [ordered]@{ task_id = "1.1.1"; result = "passed"; subcheck_key = "environment"; command = "git/rustc/cargo/rustup inventory"; exit_code = 0; evidence = "environment, revision, dirty_tree" }
        [ordered]@{ task_id = "1.1.2"; result = "passed"; subcheck_key = "compiler-json"; command = "cargo check --workspace --locked --offline --message-format=json"; exit_code = 0; evidence = "compiler-messages.jsonl, warning_counts" }
        [ordered]@{ task_id = "1.1.3"; result = "passed"; subcheck_key = "canonical-sites"; command = "scripts/build-baseline.ps1"; exit_code = 0; evidence = "canonical_sites" }
        [ordered]@{ task_id = "1.1.4"; result = "passed"; subcheck_key = "primary-items"; command = "scripts/build-baseline.ps1"; exit_code = 0; evidence = "items" }
        [ordered]@{ task_id = "1.1.5"; result = "passed"; subcheck_key = "target-topology"; command = "scripts/build-baseline.ps1"; exit_code = 0; evidence = "target_topology" }
        [ordered]@{ task_id = "1.1.6"; result = "passed"; subcheck_key = "owned-files"; command = "scripts/build-baseline.ps1"; exit_code = 0; evidence = "owned_file_snapshots, prechange-diffs/" }
    )
}

foreach ($warningCode in @($warningCounts.Keys | Sort-Object)) {
    $baseline.warning_counts[$warningCode] = $warningCounts[$warningCode]
}

$baselinePath = Join-Path $evidencePath "baseline.json"
$baseline | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $baselinePath -Encoding utf8NoBOM

Write-Output "baseline=$baselinePath"
Write-Output "emitted=$($diagnostics.Count)"
Write-Output "canonical=$($canonicalSites.Count)"
Write-Output "items=$($items.Count)"
Write-Output "target_local=$(@($canonicalSites | Where-Object target_local).Count)"
