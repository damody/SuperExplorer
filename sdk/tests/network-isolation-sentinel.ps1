[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$sdkRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$nonce = if($env:SUPEREXPLORER_SENTINEL_NONCE){$env:SUPEREXPLORER_SENTINEL_NONCE}else{[guid]::NewGuid().ToString('N')}
$marker = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-sentinel-' + $nonce + '.marker')
$oldNonce=$env:SUPEREXPLORER_SENTINEL_NONCE; $oldMarker=$env:SUPEREXPLORER_SENTINEL_MARKER
Push-Location $sdkRoot
try {
    $env:SUPEREXPLORER_SENTINEL_NONCE = $nonce; $env:SUPEREXPLORER_SENTINEL_MARKER = $marker
    & cargo build --manifest-path fixtures\egress-sentinel\Cargo.toml --locked --offline 2>&1 | Out-Host
    if ($LASTEXITCODE) { throw "sentinel build failed with exit code $LASTEXITCODE" }
    if (-not (Test-Path -LiteralPath $marker)) { throw 'sentinel marker missing.' }; $att=Get-Content $marker -Raw|ConvertFrom-Json
    if ($att.nonce -ne $nonce -or $att.direct -ne 'blocked' -or $att.child -ne 'blocked' -or -not $att.pid -or -not $att.unix_timestamp) { throw 'sentinel marker fields invalid.' }
    Write-Output 'network isolation sentinel passed'
} finally {
    if($null -eq $oldNonce){Remove-Item Env:SUPEREXPLORER_SENTINEL_NONCE -ErrorAction SilentlyContinue}else{$env:SUPEREXPLORER_SENTINEL_NONCE=$oldNonce}; if($null -eq $oldMarker){Remove-Item Env:SUPEREXPLORER_SENTINEL_MARKER -ErrorAction SilentlyContinue}else{$env:SUPEREXPLORER_SENTINEL_MARKER=$oldMarker}
    Pop-Location
    if (Test-Path -LiteralPath $marker) { Remove-Item -LiteralPath $marker -Force }
}
