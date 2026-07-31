$ErrorActionPreference = 'Stop'

$metadata = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
$ui = $metadata.packages | Where-Object name -eq 'explorer-ui'

if ($null -eq $ui) {
    throw 'explorer-ui package is missing from workspace metadata.'
}

$forbidden = @('explorer-shell-win', 'windows')
$violations = $ui.dependencies | Where-Object { $_.name -in $forbidden }

if ($violations) {
    throw "explorer-ui has forbidden dependencies: $($violations.name -join ', ')"
}

$platformNeutralPackages = @('explorer-automation', 'explorer-ai')
$platformNeutralForbidden = @(
    'explorer-shell-win',
    'gpui',
    'gpui-elements',
    'gpui-windows',
    'windows',
    'windows-core'
)
$platformNeutralDependencyViolations = foreach ($packageName in $platformNeutralPackages) {
    $package = $metadata.packages | Where-Object name -eq $packageName
    if ($null -eq $package) {
        throw "$packageName package is missing from workspace metadata."
    }
    foreach ($dependency in $package.dependencies) {
        if ($dependency.name -in $platformNeutralForbidden) {
            "$packageName -> $($dependency.name)"
        }
    }
}
if ($platformNeutralDependencyViolations) {
    throw "platform-neutral automation dependency violation: $($platformNeutralDependencyViolations -join ', ')"
}

$productionTestSupportViolations = foreach ($package in $metadata.packages) {
    if ($package.name -eq 'explorer-test-support') {
        continue
    }
    foreach ($dependency in $package.dependencies) {
        if ($dependency.name -eq 'explorer-test-support' -and $dependency.kind -ne 'dev') {
            $dependencyKind = if ($null -eq $dependency.kind) { 'normal' } else { $dependency.kind }
            "$($package.name) ($dependencyKind)"
        }
    }
}
if ($productionTestSupportViolations) {
    throw "production dependency on explorer-test-support: $($productionTestSupportViolations -join ', ')"
}

$runnerDependencyViolations = foreach ($package in $metadata.packages) {
    if ($package.name -eq 'explorer-uitest') {
        continue
    }
    foreach ($dependency in $package.dependencies) {
        if ($dependency.name -eq 'explorer-uitest') {
            "$($package.name) -> explorer-uitest"
        }
    }
}
if ($runnerDependencyViolations) {
    throw "production/workspace package depends on test runner: $($runnerDependencyViolations -join ', ')"
}

$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$uiSourceRoot = Join-Path $workspaceRoot 'crates\explorer-ui\src'
$sourcePatterns = @(
    [regex]::new('\bexplorer_shell_win\b'),
    [regex]::new('\bstd::fs\b'),
    [regex]::new('\bfs::(?:read|read_dir|read_to_string|write|copy|rename|remove_file|remove_dir)\s*\(')
)
$sourceViolations = foreach ($file in Get-ChildItem -LiteralPath $uiSourceRoot -Recurse -Filter '*.rs' -File) {
    $lineNumber = 0
    foreach ($line in Get-Content -Encoding utf8 -LiteralPath $file.FullName) {
        $lineNumber++
        if ($line.TrimStart().StartsWith('//') -or $line.Contains('architecture-check: allow')) {
            continue
        }
        foreach ($pattern in $sourcePatterns) {
            if ($pattern.IsMatch($line)) {
                [pscustomobject]@{
                    File = $file.FullName.Substring($workspaceRoot.Length + 1)
                    Line = $lineNumber
                    Text = $line.Trim()
                }
                break
            }
        }
    }
}

if ($sourceViolations) {
    $sourceViolations | Format-Table -AutoSize | Out-String | Write-Error
    throw 'explorer-ui contains Shell coupling or synchronous filesystem I/O.'
}

$platformNeutralPatterns = @(
    [regex]::new('\bgpui(?:_windows|_elements)?\b'),
    [regex]::new('\bwindows(?:_core)?::'),
    [regex]::new('\bexplorer_shell_win\b')
)
$platformNeutralSourceViolations = foreach ($packageName in $platformNeutralPackages) {
    $sourceRoot = Join-Path $workspaceRoot "crates\$packageName\src"
    foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -Recurse -Filter '*.rs' -File) {
        $lineNumber = 0
        $testModuleFollows = $false
        foreach ($line in Get-Content -Encoding utf8 -LiteralPath $file.FullName) {
            $lineNumber++
            if ($line.Trim() -eq '#[cfg(test)]') {
                $testModuleFollows = $true
                continue
            }
            if ($testModuleFollows) {
                continue
            }
            if ($line.TrimStart().StartsWith('//') -or $line.Contains('architecture-check: allow')) {
                continue
            }
            foreach ($pattern in $platformNeutralPatterns) {
                if ($pattern.IsMatch($line)) {
                    [pscustomobject]@{
                        File = $file.FullName.Substring($workspaceRoot.Length + 1)
                        Line = $lineNumber
                        Text = $line.Trim()
                    }
                    break
                }
            }
        }
    }
}

if ($platformNeutralSourceViolations) {
    $platformNeutralSourceViolations | Format-Table -AutoSize | Out-String | Write-Error
    throw 'explorer-automation or explorer-ai contains GPUI, Shell, or Win32 coupling.'
}

$mainProcessRoots = @(
    'crates\explorer-app\src',
    'crates\explorer-common\src',
    'crates\explorer-model\src',
    'crates\explorer-shell-win\src',
    'crates\explorer-ui\src'
)
$previewActivationPatterns = @(
    [regex]::new('\bIPreviewHandler\b'),
    [regex]::new('\bIInitializeWith(?:File|Stream|Item)\b'),
    [regex]::new('\bDoPreview\s*\(')
)
$previewActivationViolations = foreach ($relativeRoot in $mainProcessRoots) {
    $sourceRoot = Join-Path $workspaceRoot $relativeRoot
    foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -Recurse -Filter '*.rs' -File) {
        $lineNumber = 0
        $testModuleFollows = $false
        foreach ($line in Get-Content -Encoding utf8 -LiteralPath $file.FullName) {
            $lineNumber++
            if ($line.Trim() -eq '#[cfg(test)]') {
                $testModuleFollows = $true
                continue
            }
            if ($testModuleFollows) {
                continue
            }
            if ($line.TrimStart().StartsWith('//') -or $line.Contains('architecture-check: allow')) {
                continue
            }
            foreach ($pattern in $previewActivationPatterns) {
                if ($pattern.IsMatch($line)) {
                    [pscustomobject]@{
                        File = $file.FullName.Substring($workspaceRoot.Length + 1)
                        Line = $lineNumber
                        Text = $line.Trim()
                    }
                    break
                }
            }
        }
    }
}
if ($previewActivationViolations) {
    $previewActivationViolations | Format-Table -AutoSize | Out-String | Write-Error
    throw 'main-process production code activates a Preview Handler; activation belongs in the disposable broker worker.'
}

$roadmapOwnedFiles = Get-ChildItem -LiteralPath (Join-Path $workspaceRoot 'crates') -Recurse -Filter '*.rs' -File |
    Where-Object {
        $_.Name -match '^(roadmap|session|thumbnail|broker|preview)(?:_.+)?\.rs$' -or
        $_.FullName -match 'explorer-broker(?:-protocol|-worker)?'
    }
$unboundedPatterns = @(
    [regex]::new('\b(?:std::sync::)?mpsc::channel\s*\('),
    [regex]::new('\bcrossbeam_channel::unbounded\s*\(')
)
$unboundedViolations = foreach ($file in $roadmapOwnedFiles) {
    $lineNumber = 0
    foreach ($line in Get-Content -Encoding utf8 -LiteralPath $file.FullName) {
        $lineNumber++
        if ($line.TrimStart().StartsWith('//') -or $line.Contains('architecture-check: allow')) {
            continue
        }
        foreach ($pattern in $unboundedPatterns) {
            if ($pattern.IsMatch($line)) {
                [pscustomobject]@{
                    File = $file.FullName.Substring($workspaceRoot.Length + 1)
                    Line = $lineNumber
                    Text = $line.Trim()
                }
                break
            }
        }
    }
}
if ($unboundedViolations) {
    $unboundedViolations | Format-Table -AutoSize | Out-String | Write-Error
    throw 'roadmap-owned production code creates an unbounded channel.'
}

$lockSafetyRoots = @(
    'crates\explorer-app\src',
    'crates\explorer-model\src',
    'crates\explorer-shell-win\src',
    'crates\explorer-ui\src'
)
$unsafeLockPatterns = @(
    [regex]::new('\bTerminateProcess\s*\('),
    [regex]::new('\bRmForceShutdown\b'),
    [regex]::new('\bShellExecute(?:Ex)?W?\b.*\brunas\b', [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
)
$unsafeLockViolations = foreach ($relativeRoot in $lockSafetyRoots) {
    $sourceRoot = Join-Path $workspaceRoot $relativeRoot
    foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -Recurse -Filter '*.rs' -File) {
        if ($file.Name -eq 'explorer-lock-holder.rs') {
            continue
        }
        $lineNumber = 0
        foreach ($line in Get-Content -Encoding utf8 -LiteralPath $file.FullName) {
            $lineNumber++
            if ($line.TrimStart().StartsWith('//') -or $line.Contains('architecture-check: allow')) {
                continue
            }
            foreach ($pattern in $unsafeLockPatterns) {
                if ($pattern.IsMatch($line)) {
                    [pscustomobject]@{
                        File = $file.FullName.Substring($workspaceRoot.Length + 1)
                        Line = $lineNumber
                        Text = $line.Trim()
                    }
                    break
                }
            }
        }
    }
}
if ($unsafeLockViolations) {
    $unsafeLockViolations | Format-Table -AutoSize | Out-String | Write-Error
    throw 'locked-delete recovery contains force termination, force shutdown, or elevation.'
}

$lockContractSource = Get-Content -Raw -Encoding utf8 -LiteralPath (Join-Path $workspaceRoot 'crates\explorer-model\src\lock_recovery.rs')
if ($lockContractSource -match '(?im)^\s*pub\s+(?:process_|executable_)?(?:path|command_line|credentials?)\s*:') {
    throw 'lock-owner model contracts must not export process paths, command lines, or credentials.'
}

$domainSource = Get-Content -Raw -Encoding utf8 -LiteralPath (Join-Path $workspaceRoot 'crates\explorer-model\src\domain.rs')
if ($domainSource -match 'impl\s+From\s*<\s*(?:PathBuf|&?Path)\s*>\s+for\s+ShellItemId') {
    throw 'ShellItemId must not derive identity directly from a filesystem path.'
}

$versionedBoundaries = @(
    @{
        Path = 'crates\explorer-model\src\session.rs'
        Marker = 'SESSION_SCHEMA_VERSION'
        Description = 'session persistence'
    },
    @{
        Path = 'crates\explorer-broker-protocol\src\lib.rs'
        Marker = 'BROKER_PROTOCOL_VERSION'
        Description = 'broker protocol'
    }
)
foreach ($boundary in $versionedBoundaries) {
    $boundaryPath = Join-Path $workspaceRoot $boundary.Path
    if ((Test-Path -LiteralPath $boundaryPath -PathType Leaf) -and
        -not (Select-String -Quiet -SimpleMatch -Pattern $boundary.Marker -LiteralPath $boundaryPath)) {
        throw "$($boundary.Description) boundary is missing required version marker $($boundary.Marker)."
    }
}

Write-Output 'Architecture check passed: UI is Shell-free, automation is platform-neutral, roadmap work is bounded/versioned, Preview activation is broker-only, locked-delete recovery cannot force/elevate or export process paths, and test-only crates are absent from production dependencies.'
