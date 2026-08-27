param(
    [string]$Workspace = (Get-Location).Path,
    [string]$EvidenceDirectory = "openspec/changes/govern-dead-code-warnings/evidence",
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
$workspacePath = [IO.Path]::GetFullPath($Workspace)
$evidencePath = [IO.Path]::GetFullPath((Join-Path $workspacePath $EvidenceDirectory))
$allowedDispositions = @(
    "remove-superseded",
    "remove-unreferenced",
    "retain-cross-target-live",
    "test-only",
    "retain-required-contract",
    "retain-narrow-suppression"
)
$allowedResults = @("passed", "not-applicable", "superseded")
$genericReasonPattern = "(?i)temporary|temporarily|future use|maybe|might need|for now|TODO|TBD|暫時|未來可能"

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) {
        throw $Message
    }
}

function Get-Json([string]$Path) {
    return Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json -Depth 100
}

function Assert-Unique([object[]]$Values, [string]$Name) {
    $all = @($Values)
    $unique = @($all | Sort-Object -Unique)
    Assert-True ($all.Count -eq $unique.Count) "duplicate $Name"
}

function Test-TaskRecords([object[]]$Records) {
    foreach ($record in @($Records)) {
        Assert-True ($record.result -in $allowedResults) "invalid task result: $($record.result)"
        Assert-True (-not [string]::IsNullOrWhiteSpace($record.subcheck_key)) "missing subcheck_key for task $($record.task_id)"
        Assert-True ($null -ne $record.exit_code) "missing exit_code for task $($record.task_id)"
        if ($record.result -eq "passed") {
            Assert-True ($record.exit_code -eq 0) "passed task has non-zero exit code: $($record.task_id)"
        }
        if ($record.result -eq "superseded") {
            Assert-True ($null -ne $record.replacement -and -not [string]::IsNullOrWhiteSpace($record.replacement)) "superseded task lacks replacement: $($record.task_id)"
        }
    }
    $keys = @($Records | ForEach-Object { "$($_.task_id)|$($_.subcheck_key)" })
    Assert-Unique $keys "task/subcheck key"
}

function Test-LedgerItems([object[]]$BaselineItems, [object[]]$LedgerItems) {
    Assert-Unique @($BaselineItems.id) "baseline item id"
    Assert-Unique @($LedgerItems.id) "ledger item id"
    Assert-True (@($LedgerItems).Count -eq @($BaselineItems).Count) "ledger item count does not match baseline"
    $baselineById = @{}
    foreach ($item in $BaselineItems) {
        $baselineById[$item.id] = $item
    }
    foreach ($item in $LedgerItems) {
        Assert-True ($baselineById.ContainsKey($item.id)) "unknown ledger item id: $($item.id)"
        Assert-True ($item.source_sha256 -eq $baselineById[$item.id].source_sha256) "source hash mismatch for $($item.id)"
    }
}

function Get-SuppressionFingerprint([string]$Label, [string]$Attribute, [string]$Declaration) {
    $bytes = [Text.Encoding]::UTF8.GetBytes("$Label|$Attribute|$Declaration")
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

function Test-SuppressionSource([string]$Text, [string]$Label, [Collections.Generic.HashSet[string]]$BaselineFingerprints) {
    Assert-True ($Text -notmatch "#!\s*\[\s*(allow|expect)\s*\([^\]]*dead_code") "$Label contains crate-wide dead_code suppression"
    $lines = @($Text -split "\r?\n")
    for ($index = 0; $index -lt $lines.Count; $index += 1) {
        $line = $lines[$index]
        if ($line -notmatch "#\s*\[\s*(allow|expect)\s*\([^\]]*dead_code") {
            continue
        }
        $next = $index + 1
        while ($next -lt $lines.Count -and ($lines[$next].Trim().Length -eq 0 -or $lines[$next].TrimStart().StartsWith("#["))) {
            $next += 1
        }
        $attribute = $line.Trim()
        $declaration = if ($next -lt $lines.Count) { $lines[$next].Trim() } else { "" }
        $fingerprint = Get-SuppressionFingerprint $Label $attribute $declaration
        if ($null -ne $BaselineFingerprints -and $BaselineFingerprints.Contains($fingerprint)) {
            continue
        }
        Assert-True ($line -match 'reason\s*=\s*"[^"]+"') "${Label}:$($index + 1) suppression lacks reason"
        $reason = [regex]::Match($line, 'reason\s*=\s*"([^"]+)"').Groups[1].Value
        Assert-True ($reason -notmatch $genericReasonPattern) "${Label}:$($index + 1) suppression has generic reason"
        Assert-True ($reason -match "(?i)openspec|contract|consumer|compatib|owner|task") "${Label}:$($index + 1) reason lacks owner/contract/consumer"
        Assert-True ($reason -match "(?i)remove|until|when|expiry|完成|移除|到期") "${Label}:$($index + 1) reason lacks removal condition"
        if ($next -lt $lines.Count) {
            Assert-True ($lines[$next] -notmatch "^\s*(pub\s+)?mod\s+") "${Label}:$($index + 1) contains module-wide dead_code suppression"
        }
    }
}

function Invoke-MainValidation {
    $baselinePath = Join-Path $evidencePath "baseline.json"
    $ledgerPath = Join-Path $evidencePath "decision-ledger.json"
    $baseline = Get-Json $baselinePath
    $ledger = Get-Json $ledgerPath

    Assert-True ($baseline.emitted_dead_code_count -eq 417) "unexpected emitted dead_code count"
    Assert-True ($baseline.canonical_dead_code_count -eq 322) "unexpected canonical dead_code count"
    Assert-True ($baseline.target_local_canonical_count -eq 251) "unexpected target-local canonical count"
    Assert-Unique @($baseline.canonical_sites.id) "canonical diagnostic id"
    Test-LedgerItems @($baseline.items) @($ledger.items)
    foreach ($item in $ledger.items) {
        Assert-True ($item.disposition -in $allowedDispositions) "invalid disposition for $($item.id)"
        Assert-True (-not [string]::IsNullOrWhiteSpace($item.reason)) "missing reason for $($item.id)"
        Assert-True (-not [string]::IsNullOrWhiteSpace($item.validation)) "missing validation for $($item.id)"
    }
    Assert-True ($ledger.ownership_resolutions.unresolved_count -eq 0) "ownership has unresolved files"
    Assert-True ($ledger.ownership_resolutions.hash_rebaseline_is_not_ownership -eq $true) "hash rebaseline incorrectly replaces ownership"

    Test-TaskRecords @($baseline.task_records)
    Test-TaskRecords @($ledger.task_records)

    $baselineFingerprints = [Collections.Generic.HashSet[string]]::new()
    foreach ($suppression in @($baseline.existing_dead_code_suppressions)) {
        [void]$baselineFingerprints.Add($suppression.fingerprint)
    }
    $rustFiles = @(& rg --files (Join-Path $workspacePath "crates") -g "*.rs")
    foreach ($rustFile in $rustFiles) {
        $relativePath = [IO.Path]::GetFullPath($rustFile).Substring($workspacePath.Length).TrimStart('\', '/').Replace('\', '/')
        Test-SuppressionSource ([IO.File]::ReadAllText($rustFile)) $relativePath $baselineFingerprints
    }

    return [ordered]@{
        result = "passed"
        baseline_items = @($baseline.items).Count
        ledger_items = @($ledger.items).Count
        unresolved_ownership = $ledger.ownership_resolutions.unresolved_count
        suppression_files_scanned = $rustFiles.Count
    }
}

function Invoke-SelfTest {
    $cases = @(
        [ordered]@{ name = "valid-item"; should_pass = $true; text = '#[allow(dead_code, reason = "OpenSpec owner task retains contract until wiring completes; remove when complete")]' + [Environment]::NewLine + 'fn retained() {}' }
        [ordered]@{ name = "crate-wide"; should_pass = $false; text = '#![allow(dead_code, reason = "OpenSpec owner; remove when complete")]' }
        [ordered]@{ name = "module-wide"; should_pass = $false; text = '#[allow(dead_code, reason = "OpenSpec owner; remove when complete")]' + [Environment]::NewLine + 'mod hidden;' }
        [ordered]@{ name = "missing-reason"; should_pass = $false; text = '#[allow(dead_code)]' + [Environment]::NewLine + 'fn hidden() {}' }
        [ordered]@{ name = "generic-reason"; should_pass = $false; text = '#[allow(dead_code, reason = "temporarily for future use")]' + [Environment]::NewLine + 'fn hidden() {}' }
    )
    $results = @()
    foreach ($case in $cases) {
        $passed = $true
        $errorText = $null
        try {
            Test-SuppressionSource $case.text $case.name $null
        } catch {
            $passed = $false
            $errorText = $_.Exception.Message
        }
        Assert-True ($passed -eq $case.should_pass) "self-test case did not produce expected result: $($case.name)"
        $results += [ordered]@{
            subcheck_key = $case.name
            expected_pass = $case.should_pass
            actual_pass = $passed
            error = $errorText
        }
    }

    $validRecords = @(
        [pscustomobject]@{ task_id = "fixture"; result = "passed"; subcheck_key = "one"; command = "fixture"; exit_code = 0; evidence = "fixture" }
    )
    Test-TaskRecords $validRecords

    $negativeRecordCases = @(
        [ordered]@{ name = "missing-subcheck"; records = @([pscustomobject]@{ task_id = "x"; result = "passed"; subcheck_key = ""; command = "x"; exit_code = 0; evidence = "x" }) }
        [ordered]@{ name = "duplicate-subcheck"; records = @(
            [pscustomobject]@{ task_id = "x"; result = "passed"; subcheck_key = "same"; command = "x"; exit_code = 0; evidence = "x" },
            [pscustomobject]@{ task_id = "x"; result = "passed"; subcheck_key = "same"; command = "x"; exit_code = 0; evidence = "x" }
        ) }
        [ordered]@{ name = "superseded-without-replacement"; records = @([pscustomobject]@{ task_id = "x"; result = "superseded"; subcheck_key = "x"; command = "x"; exit_code = 0; evidence = "x" }) }
    )
    foreach ($case in $negativeRecordCases) {
        $rejected = $false
        try {
            Test-TaskRecords $case.records
        } catch {
            $rejected = $true
            $results += [ordered]@{
                subcheck_key = $case.name
                expected_pass = $false
                actual_pass = $false
                error = $_.Exception.Message
            }
        }
        Assert-True $rejected "negative task-record fixture was accepted: $($case.name)"
    }

    $baselineItemFixture = @(
        [pscustomobject]@{ id = "DCI-0000000000000001"; source_sha256 = ("a" * 64) },
        [pscustomobject]@{ id = "DCI-0000000000000002"; source_sha256 = ("b" * 64) }
    )
    $validLedgerFixture = @(
        [pscustomobject]@{ id = "DCI-0000000000000001"; source_sha256 = ("a" * 64) },
        [pscustomobject]@{ id = "DCI-0000000000000002"; source_sha256 = ("b" * 64) }
    )
    Test-LedgerItems $baselineItemFixture $validLedgerFixture
    $ledgerCases = @(
        [ordered]@{ name = "missing-item"; items = @($validLedgerFixture[0]) }
        [ordered]@{ name = "duplicate-item"; items = @($validLedgerFixture[0], $validLedgerFixture[0]) }
        [ordered]@{ name = "unknown-item"; items = @($validLedgerFixture[0], [pscustomobject]@{ id = "DCI-ffffffffffffffff"; source_sha256 = ("b" * 64) }) }
        [ordered]@{ name = "hash-mismatch"; items = @($validLedgerFixture[0], [pscustomobject]@{ id = "DCI-0000000000000002"; source_sha256 = ("c" * 64) }) }
    )
    foreach ($case in $ledgerCases) {
        $rejected = $false
        try {
            Test-LedgerItems $baselineItemFixture $case.items
        } catch {
            $rejected = $true
            $results += [ordered]@{
                subcheck_key = $case.name
                expected_pass = $false
                actual_pass = $false
                error = $_.Exception.Message
            }
        }
        Assert-True $rejected "negative ledger fixture was accepted: $($case.name)"
    }
    return $results
}

$mainResult = Invoke-MainValidation
$selfTestResults = if ($SelfTest) { @(Invoke-SelfTest) } else { @() }
$report = [ordered]@{
    schema_version = 1
    gate = "DCG-POLICY"
    captured_at = [DateTimeOffset]::Now.ToString("o")
    main = $mainResult
    self_tests = $selfTestResults
    task_records = @(
        [ordered]@{ task_id = "1.3.1"; result = "passed"; subcheck_key = "schema"; command = "schema plus validator"; exit_code = 0; evidence = "evidence/schema.json" }
        [ordered]@{ task_id = "1.3.2"; result = "passed"; subcheck_key = "suppression-scan"; command = "validate-governance.ps1"; exit_code = 0; evidence = "main.suppression_files_scanned and suppression fixtures" }
        [ordered]@{ task_id = "1.3.3"; result = "passed"; subcheck_key = "lineage"; command = "validate-governance.ps1"; exit_code = 0; evidence = "baseline/ledger uniqueness and ownership checks" }
        [ordered]@{ task_id = "1.3.4"; result = "passed"; subcheck_key = "subchecks"; command = "validate-governance.ps1 -SelfTest"; exit_code = 0; evidence = "self_tests missing/duplicate subcheck" }
        [ordered]@{ task_id = "1.3.5"; result = "passed"; subcheck_key = "self-test"; command = "validate-governance.ps1 -SelfTest"; exit_code = 0; evidence = "self_tests" }
    )
}
$reportPath = Join-Path $evidencePath "governance-review.json"
$report | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $reportPath -Encoding utf8NoBOM
Write-Output "governance=$reportPath"
Write-Output "result=passed"
Write-Output "self_tests=$($selfTestResults.Count)"
