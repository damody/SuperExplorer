$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

& (Join-Path $PSScriptRoot 'native-plugin-security-docs-contract.ps1')
if (-not $?) {
    throw 'native plugin security documentation contract failed'
}

# Exercise the production DLL fixture through real native loading. It verifies
# raw process abort residue, Safe Mode block and confirmation, slow-callback
# identity/timing, and a resident-DLL drain timeout with sticky restart state.
& (Join-Path $PSScriptRoot 'extension-dll-loader-contract.ps1')
if ($LASTEXITCODE -ne 0) {
    throw 'native DLL call-guard and lifecycle contract failed'
}

Write-Output 'native call-guard contract: PASS'
