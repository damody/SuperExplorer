param(
    [Parameter(Mandatory = $true)] [string]$InstallerPath,
    [string[]]$PayloadPaths = @(),
    [string]$CertificateSubject = 'CN=SuperExplorer Test Signer',
    [switch]$SkipDefenderExclusion,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$subject = $CertificateSubject

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not $SelfTest -and -not (Test-Administrator)) {
    $argList = @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', $PSCommandPath,
        '-InstallerPath', $InstallerPath,
        '-CertificateSubject', $CertificateSubject
    )
    if ($SkipDefenderExclusion) { $argList += '-SkipDefenderExclusion' }
    foreach ($payload in @($PayloadPaths)) {
        if (-not [string]::IsNullOrWhiteSpace($payload)) {
            $argList += '-PayloadPaths'
            $argList += $payload
        }
    }
    $elevated = Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
        -Verb RunAs -Wait -PassThru -ArgumentList $argList
    if ($null -eq $elevated -or $elevated.ExitCode -ne 0) {
        throw "elevated Windows prepare failed with exit $($elevated.ExitCode)"
    }
    $status = Get-AuthenticodeSignature -LiteralPath ([IO.Path]::GetFullPath($InstallerPath))
    if ($status.Status -eq 'NotSigned') {
        throw "elevated Windows prepare left the installer unsigned: $InstallerPath"
    }
    Write-Output ("elevated prepare completed status=" + $status.Status + " path=" + $InstallerPath)
    exit 0
}

function Get-OrCreateCodeSigningCertificate {
    $existing = @(Get-ChildItem -LiteralPath 'Cert:\CurrentUser\My' |
        Where-Object {
            $_.Subject -eq $subject -and
            $_.HasPrivateKey -and
            $_.NotAfter -gt (Get-Date).AddDays(1) -and
            ($_.EnhancedKeyUsageList | Where-Object { $_.ObjectId -eq '1.3.6.1.5.5.7.3.3' })
        } |
        Sort-Object NotAfter -Descending)
    if ($existing.Count -gt 0) { return $existing[0] }
    return New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $subject `
        -FriendlyName 'SuperExplorer Test Signer' `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -HashAlgorithm SHA256 `
        -KeyExportPolicy Exportable `
        -NotAfter (Get-Date).AddYears(10)
}

function Import-CertificateIfMissing {
    param(
        [Parameter(Mandatory = $true)] [System.Security.Cryptography.X509Certificates.X509Certificate2]$Certificate,
        [Parameter(Mandatory = $true)] [string]$StorePath
    )
    $store = Get-ChildItem -LiteralPath $StorePath -ErrorAction SilentlyContinue |
        Where-Object { $_.Thumbprint -eq $Certificate.Thumbprint }
    if ($store) { return }
    $exported = Join-Path ([IO.Path]::GetTempPath()) ("superexplorer-test-signer-" + $Certificate.Thumbprint + ".cer")
    try {
        Export-Certificate -Cert $Certificate -FilePath $exported | Out-Null
        Import-Certificate -FilePath $exported -CertStoreLocation $StorePath | Out-Null
    } catch {
        Write-Output "certificate import skipped for ${StorePath}: $($_.Exception.Message)"
    } finally {
        Remove-Item -LiteralPath $exported -Force -ErrorAction SilentlyContinue
    }
}

function Sign-WindowsBinary {
    param(
        [Parameter(Mandatory = $true)] [string]$Path,
        [Parameter(Mandatory = $true)] [System.Security.Cryptography.X509Certificates.X509Certificate2]$Certificate
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "binary does not exist: $Path"
    }
    Unblock-File -LiteralPath $Path -ErrorAction SilentlyContinue
    $zone = $Path + ':Zone.Identifier'
    if (Test-Path -LiteralPath $zone) {
        Remove-Item -LiteralPath $zone -Force -ErrorAction SilentlyContinue
    }
    $signed = $null
    try {
        $signed = Set-AuthenticodeSignature -FilePath $Path -Certificate $Certificate -HashAlgorithm SHA256 -TimestampServer 'http://timestamp.digicert.com'
    } catch {
        $signed = $null
    }
    if ($null -eq $signed -or $signed.Status -ne 'Valid') {
        $signed = Set-AuthenticodeSignature -FilePath $Path -Certificate $Certificate -HashAlgorithm SHA256
    }
    if ($null -eq $signed -or $signed.Status -eq 'NotSigned') {
        throw "Authenticode signing failed for $Path"
    }
    Write-Output ("signed " + $Path + " status=" + $signed.Status)
}

function Add-DefenderExclusionSafe {
    param([Parameter(Mandatory = $true)] [string]$Path)
    try {
        $current = @(Get-MpPreference -ErrorAction Stop | Select-Object -ExpandProperty ExclusionPath)
        if ($current -contains $Path) {
            Write-Output "defender exclusion already present: $Path"
            return
        }
        Add-MpPreference -ExclusionPath $Path -ErrorAction Stop
        Write-Output "defender exclusion added: $Path"
    } catch {
        Write-Output "defender exclusion skipped for ${Path}: $($_.Exception.Message)"
    }
}

if ($SelfTest) {
    $fixture = Join-Path ([IO.Path]::GetTempPath()) ('superexplorer-sign-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $fixture | Out-Null
    try {
        $probe = Join-Path $fixture 'probe.exe'
        Copy-Item -LiteralPath "$env:SystemRoot\System32\cmd.exe" -Destination $probe
        $InstallerPath = $probe
        $SkipDefenderExclusion = $true
        $certificate = Get-OrCreateCodeSigningCertificate
        Import-CertificateIfMissing -Certificate $certificate -StorePath 'Cert:\CurrentUser\TrustedPublisher'
        Sign-WindowsBinary -Path $InstallerPath -Certificate $certificate
        $status = Get-AuthenticodeSignature -LiteralPath $InstallerPath
        if ($status.Status -eq 'NotSigned') { throw 'self-test remained unsigned' }
        Write-Output 'prepare_test_installer_windows self-test PASS'
        exit 0
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$installer = [IO.Path]::GetFullPath($InstallerPath)
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
    throw "installer does not exist: $installer"
}

$certificate = Get-OrCreateCodeSigningCertificate
Import-CertificateIfMissing -Certificate $certificate -StorePath 'Cert:\CurrentUser\TrustedPublisher'
Import-CertificateIfMissing -Certificate $certificate -StorePath 'Cert:\LocalMachine\TrustedPublisher'
Import-CertificateIfMissing -Certificate $certificate -StorePath 'Cert:\LocalMachine\Root'

foreach ($payload in @($PayloadPaths)) {
    if (-not [string]::IsNullOrWhiteSpace($payload)) {
        Sign-WindowsBinary -Path $payload -Certificate $certificate
    }
}
Sign-WindowsBinary -Path $installer -Certificate $certificate

if (-not $SkipDefenderExclusion) {
    Add-DefenderExclusionSafe -Path $installer
    Add-DefenderExclusionSafe -Path ([IO.Path]::GetDirectoryName($installer))
    $programFiles = $env:ProgramW6432
    if ([string]::IsNullOrWhiteSpace($programFiles)) { $programFiles = $env:ProgramFiles }
    if (-not [string]::IsNullOrWhiteSpace($programFiles)) {
        Add-DefenderExclusionSafe -Path (Join-Path $programFiles 'SuperExplorer')
    }
}

$final = Get-AuthenticodeSignature -LiteralPath $installer
if ($final.Status -eq 'NotSigned') {
    throw "test installer remained unsigned: $installer"
}
Write-Output ("test installer ready status=" + $final.Status + " path=" + $installer)
