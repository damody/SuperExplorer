[CmdletBinding()]param()
$ErrorActionPreference='Stop';Import-Module (Join-Path $PSScriptRoot 'contract-test-support.psm1') -Force
$sdk=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path;$repo=(Resolve-Path (Join-Path $sdk '..')).Path;Push-Location $sdk
try{$r=& rustc -Vv|Out-String;if($LASTEXITCODE){throw 'rustc -Vv failed.'};$c=& cargo -Vv|Out-String;if($LASTEXITCODE){throw 'cargo -Vv failed.'};$a=& rustup show active-toolchain|Out-String;if($LASTEXITCODE){throw 'rustup active-toolchain failed.'};$i=& rustup target list --installed|Out-String;if($LASTEXITCODE){throw 'rustup target list failed.'};Assert-ToolchainContract $r $c $a $i (Get-Content (Join-Path $repo 'rust-toolchain.toml') -Raw) (Get-Content (Join-Path $sdk 'rust-toolchain.toml') -Raw)}finally{Pop-Location}
