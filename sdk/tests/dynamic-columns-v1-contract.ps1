$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$fixturePath = Join-Path $repo 'sdk\fixtures\dynamic-columns-v1-contract\case.json'
function Fail([string] $Message) { throw "dynamic columns contract: $Message" }
if (-not (Test-Path -LiteralPath $fixturePath -PathType Leaf)) { Fail 'missing fixture' }
$case = Get-Content -LiteralPath $fixturePath -Raw -Encoding UTF8 | ConvertFrom-Json
if ([int]$case.schema_version -ne 1) { Fail 'fixture schema version must be 1' }

# Task 5.1 foundation: IDs and descriptors are registry data, not a fixed
# enum/bitmask. Persistence migration and unknown-plugin restoration are task 5.4.
$all = @($case.built_in) + @($case.extension.column)
$ids = @($all | ForEach-Object { [string]$_.id })
if ($ids.Count -ne (@($ids | Sort-Object -Unique).Count)) { Fail 'column IDs are not unique' }
foreach ($id in $ids) {
    if ($id -notmatch '^(builtin:(name|date_modified|type|size|date_created|authors|tags|title)|[a-z0-9][a-z0-9._-]{0,63}:[a-z0-9][a-z0-9._-]{0,63})$') { Fail "unstable column ID: $id" }
}
foreach ($descriptor in $all) {
    if ([int]$descriptor.width -le 0) { Fail "non-positive width for $($descriptor.id)" }
    if ([string]$descriptor.value_type -notin @('text','time','bytes','integer','float','boolean')) { Fail "unsupported value type for $($descriptor.id)" }
    if ([string]$descriptor.alignment -notin @('leading','trailing','center')) { Fail "invalid alignment for $($descriptor.id)" }
    if ([string]$descriptor.sort -notin @('text','numeric','bytes','none')) { Fail "invalid sort semantics for $($descriptor.id)" }
}
$ordered = @($case.ordered_layout)
if ($ordered.Count -ne $ids.Count) { Fail 'ordered layout must include every registered column exactly once' }
if ((($ordered | Sort-Object) -join '|') -ne (($ids | Sort-Object) -join '|')) { Fail 'ordered layout does not match registry IDs' }
foreach ($id in @($case.visible)) {
    if ($id -notin $ids) { Fail "visible column is not registered: $id" }
}
if ('org.example.folder-size:column' -in @($case.visible)) { Fail 'extension starts disabled in foundation fixture' }
if ([string]$case.foundation_scope -notmatch 'task-5\.4') { Fail 'fixture must identify task 5.4 follow-up' }

# Execute the production-backed registry/layout tests; fixture arithmetic above
# only guards the published examples and is not evidence by itself.
Push-Location $repo
try {
    $log = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-dynamic-columns-' + [Guid]::NewGuid().ToString('N') + '.log')
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & cargo.exe test -p explorer-model --test dynamic_columns --locked --offline -- --nocapture *> $log
        $exitCode = $LASTEXITCODE
    } finally { $ErrorActionPreference = $saved }
    $output = @(Get-Content -LiteralPath $log -ErrorAction SilentlyContinue)
    Remove-Item -LiteralPath $log -Force -ErrorAction SilentlyContinue
    if ($exitCode -ne 0) { Fail "production dynamic_columns tests failed (exit $exitCode): $($output -join ' ')" }
    $text = $output -join "`n"
    if ($text -notmatch 'test result:\s+ok\.') { Fail 'production dynamic_columns tests did not report success' }
    if ($text -match 'running 0 tests') { Fail 'production dynamic_columns test target matched no tests' }
    foreach ($requiredTest in @(
        'stable_ids_and_descriptor_registry_reject_collisions_and_bad_ownership',
        'ordered_layout_preserves_width_visibility_and_deterministic_order',
        'descriptor_validation_rejects_invalid_width_range',
        'descriptor_display_type_and_host_stable_sort_key_are_independent',
        'package_revoke_hides_but_retains_layout_until_same_id_returns',
        'replace_package_is_atomic_and_namespaces_same_local_column_ids',
        'legacy_runtime_migration_preserves_custom_prefix_and_appends_built_ins'
    )) {
        if ($text -notmatch ("test " + [regex]::Escape($requiredTest) + " \.\.\. ok")) {
            Fail "production dynamic_columns coverage missing: $requiredTest"
        }
    }
} finally { Pop-Location }

if ($env:EXPLORER_UITEST_EVIDENCE_DIR) {
    New-Item -ItemType Directory -Path $env:EXPLORER_UITEST_EVIDENCE_DIR -Force | Out-Null
    Copy-Item -LiteralPath $fixturePath -Destination (Join-Path $env:EXPLORER_UITEST_EVIDENCE_DIR 'dynamic-columns-case.json') -Force
}
Write-Output 'dynamic columns contract: PASS (stable IDs, descriptors, collision checks, ordered layout foundation)'
