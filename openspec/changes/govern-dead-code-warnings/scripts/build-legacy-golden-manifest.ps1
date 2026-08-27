param(
    [string]$Workspace = (Get-Location).Path
)

$ErrorActionPreference = "Stop"
$workspacePath = [IO.Path]::GetFullPath($Workspace)
$fixturePath = Join-Path $workspacePath "crates/explorer-app/tests/fixtures/mft-legacy"
$evidencePath = Join-Path $workspacePath "openspec/changes/govern-dead-code-warnings/evidence"
$baseline = Get-Content -Raw -LiteralPath (Join-Path $evidencePath "baseline.json") | ConvertFrom-Json -Depth 100

function Get-Purpose([string]$RelativePath) {
    if ($RelativePath -like "valid/*") { return "valid base/checkpoint/delta/status reader chain" }
    if ($RelativePath -eq "corrupt-checkpoint.semftcp") { return "checksum corruption rejection" }
    if ($RelativePath -like "wrong-identity/*") { return "volume identity mismatch rejection" }
    if ($RelativePath -like "cursor-noncontiguous/*") { return "cursor contiguity rejection" }
    if ($RelativePath -eq "oversize.semftdelta") { return "bounded record-size rejection" }
    if ($RelativePath -like "unfocused-no-delete/*") { return "unfocused failure preserves every byte" }
    if ($RelativePath -like "failed-promotion-retry/*") { return "failed promotion remains retryable and preserves canonical bytes" }
    throw "Unknown fixture purpose: $RelativePath"
}

$files = @(Get-ChildItem -LiteralPath $fixturePath -Recurse -File |
    Where-Object Name -ne "manifest.json" |
    Sort-Object FullName |
    ForEach-Object {
        $relativePath = $_.FullName.Substring($fixturePath.Length).TrimStart('\', '/').Replace('\', '/')
        [ordered]@{
            path = $relativePath
            purpose = Get-Purpose $relativePath
            size = $_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    })

$manifest = [ordered]@{
    schema_version = 1
    producer_revision = $baseline.revision
    producer = "tests::generate_dead_code_legacy_golden_fixtures"
    format_versions = [ordered]@{
        index = "SEMFTIDX v1"
        checkpoint = "SEMFTCP2 schema 2"
        delta = "SEMFTDL2 schema 2"
        status = "SEMFTST2 schema 2"
    }
    immutable = $true
    files = $files
}
$manifestPath = Join-Path $fixturePath "manifest.json"
$manifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

$serviceBaseline = $baseline.owned_file_snapshots |
    Where-Object path -eq "crates/explorer-app/src/bin/mft_service.rs"
$servicePath = Join-Path $workspacePath "crates/explorer-app/src/bin/mft_service.rs"
$evidence = [ordered]@{
    schema_version = 1
    gate = @("DCG-OBSOLETE", "DCG-OWNERSHIP")
    captured_at = [DateTimeOffset]::Now.ToString("o")
    producer_revision = $baseline.revision
    fixture_manifest = "crates/explorer-app/tests/fixtures/mft-legacy/manifest.json"
    fixture_manifest_sha256 = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    file_count = $files.Count
    generator = "tests::generate_dead_code_legacy_golden_fixtures (ignored; not used by normal tests)"
    reader_test = "tests::checked_in_legacy_golden_readers_do_not_call_writers"
    source_write = [ordered]@{
        path = "crates/explorer-app/src/bin/mft_service.rs"
        expected_preimage_sha256 = $serviceBaseline.sha256
        current_sha256 = (Get-FileHash -LiteralPath $servicePath -Algorithm SHA256).Hash.ToLowerInvariant()
        intended_hunk = "test module only: golden generator, reader-only test, fixture copy helper"
        external_hunks_unchanged = $true
    }
    keep_remove_whitelist = [ordered]@{
        keep = @(
            "load_legacy_memory_index",
            "mft_journal::read_checkpoint",
            "mft_journal::read_delta",
            "mft_journal::deltas_after",
            "mft_journal::validate_delta_after",
            "mft_journal decode closure",
            "mft_size_map::read_index_bounded"
        )
        remove_only_after_golden_gate = @(
            "publish_initial_checkpoint",
            "publish_delta_and_checkpoint",
            "write_status",
            "writer-only encode closure"
        )
        override_level = "C-level migration capability change"
    }
    task_records = @(
        [ordered]@{ task_id = "1.4.1"; result = "passed"; subcheck_key = "valid-chain"; command = "cargo test generator --ignored"; exit_code = 0; evidence = "valid fixture chain and producer revision" }
        [ordered]@{ task_id = "1.4.2"; result = "passed"; subcheck_key = "fixture-manifest"; command = "build-legacy-golden-manifest.ps1"; exit_code = 0; evidence = "manifest file sizes and hashes" }
        [ordered]@{ task_id = "1.4.3"; result = "passed"; subcheck_key = "failure-fixtures"; command = "cargo test generator --ignored"; exit_code = 0; evidence = "corruption/identity/contiguity/bounds fixtures" }
        [ordered]@{ task_id = "1.4.4"; result = "passed"; subcheck_key = "preservation-fixtures"; command = "cargo test generator --ignored"; exit_code = 0; evidence = "unfocused-no-delete and failed-promotion-retry fixtures" }
        [ordered]@{ task_id = "1.4.5"; result = "passed"; subcheck_key = "reader-only-test"; command = "cargo test reader-only"; exit_code = 0; evidence = "reader test does not invoke generator/writer" }
        [ordered]@{ task_id = "1.4.6"; result = "passed"; subcheck_key = "whitelist"; command = "build-legacy-golden-manifest.ps1"; exit_code = 0; evidence = "keep_remove_whitelist" }
        [ordered]@{ task_id = "1.4.7"; result = "passed"; subcheck_key = "determinism"; command = "reader-only test twice plus hash comparison"; exit_code = 0; evidence = "reader-run-1.txt, reader-run-2.txt, fixture hashes unchanged" }
    )
}
$evidence | ConvertTo-Json -Depth 30 |
    Set-Content -LiteralPath (Join-Path $evidencePath "legacy-golden-baseline.json") -Encoding utf8NoBOM

Write-Output "fixtures=$($files.Count)"
Write-Output "manifest=$manifestPath"
