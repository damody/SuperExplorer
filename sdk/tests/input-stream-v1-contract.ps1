$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$api = Get-Content (Join-Path $repo 'crates\explorer-extension-api\src\jobs.rs') -Raw -Encoding UTF8
foreach ($required in @('pub struct InputStreamV1', 'AbiInputStreamServicesV1', 'InputStreamCapabilityV1', 'InputStreamReadRequestV1', 'InputStreamSeekRequestV1', 'InputStreamLengthOutcomeV1')) {
    if (-not $api.Contains($required)) { throw "InputStream ABI contract missing: $required" }
}
if ($api -match 'InputStreamV1[\s\S]{0,400}(PathBuf|HANDLE|RawHandle|OsString)') { throw 'InputStream ABI exposes a path or raw OS handle' }

$oldOffline = $env:CARGO_NET_OFFLINE
try {
    $env:CARGO_NET_OFFLINE = 'true'
    Push-Location $repo
    try {
        foreach ($test in @(
            'native_lifecycle::tests::sealed_lifecycle_route_delivers_input_only_to_filesystem_read_contributions',
            'extension_job_runtime::tests::input_stream_byte_credits_bound_admission_and_reclaim_on_retirement',
            'extension_job_runtime::tests::input_stream_denies_unsealed_capability_source_changes_and_control',
            'extension_job_runtime::tests::input_stream_is_capability_bound_bounded_and_generation_safe',
            'extension_job_runtime::tests::production_dispatch_ticket_delivers_only_authorized_input_stream',
            'extension_job_runtime::tests::retained_input_stream_clone_cannot_pin_source_after_job_retirement',
            'extension_job_runtime::tests::source_change_cannot_publish_before_submit_drain_or_apply',
            'extension_job_runtime::tests::stale_dispatch_ticket_cannot_publish_completed_after_source_generation_advances'
        )) {
            & cargo.exe test -p explorer-extension-host --locked --offline $test -- --exact
            if ($LASTEXITCODE -ne 0) { throw "InputStream production behavior gate failed: $test" }
        }
    } finally { Pop-Location }
    Write-Output 'input stream v1 contract: PASS'
} finally {
    if ($null -eq $oldOffline) { Remove-Item Env:CARGO_NET_OFFLINE -ErrorAction SilentlyContinue } else { $env:CARGO_NET_OFFLINE = $oldOffline }
}
