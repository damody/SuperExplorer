local script_path = assert(arg[0], "missing script path")
local scripts_dir = assert(script_path:match("^(.*)[\\/]test_component_installer_build%.lua$"))
local root = scripts_dir:match("^(.*)[\\/]scripts$")
if not root and scripts_dir:lower() == "scripts" then root = "." end
assert(root, "script must be located under the workspace scripts directory")
package.path = root .. "/build/lib/?.lua;" .. package.path

local components = require("installer_components")
local sdk_version = require("sdk_version")

local function read_file(file_path)
    local file = assert(io.open(file_path, "rb"))
    local contents = assert(file:read("*a"))
    assert(file:close())
    return contents
end

local function write_file(file_path, contents)
    local file = assert(io.open(file_path, "wb"))
    assert(file:write(contents))
    assert(file:close())
end

local function assert_contains(text, value, label)
    assert(text:find(value, 1, true), label .. " missing: " .. value)
end

local function assert_not_contains(text, value, label)
    assert(not text:lower():find(value:lower(), 1, true), label .. " contains: " .. value)
end

local function expect_failure(label, expected, action)
    local ok, failure = pcall(action)
    assert(not ok, label .. " unexpectedly passed")
    assert(tostring(failure):find(expected, 1, true), label .. " wrong diagnostic: " .. tostring(failure))
end

expect_failure("missing Git repository", "無法讀取 Git 提交資訊", function()
    sdk_version.resolve(root .. "/target/missing-git-repository")
end)

local all = components.parse_options({ "--component", "all", "--check", "--no-launch" })
assert(all.component == "all" and all.include_superexplorer and all.include_superdesktop)
local formal_with_openspec_untracked = components.parse_options({
    "--component", "all", "--ignore-superdesktop-openspec-untracked",
})
assert(formal_with_openspec_untracked.ignore_superdesktop_openspec_untracked)
local explorer = components.parse_options({ "--component=superexplorer", "--allow-superexplorer-dirty" })
assert(explorer.include_superexplorer and not explorer.include_superdesktop and explorer.allow_superexplorer_dirty)
local auto_install_explorer = components.parse_options({
    "--component=superexplorer", "--allow-superexplorer-dirty", "--auto-install",
})
assert(auto_install_explorer.auto_install)
local desktop = components.parse_options({ "--component", "superdesktop", "--allow-superdesktop-dirty" })
assert(not desktop.include_superexplorer and desktop.include_superdesktop and desktop.allow_superdesktop_dirty)

expect_failure("missing component", "必須指定唯一", function() components.parse_options({ "--check" }) end)
expect_failure("unknown component", "必須指定唯一", function() components.parse_options({ "--component", "other" }) end)
expect_failure("conflicting component", "只能指定一個", function()
    components.parse_options({ "--component", "all", "--component", "superdesktop" })
end)
expect_failure("explorer allowance leak", "只能用於 superexplorer", function()
    components.parse_options({ "--component", "all", "--allow-superexplorer-dirty" })
end)
expect_failure("desktop allowance leak", "只能用於 superdesktop", function()
    components.parse_options({ "--component", "all", "--allow-superdesktop-dirty" })
end)
expect_failure("OpenSpec untracked allowance leak", "can only be used with --component all", function()
    components.parse_options({ "--component", "superdesktop", "--ignore-superdesktop-openspec-untracked" })
end)
expect_failure("auto-install formal leak", "只能用於 superexplorer", function()
    components.parse_options({ "--component", "all", "--auto-install" })
end)
expect_failure("auto-install desktop leak", "只能用於 superexplorer", function()
    components.parse_options({ "--component", "superdesktop", "--auto-install" })
end)

local openspec_untracked = table.concat({
    "?? openspec/changes/example/evidence/focused/stdout.log",
    "?? openspec/changes/example/evidence/focused/report.json",
    "?? openspec/changes/example/proposal.md",
}, "\n")
assert(components.filter_superdesktop_status(openspec_untracked, true) == "")
assert(components.filter_superdesktop_status(openspec_untracked, false) == openspec_untracked)
local test_results = table.concat({
    "?? utit-results/gui-taskbar-live/report.json",
    " M utit-results/gui-taskbar-live/summary.md",
    "?? utit-results\\gui-taskbar-live\\taskbar.png",
}, "\n")
assert(components.filter_superdesktop_status(test_results, true) == "")
assert(components.filter_superdesktop_status(test_results, false) == "")
local mixed_status = table.concat({
    "?? openspec/changes/example/evidence/focused/stdout.log",
    "?? utit-results/gui-taskbar-live/report.json",
    " M crates/taskbar-ui/src/start.rs",
    "?? openspec/changes/example/evidence/focused/report.json",
    "?? openspec/changes/example/proposal.md",
    "?? build/installer.log",
    " M openspec/changes/example/evidence/focused/tracked.log",
}, "\n")
assert(components.filter_superdesktop_status(mixed_status, true) == table.concat({
    " M crates/taskbar-ui/src/start.rs",
    "?? build/installer.log",
    " M openspec/changes/example/evidence/focused/tracked.log",
}, "\n"))

local hash = string.rep("a", 40)
local identity = {
    initialized = true,
    head = hash,
    declared_url = components.approved_superdesktop_url,
    configured_url = components.approved_superdesktop_url,
    origin_url = components.approved_superdesktop_url,
    mode = "160000",
    gitlink = hash,
    status = "",
}
assert(components.validate_submodule_identity(identity))
local function changed(key, value)
    local copy = {}
    for name, original in pairs(identity) do copy[name] = original end
    copy[key] = value
    return copy
end
expect_failure("missing initialization", "尚未初始化", function()
    components.validate_submodule_identity(changed("initialized", false))
end)
expect_failure("wrong origin", "origin 與核准 URL 不符", function()
    components.validate_submodule_identity(changed("origin_url", "https://example.invalid/SuperDesktop.git"))
end)
expect_failure("missing gitlink", "gitlink 缺失", function()
    components.validate_submodule_identity(changed("mode", "100644"))
end)
expect_failure("gitlink mismatch", "HEAD 與 parent gitlink 不符", function()
    components.validate_submodule_identity(changed("gitlink", string.rep("b", 40)))
end)
expect_failure("dirty formal source", "未提交的 product/build source", function()
    components.validate_submodule_identity(changed("status", " M crates/app/src/main.rs"))
end)

local pe_fixture = root .. "/target/component-installer-pe-fixture.exe"
write_file(pe_fixture, "MZ" .. string.rep("\0", 1022))
assert(components.validate_pe(pe_fixture, "fixture") == 1024)
write_file(pe_fixture, "NO" .. string.rep("\0", 1022))
expect_failure("invalid selected PE", "不是有效的 Windows 執行檔", function()
    components.validate_pe(pe_fixture, "fixture")
end)
assert(os.remove(pe_fixture))
expect_failure("missing selected PE", "不存在", function()
    components.validate_pe(pe_fixture, "fixture")
end)

local build = read_file(root .. "/build/build_install.lua")
local formal_batch = read_file(root .. "/build_install.bat")
local explorer_batch = read_file(root .. "/build_test_install.bat")
local desktop_batch = read_file(root .. "/build_desktop_test_install.bat")
local explorer_nsis = read_file(root .. "/installer/SuperExplorer.nsi")
local desktop_nsis = read_file(root .. "/installer/SuperDesktop.nsi")
local desktop_include = read_file(root .. "/installer/SuperDesktopFiles.nsh")

assert_contains(formal_batch, "--component all --ignore-superdesktop-openspec-untracked", "formal batch")
assert_contains(explorer_batch,
    "--component superexplorer --allow-superexplorer-dirty --auto-install", "explorer batch")
assert_contains(desktop_batch, "--component superdesktop --allow-superdesktop-dirty", "desktop batch")
assert_contains(build, 'if options.include_superexplorer then', "SuperExplorer selection")
assert_contains(build, 'if options.include_superdesktop then', "SuperDesktop selection")
assert_contains(build, '"--workspace", "--all-targets", "--release", "--locked", "--offline"',
    "SuperDesktop reproducible build")
assert_contains(build, '":(exclude)**/utit-results/**"', "generated test-result Rust exclusion")
assert_contains(build, '":(exclude)utit-results/**"', "SuperDesktop test-result status exclusion")
assert_contains(build, 'options.component == "superdesktop" and "SuperDesktop.nsi" or "SuperExplorer.nsi"',
    "NSIS mode selection")
assert_contains(build, 'if options.auto_install and not options.no_launch then',
    "explicit SuperExplorer auto-install branch")
assert_contains(build, 'args = { "/S" }', "silent installer argument")
assert_contains(build, 'verify_installed_superexplorer(superexplorer_inputs, logs)',
    "installed binary identity gate")
assert_contains(build, 'for _, view in ipairs({ "64", "32" }) do',
    "NSIS registry-view install directory resolver")
assert_not_contains(formal_batch, "--auto-install", "formal batch auto-install isolation")
assert_not_contains(desktop_batch, "--auto-install", "desktop batch auto-install isolation")
assert_contains(explorer_nsis, "!ifdef INCLUDE_SUPERDESKTOP", "combined installer guard")
assert_contains(explorer_nsis, '!insertmacro InstallSuperDesktopFiles "$INSTDIR"', "combined install")
assert_contains(explorer_nsis, '!insertmacro QuiesceSuperDesktopFiles "$INSTDIR"',
    "combined process quiescence")
assert_contains(explorer_nsis, '!insertmacro UninstallSuperDesktopFiles "$INSTDIR"', "combined uninstall")
assert_contains(explorer_nsis, '!define MUI_FINISHPAGE_RUN "$INSTDIR\\superdesktop-app.exe"',
    "combined installer finish launch")
assert_contains(explorer_nsis, '!define MUI_FINISHPAGE_RUN_PARAMETERS "--shell"',
    "combined installer shell mode")
assert_contains(desktop_nsis, 'InstallDir "$PROGRAMFILES64\\${PRODUCT_NAME}"', "desktop install root")
assert_contains(desktop_nsis, '!insertmacro QuiesceSuperDesktopFiles "$INSTDIR"',
    "desktop process quiescence")
assert_contains(desktop_include, 'superdesktop-process-closer.exe', "embedded process closer")
assert_contains(desktop_include, 'quiesce --install-dir "${TARGET}"', "exact install root quiescence")
assert_contains(desktop_nsis, '!define MUI_FINISHPAGE_RUN "$INSTDIR\\superdesktop-app.exe"',
    "desktop installer finish launch")
assert_contains(desktop_nsis, '!define MUI_FINISHPAGE_RUN_PARAMETERS "--shell"',
    "desktop installer shell mode")
assert_contains(desktop_include,
    'CreateShortcut "$SMPROGRAMS\\SuperDesktop\\SuperDesktop.lnk" "${TARGET}\\superdesktop-app.exe" "--shell"',
    "start menu shell launch")
assert_contains(desktop_include,
    'CreateShortcut "$DESKTOP\\SuperDesktop.lnk" "${TARGET}\\superdesktop-app.exe" "--shell"',
    "desktop shortcut shell launch")
for _, executable in ipairs({
    "superdesktop-app.exe", "superdesktop-guardian.exe", "shell-installer.exe",
    "shell-provider-host.exe", "notification-area-host.exe", "taskbar-state-host.exe",
}) do
    assert_contains(desktop_include, executable, "desktop shared file set")
end
assert_not_contains(desktop_nsis, "SuperExplorer.exe", "desktop-only NSIS")
assert_not_contains(desktop_nsis, "PLUGIN_", "desktop-only NSIS")
for _, forbidden in ipairs({ "--apply", "Winlogon", "SetRebootFlag", "Reboot", "taskkill", "Stop-Process", "shutdown.exe" }) do
    assert_not_contains(desktop_include, forbidden, "SuperDesktop shared NSIS safety")
    assert_not_contains(desktop_nsis, forbidden, "SuperDesktop-only NSIS safety")
end

local report_path = arg[1]
if report_path then
    write_file(report_path, [[{
  "schema": "component-installer-fixtures/v1",
  "result": "passed",
  "mode_success_cases": 3,
  "mode_negative_cases": 5,
  "submodule_success_cases": 1,
  "submodule_negative_cases": 5,
  "pe_success_cases": 1,
  "pe_negative_cases": 2,
  "component_isolation": "passed",
  "shell_safety": "passed"
}
]])
end

print("Component installer build fixtures PASS")
