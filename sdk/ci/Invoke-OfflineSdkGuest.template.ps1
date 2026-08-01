[CmdletBinding()]
param([Parameter(Mandatory)][string]$RepositoryRoot,[Parameter(Mandatory)][string]$AttestationPath)
$ErrorActionPreference='Stop'; $guest='SuperExplorerSdkOffline-'+[guid]::NewGuid().ToString('N'); $vhd=Join-Path ([IO.Path]::GetTempPath()) "$guest.vhdx"
$config=Import-PowerShellDataFile 'C:\ProgramData\SuperExplorerCI\config.psd1'; $BundleRoot=Join-Path $RepositoryRoot 'sdk'; $BaseVhdx=$config.BaseVhdx; $GuestCredential=Import-Clixml $config.CredentialPath; $nonce=[guid]::NewGuid().ToString('N'); $session=$null; $errors=@()
try {
    if (-not (Get-Command New-VHD -ErrorAction SilentlyContinue)) { throw 'Hyper-V is required.' }
    New-VHD -Path $vhd -ParentPath (Resolve-Path $BaseVhdx).Path -Differencing | Out-Null
    New-VM -Name $guest -VHDPath $vhd -Generation 2 | Out-Null
    $adapters=(Get-VMNetworkAdapter -VMName $guest); if($adapters.Count -ne 0){Remove-VMNetworkAdapter -VMName $guest -Confirm:$false}; $before=(Get-VMNetworkAdapter -VMName $guest).Count; if($before -ne 0){throw 'guest NIC removal failed'}
    Start-VM -Name $guest | Out-Null
    $session=New-PSSession -VMName $guest -Credential $GuestCredential -ErrorAction Stop
    Invoke-Command -Session $session -ScriptBlock { New-Item -ItemType Directory -Path C:\OfflineRun -Force | Out-Null } | Out-Null
    Copy-Item -ToSession $session -Path $BundleRoot -Destination 'C:\OfflineRun\sdk' -Recurse -Force
    Copy-Item -ToSession $session -Path (Join-Path $RepositoryRoot 'rust-toolchain.toml') -Destination 'C:\OfflineRun\rust-toolchain.toml' -Force
    $bundleHash=(Get-FileHash (Join-Path $BundleRoot 'Cargo.lock') -Algorithm SHA256).Hash.ToLower(); $baseHash=(Get-FileHash $BaseVhdx -Algorithm SHA256).Hash.ToLower()
    Invoke-Command -Session $session -ScriptBlock { param($root,$n) Set-Location $root; $env:SUPEREXPLORER_SENTINEL_NONCE=$n; powershell -NoProfile -File tests\toolchain-contract.ps1; if($LASTEXITCODE){exit $LASTEXITCODE}; powershell -NoProfile -File tests\network-isolation-sentinel.ps1; if($LASTEXITCODE){exit $LASTEXITCODE}; powershell -NoProfile -File tests\offline-host-plugin-contract.ps1; if($LASTEXITCODE){exit $LASTEXITCODE}; if((Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue).Count){exit 9} } -ArgumentList 'C:\OfflineRun\sdk',$nonce -ErrorAction Stop
    $after=(Get-VMNetworkAdapter -VMName $guest).Count; $routes=Invoke-Command -Session $session { @(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue).Count }; if($after -ne 0 -or $routes -ne 0){throw 'guest network isolation check failed'}
    @{schema_version=1;bundle_sha256=$bundleHash;base_vhdx_sha256=$baseHash;nonce=$nonce;network=@{before_nics=$before;after_nics=$after;routes=@()};egress_attempts=@{direct='blocked';child='blocked'}} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $AttestationPath -Encoding utf8
} finally {
    try{if($session){Remove-PSSession $session -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; try{if(Get-VM -Name $guest -ErrorAction SilentlyContinue){Stop-VM -Name $guest -TurnOff -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; try{if(Get-VM -Name $guest -ErrorAction SilentlyContinue){Remove-VM -Name $guest -Force -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; try{if(Test-Path $vhd){Remove-Item -LiteralPath $vhd -Force -ErrorAction Stop}}catch{$errors+=$_.Exception.Message}; if($errors.Count){throw ($errors -join '; ')}
}
