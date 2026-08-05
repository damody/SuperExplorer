$ErrorActionPreference = 'Stop'
$validator = Join-Path $PSScriptRoot '..\scripts\validate-example.ps1'
$source = Join-Path $PSScriptRoot '..\fixtures\rust-folder-size-visual-column'
$temporary = Join-Path ([IO.Path]::GetTempPath()) ("superexplorer-example-validator-" + [guid]::NewGuid().ToString('N'))

function Copy-Fixture([string]$name) {
    $target = Join-Path $temporary $name
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    foreach ($item in @('Cargo.toml','Cargo.lock','plugin-project.json','README.md','README.zh-TW.md','LICENSE','NOTICE','src','locales')) {
        Copy-Item -LiteralPath (Join-Path $source $item) -Destination $target -Recurse -Force
    }
    return $target
}

function Assert-Rejected([string]$fixture, [string]$label) {
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $validator -ExampleRoot $fixture *> $null
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousPreference
    if ($exitCode -eq 0) { throw "validator accepted negative fixture: $label" }
}

try {
    $private = Copy-Fixture 'private-crate'
    Add-Content -LiteralPath (Join-Path $private 'Cargo.toml') "`nexplorer-model = { path = `"../../../crates/explorer-model`" }"
    Assert-Rejected $private 'private crate'

    $unlocked = Copy-Fixture 'unlocked-public-sdk'
    $text = (Get-Content -LiteralPath (Join-Path $unlocked 'Cargo.toml') -Raw).Replace('version = "=1.2.0"', 'version = "1.2.0"')
    Set-Content -LiteralPath (Join-Path $unlocked 'Cargo.toml') -Value $text -Encoding UTF8
    Assert-Rejected $unlocked 'unlocked SDK version'

    $bypass = Copy-Fixture 'composition-bypass'
    Set-Content -LiteralPath (Join-Path $bypass 'src\lib.rs') -Value 'pub trait MockOnly {}' -Encoding UTF8
    Assert-Rejected $bypass 'trait-only composition bypass'

    $missing = Copy-Fixture 'missing-artifact'
    Remove-Item -LiteralPath (Join-Path $missing 'README.zh-TW.md') -Force
    Assert-Rejected $missing 'missing required artifact'

    'example validator negative fixtures passed'
}
finally {
    if (Test-Path -LiteralPath $temporary) {
        Remove-Item -LiteralPath $temporary -Recurse -Force
    }
}
