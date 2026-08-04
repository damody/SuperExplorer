$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$state = Get-Content (Join-Path $root 'crates/explorer-ui/src/state.rs') -Raw
$chrome = Get-Content (Join-Path $root 'crates/explorer-ui/src/chrome.rs') -Raw
$installer = Get-Content (Join-Path $root 'installer/SuperExplorer.nsi') -Raw
$build = Get-Content (Join-Path $root 'build/build_install.lua') -Raw
$packages = @('rust-folder-size-visual-column','rust-folder-size-map-view','rust-tokei-code-lines-column','lua-tokei-code-lines-column','rust-lock-owner-column','rust-exif-rename-command','rust-7z-virtual-folder','lua-bulk-folder-generator')
foreach ($package in $packages) {
    if (-not $state.Contains("package_id: `"$package`"")) { throw "UI catalog is missing $package" }
    if (-not $build.Contains("root = `"$package`"")) { throw "installer build is missing $package" }
}
if (($installer | Select-String -Pattern 'File /oname=' -AllMatches).Matches.Count -lt 8) { throw 'NSIS package does not contain eight plugin DLL entries' }
foreach ($selector in @('view-extension-size-map','extension-command-lua-bulk-folder-button','folder-options-extensions-tab','folder-options-extensions-page')) {
    if (-not $chrome.Contains($selector)) { throw "missing production UI selector $selector" }
}
& cargo test -p explorer-ui folder_options_manage_all_eight_extensions_with_cancel_apply_and_view_fallback --locked --offline
if ($LASTEXITCODE) { exit $LASTEXITCODE }
& cargo test -p explorer-ui extension_surfaces_are_backed_by_the_shared_eight_plugin_catalog --locked --offline
exit $LASTEXITCODE
