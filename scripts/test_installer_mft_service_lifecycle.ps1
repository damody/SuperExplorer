$ErrorActionPreference = 'Stop'

$workspaceRoot = Split-Path -Parent $PSScriptRoot
$installerPath = Join-Path $workspaceRoot 'installer\SuperExplorer.nsi'
$installer = Get-Content -Raw -Encoding UTF8 -LiteralPath $installerPath
$uninstallStart = $installer.IndexOf('Section "Uninstall"', [StringComparison]::Ordinal)
if ($uninstallStart -lt 0) {
    throw 'Installer MFT lifecycle contract missing: uninstall section'
}
$uninstall = $installer.Substring($uninstallStart)

function Require-Text {
    param([string]$Text, [string]$Description)
    if (-not $installer.Contains($Text)) {
        throw "Installer MFT lifecycle contract missing: $Description"
    }
}

function Require-Order {
    param([string]$Before, [string]$After, [string]$Description)
    $beforeIndex = $installer.IndexOf($Before, [StringComparison]::Ordinal)
    $afterIndex = $installer.IndexOf($After, [StringComparison]::Ordinal)
    if ($beforeIndex -lt 0 -or $afterIndex -lt 0 -or $beforeIndex -ge $afterIndex) {
        throw "Installer MFT lifecycle order violation: $Description"
    }
}

function Require-UninstallText {
    param([string]$Text, [string]$Description)
    if (-not $uninstall.Contains($Text)) {
        throw "Uninstaller MFT lifecycle contract missing: $Description"
    }
}

function Require-UninstallOrder {
    param([string]$Before, [string]$After, [string]$Description)
    $beforeIndex = $uninstall.IndexOf($Before, [StringComparison]::Ordinal)
    $afterIndex = $uninstall.IndexOf($After, [StringComparison]::Ordinal)
    if ($beforeIndex -lt 0 -or $afterIndex -lt 0 -or $beforeIndex -ge $afterIndex) {
        throw "Uninstaller MFT lifecycle order violation: $Description"
    }
}

$query = 'nsExec::ExecToStack ''"$SYSDIR\sc.exe" query SuperExplorerMft'''
$stop = 'nsExec::ExecToStack ''"$SYSDIR\sc.exe" stop SuperExplorerMft'''
$serviceFile = 'File /oname=superexplorer-mft-service.exe "${MFT_SERVICE_EXE}"'
$start = '!insertmacro ExecServiceChecked ''"$SYSDIR\sc.exe" start SuperExplorerMft'''

Require-Text $query 'SCM query before upgrading the service binary'
Require-Text $stop 'checked SCM stop request'
Require-Text '${StrStr} $3 $1 "1060"' 'first-install service-not-found handling'
Require-Text '${StrStr} $3 $1 "1062"' 'already-stopped race handling'
Require-Text '${StrStr} $3 $1 "STOPPED"' 'STOPPED state verification'
Require-Text '${StrStr} $3 $1 "STOP_PENDING"' 'pre-existing stop-pending race handling'
Require-Text 'service_stop_timeout:' 'bounded STOPPED wait timeout'
Require-Text 'service_ready_for_files:' 'file overwrite gate after STOPPED verification'
Require-Text '${StrStr} $3 $1 "RUNNING"' 'RUNNING state verification after restart'

Require-Order $query $stop 'query must precede stop request'
Require-Order $stop 'service_ready_for_files:' 'stop request must precede the file-install gate'
Require-Order 'service_ready_for_files:' $serviceFile 'STOPPED gate must precede service binary overwrite'
Require-Order $serviceFile $start 'service binary overwrite must precede restart'
Require-Order $start '${StrStr} $3 $1 "RUNNING"' 'restart must precede RUNNING verification'

$uninstallQuery = 'nsExec::ExecToStack ''"$SYSDIR\sc.exe" query SuperExplorerMft'''
$uninstallStop = 'nsExec::ExecToStack ''"$SYSDIR\sc.exe" stop SuperExplorerMft'''
$uninstallDeleteService = 'nsExec::ExecToStack ''"$SYSDIR\sc.exe" delete SuperExplorerMft'''
$uninstallDeleteBinary = 'Delete "$INSTDIR\superexplorer-mft-service.exe"'
Require-UninstallText $uninstallQuery 'checked SCM query before deleting the service'
Require-UninstallText $uninstallStop 'checked SCM stop request'
Require-UninstallText '${UnStrStr} $3 $1 "1060"' 'already-absent service handling'
Require-UninstallText '${UnStrStr} $3 $1 "1062"' 'already-stopped race handling'
Require-UninstallText 'un.service_stop_timeout:' 'bounded STOPPED wait timeout'
Require-UninstallText 'un.service_ready_for_delete:' 'delete gate after STOPPED verification'
Require-UninstallText '${ElseIf} $0 == 1060' 'already-absent SCM delete handling'
Require-UninstallText 'Unable to delete SuperExplorer MFT Windows Service' 'SCM delete failure abort path'
Require-UninstallOrder $uninstallQuery $uninstallStop 'query must precede stop request'
Require-UninstallOrder $uninstallStop 'un.service_ready_for_delete:' 'stop must precede delete gate'
Require-UninstallOrder 'un.service_ready_for_delete:' $uninstallDeleteService 'STOPPED gate must precede service deletion'
Require-UninstallOrder $uninstallDeleteService $uninstallDeleteBinary 'service deletion must precede binary deletion'

if ($installer -match '(?im)^\s*RMDir\s+/r\s+.*MftIndex') {
    throw 'Installer must preserve the service-owned MFT cache during upgrade/repair/uninstall.'
}
Require-Text 'Preserving service-owned MFT cache for reinstall or rollback.' 'explicit retained-cache disposition'

Write-Output 'Installer MFT service lifecycle contract PASS.'
