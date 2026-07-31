package.path = "build/lib/?.lua;" .. package.path
package.cpath = "build/tools/lua/?.dll;" .. package.cpath

local lfs = require("lfs")
local cli = require("cli")
local engine = require("engine")
local fs = require("fs")
local process = require("process")
local publish = require("publish")
local sdk_version = require("sdk_version")

local separator = package.config:sub(1, 1)
local function join(...)
    return table.concat({...}, separator)
end

local M = {
    mappings = {
        { scene = "Assets/Internal/Scenes/SampleSceneFullV5.unity", name = "MagtTestV5" },
        { scene = "Assets/Internal/Scenes/AutoTestRunV2.unity", name = "MagtAutoTestV2" },
    },
}

local signing_password_variable = "MAGT_UNITY_KEYSTORE_PASSWORD"
local default_signing_password = "mtkme3"

function M.android_environment(getenv)
    getenv = getenv or os.getenv
    local password = getenv(signing_password_variable)
    if not password or password == "" then password = default_signing_password end
    return { [signing_password_variable] = password }
end

local function default_nonce()
    local temporary = os.tmpname()
    os.remove(temporary)
    local basename = temporary:match("[^\\/]+$") or temporary
    return os.date("%Y%m%d-%H%M%S") .. "-" .. basename:gsub("[^%w_.-]", "-")
end

function M.reserve_invocation(temporary_root, platform, nonce_fn)
    fs.mkdir_p(temporary_root)
    local nonce = (nonce_fn or default_nonce)()
    assert(nonce ~= "" and not nonce:find("[\\/]"), "invalid invocation nonce")
    for counter = 0, 9999 do
        local candidate = join(temporary_root, platform .. "-" .. nonce .. "-" .. counter)
        if lfs.mkdir(candidate) then return candidate end
    end
    error("could not reserve a unique Unity invocation directory", 0)
end

function M.build_paths(invocation_root, logs_root, platform, mapping, index)
    local invocation_id = assert(invocation_root:match("[^\\/]+$"))
    local stem = mapping.name .. "-" .. index
    return {
        temporary = platform == "android"
            and join(invocation_root, stem .. ".apk")
            or join(invocation_root, stem),
        log_path = join(logs_root,
            "unity-" .. platform .. "-" .. mapping.name .. "-" .. invocation_id .. "-" .. index .. ".log"),
    }
end

function M.command(unity, project, mapping, platform, version, temporary, log_path)
    assert(platform == "android" or platform == "windows", "platform must be android or windows")
    local target = platform == "android" and "BuildTarget.Android" or "BuildTarget.StandaloneWindows64"
    local output_name = mapping.name .. "_" .. version .. (platform == "android" and ".apk" or "_Windows")
    local output = platform == "android"
        and temporary
        or join(temporary, mapping.name .. "_" .. version .. ".exe")
    return {
        exe = unity,
        target = target,
        mapping = mapping,
        output_name = output_name,
        output = output,
        args = {
            "-batchmode", "-quit",
            "-projectPath", project,
            "-executeMethod", "MagtCommandLineBuild.Build",
            "-magtScene", mapping.scene,
            "-magtOutput", output,
            "-magtPlatform", target,
            "-logFile", log_path,
        },
    }
end

local function run(platform, check_only)
    assert(platform == "android" or platform == "windows", "usage: build_unity.lua android|windows [--check-only]")
    local root = assert(lfs.currentdir())
    local project = join(root, "AndroidMAGTSample")
    local unity = engine.find_unity(project)
    local version = sdk_version.resolve(root).version
    local logs = join(root, "build", "logs")
    local temporary_root = join(root, "build", "temp", "unity")
    fs.mkdir_p(logs)

    local android_env = platform == "android" and M.android_environment() or nil
    local invocation_root = M.reserve_invocation(temporary_root, platform)

    print("Unity executable: " .. unity)
    print("SDK version: " .. version)
    print("Platform: " .. platform)

    for index, mapping in ipairs(M.mappings) do
        local paths = M.build_paths(invocation_root, logs, platform, mapping, index)
        local log_path, temporary = paths.log_path, paths.temporary
        if not check_only and platform == "windows" then fs.mkdir_p(temporary) end

        local command = M.command(unity, project, mapping, platform, version, temporary, log_path)
        local final = join(root, command.output_name)
        print("Scene: " .. mapping.scene)
        print("Output: " .. final)
        print("Log: " .. log_path)

        if not check_only then
            process.run({
                stage = "Unity " .. platform .. " " .. mapping.name,
                exe = command.exe,
                args = command.args,
                cwd = root,
                log_path = log_path,
                env = android_env,
            })
            if platform == "android" then
                publish.apk(temporary, final)
            else
                local exe_name = mapping.name .. "_" .. version .. ".exe"
                local data_name = mapping.name .. "_" .. version .. "_Data"
                assert(lfs.attributes(join(temporary, data_name), "mode") == "directory",
                    "Windows package data directory is missing: " .. data_name)
                publish.windows(temporary, final, exe_name)
            end
        end
    end
    fs.remove_tree(invocation_root)
end

M.run = run
if not rawget(_G, "MAGT_BUILD_UNITY_TEST") then
    local platform = arg[1]
    local option = arg[2]
    os.exit(cli.run(function()
        assert(option == nil or option == "--check-only", "usage: build_unity.lua android|windows [--check-only]")
        run(platform, option == "--check-only")
    end))
end

return M
