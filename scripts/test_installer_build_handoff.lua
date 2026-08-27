local script_path = assert(arg[0], "missing script path")
local scripts_dir = assert(script_path:match("^(.*)[\\/]test_installer_build_handoff%.lua$"))
local root = scripts_dir:match("^(.*)[\\/]scripts$")
if not root and scripts_dir:lower() == "scripts" then
    root = "."
end
assert(root, "script must be located under the workspace scripts directory")
package.path = root .. "/build/lib/?.lua;" .. package.path
package.cpath = root .. "/build/tools/lua/?.dll;" .. package.cpath

local fs = require("fs")
local lfs = require("lfs")
local path_util = require("path")
local process = require("process")

local function path(...)
    return path_util.join(...)
end

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

local function assert_contains(text, expected, label)
    assert(text:find(expected, 1, true), label .. " is missing: " .. expected)
end

local function assert_not_contains(text, forbidden, label)
    assert(not text:lower():find(forbidden:lower(), 1, true), label .. " contains forbidden text: " .. forbidden)
end

local output = arg[1] or path(root, "target", "installer-handoff-contract")
if not output:match("^%a:[\\/]") then output = path(lfs.currentdir(), output) end
fs.mkdir_p(output)
local fixture = path(output, "Unicode 啟動 fixture")
fs.mkdir_p(fixture)
local marker = path(output, "child-marker-" .. tostring(os.time()) .. "-" .. tostring(math.floor(os.clock() * 1000000)) .. ".txt")
local evidence_marker = path(output, "child-marker.txt")
local child_script = path(output, "write-marker.lua")
write_file(child_script, [[
local marker, evidence_marker = assert(arg[1]), assert(arg[2])
assert(os.execute("ping.exe -n 3 127.0.0.1 >nul"))
for _, target in ipairs({ marker, evidence_marker }) do
    local file = assert(io.open(target, "wb"))
    assert(file:write("started"))
    assert(file:close())
end
]])

local run_log = path(output, "process-run.log")
process.run({
    stage = "既有等待程序測試",
    exe = "cmd.exe",
    cwd = output,
    log_path = run_log,
    args = { "/d", "/c", "echo run-contract" },
})
assert_contains(read_file(run_log), "run-contract", "process.run log")
local run_ok, run_failure = pcall(process.run, {
    stage = "既有失敗程序測試",
    exe = "cmd.exe",
    cwd = output,
    log_path = path(output, "process-run-failure.log"),
    args = { "/d", "/c", "exit 7" },
})
assert(not run_ok and type(run_failure) == "table" and run_failure.exit_code == 7,
    "process.run failure contract changed")

process.start({
    stage = "受控非等待啟動測試",
    exe = path(lfs.currentdir(), "build", "tools", "lua", "lua.exe"),
    cwd = output,
    args = {
        child_script, marker, evidence_marker,
    },
})
assert(lfs.attributes(marker, "mode") == nil, "process.start waited for the child to finish")

for _ = 1, 6 do
    if lfs.attributes(marker, "mode") == "file" then break end
    os.execute("ping.exe -n 2 127.0.0.1 >nul")
end
local marker_attributes = lfs.attributes(marker)
assert(marker_attributes and marker_attributes.mode == "file" and marker_attributes.size == 7,
    "controlled child did not receive the marker path")
local evidence_attributes = lfs.attributes(evidence_marker)
assert(evidence_attributes and evidence_attributes.mode == "file" and evidence_attributes.size == 7,
    "controlled child did not write the stable evidence marker")

local ok, failure = pcall(process.start, {
    stage = "拒絕啟動測試",
    exe = path(fixture, "missing installer.exe"),
    cwd = fixture,
})
assert(not ok and type(failure) == "table", "launch rejection was not structured")
assert(failure.stage == "拒絕啟動測試", "launch failure lost its stage")
assert(failure.cwd == fixture, "launch failure lost its working directory")
assert(type(failure.exit_code) == "number" and failure.exit_code ~= 0, "launch failure lost its exit code")

local build_script = read_file(path(root, "build", "build_install.lua"))
local sdk_version = read_file(path(root, "build", "lib", "sdk_version.lua"))
local batch = read_file(path(root, "build_install.bat"))
local test_batch = read_file(path(root, "build_test_install.bat"))
local desktop_test_batch = read_file(path(root, "build_desktop_test_install.bat"))
local check_return = assert(build_script:find("if options.check then", 1, true))
local output_validation = assert(build_script:find('validate_executable(output, "安裝程式")', 1, true))
local launch = assert(build_script:find("process.start({", output_validation, true))
assert(check_return < output_validation and output_validation < launch, "installer launch ordering is unsafe")
assert_contains(build_script:sub(launch), "exe = output", "installer launch")
assert_contains(build_script:sub(launch), "cwd = dist", "installer launch")
for _, forbidden in ipairs({ "Get-ChildItem", "latest installer", "newest installer", "*.exe" }) do
    assert_not_contains(build_script, forbidden, "build_install.lua")
end
assert_contains(build_script, '"*.rs"', "installer Rust cleanliness pathspec")
assert_contains(build_script, '":(exclude)sdk/**"', "installer Rust cleanliness SDK exclusion")
assert_contains(build_script, '"status", "--porcelain=v1", "--untracked-files=all"',
    "installer Rust cleanliness guard")
assert_contains(build_script, 'installer_components.parse_options(arg)',
    "component option parser")
assert_contains(build_script, 'if options.component == "all" then',
    "formal installer cleanliness boundary")
assert_contains(build_script, 'path(logs, "installer-superdesktop-status.log")',
    "SuperDesktop status capture")
assert_contains(build_script, 'echo_output = echo_output',
    "captured command output control")
assert_contains(batch,
    '"%LUA_EXE%" "%BUILD_SCRIPT%" --component all --ignore-superdesktop-openspec-untracked %*',
    "build_install.bat")
assert_contains(batch, 'exit /b %BUILD_EXIT_CODE%', "build_install.bat")
assert(batch:sub(1, 3) ~= "\239\187\191", "build_install.bat must not contain a UTF-8 BOM")
assert_contains(batch, '"%SystemRoot%\\System32\\chcp.com" 65001',
    "build_install.bat UTF-8 setup")
assert_contains(batch, '"%ProgramFiles%\\Git\\cmd\\git.exe"',
    "build_install.bat Git fallback")
assert_contains(batch, 'if not defined GIT_EXE (', "build_install.bat Git requirement")
assert_contains(batch, 'set "PATH=%GIT_BIN_DIR%;%SystemRoot%\\System32;',
    "build_install.bat deterministic tool path")
assert_not_contains(batch, 'set "GIT_DIR=', "build_install.bat Git environment isolation")
assert_contains(batch, "Installer build completed and launched", "build_install.bat")
assert_contains(batch, "Installer build check completed; no installer was created or launched", "build_install.bat")
assert_contains(batch, '"%%~A"=="--check"', "build_install.bat")
assert_contains(batch, 'set "PAUSE_ON_FAILURE=0"', "build_install.bat")
assert_contains(batch, 'if "%~1"=="" if not defined CI set "PAUSE_ON_FAILURE=1"',
    "build_install.bat")
assert_contains(batch, 'if not "%PAUSE_ON_FAILURE%"=="1" goto :report_done',
    "build_install.bat")
assert_contains(batch, "pause >nul", "build_install.bat")
local successful_paths = assert(batch:match("\n:finish(.-)\n:report_failure"))
local failure_path = assert(batch:match("\n:report_failure(.-)\n:report_done"))
assert_not_contains(successful_paths, "pause >nul", "build_install.bat success paths")
assert_contains(failure_path, "pause >nul", "build_install.bat failure path")
assert_contains(sdk_version, "無法讀取 Git 提交資訊", "sdk_version Git diagnostic")
assert_contains(sdk_version, "2>&1", "sdk_version Git stderr capture")
assert_not_contains(sdk_version, "2>NUL", "sdk_version Git stderr capture")
assert_not_contains(batch, "請按任意鍵", "build_install.bat")
assert_not_contains(batch, "dist\\", "build_install.bat")
assert_contains(test_batch, '"%LUA_EXE%" "%BUILD_SCRIPT%" --component superexplorer --allow-superexplorer-dirty %*',
    "build_test_install.bat")
assert_contains(test_batch, 'exit /b %BUILD_EXIT_CODE%', "build_test_install.bat")
assert_contains(test_batch, '"%%~A"=="--check"', "build_test_install.bat")
assert_contains(test_batch, "SuperExplorer test installer build completed", "build_test_install.bat")
assert_not_contains(test_batch, "git.exe", "build_test_install.bat")
assert_contains(test_batch, 'set "KEEP_CONSOLE=0"', "build_test_install.bat")
assert_contains(test_batch, 'if "%~1"=="" if not defined CI set "KEEP_CONSOLE=1"',
    "build_test_install.bat interactive console")
assert_contains(test_batch, "pause >nul", "build_test_install.bat interactive console")
assert_contains(test_batch, "RustGpuiExplorer\\logs\\error.log", "build_test_install.bat error log hint")
assert_contains(desktop_test_batch,
    '"%LUA_EXE%" "%BUILD_SCRIPT%" --component superdesktop --allow-superdesktop-dirty %*',
    "build_desktop_test_install.bat")
assert_contains(desktop_test_batch, 'exit /b %BUILD_EXIT_CODE%', "build_desktop_test_install.bat")
assert_contains(desktop_test_batch, '"%%~A"=="--check"', "build_desktop_test_install.bat")
assert_contains(desktop_test_batch, "SuperDesktop test installer build completed",
    "build_desktop_test_install.bat")
assert_not_contains(desktop_test_batch, "git.exe", "build_desktop_test_install.bat")
assert_not_contains(desktop_test_batch, "pause", "build_desktop_test_install.bat")

write_file(path(output, "report.json"), [[{
  "schema": "installer-build-handoff-v1",
  "result": "PASS",
  "controlled_child": "lua-marker",
  "real_installer_launched": false,
  "non_waiting": true,
  "unicode_rejection_path": true,
  "launch_rejection_structured": true,
  "batch_exit_forwarding": true
}
]])

os.remove(child_script)
print("Installer build handoff contract PASS: " .. output)
