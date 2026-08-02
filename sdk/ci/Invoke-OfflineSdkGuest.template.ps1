[CmdletBinding()]
param([Parameter(Mandatory)][string]$RepositoryRoot,[Parameter(Mandatory)][string]$AttestationPath)
$ErrorActionPreference='Stop'; $guest='SuperExplorerSdkOffline-'+[guid]::NewGuid().ToString('N'); $vhd=Join-Path ([IO.Path]::GetTempPath()) "$guest.vhdx"
$config=Import-PowerShellDataFile 'C:\ProgramData\SuperExplorerCI\config.psd1'; $BundleRoot=Join-Path $RepositoryRoot 'sdk'; $BaseVhdx=$config.BaseVhdx; $GuestCredential=Import-Clixml $config.CredentialPath; $nonce=[guid]::NewGuid().ToString('N'); $session=$null; $errors=@()
try {
    if (-not (Get-Command New-VHD -ErrorAction SilentlyContinue)) { throw 'Hyper-V is required.' }
    $baseHash=(Get-FileHash (Resolve-Path $BaseVhdx).Path -Algorithm SHA256).Hash.ToLower()
    if ($config.BaseVhdxSha256 -notmatch '^[0-9a-fA-F]{64}$' -or $baseHash -ne $config.BaseVhdxSha256.ToLower()) { throw 'Base VHDX is not the runner-approved image.' }
    New-VHD -Path $vhd -ParentPath (Resolve-Path $BaseVhdx).Path -Differencing | Out-Null
    New-VM -Name $guest -VHDPath $vhd -Generation 2 | Out-Null
    $adapters=(Get-VMNetworkAdapter -VMName $guest); if($adapters.Count -ne 0){Remove-VMNetworkAdapter -VMName $guest -Confirm:$false}; $before=(Get-VMNetworkAdapter -VMName $guest).Count; if($before -ne 0){throw 'guest NIC removal failed'}
    Start-VM -Name $guest | Out-Null
    $session=New-PSSession -VMName $guest -Credential $GuestCredential -ErrorAction Stop
    $guestRepository='C:\OfflineRun\repo'; $guestArtifacts='C:\OfflineRun\offline-artifacts'
    Invoke-Command -Session $session -ScriptBlock { param($root,$artifacts) New-Item -ItemType Directory -Path $root,$artifacts,(Join-Path $root 'vendor') -Force | Out-Null } -ArgumentList $guestRepository,$guestArtifacts | Out-Null
    # Materialize precisely the roots used by the SDK inventory and verifier;
    # never copy arbitrary/untracked workspace content into the offline guest.
    Copy-Item -ToSession $session -Path (Join-Path $RepositoryRoot 'sdk') -Destination $guestRepository -Recurse -Force
    Copy-Item -ToSession $session -Path (Join-Path $RepositoryRoot 'vendor\gpui-ce') -Destination (Join-Path $guestRepository 'vendor') -Recurse -Force
    foreach($file in @('Cargo.toml','rust-toolchain.toml')){Copy-Item -ToSession $session -Path (Join-Path $RepositoryRoot $file) -Destination (Join-Path $guestRepository $file) -Force}
    $bundleHash=(Get-FileHash (Join-Path $BundleRoot 'bundle-manifest.json') -Algorithm SHA256).Hash.ToLower()
    # The runner owns VM creation/image deployment only.  The copied repository
    # owns the guest checks and attestation contents; do not synthesize success here.
    Invoke-Command -Session $session -ScriptBlock { param($repo,$runNonce,$artifacts) Set-Location $repo; $env:CARGO_NET_OFFLINE='true'; powershell -NoProfile -File sdk\tests\toolchain-contract.ps1; if($LASTEXITCODE){exit $LASTEXITCODE}; Push-Location sdk\tools\bundle-generator; try{cargo.exe run --release --locked --offline -- verify;if($LASTEXITCODE){exit $LASTEXITCODE}}finally{Pop-Location}; powershell -NoProfile -File sdk\tests\offline-guest-gate.ps1 -SdkRoot (Join-Path $repo 'sdk') -ArtifactOutputRoot $artifacts -AttestationPath 'C:\OfflineRun\offline-attestation.json' -RunNonce $runNonce; if($LASTEXITCODE){exit $LASTEXITCODE} } -ArgumentList $guestRepository,$nonce,$guestArtifacts -ErrorAction Stop
    # Independently verify the exported artifacts after the repository gate has
    # returned.  This catches an attestation that does not bind its retained DLLs.
    $guestArtifactEvidence=Invoke-Command -Session $session -ScriptBlock { param($artifacts) $host=Join-Path $artifacts 'abi-root-fixture-host.exe';$plugin=Join-Path $artifacts 'abi_root_fixture_plugin.dll';if(-not(Test-Path -LiteralPath $host) -or -not(Test-Path -LiteralPath $plugin)){throw 'repo gate did not retain exported fixture artifacts'};& $host compatible $plugin;if($LASTEXITCODE){throw "retained host/plugin compatible load failed ($LASTEXITCODE)"};[pscustomobject]@{host_sha256=(Get-FileHash -LiteralPath $host -Algorithm SHA256).Hash.ToLowerInvariant();plugin_sha256=(Get-FileHash -LiteralPath $plugin -Algorithm SHA256).Hash.ToLowerInvariant()} } -ArgumentList $guestArtifacts -ErrorAction Stop
    $after=(Get-VMNetworkAdapter -VMName $guest).Count; if($after -ne 0){throw 'guest NIC changed during offline gate'}
    Copy-Item -FromSession $session -Path 'C:\OfflineRun\offline-attestation.json' -Destination $AttestationPath -Force
    $hostArtifacts=Join-Path (Split-Path -Parent $AttestationPath) 'offline-artifacts'; Copy-Item -FromSession $session -Path $guestArtifacts -Destination $hostArtifacts -Recurse -Force
    $attestation=Get-Content -LiteralPath $AttestationPath -Raw|ConvertFrom-Json
    if($guestArtifactEvidence.host_sha256 -ne $attestation.artifacts.host.sha256 -or $guestArtifactEvidence.plugin_sha256 -ne $attestation.artifacts.plugin.sha256){throw 'independent guest artifact hashes do not match repo attestation'}
    foreach($artifact in @(@('abi-root-fixture-host.exe',$attestation.artifacts.host.sha256),@('abi_root_fixture_plugin.dll',$attestation.artifacts.plugin.sha256))){$path=Join-Path $hostArtifacts $artifact[0];if(-not(Test-Path -LiteralPath $path) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $artifact[1]){throw "copied guest artifact binding failed: $($artifact[0])"}}
    if($attestation.schema_version -ne 2 -or $attestation.producer -ne 'sdk/tests/offline-guest-gate.ps1' -or $attestation.guest_run_nonce -ne $nonce -or $attestation.bundle_sha256 -ne $bundleHash -or $attestation.copied_inventory_root_sha256 -notmatch '^[0-9a-f]{64}$' -or $attestation.network.before_nics -ne 0 -or $attestation.network.after_nics -ne 0 -or @($attestation.network.routes).Count -ne 0 -or $attestation.egress_attempts.direct -ne 'blocked' -or $attestation.egress_attempts.child -ne 'blocked'){throw 'repo-owned guest attestation validation failed'}
} finally {
    try{if($session){Remove-PSSession $session -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; try{if(Get-VM -Name $guest -ErrorAction SilentlyContinue){Stop-VM -Name $guest -TurnOff -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; try{if(Get-VM -Name $guest -ErrorAction SilentlyContinue){Remove-VM -Name $guest -Force -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; try{if(Test-Path $vhd){Remove-Item -LiteralPath $vhd -Force -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; if($errors.Count){throw ($errors -join '; ')}
}
