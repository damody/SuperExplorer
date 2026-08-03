Set-StrictMode -Version Latest

function Get-GpgValidSigPrimaryFingerprintV1 {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]]$StatusLines,
        [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{40}$')][string]$ExpectedPrimaryFingerprint,
        [Parameter(Mandatory)][string]$EvidenceName
    )

    $valid = @($StatusLines | Where-Object { $_ -match '^\[GNUPG:\]\s+VALIDSIG\s+' })
    if ($valid.Count -ne 1) { throw "$EvidenceName did not emit exactly one VALIDSIG status record" }
    $fields = @($valid[0] -split '\s+' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    # VALIDSIG always begins with the actual signing fingerprint.  Its optional
    # final primary-key-fingerprint is present only for a signing subkey; for a
    # direct primary-key signature the final field is the signature class.
    if ($fields.Count -lt 3 -or $fields[2] -notmatch '^[0-9A-Fa-f]{40}$') { throw "$EvidenceName VALIDSIG has no signing fingerprint" }
    $primary = if ($fields[-1] -match '^[0-9A-Fa-f]{40}$') { [string]$fields[-1] } else { [string]$fields[2] }
    if ($primary.ToUpperInvariant() -cne $ExpectedPrimaryFingerprint.ToUpperInvariant()) {
        throw "$EvidenceName VALIDSIG primary fingerprint does not match the protected release policy"
    }
    return $primary.ToUpperInvariant()
}

function Invoke-GpgvVerifiedPrimaryV1 {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Keyring,
        [Parameter(Mandatory)][string]$Signature,
        [Parameter(Mandatory)][string]$Data,
        [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{40}$')][string]$ExpectedPrimaryFingerprint
    )

    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $status = & gpgv.exe --status-fd 1 --keyring $Keyring $Signature $Data 2>&1
        $exitCode = $LASTEXITCODE
    } finally { $ErrorActionPreference = $saved }
    if ($exitCode -ne 0) { throw "Detached release-bundle signature verification failed: $($status -join "`n")" }
    return Get-GpgValidSigPrimaryFingerprintV1 -StatusLines @($status | ForEach-Object { [string]$_ }) -ExpectedPrimaryFingerprint $ExpectedPrimaryFingerprint -EvidenceName 'Detached release-bundle signature'
}

function Get-CanonicalGateAttestationDigestV1 {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Gates)
    $body = [ordered]@{
        schema_version = $Gates.schema_version
        gate_manifest_sha256 = $Gates.gate_manifest_sha256
        candidate_plan_digest = $Gates.candidate_plan_digest
        workflow_run_id = $Gates.workflow_run_id
        nonce = $Gates.nonce
        results = @($Gates.results | ForEach-Object { [ordered]@{ id = $_.id; exit_code = $_.exit_code } })
    }
    $json = $body | ConvertTo-Json -Depth 10 -Compress
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($json)))).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Assert-ExactRemoteProtectedTagV1 {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Repository,
        [Parameter(Mandatory)][string]$Tag,
        [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedTagObject,
        [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedRevision,
        [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedTree
    )
    & git -C $Repository fetch --no-tags origin ("+refs/tags/{0}:refs/tags/{0}" -f $Tag)
    if ($LASTEXITCODE -ne 0) { throw 'Unable to fetch protected tag from the authorized remote.' }
    $escapedTag = [regex]::Escape($Tag)
    $remote = @(& git -C $Repository ls-remote --tags origin "refs/tags/$Tag" "refs/tags/$Tag^{}" 2>&1)
    if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve protected tag from the authorized remote.' }
    $tagLine = @($remote | Where-Object { $_ -match ("^[0-9a-f]{{40}}\s+refs/tags/{0}$" -f $escapedTag) })
    $peeledLine = @($remote | Where-Object { $_ -match ("^[0-9a-f]{{40}}\s+refs/tags/{0}\^\{{\}}$" -f $escapedTag) })
    if ($tagLine.Count -ne 1 -or $peeledLine.Count -ne 1) { throw 'Authorized remote did not return one annotated tag object and one peeled revision.' }
    $remoteObject = (($tagLine[0] -split '\s+')[0]).Trim(); $remoteRevision = (($peeledLine[0] -split '\s+')[0]).Trim()
    $localObject = (& git -C $Repository rev-parse --verify "refs/tags/$Tag").Trim(); if ($LASTEXITCODE -ne 0) { throw 'Local protected tag object is absent after remote fetch.' }
    $localRevision = (& git -C $Repository rev-parse --verify "$Tag^{}").Trim(); if ($LASTEXITCODE -ne 0) { throw 'Local protected tag peel is absent after remote fetch.' }
    $localTree = (& git -C $Repository show -s --format=%T $localRevision).Trim(); if ($LASTEXITCODE -ne 0) { throw 'Local protected tag tree is absent after remote fetch.' }
    if ($remoteObject -ne $ExpectedTagObject -or $remoteRevision -ne $ExpectedRevision -or $localObject -ne $ExpectedTagObject -or $localRevision -ne $ExpectedRevision -or $localTree -ne $ExpectedTree) {
        throw 'Protected tag authority differs between remote tag object, local tag object, revision, or tree.'
    }
}

Export-ModuleMember -Function Get-GpgValidSigPrimaryFingerprintV1,Invoke-GpgvVerifiedPrimaryV1,Get-CanonicalGateAttestationDigestV1,Assert-ExactRemoteProtectedTagV1
