$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    cargo test -p explorer-extension-host --locked --offline 'plugin_call_guard::tests::' -- --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'call guard tests failed' }
    cargo test -p explorer-extension-host --locked --offline 'native_lifecycle::tests::'
    if ($LASTEXITCODE -ne 0) { throw 'lifecycle Safe Mode tests failed' }

    $doc = Get-Content -Raw -Encoding UTF8 'docs/extension-native-safety.md'
    $review = Get-Content -Raw -Encoding UTF8 'openspec/changes/build-extensible-plugin-platform/reviews/native-safety-review.json' | ConvertFrom-Json
    foreach ($needle in @('not a sandbox', 'does not hot-unload', 'pending-restart', 'explicit confirmation', '## English')) {
        if (-not $doc.Contains($needle)) { throw "native safety documentation missing: $needle" }
    }
    if (($review.role -ne 'security-reviewer') -or ($review.decision -ne 'approved') -or (@($review.checks).Count -ne 6)) {
        throw 'security review is incomplete'
    }

    $revision = (git rev-parse HEAD).Trim()
    $files = @(
        'crates/explorer-extension-host/src/plugin_call_guard.rs',
        'crates/explorer-extension-host/src/native_lifecycle.rs',
        'docs/extension-native-safety.md',
        'openspec/changes/build-extensible-plugin-platform/reviews/native-safety-review.json'
    )
    $sha = [Security.Cryptography.SHA256]::Create()
    $bytes = New-Object Collections.Generic.List[byte]
    foreach ($path in $files) { $bytes.AddRange([IO.File]::ReadAllBytes((Resolve-Path $path))) }
    $digest = ([BitConverter]::ToString($sha.ComputeHash($bytes.ToArray()))).Replace('-', '').ToLowerInvariant()
    $ids = @('3.5.8', '3.5.9', '3.5.10', '3.5.11', '3.5.12') + (1..9 | ForEach-Object { "3.6.$_" })
    foreach ($id in $ids) {
        $dir = Join-Path 'target/openspec-evidence/build-extensible-plugin-platform' $id
        New-Item -ItemType Directory -Force $dir | Out-Null
        $result = [ordered]@{
            schema_version = 1; task_id = $id; procedure_kind = 'command'
            command = 'powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run_native_guard_contract.ps1'
            cwd = '.'; environment = [ordered]@{ validation_authority = 'local-only'; uitest_executed = 'false'; offline = 'true' }
            expected = 'exit code 0'; actual = 'passed'; exit_code = 0; source_revision = $revision
            input_sha256 = [ordered]@{ native_guard_sha256 = $digest }
        }
        $result | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $dir 'result.json')
        Write-Output "REPORT $id $digest"
    }
} finally {
    Pop-Location
}
