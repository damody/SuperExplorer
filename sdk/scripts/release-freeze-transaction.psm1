Set-StrictMode -Version Latest

function Publish-ReleaseFreezeTransaction {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$LedgerPath,
        [Parameter(Mandatory)][string]$StagedLedgerPath,
        [Parameter(Mandatory)][string]$SnapshotPath,
        [Parameter(Mandatory)][string]$StagedSnapshotPath,
        [Parameter(Mandatory)][string]$EvidenceDirectory,
        [Parameter(Mandatory)][string]$StagedEvidenceDirectory,
        [Parameter(Mandatory)][scriptblock]$VerifyPublished
    )

    if (-not (Test-Path -LiteralPath $LedgerPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $StagedLedgerPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $StagedSnapshotPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $StagedEvidenceDirectory -PathType Container)) {
        throw 'release-freeze transaction inputs are incomplete'
    }
    if ((Test-Path -LiteralPath $SnapshotPath) -or (Test-Path -LiteralPath $EvidenceDirectory)) { throw 'release-freeze output is already published' }
    $ledgerBackup = "$LedgerPath.rollback-$([Guid]::NewGuid().ToString('N'))"
    $ledgerPublished = $false
    $snapshotPublished = $false
    $evidencePublished = $false
    try {
        # Same-directory replacements keep the publish operation on one volume.
        [IO.File]::Copy($LedgerPath, $ledgerBackup, $false)
        [IO.File]::Replace($StagedLedgerPath, $LedgerPath, $null)
        $ledgerPublished = $true
        [IO.Directory]::Move($StagedEvidenceDirectory, $EvidenceDirectory)
        $evidencePublished = $true
        [IO.File]::Move($StagedSnapshotPath, $SnapshotPath, $false)
        $snapshotPublished = $true
        & $VerifyPublished
    } catch {
        $failure = $_
        if ($snapshotPublished -and (Test-Path -LiteralPath $SnapshotPath)) { [IO.File]::Delete($SnapshotPath) }
        if ($evidencePublished -and (Test-Path -LiteralPath $EvidenceDirectory)) { [IO.Directory]::Delete($EvidenceDirectory, $true) }
        if ($ledgerPublished -and (Test-Path -LiteralPath $ledgerBackup)) { [IO.File]::Replace($ledgerBackup, $LedgerPath, $null) }
        throw $failure
    } finally {
        foreach ($path in @($StagedLedgerPath, $StagedSnapshotPath, $ledgerBackup)) {
            if (Test-Path -LiteralPath $path) { [IO.File]::Delete($path) }
        }
        if (Test-Path -LiteralPath $StagedEvidenceDirectory) { [IO.Directory]::Delete($StagedEvidenceDirectory, $true) }
    }
}

Export-ModuleMember -Function Publish-ReleaseFreezeTransaction
