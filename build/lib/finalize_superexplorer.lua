local fs = require("fs")
local lfs = require("lfs")
local process = require("process")

local M = {}
local sep = package.config:sub(1, 1)
local function join(...)
    local values = {...}
    return table.concat(values, sep):gsub("[\\/]+", sep)
end
local function file(path)
    local handle = assert(io.open(path, "rb"), "missing file: " .. path)
    local bytes = assert(handle:read("*a")); assert(handle:close()); return bytes
end
local function require_file(path)
    assert(lfs.attributes(path, "mode") == "file", "missing required file: " .. path); return path
end
local function utf16le(value)
    local bytes = {}
    for _, codepoint in utf8.codes(value) do bytes[#bytes + 1] = string.char(codepoint & 255, codepoint >> 8) end
    return table.concat(bytes)
end
local function assert_x64_pe(path)
    local bytes = file(path)
    assert(#bytes >= 1024, "executable is unexpectedly small: " .. path)
    local offset = string.unpack("<I4", bytes, 0x3d)
    assert(offset >= 0 and offset + 6 <= #bytes, "invalid PE header offset: " .. path)
    local signature = string.unpack("<I4", bytes, offset + 1)
    local machine = string.unpack("<I2", bytes, offset + 5)
    assert(signature == 0x00004550 and machine == 0x8664, "expected an x64 PE executable: " .. path)
    return bytes, machine
end
local function find_mt()
    local program_files = assert(os.getenv("ProgramFiles(x86)"), "ProgramFiles(x86) is unavailable")
    local root = join(program_files, "Windows Kits", "10", "bin")
    local versions = {}
    for name in lfs.dir(root) do
        if name ~= "." and name ~= ".." and lfs.attributes(join(root, name), "mode") == "directory" then versions[#versions + 1] = name end
    end
    table.sort(versions, function(a, b) return a > b end)
    for _, version in ipairs(versions) do
        local candidate = join(root, version, "x64", "mt.exe")
        if lfs.attributes(candidate, "mode") == "file" then return candidate end
    end
    error("Windows Manifest Tool (mt.exe) was not found", 0)
end

function M.run(root, profile, logs)
    profile = profile or "release"
    process.run({ stage = "建置 SuperExplorer release binaries", exe = "cargo.exe",
        args = {"build", "-p", "explorer-app", "-p", "explorer-extension-broker", "--locked", "--release"},
        cwd = root, log_path = join(logs, "installer-superexplorer-cargo.log") })
    local target = join(root, "target", profile)
    local executable = require_file(join(target, "SuperExplorer.exe"))
    local broker = require_file(join(target, "explorer-extension-broker.exe"))
    local worker = require_file(join(target, "explorer-extension-worker.exe"))
    local manifest = require_file(join(root, "crates", "explorer-app", "app.manifest"))
    local mt = find_mt()
    local evidence = join(root, "target", "manifest-evidence")
    fs.mkdir_p(evidence)
    local staging = join(evidence, "SuperExplorer-" .. profile .. "-staging.exe")
    if lfs.attributes(staging) then assert(os.remove(staging)) end
    assert(fs.copy_if_different(executable, staging))
    local updated = false
    for attempt = 1, 5 do
        local ok = pcall(process.run, { stage = "嵌入 SuperExplorer manifest（第 " .. attempt .. " 次）", exe = mt,
            args = {"-nologo", "-manifest", manifest, "-outputresource:" .. staging .. ";#1"}, cwd = root,
            log_path = join(logs, "installer-superexplorer-manifest-embed-" .. attempt .. ".log"), echo_output = false })
        if ok then updated = true; break end
    end
    assert(updated, "manifest update failed after 5 attempts")
    if lfs.attributes(executable) then assert(os.remove(executable)) end
    assert(os.rename(staging, executable))
    local extracted = join(evidence, "SuperExplorer-" .. profile .. ".manifest")
    process.run({ stage = "擷取 SuperExplorer manifest", exe = mt,
        args = {"-nologo", "-inputresource:" .. executable .. ";#1", "-out:" .. extracted}, cwd = root,
        log_path = join(logs, "installer-superexplorer-manifest-extract.log"), echo_output = false })
    process.run({ stage = "驗證 SuperExplorer manifest", exe = mt,
        args = {"-nologo", "-validate_manifest", "-manifest", extracted}, cwd = root,
        log_path = join(logs, "installer-superexplorer-manifest-validate.log"), echo_output = false })
    local manifest_text = file(extracted)
    for _, required in ipairs({'name="Damody.SuperExplorer"','processorArchitecture="amd64"',">PerMonitorV2<",">SegmentHeap<",'name="Microsoft.Windows.Common-Controls"','version="6.0.0.0"'}) do
        assert(manifest_text:find(required, 1, true), "final manifest is missing required value: " .. required)
    end
    local executable_bytes, machine = assert_x64_pe(executable)
    for _, value in ipairs({"SuperExplorer", "SuperExplorer.exe"}) do
        assert(executable_bytes:find(utf16le(value), 1, true), "VERSIONINFO is missing: " .. value)
    end
    for _, binary in ipairs({broker, worker}) do
        assert_x64_pe(binary)
        local _, marker = process.run({ stage = "驗證 extension binary marker", exe = binary, args = {"--version-json"}, cwd = root,
            log_path = join(logs, "installer-" .. binary:match("([^\\/]+)$") .. "-marker.log"), echo_output = false })
        assert(marker:find('"protocol":1', 1, true) and marker:find('"arch":"x64"', 1, true), "invalid extension binary marker: " .. binary)
    end
    print("Finalized and validated with Lua: " .. executable)
    print("Extracted manifest: " .. extracted)
    print(string.format("PE machine: 0x%04X (x64)", machine))
end

return M
