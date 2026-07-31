[CmdletBinding()]
param([Parameter(Mandatory)][string]$PluginRoot)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$root = (Resolve-Path -LiteralPath $PluginRoot).Path
$core = Join-Path $sdk 'tools\plugin-tooling'
$manifestPath = Join-Path $root 'plugin-project.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'plugin-project.json required' }
if (-not (Test-Path -LiteralPath (Join-Path $core 'Cargo.toml'))) { throw 'plugin Rust core missing' }

$savedErrorAction = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$reportJson = & cargo.exe run --manifest-path (Join-Path $core 'Cargo.toml') --locked --offline -- validate $root
$exitCode = $LASTEXITCODE
$ErrorActionPreference = $savedErrorAction
$reportText = ($reportJson -join "`n")
Write-Output $reportText
if ($exitCode -ne 0) { throw 'plugin validation failed' }

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$reportDir = Join-Path $root ("target\superexplorer\$($manifest.sdk.bundle_id)\reports")
New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
[IO.File]::WriteAllText((Join-Path $reportDir 'validation.json'), "$reportText`n", [Text.UTF8Encoding]::new($false))
