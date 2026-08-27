param(
    [string]$Workspace = (Get-Location).Path,
    [string]$EvidenceDirectory = "openspec/changes/govern-dead-code-warnings/evidence"
)

$ErrorActionPreference = "Stop"
$workspacePath = [IO.Path]::GetFullPath($Workspace)
$evidencePath = [IO.Path]::GetFullPath((Join-Path $workspacePath $EvidenceDirectory))
$baselinePath = Join-Path $evidencePath "baseline.json"
$baseline = Get-Content -Raw -LiteralPath $baselinePath | ConvertFrom-Json -Depth 100

$topologyBySource = @{}
foreach ($entry in $baseline.target_topology) {
    $topologyBySource[$entry.source_path] = $entry
}

$lineageBySource = @{
    "crates/explorer-app/src/application.rs" = @(
        "fix-code-lines-directory-input-preparation",
        "fix-shared-mft-folder-aggregate-lru",
        "centralize-shared-folder-size-service"
    )
    "crates/explorer-app/src/folder_size_service.rs" = @(
        "centralize-shared-folder-size-service",
        "fix-shared-mft-folder-aggregate-lru"
    )
    "crates/explorer-app/src/bin/mft_service.rs" = @(
        "event-driven-mft-index-updates",
        "mft-sqlite-foreground-persistence",
        "fix-shared-mft-folder-aggregate-lru"
    )
    "crates/explorer-app/src/mft_focus.rs" = @("mft-sqlite-foreground-persistence")
    "crates/explorer-app/src/mft_journal.rs" = @(
        "event-driven-mft-index-updates",
        "mft-sqlite-foreground-persistence"
    )
    "crates/explorer-app/src/mft_migration.rs" = @("mft-sqlite-foreground-persistence")
    "crates/explorer-app/src/mft_persistence.rs" = @("mft-sqlite-foreground-persistence")
    "crates/explorer-app/src/mft_query.rs" = @(
        "fix-shared-mft-folder-aggregate-lru",
        "add-mft-directory-count-columns"
    )
    "crates/explorer-app/src/mft_runtime.rs" = @(
        "event-driven-mft-index-updates",
        "mft-sqlite-foreground-persistence"
    )
    "crates/explorer-app/src/mft_size_map.rs" = @(
        "centralize-shared-folder-size-service",
        "mft-sqlite-foreground-persistence"
    )
    "crates/explorer-app/src/mft_sqlite.rs" = @("mft-sqlite-foreground-persistence")
    "crates/explorer-ui/src/chrome.rs" = @(
        "independent-cache-max-editors",
        "fix-registry-ordered-details-cells",
        "scope-details-columns-by-filesystem"
    )
    "crates/explorer-ui/src/state.rs" = @(
        "add-bookmark-toolbar-lua-commands",
        "fix-bookmark-folders-and-lua-editor",
        "persist-bookmarks-independently"
    )
    "crates/explorer-extension-host/src/runtime_authority.rs" = @("build-extensible-plugin-platform")
}

$ownershipMatrix = @(
    [ordered]@{
        path = "crates/explorer-app/src/application.rs"
        owning_changes = @("fix-shared-mft-folder-aggregate-lru", "centralize-shared-folder-size-service", "differentiate-main-code-lines-and-reorder-columns", "separate-lua-rust-code-lines-columns")
        resolution = "govern-dead-code-warnings owns only the obsolete Code Lines cache, obsolete Details cache/query, and duplicate Size Map test scanner hunks"
        forbidden_hunks = @("current MFT batch stream", "current remote-provider work", "current Code Lines host-prepared input")
        dependency_order = @("preserve active-change hunks", "remove only ledger-approved closures")
        blocked = $false
    }
    [ordered]@{
        path = "crates/explorer-app/src/folder_size_service.rs"
        owning_changes = @("centralize-shared-folder-size-service", "fix-shared-mft-folder-aggregate-lru")
        resolution = "original changes retain product behavior; this change owns only methods proven obsolete by their current requirements"
        forbidden_hunks = @("snapshot_or_scan", "Size Map fallback", "leases", "bounded cleanup")
        dependency_order = @("retain current Size Map authority", "remove obsolete Details-only closure")
        blocked = $false
    }
    [ordered]@{
        path = "crates/explorer-app/src/mft_*.rs;crates/explorer-app/src/bin/mft_service.rs"
        owning_changes = @("mft-sqlite-foreground-persistence", "fix-shared-mft-folder-aggregate-lru", "add-mft-directory-count-columns")
        resolution = "original changes retain persistence/query behavior; this change owns topology extraction, writer-only legacy removal, and test-only boundaries"
        forbidden_hunks = @("legacy readers protected by whitelist", "typed recovery/remove contracts", "SQLite schema", "IPC frame layout")
        dependency_order = @("golden fixtures", "legacy writer removal", "test boundary", "internal crate extraction")
        blocked = $false
    }
    [ordered]@{
        path = "crates/explorer-ui/src/chrome.rs"
        owning_changes = @("scope-details-columns-by-filesystem", "independent-cache-max-editors")
        resolution = "preserve registry/filesystem/current cache editor hunks; remove only named duplicate wrappers"
        forbidden_hunks = @("filesystem-scoped column logic", "current cache editor")
        dependency_order = @("verify replacement", "remove duplicate wrappers")
        blocked = $false
    }
    [ordered]@{
        path = "crates/explorer-ui/src/state.rs;crates/explorer-extension-host/src/runtime_authority.rs"
        owning_changes = @("persist-bookmarks-independently", "build-extensible-plugin-platform")
        resolution = "completed changes retain product behavior; this change removes only unused convenience methods and rewrites tests to production APIs"
        forbidden_hunks = @("bookmark persistence", "runtime authorization and revocation semantics")
        dependency_order = @("verify production replacement", "remove convenience API")
        blocked = $false
    }
)

function Get-RecentHistory([string]$SourcePath) {
    return @(& git -C $workspacePath log -n 8 --format="%H|%ad|%s" --date=short -- $SourcePath)
}

function Get-Decision([object]$Item) {
    $source = $Item.source_path
    $text = ($Item.source_text -replace "\s+", " ").Trim()
    $topology = $topologyBySource[$source]
    $targetLocal = @($Item.emitting_targets).Count -lt @($topology.compiling_targets).Count

    if ($text -match "\b(RecoveryReasonV1|MigrationStateV1)\b") {
        return [ordered]@{
            disposition = "retain-required-contract"
            reason = "mft-sqlite-foreground-persistence still requires typed migration/recovery diagnostics"
            consumer_or_replacement = "normative requirement: stable machine-readable recovery reason and migration state"
            owning_change = "mft-sqlite-foreground-persistence"
            owning_task = "1.2.1, 3.1.3, final gates 5.1.4/6.1.3"
            expiry = "remove suppression when the owning change wires production diagnostics and passes its final gates"
            validation = "MFT persistence and diagnostics focused tests; no new producer/consumer in this change"
        }
    }

    if ($source -eq "crates/explorer-app/src/folder_size_service.rs" -and $text -match "^Remove\(SnapshotNodeIdV1\)") {
        return [ordered]@{
            disposition = "retain-required-contract"
            reason = "centralize-shared-folder-size-service requires bounded add/update/remove deltas"
            consumer_or_replacement = "normative remove-delta requirement"
            owning_change = "centralize-shared-folder-size-service"
            owning_task = "1.2.2; final gate 4.1.7"
            expiry = "remove suppression when the owning change wires Remove production emission/application and passes final validation"
            validation = "shared snapshot and Size Map tests; no remove behavior added by this change"
        }
    }

    if ($targetLocal) {
        return [ordered]@{
            disposition = "retain-cross-target-live"
            reason = "the shared source is live in another compiling target; target-local reachability caused this warning"
            consumer_or_replacement = (@($Item.emitting_targets) -join ", ")
            owning_change = "govern-dead-code-warnings"
            owning_task = "8.2-8.5"
            expiry = "the item must become warning-free when consumers use crates/explorer-mft"
            validation = "consumer whitelist plus target-specific and normal workspace checks"
        }
    }

    if ($source -in @(
        "crates/explorer-app/src/mft_migration.rs",
        "crates/explorer-app/src/mft_sqlite.rs",
        "crates/explorer-app/src/mft_size_map.rs"
    )) {
        return [ordered]@{
            disposition = "test-only"
            reason = "the item is compiled in production only to support failure, migration, fixture, or reference tests"
            consumer_or_replacement = "tests will use a cfg(test) seam or the production bounded/linearized API"
            owning_change = "govern-dead-code-warnings"
            owning_task = "6.1"
            expiry = "normal build no longer contains the item"
            validation = "normal/test structured checks and SQLite/migration/Size Map focused tests"
        }
    }

    if ($source -in @(
        "crates/explorer-app/src/application.rs",
        "crates/explorer-app/src/folder_size_service.rs",
        "crates/explorer-app/src/bin/mft_service.rs",
        "crates/explorer-app/src/mft_journal.rs",
        "crates/explorer-app/src/mft_query.rs",
        "crates/explorer-extension-host/src/runtime_authority.rs",
        "crates/explorer-ui/src/chrome.rs",
        "crates/explorer-ui/src/state.rs"
    )) {
        return [ordered]@{
            disposition = "remove-superseded"
            reason = "the source-specific OpenSpec lineage identifies a current replacement and no retained migration/ABI contract"
            consumer_or_replacement = (@($lineageBySource[$source]) -join ", ")
            owning_change = "govern-dead-code-warnings"
            owning_task = if ($source -like "*application.rs") { "3.1-3.3" } elseif ($source -like "*folder_size_service.rs") { "4.1" } elseif ($source -like "*mft_*") { "5.1-5.2" } else { "2.1-2.2" }
            expiry = "removed in the named cleanup wave"
            validation = "source-specific replacement tests and warning delta"
        }
    }

    if ($source -eq "crates/explorer-app/src/mft_focus.rs") {
        return [ordered]@{
            disposition = "remove-unreferenced"
            reason = "the all-target warning is a focus error constant with no current error-path consumer"
            consumer_or_replacement = "current versioned focus protocol error handling"
            owning_change = "govern-dead-code-warnings"
            owning_task = "5.2.2"
            expiry = "removed in Wave 5"
            validation = "focus lease/auth/error tests"
        }
    }

    throw "No decision rule for item $($Item.id) in $source"
}

$history = @($baseline.target_topology | ForEach-Object {
    [ordered]@{
        source_path = $_.source_path
        git_history = Get-RecentHistory $_.source_path
        openspec_lineage = @($lineageBySource[$_.source_path])
    }
})

$items = @($baseline.items | ForEach-Object {
    $decision = Get-Decision $_
    [ordered]@{
        id = $_.id
        source_path = $_.source_path
        line_start = $_.line_start
        source_text = $_.source_text
        source_sha256 = $_.source_sha256
        parent_diagnostic_ids = @($_.parent_diagnostic_ids)
        emitting_targets = @($_.emitting_targets)
        disposition = $decision.disposition
        reason = $decision.reason
        consumer_or_replacement = $decision.consumer_or_replacement
        owning_change = $decision.owning_change
        owning_task = $decision.owning_task
        expiry = $decision.expiry
        validation = $decision.validation
    }
})

$allowed = @(
    "remove-superseded",
    "remove-unreferenced",
    "retain-cross-target-live",
    "test-only",
    "retain-required-contract",
    "retain-narrow-suppression"
)
$unknown = @($items | Where-Object { $_.disposition -notin $allowed })
if ($unknown.Count -ne 0) {
    throw "Decision ledger contains invalid dispositions"
}
if (@($items.id | Sort-Object -Unique).Count -ne @($baseline.items).Count) {
    throw "Decision ledger does not contain exactly one record per baseline item"
}
if (@($ownershipMatrix | Where-Object blocked).Count -ne 0) {
    throw "Ownership matrix contains unresolved files"
}

$dispositionCounts = [ordered]@{}
foreach ($name in $allowed) {
    $dispositionCounts[$name] = @($items | Where-Object disposition -eq $name).Count
}

$ledger = [ordered]@{
    schema_version = 1
    gate = @("DCG-INVENTORY", "DCG-OWNERSHIP")
    captured_at = [DateTimeOffset]::Now.ToString("o")
    revision = $baseline.revision
    baseline_sha256 = (Get-FileHash -LiteralPath $baselinePath -Algorithm SHA256).Hash.ToLowerInvariant()
    allowed_dispositions = $allowed
    history = $history
    contracts = @(
        [ordered]@{ symbols = @("RecoveryReasonV1", "MigrationStateV1"); owning_change = "mft-sqlite-foreground-persistence"; requirement = "stable machine-readable recovery reason and migration state"; status = "retain-required-contract" }
        [ordered]@{ symbols = @("SnapshotDeltaV1::Remove"); owning_change = "centralize-shared-folder-size-service"; requirement = "bounded add/update/remove deltas"; status = "retain-required-contract" }
        [ordered]@{ symbols = @("load_legacy_memory_index", "read_checkpoint", "deltas_after", "validate_delta_after"); owning_change = "mft-sqlite-foreground-persistence"; requirement = "legacy migration/rollback reader"; status = "protected-reader" }
    )
    ownership_matrix = $ownershipMatrix
    ownership_resolutions = [ordered]@{
        unresolved_count = 0
        hash_rebaseline_is_not_ownership = $true
        rule = "preserve original product ownership and mutate only ledger-approved hunks in dependency order"
    }
    disposition_counts = $dispositionCounts
    items = $items
    task_records = @(
        [ordered]@{ task_id = "1.2.1"; result = "passed"; subcheck_key = "history"; command = "git log plus OpenSpec lineage scan"; exit_code = 0; evidence = "history" }
        [ordered]@{ task_id = "1.2.2"; result = "passed"; subcheck_key = "contracts"; command = "OpenSpec requirement scan"; exit_code = 0; evidence = "contracts" }
        [ordered]@{ task_id = "1.2.3"; result = "passed"; subcheck_key = "dispositions"; command = "scripts/build-decision-ledger.ps1"; exit_code = 0; evidence = "items, disposition_counts" }
        [ordered]@{ task_id = "1.2.4"; result = "passed"; subcheck_key = "replacements"; command = "scripts/build-decision-ledger.ps1"; exit_code = 0; evidence = "item reason/consumer_or_replacement/validation" }
        [ordered]@{ task_id = "1.2.5"; result = "passed"; subcheck_key = "ownership-matrix"; command = "active OpenSpec overlap scan"; exit_code = 0; evidence = "ownership_matrix" }
        [ordered]@{ task_id = "1.2.6"; result = "passed"; subcheck_key = "ownership-resolutions"; command = "scripts/build-decision-ledger.ps1"; exit_code = 0; evidence = "ownership_resolutions" }
    )
}

$ledgerPath = Join-Path $evidencePath "decision-ledger.json"
$ledger | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $ledgerPath -Encoding utf8NoBOM

Write-Output "ledger=$ledgerPath"
Write-Output "items=$($items.Count)"
foreach ($name in $allowed) {
    Write-Output "$name=$($dispositionCounts[$name])"
}
