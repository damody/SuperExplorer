$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'gpui-contract-test-support.psm1') -Force

$rejected = $false
try {
    Assert-ExactGpuiFeatureSet -Actual @('screen-capture') -Expected @()
} catch {
    $rejected = $true
}
if (-not $rejected) {
    throw 'GPUI contract accepted an unexpected production feature.'
}

Assert-ExactGpuiFeatureSet -Actual @() -Expected @()

$approvedPath = 'C:\fixture\vendor\gpui-ce\crates\gpui\Cargo.toml'
function New-MetadataFixture {
    param([switch]$Duplicate, [switch]$WrongPath, [switch]$Unreachable, [switch]$MissingNode)
    $gpui = [pscustomobject]@{ id='gpui-id'; name='gpui'; version='0.2.2'; manifest_path=$(if ($WrongPath) { 'C:\other\gpui\Cargo.toml' } else { $approvedPath }) }
    $packages = @([pscustomobject]@{ id='app-id'; name='explorer-app'; version='0.1.0'; manifest_path='C:\fixture\app\Cargo.toml' }, $gpui)
    if ($Duplicate) { $packages += [pscustomobject]@{ id='gpui-2'; name='gpui'; version='9.9.9'; manifest_path='C:\other\Cargo.toml' } }
    $nodes = @([pscustomobject]@{ id='app-id'; dependencies=$(if ($Unreachable) { @() } else { @('gpui-id') }); features=@() })
    if (-not $MissingNode) { $nodes += [pscustomobject]@{ id='gpui-id'; dependencies=@(); features=@() } }
    [pscustomobject]@{ packages=$packages; resolve=[pscustomobject]@{ nodes=$nodes } }
}
$null = Assert-ApprovedGpuiMetadata (New-MetadataFixture) '0.2.2' $approvedPath
foreach ($case in @(
    @{ Name='second GPUI'; Data=(New-MetadataFixture -Duplicate) },
    @{ Name='wrong manifest'; Data=(New-MetadataFixture -WrongPath) },
    @{ Name='unreachable'; Data=(New-MetadataFixture -Unreachable) },
    @{ Name='missing node'; Data=(New-MetadataFixture -MissingNode) }
)) {
    try { $null = Assert-ApprovedGpuiMetadata $case.Data '0.2.2' $approvedPath; throw "did not reject $($case.Name)" }
    catch { if ($_.Exception.Message -like 'did not reject*') { throw }; Write-Output "rejected $($case.Name)" }
}
try { $null = Get-GpuiProductionFeatures 'explorer-app v0.1.0|' '0.2.2'; throw 'did not reject missing cargo tree GPUI line' }
catch { if ($_.Exception.Message -like 'did not reject*') { throw }; Write-Output 'rejected missing cargo tree GPUI line' }
Write-Output 'GPUI baseline contract self-tests passed.'
