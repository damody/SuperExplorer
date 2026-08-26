[CmdletBinding()]
param(
    [string]$OutputPath = "evidence/baseline.json",
    [string]$CompilerJsonlPath = "evidence/compiler-messages.jsonl",
    [string]$DiffDirectory = "evidence/prechange-diffs"
)

$ErrorActionPreference = "Stop"
$changeRoot = Split-Path -Parent $PSScriptRoot
$workspaceRoot = [System.IO.Path]::GetFullPath((Join-Path $changeRoot "..\..\.."))
$outputFull = [System.IO.Path]::GetFullPath((Join-Path $changeRoot $OutputPath))
$jsonlFull = [System.IO.Path]::GetFullPath((Join-Path $changeRoot $CompilerJsonlPath))
$diffFull = [System.IO.Path]::GetFullPath((Join-Path $changeRoot $DiffDirectory))

$ownedFiles = @(
    "crates/explorer-app/src/main.rs",
    "crates/explorer-app/src/application.rs",
    "crates/explorer-app/src/brokered_service.rs",
    "crates/explorer-app/src/remote_service.rs",
    "crates/explorer-extension-host/src/virtual_container_mutation.rs",
    "crates/explorer-app/src/mft_focus.rs",
    "crates/explorer-app/src/mft_journal.rs",
    "crates/explorer-app/src/mft_migration.rs",
    "crates/explorer-app/src/mft_size_map.rs",
    "crates/explorer-app/src/mft_sqlite.rs",
    "crates/explorer-app/src/mft_query.rs",
    "crates/explorer-app/src/bin/mft_service.rs"
)

function Get-TextSha256([string]$Text) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $hash = [System.Security.Cryptography.SHA256]::HashData($bytes)
    return [System.Convert]::ToHexString($hash).ToLowerInvariant()
}

function Get-CanonicalRelativePath([string]$Path) {
    $candidate = $Path.Replace("/", [System.IO.Path]::DirectorySeparatorChar)
    $full = if ([System.IO.Path]::IsPathRooted($candidate)) {
        [System.IO.Path]::GetFullPath($candidate)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $candidate))
    }
    return [System.IO.Path]::GetRelativePath($workspaceRoot, $full).Replace("\", "/")
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outputFull), (Split-Path -Parent $jsonlFull), $diffFull | Out-Null

Push-Location $workspaceRoot
try {
    $rawLines = @(& cargo check --workspace --locked --message-format=json 2>&1 | ForEach-Object { $_.ToString() })
    $cargoExit = $LASTEXITCODE
    $jsonLines = [System.Collections.Generic.List[string]]::new()
    $diagnostics = [System.Collections.Generic.List[object]]::new()
    foreach ($line in $rawLines) {
        try {
            $event = $line | ConvertFrom-Json -Depth 30
        } catch {
            continue
        }
        if ($null -eq $event.reason) {
            continue
        }
        $jsonLines.Add($line)
        if ($event.reason -ne "compiler-message" -or $event.message.level -ne "warning") {
            continue
        }
        $primary = $event.message.spans | Where-Object is_primary | Select-Object -First 1
        if ($null -eq $primary) {
            continue
        }
        $code = if ($null -eq $event.message.code) { "(no-code)" } else { $event.message.code.code }
        $diagnostics.Add([pscustomobject]@{
            code = $code
            file = Get-CanonicalRelativePath $primary.file_name
            line = [int]$primary.line_start
            column = [int]$primary.column_start
            message = $event.message.message
            target = $event.target.name
        })
    }
    [System.IO.File]::WriteAllLines($jsonlFull, $jsonLines, [System.Text.UTF8Encoding]::new($false))

    $unsafeGroups = $diagnostics | Where-Object code -eq "unsafe_code" |
        Group-Object { "$($_.file):$($_.line):$($_.column):$($_.message)" } |
        Sort-Object { $_.Group[0].file }, { $_.Group[0].line }, { $_.Group[0].column }
    $locations = [System.Collections.Generic.List[object]]::new()
    $sequence = 1
    foreach ($group in $unsafeGroups) {
        $first = $group.Group[0]
        $locations.Add([ordered]@{
            id = "UCG-{0:D4}" -f $sequence
            file = $first.file
            line = $first.line
            column = $first.column
            message = $first.message
            targets = @($group.Group.target | Sort-Object -Unique)
            diagnostic_count = $group.Count
            disposition = $null
            expectation_reason = $null
            safety_review = $null
        })
        $sequence++
    }

    $warningCounts = [ordered]@{}
    foreach ($group in ($diagnostics | Where-Object code -ne "unsafe_code" | Group-Object code | Sort-Object Name)) {
        $warningCounts[$group.Name] = $group.Count
    }

    $fileSnapshots = [System.Collections.Generic.List[object]]::new()
    foreach ($relativePath in $ownedFiles) {
        $absolutePath = Join-Path $workspaceRoot $relativePath
        $diffLines = @(& git diff --no-ext-diff -- $relativePath | ForEach-Object { $_.ToString() })
        $diffText = ($diffLines -join "`n")
        if ($diffText.Length -gt 0) { $diffText += "`n" }
        $safeName = $relativePath.Replace("/", "__") + ".patch"
        $diffPath = Join-Path $diffFull $safeName
        [System.IO.File]::WriteAllText($diffPath, $diffText, [System.Text.UTF8Encoding]::new($false))
        $fileSnapshots.Add([ordered]@{
            path = $relativePath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $absolutePath).Hash.ToLowerInvariant()
            scoped_diff_path = [System.IO.Path]::GetRelativePath($changeRoot, $diffPath).Replace("\", "/")
            scoped_diff_sha256 = Get-TextSha256 $diffText
            dirty = $diffText.Length -gt 0
        })
    }

    $suppressionRows = [System.Collections.Generic.List[object]]::new()
    $suppressionLines = @(& rg -n --no-heading '#!?\[\s*allow\s*\(\s*unsafe_code' crates 2>$null)
    foreach ($row in $suppressionLines) {
        if ($row -match '^(?<path>[^:]+):(?<line>\d+):(?<text>.*)$') {
            $scope = if ($Matches.text.TrimStart().StartsWith("#!")) { "module-or-crate" } else { "item" }
            $suppressionRows.Add([ordered]@{
                path = $Matches.path.Replace("\", "/")
                line = [int]$Matches.line
                scope = $scope
                text = $Matches.text.Trim()
                disposition = "deferred-residual-risk"
            })
        }
    }

    $dirtyState = @(& git status --short | ForEach-Object { $_.ToString() })
    $rustcVersion = @(& rustc -Vv | ForEach-Object { $_.ToString() })
    $cargoVersion = (& cargo -V).ToString()
    $revision = (& git rev-parse HEAD).Trim()
    $cargoConfigPaths = @(".cargo/config.toml", ".cargo/config") | Where-Object { Test-Path (Join-Path $workspaceRoot $_) }
    $environment = [ordered]@{
        RUSTFLAGS = [Environment]::GetEnvironmentVariable("RUSTFLAGS")
        CARGO_TARGET_DIR = [Environment]::GetEnvironmentVariable("CARGO_TARGET_DIR")
        CARGO_BUILD_TARGET = [Environment]::GetEnvironmentVariable("CARGO_BUILD_TARGET")
    }
    $baseline = [ordered]@{
        schema_version = 1
        gate = "UCG-BASE"
        captured_at = [DateTimeOffset]::Now.ToString("o")
        revision = $revision
        command = "cargo check --workspace --locked --message-format=json"
        exit_status = $cargoExit
        toolchain = [ordered]@{
            rustc = $rustcVersion
            cargo = $cargoVersion
            target_triple = (($rustcVersion | Where-Object { $_ -like "host:*" }) -replace '^host:\s*', '')
            cargo_config_paths = @($cargoConfigPaths)
            features = "default"
            environment = $environment
        }
        compiler_jsonl = [System.IO.Path]::GetRelativePath($changeRoot, $jsonlFull).Replace("\", "/")
        compiler_jsonl_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $jsonlFull).Hash.ToLowerInvariant()
        dirty_tree = $dirtyState
        canonical_unsafe_count = $locations.Count
        emitted_unsafe_count = ($diagnostics | Where-Object code -eq "unsafe_code").Count
        unsafe_locations = $locations
        non_unsafe_warning_counts = $warningCounts
        owned_file_snapshots = $fileSnapshots
        deferred_suppressions = $suppressionRows
    }
    $json = $baseline | ConvertTo-Json -Depth 30
    [System.IO.File]::WriteAllText($outputFull, $json + "`n", [System.Text.UTF8Encoding]::new($false))
    Write-Output "baseline=$outputFull unsafe_canonical=$($locations.Count) unsafe_emitted=$($baseline.emitted_unsafe_count) cargo_exit=$cargoExit"
    exit $cargoExit
} finally {
    Pop-Location
}
