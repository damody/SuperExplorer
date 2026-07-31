param(
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $workspaceRoot ('target\overlay-evidence\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
} elseif (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = [IO.Path]::GetFullPath((Join-Path $workspaceRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

$registryPath = 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\ShellIconOverlayIdentifiers'
$handlers = @(Get-ChildItem -LiteralPath $registryPath | ForEach-Object {
    $classId = (Get-ItemProperty -LiteralPath $_.PSPath).'(default)'
    [pscustomobject][ordered]@{
        name = $_.PSChildName.Trim()
        registry_sort_name = $_.PSChildName
        class_id = $classId
    }
})

# Windows reserves overlay image-list entries; only the first 15 registered identifiers are
# generally available to Explorer. Preserve the exact registry ordering instead of guessing which
# third-party badge Windows selected on this machine.
for ($index = 0; $index -lt $handlers.Count; $index++) {
    $handlers[$index] | Add-Member -NotePropertyName registry_index -NotePropertyValue $index
    $handlers[$index] | Add-Member -NotePropertyName within_first_15 -NotePropertyValue ($index -lt 15)
}

$tortoise = @($handlers | Where-Object name -Like 'Tortoise*')
$report = [ordered]@{
    schema_version = 1
    captured_utc = [DateTime]::UtcNow.ToString('o')
    registry_path = $registryPath
    documented_overlay_identifier_limit = 15
    total_handlers = $handlers.Count
    handlers = $handlers
    tortoise_handlers = $tortoise
    tortoise_status_availability = [ordered]@{
        normal = [bool]($tortoise | Where-Object name -EQ 'Tortoise1Normal' | Where-Object within_first_15)
        modified = [bool]($tortoise | Where-Object name -EQ 'Tortoise2Modified' | Where-Object within_first_15)
        conflict = [bool]($tortoise | Where-Object name -EQ 'Tortoise3Conflict' | Where-Object within_first_15)
        added = [bool]($tortoise | Where-Object name -EQ 'Tortoise7Added' | Where-Object within_first_15)
        ignored = [bool]($tortoise | Where-Object name -EQ 'Tortoise8Ignored' | Where-Object within_first_15)
        unversioned = [bool]($tortoise | Where-Object name -EQ 'Tortoise9Unversioned' | Where-Object within_first_15)
    }
}
$reportPath = Join-Path $OutputDirectory 'report.json'
$report | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $reportPath
Write-Output "Shell overlay audit saved: $reportPath"
