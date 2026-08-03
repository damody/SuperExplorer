$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$fixture = Join-Path $repo 'sdk\fixtures\ui-invalidation-batcher-contract\case.json'
$batcherSource = Join-Path $repo 'crates\explorer-extension-host\src\ui_invalidation_batcher.rs'
$guardSource = Join-Path $repo 'crates\explorer-extension-host\src\plugin_call_guard.rs'
$case = Get-Content -LiteralPath $fixture -Raw -Encoding UTF8 | ConvertFrom-Json

function Fail([string] $Message) { throw "ui invalidation batcher contract: $Message" }

if ([int]$case.schema_version -ne 1) { Fail 'fixture schema version changed' }
if ([int]$case.item_count -ne 1000) { Fail 'fixture must contain exactly 1,000 results' }
if ([int]$case.coalescing_window_ms -lt 16 -or [int]$case.coalescing_window_ms -gt 50) {
    Fail 'fixture window is outside the required 16-50 ms range'
}
if ([int]$case.arrival_interval_ms -le 0) { Fail 'arrival interval must be positive' }

# Deterministic stream arithmetic: results at t=0..999 ms are grouped into
# half-open windows [0,20), [20,40), ...; no wall clock is used by this gate.
$lastArrival = ([int]$case.item_count - 1) * [int]$case.arrival_interval_ms
$expectedBatches = [math]::Ceiling(($lastArrival + 1) / [double]$case.coalescing_window_ms)
if ([int]$case.expected_max_redraws -ne [int]$expectedBatches) {
    Fail "fixture expected_max_redraws is stale: expected $expectedBatches"
}
if ($expectedBatches -ge [int]$case.item_count) {
    Fail '1,000 results would still cause one redraw per item'
}

foreach ($path in @($batcherSource, $guardSource)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "missing production source: $path" }
}
$batcher = Get-Content -LiteralPath $batcherSource -Raw -Encoding UTF8
foreach ($symbol in @(
        'UiInvalidationBatcherV1',
        'MIN_UI_INVALIDATION_WINDOW_V1',
        'MAX_UI_INVALIDATION_WINDOW_V1',
        'record_accepted_batch',
        'drain_due',
        'next_deadline')) {
    if (-not $batcher.Contains($symbol)) { Fail "production batcher lost required symbol: $symbol" }
}
$guard = Get-Content -LiteralPath $guardSource -Raw -Encoding UTF8
foreach ($symbol in @('package_id', 'primary_interface_namespace', 'primary_interface_value', 'elapsed')) {
    if (-not $guard.Contains($symbol)) { Fail "timing diagnostics lost identity field: $symbol" }
}

# Exercise the production batcher, its real job-runtime enqueue route, and the
# native-provider timing route.  Keep each filter exact enough that a renamed
# or deleted contract test cannot be hidden by an unrelated passing test.
Push-Location $repo
try {
    $testCases = @(
        @{ Package = 'explorer-extension-host'; Target = @('--lib'); Filter = 'ui_invalidation_batcher::tests::one_thousand_one_ms_arrivals_use_fifty_non_sliding_twenty_ms_transactions' },
        @{ Package = 'explorer-extension-host'; Target = @('--lib'); Filter = 'extension_job_ui_bridge::tests::delayed_ui_poll_preserves_the_original_post_apply_deadline' },
        @{ Package = 'explorer-extension-host'; Target = @('--lib'); Filter = 'extension_job_runtime::tests::rapid_one_thousand_batches_coalesce_without_per_item_redraw' },
        @{ Package = 'explorer-extension-host'; Target = @('--lib'); Filter = 'extension_job_runtime::tests::thousand_item_scheduler_runtime_ui_pipeline' },
        @{ Package = 'explorer-app'; Target = @('--lib'); Filter = 'application::tests::directory_fixture_is_visible_before_extension_projection_runs' },
        @{ Package = 'explorer-app'; Target = @('--lib'); Filter = 'application::tests::projector_injection_runs_before_poll_and_neither_deferred_nor_error_consumes_ready_work' },
        @{ Package = 'explorer-extension-host'; Target = @('--lib'); Filter = 'native_lifecycle::tests::provider_timing_measures_only_the_callback_not_publish_delay' },
        @{ Package = 'explorer-extension-host'; Target = @('--lib'); Filter = 'native_lifecycle::tests::provider_dispatch_failure_records_a_bounded_timing_terminal' },
        @{ Package = 'explorer-extension-host'; Target = @('--lib'); Filter = 'native_lifecycle::tests::provider_terminal_diagnostics_preserve_error_classes' },
        @{ Package = 'explorer-jobs'; Target = @('--test', 'extension_scheduler_contract'); Filter = 'visible_priority_is_fifo_and_lower_lane_starts_within_burst_bound' },
        @{ Package = 'explorer-jobs'; Target = @('--test', 'extension_scheduler_contract'); Filter = 'queued_running_deadline_and_scope_cancellation_are_cooperative_and_exact' },
        @{ Package = 'explorer-ui'; Target = @('--lib'); Filter = 'tests::prelayout_icon_range_primes_one_bounded_first_viewport' }
    )
    foreach ($testCase in $testCases) {
        $testPackage = [string]$testCase.Package
        $testFilter = [string]$testCase.Filter
        $testTarget = @($testCase.Target)
        $cargoLog = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-ui-batcher-cargo-' + [Guid]::NewGuid().ToString('N') + '.log')
        $savedErrorAction = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            & cargo.exe test -p $testPackage @testTarget $testFilter --locked --offline -- --exact --nocapture --test-threads=1 *> $cargoLog
            $cargoExit = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $savedErrorAction
        }
        $output = @(Get-Content -LiteralPath $cargoLog -ErrorAction SilentlyContinue)
        Remove-Item -LiteralPath $cargoLog -Force -ErrorAction SilentlyContinue
        if ($cargoExit -ne 0) { Fail "production test '$testPackage::$testFilter' failed (exit $cargoExit): $($output -join ' ')" }
        $text = $output -join "`n"
        if ($text -notmatch 'test result:\s+ok\.') { Fail "production test '$testPackage::$testFilter' did not report success" }
        if ($text -match 'running 0 tests') { Fail "production test '$testPackage::$testFilter' matched no tests" }
        if ($text -notmatch 'test [^\r\n]+\.\.\. ok') { Fail "production test '$testPackage::$testFilter' did not execute an exact passing test" }
    }
} finally {
    Pop-Location
}

# Path-free diagnostic fixture: identity is stable package/interface data only.
$diagnostic = "package=$($case.package_id);interface=$($case.interface_namespace):$($case.interface_value);slow_ms=$($case.slow_threshold_ms)"
if ($diagnostic -match '[A-Za-z]:[\\/]|\\\\|/[^; ]') { Fail 'timing diagnostic fixture contains a filesystem path' }
if ($env:EXPLORER_UITEST_EVIDENCE_DIR) {
    New-Item -ItemType Directory -Path $env:EXPLORER_UITEST_EVIDENCE_DIR -Force | Out-Null
    Copy-Item -LiteralPath $fixture -Destination (Join-Path $env:EXPLORER_UITEST_EVIDENCE_DIR 'case.json') -Force
}
Write-Output "ui invalidation batcher contract: PASS (1000-item vertical production pipeline; deterministic window upper bound $expectedBatches)"
