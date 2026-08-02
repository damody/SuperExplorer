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
local marker = path(fixture, "child marker-" .. tostring(os.time()) .. "-" .. tostring(math.floor(os.clock() * 1000000)) .. ".txt")
local evidence_marker = path(fixture, "child marker.txt")
local child_script = path(output, "write-marker.ps1")
write_file(child_script, [[
param([string]$Marker, [string]$EvidenceMarker)
Start-Sleep -Milliseconds 1400
[IO.File]::WriteAllText($Marker, 'started', [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText($EvidenceMarker, 'started', [Text.UTF8Encoding]::new($false))
]])

local run_log = path(output, "process-run.log")
process.run({
    stage = "既有等待程序測試",
    exe = "powershell.exe",
    cwd = output,
    log_path = run_log,
    args = { "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", "Write-Output 'run-contract'" },
})
assert_contains(read_file(run_log), "run-contract", "process.run log")
local run_ok, run_failure = pcall(process.run, {
    stage = "既有失敗程序測試",
    exe = "powershell.exe",
    cwd = output,
    log_path = path(output, "process-run-failure.log"),
    args = { "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", "exit 7" },
})
assert(not run_ok and type(run_failure) == "table" and run_failure.exit_code == 7,
    "process.run failure contract changed")

process.start({
    stage = "受控非等待啟動測試",
    exe = "powershell.exe",
    cwd = fixture,
    args = {
        "-NoLogo", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden",
        "-ExecutionPolicy", "Bypass", "-File", child_script, marker, evidence_marker,
    },
})
assert(lfs.attributes(marker, "mode") == nil, "process.start waited for the child to finish")

for _ = 1, 6 do
    if lfs.attributes(marker, "mode") == "file" then break end
    os.execute("ping.exe -n 2 127.0.0.1 >nul")
end
local marker_attributes = lfs.attributes(marker)
assert(marker_attributes and marker_attributes.mode == "file" and marker_attributes.size == 7,
    "controlled child did not receive the Unicode marker path")
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
local batch = read_file(path(root, "build_install.bat"))
local check_return = assert(build_script:find("if options.check then", 1, true))
local output_validation = assert(build_script:find('validate_executable(output, "安裝程式")', 1, true))
local launch = assert(build_script:find("process.start({", output_validation, true))
assert(check_return < output_validation and output_validation < launch, "installer launch ordering is unsafe")
assert_contains(build_script:sub(launch), "exe = output", "installer launch")
assert_contains(build_script:sub(launch), "cwd = dist", "installer launch")
for _, forbidden in ipairs({ "Get-ChildItem", "latest installer", "newest installer", "*.exe" }) do
    assert_not_contains(build_script, forbidden, "build_install.lua")
end
assert_contains(build_script, '"*.rs", ":(exclude)sdk/**"', "installer Rust cleanliness SDK exclusion")
assert_contains(build_script, '"status", "--porcelain=v1", "--untracked-files=all"',
    "installer Rust cleanliness guard")
assert_contains(batch, '"%LUA_EXE%" "%BUILD_SCRIPT%" %*', "build_install.bat")
assert_contains(batch, 'exit /b %BUILD_EXIT_CODE%', "build_install.bat")
assert_contains(batch, "Installer build completed and launched", "build_install.bat")
assert_contains(batch, "Installer build check completed; no installer was created or launched", "build_install.bat")
assert_contains(batch, '"%%~A"=="--check"', "build_install.bat")
assert_not_contains(batch, "pause", "build_install.bat")
assert_not_contains(batch, "請按任意鍵", "build_install.bat")
assert_not_contains(batch, "dist\\", "build_install.bat")

write_file(path(output, "report.json"), [[{
  "schema": "installer-build-handoff-v1",
  "result": "PASS",
  "controlled_child": "powershell-marker",
  "real_installer_launched": false,
  "non_waiting": true,
  "unicode_path": true,
  "launch_rejection_structured": true,
  "batch_exit_forwarding": true
}
]])

os.remove(child_script)
print("Installer build handoff contract PASS: " .. output)
