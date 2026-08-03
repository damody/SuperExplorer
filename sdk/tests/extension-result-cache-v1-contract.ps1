$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$hostSource = Join-Path $repo 'crates\explorer-extension-host\src\extension_result_cache.rs'
if (-not (Test-Path -LiteralPath $hostSource -PathType Leaf)) { throw 'production result-cache module is missing' }
$source = Get-Content -LiteralPath $hostSource -Raw -Encoding UTF8
foreach ($required in @(
    'pub struct ExtensionResultCacheV1',
    'pub struct ExtensionResultCacheKeyV1',
    'pub fn invalidate_watcher_scope',
    'pub fn invalidate_manual',
    'pub fn invalidate_data_version',
    'saturating_duration_since(entry.inserted_at)',
    'RejectedStale'
)) {
    if (-not $source.Contains($required)) { throw "production result-cache contract is missing: $required" }
}

$oldOffline = $env:CARGO_NET_OFFLINE
try {
    $env:CARGO_NET_OFFLINE = 'true'
    Push-Location $repo
    try {
        foreach ($test in @(
            'extension_result_cache::tests::exhausted_epoch_or_watcher_counter_permanently_fails_closed',
            'extension_job_runtime::tests::cache_hit_rebinds_to_revocable_runtime_and_cache_generations',
            'extension_job_runtime::tests::cache_hit_identity_mapper_can_reenter_runtime_after_validation',
            'extension_job_runtime::tests::cache_admissions_reject_late_invalidation_and_cross_job_batches',
            'extension_job_runtime::tests::cache_capacity_is_exact_replaces_in_place_and_reclaims_expired_entries',
            'extension_job_runtime::tests::cache_separates_contributions_data_versions_and_current_views',
            'extension_job_runtime::tests::cache_ttl_and_watcher_rollover_reject_replay_at_exact_boundaries',
            'extension_job_runtime::tests::stale_generations_are_discarded_and_release_package_credits',
            'extension_job_runtime::tests::generation_advance_revokes_previously_applied_rows_and_reclaims_retention',
            'extension_job_runtime::tests::feature_scoped_lifecycle_cancel_preserves_sibling_and_newer_epoch',
            'extension_job_runtime::tests::lifecycle_revoke_invalidates_opaque_rows_after_their_job_has_retired'
        )) {
            & cargo.exe test -p explorer-extension-host --locked --offline $test -- --exact
            if ($LASTEXITCODE -ne 0) { throw "production result-cache generation gate failed: $test" }
        }
    } finally { Pop-Location }
    Write-Output 'extension result cache v1 contract: PASS'
} finally {
    if ($null -eq $oldOffline) { Remove-Item Env:CARGO_NET_OFFLINE -ErrorAction SilentlyContinue } else { $env:CARGO_NET_OFFLINE = $oldOffline }
}
