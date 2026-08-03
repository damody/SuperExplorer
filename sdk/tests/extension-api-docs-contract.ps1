$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$doc = Join-Path $repo 'sdk\EXTENSION_API_GUIDE.md'
$readme = Get-Content -LiteralPath (Join-Path $repo 'sdk\README.md') -Raw -Encoding UTF8
$diagnostics = Get-Content -LiteralPath (Join-Path $repo 'sdk\PLUGIN_DIAGNOSTICS.md') -Raw -Encoding UTF8
$nativeOps = Get-Content -LiteralPath (Join-Path $repo 'sdk\NATIVE_PLUGIN_OPERATIONS.md') -Raw -Encoding UTF8
if (-not (Test-Path -LiteralPath $doc -PathType Leaf)) { throw 'missing extension API guide' }
$text = Get-Content -LiteralPath $doc -Raw -Encoding UTF8
foreach ($required in @('abi_stable','sabi_trait','RArc','StableAbi','Host-internal','backpressure','generation','cancellation','InputStream','Cache','absolute paths','task 5')) {
    if ($text.IndexOf($required, [StringComparison]::OrdinalIgnoreCase) -lt 0) { throw "docs contract missing '$required'" }
}
foreach ($forbidden in @('task 5 is complete','task 5 completed','roadmap task 5 complete')) {
    if ($text.IndexOf($forbidden, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "docs overclaims '$forbidden'" }
}
foreach ($forbidden in @('finite-credit stream','finite credits','exposed cancellation token')) {
    if ($text.IndexOf($forbidden, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "docs contains forbidden contract wording '$forbidden'" }
}
foreach ($forbiddenClaim in @('Malformed or out-of-order transport is a protocol fault','structured/opaque values are not sortable','callback-only threshold')) {
    if ($text.IndexOf($forbiddenClaim, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "docs retain inaccurate claim '$forbiddenClaim'" }
}
foreach ($requiredText in @('1,024 items','1 MiB','64 KiB','8 MiB','BACKPRESSURED','250 ms','128 records','rejected_batch','remaining_batch_credits','remaining_item_credits','remaining_byte_credits','instantaneous post-attempt snapshots','reservations','retains ownership','sealed manifest digest','RejectedCapacity','non-sliding','callback_id','RegistrarFactoryV1(extern "C" fn)','ExtensionRootModuleV1::new','filesystem.read','WRONG_THREAD','starting at 0','No stream credit counter','128 entries','4 MiB','test-only host harness','not author API','guarded activation envelope','plugin-only execution time')) {
    if ($text.IndexOf($requiredText, [StringComparison]::OrdinalIgnoreCase) -lt 0) { throw "docs missing exact declaration '$requiredText'" }
}
# Keep this script Windows PowerShell 5 compatible (ASCII source, UTF-8 guide)
# while pinning the zh-TW timing distinctions through their UTF-8 encodings.
foreach ($requiredZhBase64 in @(
    'YEpvYlByb3ZpZGVyYCDnmoQgZWxhcHNlZCDlj6rmtrXok4vlkIzmraUgcHJvdmlkZXIgY2FsbGJhY2s=',
    'YFJlZ2lzdHJhcmAg55qEIGVsYXBzZWQ=',
    '5ra16JOLIGd1YXJkZWQgYWN0aXZhdGlvbiBlbnZlbG9wZQ==',
    '5LiN5Y+v6Kej6K6A54K6IHBsdWdpbi1vbmx5IOWft+ihjOaZgumWkw==',
    '5YWp6ICF6YO95LiN5YyF5ZCr',
    'cHVibGlzaC9VSSDlu7bpgbI='
)) {
    $requiredZh = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($requiredZhBase64))
    if ($text.IndexOf($requiredZh, [StringComparison]::Ordinal) -lt 0) { throw 'docs missing zh-TW timing distinction' }
}
foreach ($guideStatus in @('ACCEPTED','WOULD_BLOCK','STALE','CLOSED','WRONG_THREAD','INVALID','OK','EOF','CANCELLED','DEADLINE_ELAPSED','UNSUPPORTED')) {
    if ($text.IndexOf($guideStatus, [StringComparison]::Ordinal) -lt 0) { throw "guide missing status '$guideStatus'" }
}
foreach ($guideTerminal in @('COMPLETED','UNAVAILABLE','BACKPRESSURED','PLUGIN_ERROR','INCOMPATIBLE','PANICKED')) {
    if ($text.IndexOf($guideTerminal, [StringComparison]::Ordinal) -lt 0) { throw "guide missing terminal/outcome '$guideTerminal'" }
}
$authorFixturePath = Join-Path $repo 'sdk\fixtures\extension-author-jobs-v1\src\main.rs'
if (-not (Test-Path -LiteralPath $authorFixturePath -PathType Leaf)) { throw 'compiled production-semantics author fixture is missing' }
$fixtureSource = Get-Content -LiteralPath $authorFixturePath -Raw -Encoding UTF8
foreach ($fixtureSymbol in @('ExtensionRegistrarImplementationV1','JobProviderImplementationV1','JobProviderObjectV1::new','ExtensionRootModuleV1::new','try_submit','sequence: 0','SinkSubmitStatusV1::WOULD_BLOCK')) {
    if ($fixtureSource.IndexOf($fixtureSymbol, [StringComparison]::Ordinal) -lt 0) { throw "author fixture missing '$fixtureSymbol'" }
}
if ($text -notmatch '(?m)^## Two API surfaces') { throw 'docs must distinguish author and host-internal APIs' }
if ($text -notmatch '(?m)^## Limits and backpressure') { throw 'docs must document bounded limits/backpressure' }
if ($text -notmatch '(?m)^## Generation and cancellation') { throw 'docs must document generation/cancellation' }
if ($readme -notmatch 'EXTENSION_API_GUIDE\.md' -or $diagnostics -notmatch 'EXTENSION_API_GUIDE\.md') { throw 'SDK discovery links to the API guide are missing' }
foreach ($requiredNative in @('Registrar','JobProvider','RegistrarInProgress','operation','guarded native incident/callback')) {
    if ($nativeOps.IndexOf($requiredNative, [StringComparison]::OrdinalIgnoreCase) -lt 0) { throw "native operations docs missing '$requiredNative'" }
}
foreach ($forbiddenNative in @('only attributable by a durable marker is the registrar','only protects registrar','scoped registrar incident','只保護 registrar','受 guard 保護的 registrar callback')) {
    if ($nativeOps.IndexOf($forbiddenNative, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw "native operations docs retain obsolete claim '$forbiddenNative'" }
}

# Cross-check the guide against the shipped API/host symbols so a self-authored
# prose-only contract cannot drift from the Rust-first SDK.
$api = Get-Content -LiteralPath (Join-Path $repo 'crates\explorer-extension-api\src\jobs.rs') -Raw -Encoding UTF8
$apiRoot = Get-Content -LiteralPath (Join-Path $repo 'crates\explorer-extension-api\src\lib.rs') -Raw -Encoding UTF8
$hostSource = Get-Content -LiteralPath (Join-Path $repo 'crates\explorer-extension-host\src\plugin_call_guard.rs') -Raw -Encoding UTF8
$runtimeSource = Get-Content -LiteralPath (Join-Path $repo 'crates\explorer-extension-host\src\extension_job_runtime.rs') -Raw -Encoding UTF8
$cacheSource = Get-Content -LiteralPath (Join-Path $repo 'crates\explorer-extension-host\src\extension_result_cache.rs') -Raw -Encoding UTF8
$lifecycleSource = Get-Content -LiteralPath (Join-Path $repo 'crates\explorer-extension-host\src\native_lifecycle.rs') -Raw -Encoding UTF8
$uiBatcherSource = Get-Content -LiteralPath (Join-Path $repo 'crates\explorer-extension-host\src\ui_invalidation_batcher.rs') -Raw -Encoding UTF8
foreach ($symbol in @('MAX_INCREMENTAL_RESULT_ITEMS_V1','MAX_INCREMENTAL_RESULT_BYTES_V1','MAX_PLUGIN_VALUE_BYTES_V1','MAX_INPUT_STREAM_READ_BYTES_V1','PluginValueV1','StableSortValueV1','SinkSubmitStatusV1','InputStreamStatusV1','JobContextV1','try_submit')) {
    if ($api.IndexOf($symbol, [StringComparison]::Ordinal) -lt 0 -and $symbol -ne 'try_submit') { throw "API source missing '$symbol'" }
}
if ($api -notmatch 'fn\s+try_submit') { throw 'API source missing try_submit' }
if ($api -notmatch 'BACKPRESSURED') { throw 'API source missing BACKPRESSURED terminal' }
foreach ($declaration in @(
    'MAX_INCREMENTAL_RESULT_ITEMS_V1:\s*usize\s*=\s*1_024',
    'MAX_INCREMENTAL_RESULT_BYTES_V1:\s*usize\s*=\s*1024\s*\*\s*1024',
    'MAX_PLUGIN_VALUE_BYTES_V1:\s*usize\s*=\s*64\s*\*\s*1024',
    'MAX_INPUT_STREAM_READ_BYTES_V1:\s*u32\s*=\s*64\s*\*\s*1024'
)) {
    if ($api -notmatch $declaration) { throw "API numeric declaration drifted: $declaration" }
}
if ($runtimeSource -notmatch 'next_sequence:\s*0' -or $runtimeSource -notmatch 'next_progress_sequence:\s*0') { throw 'runtime sequence origin drifted' }
if ($runtimeSource -notmatch 'MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1:\s*usize\s*=\s*8\s*\*\s*1024\s*\*\s*1024') { throw 'host input stream source ceiling drifted' }
foreach ($cacheDefault in @('max_entries:\s*1_024','max_bytes:\s*32\s*\*\s*1024\s*\*\s*1024','Duration::from_secs\(30\)')) {
    if ($cacheSource -notmatch $cacheDefault) { throw "cache default drifted: $cacheDefault" }
}
foreach ($cacheScopedDefault in @('max_entries_per_package:\s*128','max_entries_per_interface:\s*128','max_bytes_per_package:\s*4\s*\*\s*1024\s*\*\s*1024','max_bytes_per_interface:\s*4\s*\*\s*1024\s*\*\s*1024')) {
    if ($cacheSource -notmatch $cacheScopedDefault) { throw "cache scoped default drifted: $cacheScopedDefault" }
}
foreach ($runtimeDefault in @('max_active_jobs:\s*256','max_active_jobs_per_package:\s*32','max_batches:\s*1_024','max_batches_per_package:\s*128','max_batches_per_job:\s*32','max_items:\s*32_768','max_items_per_package:\s*4_096','max_items_per_job:\s*1_024','max_bytes:\s*64\s*\*\s*1024\s*\*\s*1024','max_bytes_per_package:\s*8\s*\*\s*1024\s*\*\s*1024','max_bytes_per_job:\s*1024\s*\*\s*1024')) {
    if ($runtimeSource -notmatch $runtimeDefault) { throw "result-runtime default drifted: $runtimeDefault" }
}
if ($lifecycleSource -notmatch 'slow_callback_threshold:\s*Duration::from_millis\(250\)') { throw 'slow callback threshold drifted' }
if ($hostSource -notmatch 'MAX_NATIVE_CALL_TIMINGS_V1:\s*usize\s*=\s*128') { throw 'native timing ring capacity drifted' }
if ($hostSource -notmatch 'enum\s+NativeCallOperationV1\s*\{[\s\S]{0,160}?Registrar\s*,[\s\S]{0,80}?JobProvider\s*,') { throw 'guarded native operation set drifted' }
if ($apiRoot -notmatch 'RegistrarFactoryV1\(extern\s+"C"\s+fn') { throw 'SDK-owned registrar factory ABI drifted' }
if ($uiBatcherSource -notmatch 'MIN_UI_INVALIDATION_WINDOW_V1[^\r\n]*Duration::from_millis\(16\)' -or $uiBatcherSource -notmatch 'MAX_UI_INVALIDATION_WINDOW_V1[^\r\n]*Duration::from_millis\(50\)') { throw 'UI invalidation window bounds drifted' }
foreach ($cacheKeyField in @('package_id','sealed_manifest_digest','contribution_id','data_version','interface_id','feature_id','feature_epoch','file','option_hash','watcher_scope','watcher_generation','recursive')) {
    if ($cacheSource -notmatch ("(?m)^\s*" + [regex]::Escape($cacheKeyField) + ':')) { throw "cache key field drifted: $cacheKeyField" }
}
foreach ($cacheInsertOutcome in @('Inserted','RejectedStale','RejectedCapacity')) {
    if ($cacheSource.IndexOf($cacheInsertOutcome, [StringComparison]::Ordinal) -lt 0) { throw "cache insertion outcome drifted: $cacheInsertOutcome" }
}
foreach ($status in @('OK','EOF','CANCELLED','DEADLINE_ELAPSED','STALE','CLOSED','WRONG_THREAD','UNSUPPORTED','INVALID','WOULD_BLOCK','rejected_batch')) {
    if ($api.IndexOf($status, [StringComparison]::Ordinal) -lt 0) { throw "API source missing status '$status'" }
}
foreach ($symbol in @('NativeCallTimingV1','elapsed','package_id','primary_interface_namespace','primary_interface_value')) {
    if ($hostSource.IndexOf($symbol, [StringComparison]::Ordinal) -lt 0) { throw "host diagnostics source missing '$symbol'" }
}
Write-Output 'extension API docs contract: PASS'
